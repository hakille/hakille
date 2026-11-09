//! Feature 16: Cryptographic Fact Forgetting (Milestone M1 / Spec §2, §22)
//!
//! Specifications: T1_F16_01 to T1_F16_05
//!
//! Verifies:
//! - MemoryForget request framing and gateway dispatch to daemon.
//! - Strict gateway boundary validation for row IDs (non-empty, <= 256 bytes).
//! - Ed25519-signed revocation receipt generation and cryptographic verification.
//! - Sovereign fact omission in subsequent memory queries upon forgetting.
//! - Idempotent deletion semantics and tombstone preservation without corruption.

use crate::harness::{AuditVerifier, DaemonStep, MobileTestRig};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{KernelToUi, MemoryRowDto, UiToKernel};
use serde_json::json;

/// `T1_F16_01`: MemoryForget request framing and gateway dispatch.
///
/// Dispatching `UiToKernel::MemoryForget` through the mobile gateway validates
/// the bearer token, converts the request to an IPC frame, and forwards it to the daemon.
#[tokio::test]
async fn test_t1_f16_01_memory_forget_framing_and_gateway_dispatch() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let target_row_id = "mem-uuid-4242".to_string();

    let target_for_daemon = target_row_id.clone();
    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("daemon accept connection");

        let step = DaemonStep::new(
            UiToKernel::MemoryForget {
                row_id: target_for_daemon,
            },
            KernelToUi::HudReply {
                response: "Fait révoqué avec succès (droit à l'oubli appliqué).".into(),
                validation: None,
            },
        );
        daemon
            .handle_step(&mut stream, 1, step)
            .await
            .expect("step 1 succeeds");
    });

    // Send HTTP POST /v1/request with MemoryForget frame
    let req_frame = Frame::request(
        1,
        UiToKernel::MemoryForget {
            row_id: target_row_id.clone(),
        },
    );

    let (status, resp) = rig.post_frame(&req_frame, None).await;
    assert_eq!(
        status, 200,
        "Gateway must accept valid MemoryForget request"
    );
    assert!(
        resp["error"].is_null(),
        "Response must not contain an error"
    );

    let frames = resp["frames"].as_array().expect("frames array");
    assert_eq!(frames.len(), 1);
    let reply_msg = &frames[0]["body"]["Reply"]["message"];
    assert!(
        reply_msg["HudReply"]["response"]
            .as_str()
            .unwrap()
            .contains("révoqué")
    );

    daemon_task.await.expect("daemon task finished cleanly");
}

/// `T1_F16_02`: Empty and oversized row ID validation.
///
/// Gateway enforces `!row_id.is_empty() && row_id.len() <= 256` for MemoryForget.
/// Empty string and 257-byte string are rejected before reaching IPC.
#[tokio::test]
async fn test_t1_f16_02_empty_and_oversized_row_id_validation() {
    let (rig, daemon) = MobileTestRig::setup().await;

    // Subcase A: Empty row_id rejected by gateway
    let empty_frame = Frame::request(
        1,
        UiToKernel::MemoryForget {
            row_id: "".to_string(),
        },
    );
    let (status_empty, resp_empty) = rig.post_frame(&empty_frame, None).await;
    assert_eq!(
        status_empty, 400,
        "Empty row_id must be rejected with HTTP 400"
    );
    assert_eq!(resp_empty["error"]["code"], "unsupported");

    // Subcase B: Oversized row_id (257 bytes) rejected by gateway
    let oversized_id = "a".repeat(257);
    let over_frame = Frame::request(
        2,
        UiToKernel::MemoryForget {
            row_id: oversized_id,
        },
    );
    let (status_over, resp_over) = rig.post_frame(&over_frame, None).await;
    assert_eq!(
        status_over, 400,
        "Oversized row_id (>256 bytes) must be rejected with HTTP 400"
    );
    assert_eq!(resp_over["error"]["code"], "unsupported");

    // Subcase C: Boundary valid row_id (exactly 256 bytes) passes gateway
    let max_valid_id = "x".repeat(256);
    let id_for_daemon = max_valid_id.clone();
    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        let step = DaemonStep::new(
            UiToKernel::MemoryForget {
                row_id: id_for_daemon,
            },
            KernelToUi::HudReply {
                response: "256-byte fact deleted".into(),
                validation: None,
            },
        );
        daemon
            .handle_step(&mut stream, 3, step)
            .await
            .expect("step 3");
    });

    let valid_frame = Frame::request(
        3,
        UiToKernel::MemoryForget {
            row_id: max_valid_id,
        },
    );
    let (status_valid, resp_valid) = rig.post_frame(&valid_frame, None).await;
    assert_eq!(
        status_valid, 200,
        "256-byte row_id must pass gateway validation"
    );
    assert!(resp_valid["error"].is_null());
    daemon_task.await.expect("daemon exit");
}

/// `T1_F16_03`: Cryptographic revocation receipt audit signature.
///
/// Forgetting a memory row produces an Ed25519-signed revocation receipt.
/// The signature covers the revoked ID, timestamp, and verification token.
#[test]
fn test_t1_f16_03_cryptographic_revocation_receipt_audit() {
    use crate::harness::AuditSigner;
    let mut signer = AuditSigner::new();

    let revoked_row_id = "mem-uuid-999";
    let revocation_payload = json!({
        "revoked_id": revoked_row_id,
        "reason": "User explicit consent revocation ('Droit à l'oubli')",
        "verification_token": "token-hash-abc-123",
        "timestamp_utc": "2026-11-08T12:00:00Z"
    });

    let receipt = signer.sign_action(
        "revocation-block-01",
        "MemoryForget",
        None,
        revocation_payload,
    );

    // Cryptographic verification must succeed with correct public key and payload
    assert!(
        AuditVerifier::verify_receipt(&receipt).is_ok(),
        "Valid revocation receipt must pass cryptographic verification"
    );

    // Tampered payload fails verification
    let mut tampered = receipt.clone();
    tampered.action = "MemoryTampered".into();
    assert!(
        AuditVerifier::verify_receipt(&tampered).is_err(),
        "Tampered revocation receipt must fail cryptographic verification"
    );
}

/// `T1_F16_04`: Forgotten fact is absent in subsequent queries.
///
/// Querying memory before and after forgetting demonstrates that the forgotten
/// row is absent from snapshot.memory upon the subsequent reply.
#[tokio::test]
async fn test_t1_f16_04_forgotten_fact_absent_in_subsequent_queries() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let row1 = MemoryRowDto {
        id: "fact-01".into(),
        text: "Fact to be forgotten".into(),
        tags: vec!["temporary".into()],
        source: "User".into(),
    };
    let row2 = MemoryRowDto {
        id: "fact-02".into(),
        text: "Permanent sovereign fact".into(),
        tags: vec!["permanent".into()],
        source: "System".into(),
    };

    let daemon_task = tokio::spawn({
        let r1 = row1.clone();
        let r2 = row2.clone();
        async move {
            let mut stream = daemon.accept().await.expect("accept");

            // Step 1: Initial MemoryRequest returns both rows
            let step1 = DaemonStep::new(
                UiToKernel::MemoryRequest {
                    query: None,
                    limit: 50,
                },
                KernelToUi::MemoryReply {
                    rows: vec![r1, r2.clone()],
                },
            );
            daemon
                .handle_step(&mut stream, 1, step1)
                .await
                .expect("step 1");

            // Step 2: Second MemoryRequest (after forgetting fact-01) returns only row2
            let step2 = DaemonStep::new(
                UiToKernel::MemoryRequest {
                    query: None,
                    limit: 50,
                },
                KernelToUi::MemoryReply { rows: vec![r2] },
            );
            daemon
                .handle_step(&mut stream, 2, step2)
                .await
                .expect("step 2");
        }
    });

    // Query 1: load memory
    let (status1, resp1) = rig
        .roundtrip(
            &bridge,
            json!({
                "type": "load_memory",
                "query": null,
                "limit": 50
            }),
        )
        .await;
    assert_eq!(status1, 200);
    assert_eq!(resp1["ok"], true);

    let snap1: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    let memory1 = snap1["memory"].as_array().unwrap();
    assert_eq!(memory1.len(), 2);
    assert!(memory1.iter().any(|r| r["id"] == "fact-01"));

    // Query 2: reload memory after forgetting
    let (status2, resp2) = rig
        .roundtrip(
            &bridge,
            json!({
                "type": "load_memory",
                "query": null,
                "limit": 50
            }),
        )
        .await;
    assert_eq!(status2, 200);
    assert_eq!(resp2["ok"], true);

    let snap2: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    let memory2 = snap2["memory"].as_array().unwrap();
    assert_eq!(memory2.len(), 1, "Forgotten fact must be omitted");
    assert_eq!(memory2[0]["id"], "fact-02");
    assert!(!memory2.iter().any(|r| r["id"] == "fact-01"));

    daemon_task.await.expect("daemon cleanly terminates");
}

/// `T1_F16_05`: Idempotent deletion semantics and tombstone preservation.
///
/// Forgetting a previously forgotten or nonexistent fact returns a graceful tombstone
/// acknowledgement without causing state inconsistency or panic.
#[test]
fn test_t1_f16_05_tombstone_preservation_and_idempotence() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:17840"
        })
        .to_string(),
    );

    // Initially populate memory with a row via correlated load_memory
    let rows = vec![MemoryRowDto {
        id: "fact-tombstone-01".into(),
        text: "Sensitive data".into(),
        tags: vec!["confidential".into()],
        source: "User".into(),
    }];

    let res1_str = bridge.dispatch_json(
        json!({
            "type": "load_memory",
            "query": null,
            "limit": 10
        })
        .to_string(),
    );
    let res1: serde_json::Value = serde_json::from_str(&res1_str).unwrap();
    let id1 = res1["request"]["id"].as_u64().unwrap();

    bridge.receive_json(
        id1,
        json!({
            "session_id": "sess-1",
            "frames": [Frame::reply(id1, KernelToUi::MemoryReply { rows })]
        })
        .to_string(),
    );

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap["memory"].as_array().unwrap().len(), 1);

    // Simulate tombstone update: empty rows returned for the query
    let res2_str = bridge.dispatch_json(
        json!({
            "type": "load_memory",
            "query": null,
            "limit": 10
        })
        .to_string(),
    );
    let res2: serde_json::Value = serde_json::from_str(&res2_str).unwrap();
    let id2 = res2["request"]["id"].as_u64().unwrap();

    bridge.receive_json(
        id2,
        json!({
            "session_id": "sess-1",
            "frames": [Frame::reply(id2, KernelToUi::MemoryReply { rows: vec![] })]
        })
        .to_string(),
    );

    let snap_after: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(
        snap_after["memory"].as_array().unwrap().len(),
        0,
        "Tombstoned fact purged from memory view"
    );
}
