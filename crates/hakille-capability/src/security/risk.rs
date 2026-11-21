//! 4-Tier Risk Governance and ActionCard physical hold verification.

use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Errors arising during risk evaluation or physical action confirmation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RiskVerificationError {
    #[error("Physical confirmation required for Tier 3 action")]
    MissingConfirmationProof,

    #[error("ActionCard physical hold duration insufficient: {held_ms} ms (required: >= 600 ms)")]
    InsufficientHoldDuration { held_ms: u64 },

    #[error("ActionCard confirmation proof expired at {expired_at}")]
    ProofExpired { expired_at: DateTime<Utc> },

    #[error("Action confirmation hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },

    #[error("Action confirmation timestamp is in the future")]
    FutureTimestamp,

    #[error("Action confirmation cryptographic signature invalid: {0}")]
    InvalidSignature(String),
}

/// Cryptographic confirmation proof emitted by the UI Shell upon 600ms physical hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionConfirmationProof {
    /// ActionCard identifier.
    pub card_id: String,
    /// SHA-256 hash of the intended action and its parameters.
    pub action_hash: [u8; 32],
    /// Identity of the confirming human user.
    pub user_id: String,
    /// Measured physical interaction duration (hold ring) in milliseconds.
    pub physical_hold_duration_ms: u64,
    /// Timestamp when hold confirmation completed.
    pub confirmed_at: DateTime<Utc>,
    /// Optional Ed25519 signature of the confirmation token (64-byte signature or 96-byte pubkey||signature).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signature: Vec<u8>,
}

impl ActionConfirmationProof {
    /// Creates a new action confirmation proof.
    pub fn new(
        card_id: impl Into<String>,
        action_hash: [u8; 32],
        user_id: impl Into<String>,
        physical_hold_duration_ms: u64,
        confirmed_at: DateTime<Utc>,
    ) -> Self {
        Self {
            card_id: card_id.into(),
            action_hash,
            user_id: user_id.into(),
            physical_hold_duration_ms,
            confirmed_at,
            signature: Vec::new(),
        }
    }

    /// Attaches an optional cryptographic signature.
    pub fn with_signature(mut self, signature: Vec<u8>) -> Self {
        self.signature = signature;
        self
    }

    /// Canonical domain-separated, length-prefixed payload covered by Ed25519 signature.
    pub fn canonical_signing_payload(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(128);
        buf.extend_from_slice(b"hakille-action-confirmation-v1:");
        buf.extend_from_slice(&(self.card_id.len() as u64).to_le_bytes());
        buf.extend_from_slice(self.card_id.as_bytes());
        buf.extend_from_slice(&self.action_hash);
        buf.extend_from_slice(&(self.user_id.len() as u64).to_le_bytes());
        buf.extend_from_slice(self.user_id.as_bytes());
        buf.extend_from_slice(&self.physical_hold_duration_ms.to_le_bytes());
        buf.extend_from_slice(&self.confirmed_at.timestamp_millis().to_le_bytes());
        buf
    }

    /// Cryptographically signs this confirmation proof with an Ed25519 `SigningKey`,
    /// storing the 32-byte verifying key + 64-byte signature (96 bytes total) in `self.signature`.
    pub fn sign_ed25519(mut self, signing_key: &SigningKey) -> Self {
        let payload = self.canonical_signing_payload();
        let sig: Signature = signing_key.sign(&payload);
        let mut sig_bundle = Vec::with_capacity(96);
        sig_bundle.extend_from_slice(signing_key.verifying_key().as_bytes());
        sig_bundle.extend_from_slice(&sig.to_bytes());
        self.signature = sig_bundle;
        self
    }

    /// Computes deterministic SHA-256 action hash for a tool name and arguments payload.
    pub fn compute_action_hash(tool_name: &str, arguments: &serde_json::Value) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(tool_name.as_bytes());
        hasher.update(b":");
        let canonical_args = serde_json::to_vec(arguments).unwrap_or_default();
        hasher.update(&canonical_args);
        hasher.finalize().into()
    }

    /// Verifies the physical hold duration, validity timeframe, action hash, and
    /// cryptographic Ed25519 signature (if present).
    ///
    /// Rules:
    /// 1. `physical_hold_duration_ms >= 600` ms.
    /// 2. `confirmed_at` is not more than 30 seconds old.
    /// 3. `confirmed_at` is not more than 5 seconds in the future (clock skew tolerance).
    /// 4. `action_hash` matches `expected_action_hash`.
    /// 5. If `signature` is non-empty, it must be a valid 96-byte `(VerifyingKey || Signature)` bundle passing `verify_strict`.
    pub fn verify(&self, expected_action_hash: &[u8; 32]) -> Result<(), RiskVerificationError> {
        if self.physical_hold_duration_ms < 600 {
            return Err(RiskVerificationError::InsufficientHoldDuration {
                held_ms: self.physical_hold_duration_ms,
            });
        }

        let now = Utc::now();
        let max_validity = Duration::seconds(30);

        if self.confirmed_at > now + Duration::seconds(5) {
            return Err(RiskVerificationError::FutureTimestamp);
        }

        if now - self.confirmed_at > max_validity {
            return Err(RiskVerificationError::ProofExpired {
                expired_at: self.confirmed_at + max_validity,
            });
        }

        if &self.action_hash != expected_action_hash {
            return Err(RiskVerificationError::HashMismatch {
                expected: hex::encode(expected_action_hash),
                actual: hex::encode(self.action_hash),
            });
        }

        if !self.signature.is_empty() {
            if self.signature.len() != 96 {
                return Err(RiskVerificationError::InvalidSignature(format!(
                    "expected 96-byte (pubkey||sig) bundle, got {} bytes",
                    self.signature.len()
                )));
            }
            let pk_bytes: [u8; 32] = self.signature[0..32].try_into().unwrap();
            let sig_bytes: [u8; 64] = self.signature[32..96].try_into().unwrap();
            let verifying_key = VerifyingKey::from_bytes(&pk_bytes).map_err(|e| {
                RiskVerificationError::InvalidSignature(format!("invalid Ed25519 public key: {e}"))
            })?;
            let sig = Signature::from_bytes(&sig_bytes);
            verifying_key
                .verify_strict(&self.canonical_signing_payload(), &sig)
                .map_err(|e| {
                    RiskVerificationError::InvalidSignature(format!(
                        "Ed25519 signature verification failed: {e}"
                    ))
                })?;
        }

        Ok(())
    }

    /// Strictly verifies the proof AND requires a valid Ed25519 signature matching `expected_public_key`.
    pub fn verify_with_public_key(
        &self,
        expected_action_hash: &[u8; 32],
        expected_public_key: &VerifyingKey,
    ) -> Result<(), RiskVerificationError> {
        self.verify(expected_action_hash)?;
        let sig_bytes: [u8; 64] = match self.signature.len() {
            64 => self.signature[0..64].try_into().unwrap(),
            96 => {
                if &self.signature[0..32] != expected_public_key.as_bytes() {
                    return Err(RiskVerificationError::InvalidSignature(
                        "embedded public key does not match pinned verifying key".to_string(),
                    ));
                }
                self.signature[32..96].try_into().unwrap()
            }
            other => {
                return Err(RiskVerificationError::InvalidSignature(format!(
                    "missing or malformed Ed25519 signature ({other} bytes)"
                )));
            }
        };
        let sig = Signature::from_bytes(&sig_bytes);
        expected_public_key
            .verify_strict(&self.canonical_signing_payload(), &sig)
            .map_err(|e| {
                RiskVerificationError::InvalidSignature(format!(
                    "Ed25519 signature verification failed: {e}"
                ))
            })
    }
}
