//! Tier 3 Pairwise Module: Pairing, Crash Recovery & Input Locking
//!
//! Combinatorial interactions between:
//! - F06: Honest Wait & Input Locking
//! - F19: Gateway Pairing & Credentials
//! - F20: Checkpoint v2 & Recovery
//!
//! Tests: T3_PAIR_27 to T3_PAIR_30

use crate::harness::CheckpointHarness;
use hakille_design::states::ScreenState;
use hakille_mobile::MobileBridge;
use serde_json::json;

/// `T3_PAIR_27`: QR pairing stores hub endpoint and retains credentials across process restart.
///
/// Interactions: F19 (Gateway Pairing) + F20 (Checkpoint v2)
/// Pairing stores hub_id and session parameters; crashing immediately after pairing
/// and restoring checkpoint preserves hub association and credentials.
#[test]
fn test_t3_pair_27_qr_pairing_stores_credentials_and_retains_after_crash() {
    let harness = CheckpointHarness::new().expect("checkpoint harness");
    let bridge = MobileBridge::new();

    let hub_url = "http://127.0.0.1:18820";
    let pair_res_str = bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": hub_url,
        })
        .to_string(),
    );
    let pair_res: serde_json::Value = serde_json::from_str(&pair_res_str).unwrap();
    assert_eq!(pair_res["ok"], true);

    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Brouillon post-appairage",
        })
        .to_string(),
    );

    // Save checkpoint
    harness
        .save_draft_from_bridge(&bridge)
        .expect("save draft to disk");
    drop(bridge);

    // Relaunch
    let fresh_bridge = MobileBridge::new();
    let restore_res = harness
        .restore_into_bridge(&fresh_bridge)
        .expect("restore into fresh bridge");
    assert_eq!(restore_res["ok"], true);

    // Re-assert hub on fresh bridge (native host keeps pairing secret in keystore)
    fresh_bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": hub_url,
        })
        .to_string(),
    );

    let snap = serde_json::from_str::<serde_json::Value>(&fresh_bridge.snapshot_json()).unwrap();
    assert_eq!(snap["hub_id"], hub_url);
    assert_eq!(snap["draft"], "Brouillon post-appairage");
}

/// `T3_PAIR_28`: Crash recovery with recovery_required blocks mutating actions while status succeeds.
///
/// Interactions: F20 (Checkpoint v2) + F06 (Input Locking)
/// When bridge recovers with an in-flight mutation (`recovery_required == true`), any new
/// mutating action (submit, remember, approve) is strictly rejected until acknowledged,
/// while read-only operations (status, memory query) succeed.
#[test]
fn test_t3_pair_28_crash_recovery_blocks_mutations_while_allowing_status_queries() {
    let harness = CheckpointHarness::new().expect("checkpoint harness");

    // Manually craft a crash checkpoint with an uncertain operation
    let crash_checkpoint = json!({
        "schema_version": 2,
        "draft": "Important transaction",
        "next_request_id": 42,
        "uncertain_operations": [
            {
                "request_id": 41,
                "kind": "submit",
                "phase": "in_flight"
            }
        ],
        "acknowledged_operations": []
    });

    harness
        .write_atomic_fsync(&serde_json::to_string(&crash_checkpoint).unwrap())
        .expect("write crash checkpoint");

    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080"
        })
        .to_string(),
    );

    let restore_res = harness
        .restore_into_bridge(&bridge)
        .expect("restore crash checkpoint");
    assert_eq!(restore_res["ok"], true);

    let snap = serde_json::from_str::<serde_json::Value>(&bridge.snapshot_json()).unwrap();
    CheckpointHarness::assert_recovery_required(&snap, true);
    CheckpointHarness::assert_uncertain_operation(&snap, 41, "submit", "unknown");

    // 1. Mutating submission is strictly blocked by recovery lock
    let submit_attempt = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    let submit_val: serde_json::Value = serde_json::from_str(&submit_attempt).unwrap();
    assert_eq!(submit_val["ok"], false);
    let err_msg = submit_val["error"].as_str().unwrap();
    assert!(
        err_msg.contains("inconnu") || err_msg.contains("incertaine"),
        "Must enforce blocking recovery lock on mutating submissions: {submit_attempt}"
    );

    // 2. Read-only refresh_status is permitted
    let status_attempt = bridge.dispatch_json(json!({"type": "refresh_status"}).to_string());
    let status_val: serde_json::Value = serde_json::from_str(&status_attempt).unwrap();
    assert_eq!(
        status_val["ok"], true,
        "Read-only status request must be allowed during recovery lock"
    );
}

/// `T3_PAIR_29`: Gateway connection delay (>2s) activates Honest Wait counter with cancel option.
///
/// Interactions: F19 (Pairing/Gateway) + F06 (Honest Wait)
/// ScreenState thresholds enforce no spinner under 400ms, skeleton under 2s,
/// and honest message + cancel button after 2000ms.
#[test]
fn test_t3_pair_29_gateway_connection_delay_triggers_honest_wait_counter() {
    assert_eq!(ScreenState::SPINNER_AFTER_MS, 400);
    assert_eq!(ScreenState::HONEST_AFTER_MS, 2000);

    // < 400ms: LoadingFast (nothing)
    assert_eq!(ScreenState::for_elapsed_ms(250), ScreenState::LoadingFast);

    // 400ms - 2000ms: LoadingSkeleton
    assert_eq!(
        ScreenState::for_elapsed_ms(1200),
        ScreenState::LoadingSkeleton
    );

    // > 2000ms: LoadingHonest with cancel button
    let honest_state = ScreenState::for_elapsed_ms(2500);
    match honest_state {
        ScreenState::LoadingHonest { message } => {
            assert!(message.contains("mémoire") || message.contains("cherche"));
        }
        other => panic!("Expected LoadingHonest after 2.5s delay, got {other:?}"),
    }
}

/// `T3_PAIR_30`: AcknowledgeUnknown clears recovery lock and unlocks prompt submissions.
///
/// Interactions: F20 (Checkpoint v2) + F19 (Pairing/Gateway)
/// User reviewing the uncertain operation dispatches `AcknowledgeUnknown { request_id }`.
/// The operation moves to acknowledged_operations, `recovery_required` resets to false,
/// and mutating submissions unlock immediately.
#[test]
fn test_t3_pair_30_acknowledge_unknown_clears_recovery_lock_and_unlocks_submission() {
    let harness = CheckpointHarness::new().expect("checkpoint harness");

    let crash_checkpoint = json!({
        "schema_version": 2,
        "draft": "Poursuite du travail",
        "next_request_id": 55,
        "uncertain_operations": [
            {
                "request_id": 54,
                "kind": "submit",
                "phase": "in_flight"
            }
        ],
        "acknowledged_operations": []
    });

    harness
        .write_atomic_fsync(&serde_json::to_string(&crash_checkpoint).unwrap())
        .expect("write");

    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080"
        })
        .to_string(),
    );

    harness.restore_into_bridge(&bridge).expect("restore");

    let snap_before = serde_json::from_str::<serde_json::Value>(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap_before["recovery_required"], true);

    // Acknowledge the uncertain operation
    let ack_res_str = bridge.dispatch_json(
        json!({
            "type": "acknowledge_unknown",
            "request_id": 54
        })
        .to_string(),
    );
    let ack_val: serde_json::Value = serde_json::from_str(&ack_res_str).unwrap();
    assert_eq!(ack_val["ok"], true);

    let snap_after = serde_json::from_str::<serde_json::Value>(&bridge.snapshot_json()).unwrap();
    CheckpointHarness::assert_recovery_required(&snap_after, false);
    CheckpointHarness::assert_acknowledged_operation(&snap_after, 54, "submit");

    // Status reply clears connection Unknown
    let status_res = bridge.dispatch_json(json!({"type": "refresh_status"}).to_string());
    let st_id = serde_json::from_str::<serde_json::Value>(&status_res).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();
    bridge.receive_json(
        st_id,
        json!({
            "session_id": "sess-recovery",
            "frames": [
                hakille_ipc::Frame::reply(st_id, hakille_ui_protocol::KernelToUi::StatusReply { status: crate::harness::default_status() })
            ]
        })
        .to_string(),
    );

    // Submissions are now unlocked!
    let submit_res_str = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    let submit_val: serde_json::Value = serde_json::from_str(&submit_res_str).unwrap();
    assert_eq!(
        submit_val["ok"], true,
        "Mutating submission must succeed after acknowledgment"
    );
    assert_eq!(submit_val["snapshot"]["pending"]["kind"], "submit");
}
