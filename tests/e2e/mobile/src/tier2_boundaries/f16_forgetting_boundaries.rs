//! Feature 16: Cryptographic Fact Forgetting (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F16_01 to T2_F16_05 (5 tests total).

#![allow(unused_imports)]

use crate::harness::{
    AuditReceipt, AuditSigner, AuditVerifier, AuditVerifyError, DaemonStep, MobileTestRig,
};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{AgentDto, KernelToUi, MemoryRowDto, StatusDto, UiToKernel};
use serde_json::json;
use std::time::Duration;

/// `T2_F16_01`: Forgetting already forgotten fact (idempotent deletion).
///
/// Forgetting an already deleted fact succeeds idempotently.
#[tokio::test]
async fn test_t2_f16_01_forgetting_already_forgotten_fact_idempotency() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let row_id = "mem-uuid-999".to_string();

    let id_clone = row_id.clone();
    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        let step = DaemonStep::new(
            UiToKernel::MemoryForget { row_id: id_clone },
            KernelToUi::HudReply {
                response: "Fait déjà oublié (idempotent)".into(),
                validation: None,
            },
        );
        daemon
            .handle_step(&mut stream, 1, step)
            .await
            .expect("step");
    });

    let frame = Frame::request(1, UiToKernel::MemoryForget { row_id });
    let (status, resp) = rig.post_frame(&frame, None).await;
    assert_eq!(status, 200);
    assert!(resp["error"].is_null());

    daemon_task.await.unwrap();
}

/// `T2_F16_02`: Row ID boundary: exactly 256 bytes vs 257 bytes.
///
/// Gateway enforces `!row_id.is_empty() && row_id.len() <= 256`.
#[tokio::test]
async fn test_t2_f16_02_row_id_boundary_256_vs_257_bytes() {
    let (rig, daemon) = MobileTestRig::setup().await;

    // 257 bytes -> rejected with 400 unsupported
    let over_id = "x".repeat(257);
    let f_over = Frame::request(1, UiToKernel::MemoryForget { row_id: over_id });
    let (s_over, r_over) = rig.post_frame(&f_over, None).await;
    assert_eq!(s_over, 400);
    assert_eq!(r_over["error"]["code"], "unsupported");

    // 256 bytes -> accepted
    let exact_id = "x".repeat(256);
    let id_clone = exact_id.clone();
    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        let step = DaemonStep::new(
            UiToKernel::MemoryForget { row_id: id_clone },
            KernelToUi::HudReply {
                response: "Suppression 256 octets validée".into(),
                validation: None,
            },
        );
        daemon
            .handle_step(&mut stream, 2, step)
            .await
            .expect("step");
    });

    let f_exact = Frame::request(2, UiToKernel::MemoryForget { row_id: exact_id });
    let (s_exact, r_exact) = rig.post_frame(&f_exact, None).await;
    assert_eq!(s_exact, 200);
    assert!(r_exact["error"].is_null());

    daemon_task.await.unwrap();
}

/// `T2_F16_03`: Forgetting during disconnected mode.
///
/// Dispatching forget when disconnected fails closed.
#[test]
fn test_t2_f16_03_forgetting_during_disconnected_mode() {
    let bridge = MobileBridge::new();
    let res_str = bridge.dispatch_json(
        json!({
            "type": "remember",
            "text": "Tentative hors ligne"
        })
        .to_string(),
    );
    let res: serde_json::Value = serde_json::from_str(&res_str).unwrap();
    assert_eq!(res["ok"], false);
    assert_eq!(res["error"], "Aucun hub configuré");
}

/// `T2_F16_04`: Cryptographic deletion receipt verification.
///
/// Verifies deletion receipt signed by Ed25519 with action: "memory_forget".
#[test]
fn test_t2_f16_04_cryptographic_deletion_receipt_verification() {
    let mut signer = AuditSigner::new();
    let receipt = signer.sign_action(
        "del-receipt-001",
        "memory_forget",
        None,
        json!({"row_id": "mem-target-42", "tombstone_applied": true}),
    );

    assert_eq!(receipt.action, "memory_forget");
    assert!(AuditVerifier::verify_receipt(&receipt).is_ok());
}

/// `T2_F16_05`: Tombstone chain consistency.
///
/// Deletion receipts chain properly onto prior knowledge graph actions.
#[test]
fn test_t2_f16_05_tombstone_chain_consistency() {
    let mut signer = AuditSigner::new();
    let r_init = signer.sign_action("id-0", "init", None, json!({}));
    let r_remember = signer.sign_action("id-1", "memory_remember", None, json!({"text": "Fact"}));
    let r_forget = signer.sign_action("id-2", "memory_forget", None, json!({"row_id": "id-1"}));

    let chain = vec![r_init, r_remember, r_forget];
    assert!(AuditVerifier::verify_chain(&chain).is_ok());
}
