//! Feature 13: Ed25519 Signed Audit Journal (Milestone M1 / Spec §2, §22)
//!
//! Specifications: T1_F13_01 to T1_F13_05
//!
//! Verifies:
//! - Genuine Ed25519 signature generation and verification over canonical audit receipts.
//! - Hash chain continuity and genesis block all-zeros verification.
//! - Cryptographic rejection of tampered signature bits (Zero Simulated Success).
//! - Linear chain break detection upon modified prev_hash.
//! - CardDecisionAck correlation with signed receipt audit ID and card ID.

use crate::harness::{AuditSigner, AuditVerifier, AuditVerifyError};
use serde_json::json;

/// `T1_F13_01`: Ed25519 receipt signature generation and verification.
///
/// Every action logged in the audit journal is cryptographically signed using
/// an Ed25519 private key and verified using the corresponding public key.
#[test]
fn test_t1_f13_01_ed25519_receipt_signature_generation_and_verification() {
    let mut signer = AuditSigner::new();

    let receipt = signer.sign_action(
        "audit-record-001",
        "card_approval",
        Some(101),
        json!({"action": "payment", "amount_cents": 5000, "currency": "EUR"}),
    );

    assert_eq!(receipt.audit_id, "audit-record-001");
    assert_eq!(receipt.action, "card_approval");
    assert_eq!(receipt.card_id, Some(101));
    assert_eq!(receipt.signature.len(), 64, "Ed25519 signature is 64 bytes");
    assert_eq!(
        receipt.public_key.len(),
        32,
        "Ed25519 public key is 32 bytes"
    );

    // Must verify successfully with genuine Ed25519 cryptography
    AuditVerifier::verify_receipt(&receipt)
        .expect("Canonical Ed25519 signature verification must succeed");
}

/// `T1_F13_02`: Hash chain continuity and genesis verification.
///
/// Successive receipts bind their prev_hash to the preceding entry_hash.
/// The genesis receipt has an all-zero prev_hash.
#[test]
fn test_t1_f13_02_hash_chain_linkage_verification() {
    let mut signer = AuditSigner::new();

    let r0 = signer.sign_action("id-genesis", "system_init", None, json!({}));
    let r1 = signer.sign_action("id-action-1", "user_login", None, json!({}));
    let r2 = signer.sign_action(
        "id-action-2",
        "settings_change",
        None,
        json!({"sound": false}),
    );

    // Genesis check
    assert_eq!(
        r0.prev_hash, [0u8; 32],
        "Genesis prev_hash must be all zeros"
    );
    // Linkage checks
    assert_eq!(r1.prev_hash, r0.entry_hash, "r1 must link to r0");
    assert_eq!(r2.prev_hash, r1.entry_hash, "r2 must link to r1");

    let chain = vec![r0, r1, r2];
    AuditVerifier::verify_chain(&chain).expect("Complete uncorrupted hash chain must verify");
}

/// `T1_F13_03`: Tampered signature cryptographic rejection.
///
/// Modifying even a single bit in the signature causes verify_receipt to genuinely fail.
#[test]
fn test_t1_f13_03_tampered_signature_cryptographic_rejection() {
    let mut signer = AuditSigner::new();
    let mut receipt = signer.sign_action("id-secure", "transfer", Some(42), json!({}));

    // Tamper single bit
    receipt.signature[0] ^= 0x01;

    let res = AuditVerifier::verify_receipt(&receipt);
    assert!(
        matches!(res, Err(AuditVerifyError::InvalidSignature)),
        "Tampered signature must be rejected: {res:?}"
    );
}

/// `T1_F13_04`: Linear chain break detection upon modified prev_hash.
///
/// Modifying prev_hash breaks the cryptographic chain linkage at the exact index.
#[test]
fn test_t1_f13_04_broken_hash_chain_rejection() {
    let mut signer = AuditSigner::new();
    let r0 = signer.sign_action("id-0", "start", None, json!({}));
    let mut r1 = signer.sign_action("id-1", "step", None, json!({}));
    let r2 = signer.sign_action("id-2", "finish", None, json!({}));

    // Tamper r1 prev_hash
    r1.prev_hash[5] ^= 0xFF;

    let chain = vec![r0, r1, r2];
    let res = AuditVerifier::verify_chain(&chain);

    match res {
        Err(AuditVerifyError::ChainBreak { index }) => {
            assert_eq!(index, 1, "Chain break must be pinpointed to index 1");
        }
        other => panic!("Expected ChainBreak at index 1, got {other:?}"),
    }
}

/// `T1_F13_05`: CardDecisionAck correlation with signed receipt.
///
/// Verifies that CardDecisionAck audit_id and card_id correspond strictly
/// to the cryptographically verified receipt.
#[test]
fn test_t1_f13_05_card_decision_ack_audit_id_correlation() {
    let mut signer = AuditSigner::new();
    let receipt = signer.sign_action(
        "audit-uuid-valid",
        "card_approve",
        Some(101),
        json!({"verb": "Payer"}),
    );

    // 1. Valid correlation
    AuditVerifier::verify_card_ack("audit-uuid-valid", 101, &receipt)
        .expect("Valid correlation must verify");

    // 2. Mismatched audit ID
    let bad_audit_res = AuditVerifier::verify_card_ack("audit-uuid-wrong", 101, &receipt);
    assert!(
        matches!(bad_audit_res, Err(AuditVerifyError::AuditIdMismatch { .. })),
        "Mismatched audit ID must fail"
    );

    // 3. Mismatched card ID
    let bad_card_res = AuditVerifier::verify_card_ack("audit-uuid-valid", 999, &receipt);
    assert!(
        matches!(bad_card_res, Err(AuditVerifyError::CardIdMismatch { .. })),
        "Mismatched card ID must fail"
    );
}
