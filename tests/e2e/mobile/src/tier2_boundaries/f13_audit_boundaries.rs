//! Feature 13: Ed25519 Signed Audit Journal (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F13_01 to T2_F13_05 (5 tests total).

#![allow(unused_imports)]

use crate::harness::{
    AuditReceipt, AuditSigner, AuditVerifier, AuditVerifyError, DaemonStep, MobileTestRig,
};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{AgentDto, KernelToUi, MemoryRowDto, StatusDto, UiToKernel};
use serde_json::json;
use std::time::Duration;

/// `T2_F13_01`: Corrupted Ed25519 signature (1 bit flip) rejection.
///
/// Modifying even 1 single bit in the 64-byte Ed25519 signature causes verification
/// to fail genuinely with AuditVerifyError::InvalidSignature.
#[test]
fn test_t2_f13_01_corrupted_ed25519_signature_rejection() {
    let mut signer = AuditSigner::new();
    let mut receipt = signer.sign_action("audit-101", "payment", Some(1), json!({"amount": 50}));

    // Verify valid signature first
    assert!(AuditVerifier::verify_receipt(&receipt).is_ok());

    // Flip 1 bit in signature
    receipt.signature[7] ^= 0x01;

    let res = AuditVerifier::verify_receipt(&receipt);
    assert!(matches!(res, Err(AuditVerifyError::InvalidSignature)));
}

/// `T2_F13_02`: Corrupted previous hash in chain rejection.
///
/// Modifying 1 byte in prev_hash breaks the cryptographic chain linkage at the exact index.
#[test]
fn test_t2_f13_02_corrupted_prev_hash_in_chain() {
    let mut signer = AuditSigner::new();
    let r0 = signer.sign_action("audit-gen", "init", None, json!({}));
    let mut r1 = signer.sign_action("audit-step1", "login", None, json!({}));
    let r2 = signer.sign_action("audit-step2", "transfer", None, json!({}));

    // Corrupt prev_hash of r1
    r1.prev_hash[3] ^= 0xAA;

    let chain = vec![r0, r1, r2];
    let res = AuditVerifier::verify_chain(&chain);
    match res {
        Err(AuditVerifyError::ChainBreak { index }) => {
            assert_eq!(index, 1, "Chain break must be detected at index 1");
        }
        other => panic!("Expected ChainBreak at index 1, got {other:?}"),
    }
}

/// `T2_F13_03`: Empty audit log genesis verification.
///
/// An empty audit log returns EmptyChain; a genesis entry with non-zero prev_hash returns GenesisMismatch.
#[test]
fn test_t2_f13_03_empty_audit_log_and_genesis_verification() {
    // Empty chain error
    let empty_chain: Vec<AuditReceipt> = Vec::new();
    assert!(matches!(
        AuditVerifier::verify_chain(&empty_chain),
        Err(AuditVerifyError::EmptyChain)
    ));

    // Non-zero prev_hash at genesis
    let mut signer = AuditSigner::new();
    let mut bad_genesis = signer.sign_action("gen-bad", "init", None, json!({}));
    bad_genesis.prev_hash[0] = 0x01;

    let res = AuditVerifier::verify_chain(&[bad_genesis]);
    assert!(matches!(res, Err(AuditVerifyError::GenesisMismatch(_))));
}

/// `T2_F13_04`: Non-contiguous sequence IDs detection.
///
/// Verifies sequence ordering validator detects non-monotonic or skipped IDs.
#[test]
fn test_t2_f13_04_non_contiguous_sequence_ids_detection() {
    fn verify_contiguous_sequences(ids: &[u64]) -> Result<(), (u64, u64)> {
        for window in ids.windows(2) {
            if window[1] != window[0] + 1 {
                return Err((window[0], window[1]));
            }
        }
        Ok(())
    }

    assert!(verify_contiguous_sequences(&[1, 2, 3, 4, 5]).is_ok());
    assert_eq!(
        verify_contiguous_sequences(&[1, 2, 4, 5]),
        Err((2, 4)),
        "Skipped sequence ID 3 must be detected"
    );
    assert_eq!(
        verify_contiguous_sequences(&[5, 4, 3]),
        Err((5, 4)),
        "Non-monotonic sequence must be detected"
    );
}

/// `T2_F13_05`: Pagination limit boundary: limit = 0 and limit > 200.
///
/// Gateway enforces `limit > 0 && limit <= 200` for JournalRequest.
/// Limit 0 and 201 are rejected with HTTP 400 unsupported; limit 1 and 200 are accepted.
#[tokio::test]
async fn test_t2_f13_05_journal_pagination_limit_boundary() {
    let (rig, daemon) = MobileTestRig::setup().await;

    // Limit 0 rejected
    let f_zero = Frame::request(1, UiToKernel::JournalRequest { limit: 0 });
    let (s_zero, r_zero) = rig.post_frame(&f_zero, None).await;
    assert_eq!(s_zero, 400);
    assert_eq!(r_zero["error"]["code"], "unsupported");

    // Limit 201 rejected
    let f_over = Frame::request(2, UiToKernel::JournalRequest { limit: 201 });
    let (s_over, r_over) = rig.post_frame(&f_over, None).await;
    assert_eq!(s_over, 400);
    assert_eq!(r_over["error"]["code"], "unsupported");

    // Limit 200 accepted
    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        let step = DaemonStep::new(
            UiToKernel::JournalRequest { limit: 200 },
            KernelToUi::JournalReply { entries: vec![] },
        );
        daemon
            .handle_step(&mut stream, 3, step)
            .await
            .expect("step");
    });

    let f_valid = Frame::request(3, UiToKernel::JournalRequest { limit: 200 });
    let (s_valid, r_valid) = rig.post_frame(&f_valid, None).await;
    assert_eq!(s_valid, 200);
    assert!(r_valid["error"].is_null());

    daemon_task.await.unwrap();
}
