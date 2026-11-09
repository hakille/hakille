//! Smoke tests verifying that the E2E mobile harness compiles, spawns,
//! and communicates over real loopback IPC and gateway HTTP transports.

use hakille_e2e_mobile::harness::{
    AuditSigner, AuditVerifier, CheckpointHarness, DaemonStep, MobileTestRig,
};
use hakille_mobile::MobileBridge;
use serde_json::json;

#[tokio::test]
async fn test_harness_spawn_and_health_check() {
    let (rig, _daemon) = MobileTestRig::setup().await;

    let response = rig.health().send().await.expect("health check request");
    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("parse health JSON");
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn test_harness_bridge_to_gateway_to_mock_daemon_roundtrip() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept gateway IPC stream");
        daemon
            .handle_step(&mut stream, 1, DaemonStep::status())
            .await
            .expect("execute status step");
    });

    let (status_code, result) = rig
        .roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;

    assert_eq!(status_code, 200);
    assert_eq!(result["ok"], true, "roundtrip result: {result}");
    assert!(result["snapshot"]["pending"].is_null());

    let outcomes = result["snapshot"]["outcomes"]
        .as_array()
        .expect("outcomes array");
    assert_eq!(outcomes.last().unwrap()["state"], "confirmed");

    let snapshot = rig.snapshot(&bridge);
    assert_eq!(snapshot["status"]["model"], "mock-daemon-qwen");

    daemon_task.await.expect("daemon task completion");
}

#[tokio::test]
async fn test_harness_fsync_checkpoint_persistence_and_recovery() {
    let harness = CheckpointHarness::new().expect("create checkpoint harness");
    let bridge1 = MobileBridge::new();

    let test_draft = "Opaque-box mobile checkpoint v2 smoke draft";
    let set_draft_res = bridge1.dispatch_json(
        json!({
            "type": "set_draft",
            "text": test_draft,
        })
        .to_string(),
    );
    assert!(set_draft_res.contains("\"ok\":true"));

    let saved_json = harness
        .save_draft_from_bridge(&bridge1)
        .expect("save draft to disk");

    let validated =
        CheckpointHarness::validate_checkpoint_schema(&saved_json).expect("schema v2 validation");
    assert_eq!(validated["schema_version"], 2);
    assert_eq!(validated["draft"], test_draft);

    // Drop bridge1 to simulate crash
    drop(bridge1);

    // Fresh bridge instance
    let bridge2 = MobileBridge::new();
    let restore_res = harness
        .restore_into_bridge(&bridge2)
        .expect("restore draft into fresh bridge");
    assert_eq!(restore_res["ok"], true, "restore result: {restore_res}");

    let snapshot: serde_json::Value =
        serde_json::from_str(&bridge2.snapshot_json()).expect("parse bridge2 snapshot");
    assert_eq!(snapshot["draft"], test_draft);
}

#[tokio::test]
async fn test_harness_crypto_audit_receipt_chain_verification() {
    let mut signer = AuditSigner::new();

    let receipt1 = signer.sign_action(
        "audit-uuid-1",
        "card_approval",
        Some(101),
        json!({"amount": 500, "currency": "EUR"}),
    );
    let receipt2 = signer.sign_action(
        "audit-uuid-2",
        "card_rejection",
        Some(102),
        json!({"reason": "user_cancelled"}),
    );

    // Verify individual receipts
    AuditVerifier::verify_receipt(&receipt1).expect("receipt 1 must verify");
    AuditVerifier::verify_receipt(&receipt2).expect("receipt 2 must verify");

    // Verify chain linkage
    let chain = vec![receipt1.clone(), receipt2.clone()];
    AuditVerifier::verify_chain(&chain).expect("audit chain must verify");

    // Verify card decision ack correlation
    AuditVerifier::verify_card_ack("audit-uuid-1", 101, &receipt1)
        .expect("card ack 1 must correlate");
    AuditVerifier::verify_card_ack("audit-uuid-2", 102, &receipt2)
        .expect("card ack 2 must correlate");

    // Zero Simulated Success: Negative verification tests
    // 1. Tampered signature must fail verification
    let mut tampered_sig = receipt1.clone();
    tampered_sig.signature[0] ^= 0xFF;
    assert!(
        AuditVerifier::verify_receipt(&tampered_sig).is_err(),
        "tampered signature must genuinely fail cryptographic verification"
    );

    // 2. Tampered prev_hash must break chain
    let mut broken_chain = chain.clone();
    broken_chain[1].prev_hash[0] ^= 0x01;
    assert!(
        AuditVerifier::verify_chain(&broken_chain).is_err(),
        "broken prev_hash linkage must genuinely fail chain verification"
    );

    // 3. Mismatched card ID must fail
    assert!(
        AuditVerifier::verify_card_ack("audit-uuid-1", 999, &receipt1).is_err(),
        "mismatched card id must genuinely fail card ack verification"
    );
}
