//! Ed25519 signature and SHA-256 hash verifier for Mobile audit receipts.
//!
//! Enforces Zero Simulated Success: no dummy audit IDs ("ok", "fake_123").
//! Every audit receipt is cryptographically verified against an Ed25519 public key
//! and checked along its hash chain.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum AuditVerifyError {
    #[error("Invalid Ed25519 public key bytes: {0}")]
    InvalidPublicKey(String),
    #[error("Invalid Ed25519 signature: verification failed")]
    InvalidSignature,
    #[error("Entry hash mismatch: expected {expected}, computed {computed}")]
    HashMismatch { expected: String, computed: String },
    #[error("Chain break at index {index}: prev_hash does not match previous entry_hash")]
    ChainBreak { index: usize },
    #[error("Genesis prev_hash mismatch: expected all-zeros, got {0}")]
    GenesisMismatch(String),
    #[error("Audit ID mismatch: expected {expected}, got {actual}")]
    AuditIdMismatch { expected: String, actual: String },
    #[error("Card ID mismatch: expected {expected:?}, got {actual:?}")]
    CardIdMismatch {
        expected: Option<u64>,
        actual: Option<u64>,
    },
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Empty audit chain")]
    EmptyChain,
}

/// A cryptographically signed audit receipt verifying an action or decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditReceipt {
    pub audit_id: String,
    pub timestamp: String,
    pub action: String,
    pub card_id: Option<u64>,
    pub parameters: serde_json::Value,
    pub prev_hash: [u8; 32],
    pub entry_hash: [u8; 32],
    pub signature: Vec<u8>,
    pub public_key: [u8; 32],
}

#[derive(Serialize)]
struct CanonicalUnsigned<'a> {
    audit_id: &'a str,
    timestamp: &'a str,
    action: &'a str,
    card_id: Option<u64>,
    parameters: &'a serde_json::Value,
    prev_hash: &'a [u8; 32],
}

#[derive(Serialize)]
struct CanonicalSigned<'a> {
    audit_id: &'a str,
    timestamp: &'a str,
    action: &'a str,
    card_id: Option<u64>,
    parameters: &'a serde_json::Value,
    prev_hash: &'a [u8; 32],
    signature: &'a [u8],
}

fn canonical_unsigned_bytes(
    audit_id: &str,
    timestamp: &str,
    action: &str,
    card_id: Option<u64>,
    parameters: &serde_json::Value,
    prev_hash: &[u8; 32],
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&CanonicalUnsigned {
        audit_id,
        timestamp,
        action,
        card_id,
        parameters,
        prev_hash,
    })
}

fn compute_entry_hash(
    audit_id: &str,
    timestamp: &str,
    action: &str,
    card_id: Option<u64>,
    parameters: &serde_json::Value,
    prev_hash: &[u8; 32],
    signature: &[u8],
) -> Result<[u8; 32], serde_json::Error> {
    let bytes = serde_json::to_vec(&CanonicalSigned {
        audit_id,
        timestamp,
        action,
        card_id,
        parameters,
        prev_hash,
        signature,
    })?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hasher.finalize().into())
}

/// Helper to generate chained cryptographic audit receipts in tests.
pub struct AuditSigner {
    signing_key: SigningKey,
    last_hash: [u8; 32],
}

impl Default for AuditSigner {
    fn default() -> Self {
        Self::new()
    }
}

impl AuditSigner {
    pub fn new() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self {
            signing_key,
            last_hash: [0u8; 32],
        }
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.verifying_key().to_bytes()
    }

    pub fn sign_action(
        &mut self,
        audit_id: impl Into<String>,
        action: impl Into<String>,
        card_id: Option<u64>,
        parameters: serde_json::Value,
    ) -> AuditReceipt {
        let audit_id = audit_id.into();
        let action = action.into();
        let timestamp = chrono::Utc::now().to_rfc3339();
        let prev_hash = self.last_hash;

        let unsigned = canonical_unsigned_bytes(
            &audit_id,
            &timestamp,
            &action,
            card_id,
            &parameters,
            &prev_hash,
        )
        .expect("canonical serialization");

        let signature = self.signing_key.sign(&unsigned).to_bytes().to_vec();
        let entry_hash = compute_entry_hash(
            &audit_id,
            &timestamp,
            &action,
            card_id,
            &parameters,
            &prev_hash,
            &signature,
        )
        .expect("canonical hash calculation");

        self.last_hash = entry_hash;

        AuditReceipt {
            audit_id,
            timestamp,
            action,
            card_id,
            parameters,
            prev_hash,
            entry_hash,
            signature,
            public_key: self.public_key_bytes(),
        }
    }
}

/// Standalone cryptographic verifier for receipts and chains.
pub struct AuditVerifier;

impl AuditVerifier {
    /// Cryptographically verifies an individual audit receipt.
    pub fn verify_receipt(receipt: &AuditReceipt) -> Result<(), AuditVerifyError> {
        let verifying_key = VerifyingKey::from_bytes(&receipt.public_key)
            .map_err(|e| AuditVerifyError::InvalidPublicKey(e.to_string()))?;

        let unsigned = canonical_unsigned_bytes(
            &receipt.audit_id,
            &receipt.timestamp,
            &receipt.action,
            receipt.card_id,
            &receipt.parameters,
            &receipt.prev_hash,
        )?;

        let sig = Signature::from_slice(&receipt.signature)
            .map_err(|_| AuditVerifyError::InvalidSignature)?;

        verifying_key
            .verify(&unsigned, &sig)
            .map_err(|_| AuditVerifyError::InvalidSignature)?;

        let computed_hash = compute_entry_hash(
            &receipt.audit_id,
            &receipt.timestamp,
            &receipt.action,
            receipt.card_id,
            &receipt.parameters,
            &receipt.prev_hash,
            &receipt.signature,
        )?;

        if computed_hash != receipt.entry_hash {
            return Err(AuditVerifyError::HashMismatch {
                expected: hex::encode(receipt.entry_hash),
                computed: hex::encode(computed_hash),
            });
        }

        Ok(())
    }

    /// Verifies a complete linear chain of audit receipts.
    pub fn verify_chain(receipts: &[AuditReceipt]) -> Result<(), AuditVerifyError> {
        if receipts.is_empty() {
            return Err(AuditVerifyError::EmptyChain);
        }

        // Genesis check
        if receipts[0].prev_hash != [0u8; 32] {
            return Err(AuditVerifyError::GenesisMismatch(hex::encode(
                receipts[0].prev_hash,
            )));
        }

        // First receipt
        Self::verify_receipt(&receipts[0])?;

        // Chain checks
        for (i, pair) in receipts.windows(2).enumerate() {
            let prev = &pair[0];
            let curr = &pair[1];

            if curr.prev_hash != prev.entry_hash {
                return Err(AuditVerifyError::ChainBreak { index: i + 1 });
            }

            Self::verify_receipt(curr)?;
        }

        Ok(())
    }

    /// Verifies that a CardDecisionAck contains a valid cryptographically-signed audit receipt.
    pub fn verify_card_ack(
        ack_audit_id: &str,
        ack_card_id: u64,
        receipt: &AuditReceipt,
    ) -> Result<(), AuditVerifyError> {
        Self::verify_receipt(receipt)?;

        if receipt.audit_id != ack_audit_id {
            return Err(AuditVerifyError::AuditIdMismatch {
                expected: ack_audit_id.to_string(),
                actual: receipt.audit_id.clone(),
            });
        }

        if receipt.card_id != Some(ack_card_id) {
            return Err(AuditVerifyError::CardIdMismatch {
                expected: Some(ack_card_id),
                actual: receipt.card_id,
            });
        }

        Ok(())
    }
}

pub mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        let bytes = bytes.as_ref();
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            use std::fmt::Write;
            let _ = write!(s, "{:02x}", b);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_chain_fails() {
        let empty: Vec<AuditReceipt> = vec![];
        let res = AuditVerifier::verify_chain(&empty);
        assert!(matches!(res, Err(AuditVerifyError::EmptyChain)));
    }

    #[test]
    fn test_non_genesis_fails() {
        let mut signer = AuditSigner::new();
        let mut receipt = signer.sign_action("id-1", "action", None, serde_json::json!({}));
        receipt.prev_hash = [1u8; 32];
        let res = AuditVerifier::verify_chain(&[receipt]);
        assert!(matches!(res, Err(AuditVerifyError::GenesisMismatch(_))));
    }

    #[test]
    fn test_invalid_public_key_or_signature_fails() {
        let mut signer = AuditSigner::new();
        let mut receipt = signer.sign_action("id-1", "action", None, serde_json::json!({}));
        receipt.signature[0] ^= 0xFF;
        let res = AuditVerifier::verify_receipt(&receipt);
        assert!(matches!(res, Err(AuditVerifyError::InvalidSignature)));
    }

    #[test]
    fn test_tampered_hash_fails() {
        let mut signer = AuditSigner::new();
        let mut receipt = signer.sign_action("id-1", "action", None, serde_json::json!({}));
        receipt.entry_hash[0] ^= 0xFF;
        let res = AuditVerifier::verify_receipt(&receipt);
        assert!(matches!(res, Err(AuditVerifyError::HashMismatch { .. })));
    }
}
