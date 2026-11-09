//! 4-Tier Risk Governance and ActionCard physical hold verification.

use chrono::{DateTime, Duration, Utc};
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
    /// Optional Ed25519 signature of the confirmation token.
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

    /// Computes deterministic SHA-256 action hash for a tool name and arguments payload.
    pub fn compute_action_hash(tool_name: &str, arguments: &serde_json::Value) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(tool_name.as_bytes());
        hasher.update(b":");
        let canonical_args = serde_json::to_vec(arguments).unwrap_or_default();
        hasher.update(&canonical_args);
        hasher.finalize().into()
    }

    /// Verifies the physical hold duration, validity timeframe, and action hash.
    ///
    /// Rules:
    /// 1. `physical_hold_duration_ms >= 600` ms.
    /// 2. `confirmed_at` is not more than 30 seconds old.
    /// 3. `confirmed_at` is not more than 5 seconds in the future (clock skew tolerance).
    /// 4. `action_hash` matches `expected_action_hash`.
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

        Ok(())
    }
}
