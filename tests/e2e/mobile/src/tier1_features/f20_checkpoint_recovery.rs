//! Feature 20: Checkpoint v2 & Recovery (Milestone M1 / Spec §2, ADR-011)
//!
//! Specifications: T1_F20_01 to T1_F20_05
//!
//! Verifies:
//! - Checkpoint v2 schema validation and atomic fsync on-disk persistence.
//! - Crash restoration marks in-flight mutations as `unknown` and activates `recovery_required`.
//! - Strict locking of all mutating actions during `recovery_required`.
//! - Resolution via `AcknowledgeUnknown` restores normal mutating operation.
//! - Rejection of corrupted or truncated checkpoint files with zero state corruption.

use crate::harness::CheckpointHarness;
use hakille_mobile::MobileBridge;
use serde_json::json;

/// `T1_F20_01`: Checkpoint v2 schema and atomic fsync persistence.
///
/// Ensures `draft_json()` outputs a valid schema v2 structure and is written
/// atomically to disk via fsync before HTTP submission.
#[test]
fn test_t1_f20_01_checkpoint_v2_schema_and_atomic_fsync_persistence() {
    let harness = CheckpointHarness::new().expect("checkpoint harness");
    let bridge = MobileBridge::new();

    // Set draft content
    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Brouillon confidentiel persistant"
        })
        .to_string(),
    );

    let draft_json = harness
        .save_draft_from_bridge(&bridge)
        .expect("save draft to disk");

    // Validate Checkpoint v2 JSON schema
    let parsed =
        CheckpointHarness::validate_checkpoint_schema(&draft_json).expect("schema v2 validation");
    assert_eq!(parsed["schema_version"], 2);
    assert_eq!(parsed["draft"], "Brouillon confidentiel persistant");
    assert!(parsed["next_request_id"].is_u64());
    assert!(parsed["uncertain_operations"].is_array());
    assert!(parsed["acknowledged_operations"].is_array());

    // File on disk matches memory state
    let disk_content = harness.read_checkpoint().expect("read checkpoint");
    assert_eq!(disk_content, draft_json);
}

/// `T1_F20_02`: Crash restore marks in-flight mutations as unknown and sets recovery_required.
///
/// Restoring a checkpoint that contained an in-flight operation transitions it
/// to `phase: "unknown"`, sets `recovery_required: true`, and marks `connection: "unknown"`.
#[test]
fn test_t1_f20_02_crash_restore_marks_in_flight_as_unknown_and_sets_recovery_required() {
    let harness = CheckpointHarness::new().expect("checkpoint harness");

    // Prepare a Checkpoint v2 payload representing an interrupted in-flight operation
    let in_flight_checkpoint = json!({
        "schema_version": 2,
        "draft": "Message interrompu par coupure courant",
        "next_request_id": 10,
        "uncertain_operations": [
            {
                "request_id": 9,
                "kind": "submit",
                "phase": "in_flight"
            }
        ],
        "acknowledged_operations": []
    });

    harness
        .write_atomic_fsync(&serde_json::to_string(&in_flight_checkpoint).unwrap())
        .expect("write to disk");

    // Restore into a fresh MobileBridge
    let fresh_bridge = MobileBridge::new();
    let restore_result = harness.restore_into_bridge(&fresh_bridge);
    assert!(restore_result.is_ok(), "Restoration must succeed");

    let snap: serde_json::Value = serde_json::from_str(&fresh_bridge.snapshot_json()).unwrap();

    // Must require recovery
    CheckpointHarness::assert_recovery_required(&snap, true);

    // In-flight operation must be marked unknown
    CheckpointHarness::assert_uncertain_operation(&snap, 9, "submit", "unknown");

    // Connection state must be unknown
    assert_eq!(snap["connection"], "unknown");

    // Draft is preserved
    assert_eq!(snap["draft"], "Message interrompu par coupure courant");
}

/// `T1_F20_03`: Mutating actions strictly blocked during recovery_required.
///
/// When `recovery_required == true`, any mutating action (`Submit`, `Remember`, `Approve`)
/// is strictly rejected, while non-mutating actions (`RefreshStatus`, `LoadMemory`) remain allowed.
#[test]
fn test_t1_f20_03_mutations_blocked_during_recovery_required() {
    let harness = CheckpointHarness::new().expect("checkpoint harness");
    let bridge = MobileBridge::new();

    // Prepare in-flight recovery checkpoint
    let checkpoint = json!({
        "schema_version": 2,
        "draft": "Texte",
        "next_request_id": 5,
        "uncertain_operations": [
            {"request_id": 4, "kind": "remember", "phase": "unknown"}
        ],
        "acknowledged_operations": []
    });
    harness.write_atomic_fsync(&checkpoint.to_string()).unwrap();
    harness.restore_into_bridge(&bridge).unwrap();

    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:17840"
        })
        .to_string(),
    );

    // Mutation 1: Submit must be blocked
    let res_submit: serde_json::Value =
        serde_json::from_str(&bridge.dispatch_json(json!({"type": "submit"}).to_string())).unwrap();
    assert_eq!(res_submit["ok"], false);
    let err1 = res_submit["error"].as_str().unwrap();
    assert!(err1.contains("incertaine") || err1.contains("inconnu"));

    // Mutation 2: Remember must be blocked
    let res_remember: serde_json::Value = serde_json::from_str(
        &bridge.dispatch_json(json!({"type": "remember", "text": "Nouvelle note"}).to_string()),
    )
    .unwrap();
    assert_eq!(res_remember["ok"], false);
    let err2 = res_remember["error"].as_str().unwrap();
    assert!(err2.contains("incertaine") || err2.contains("inconnu"));

    // Non-mutating action: LoadMemory must remain permitted
    let res_load: serde_json::Value = serde_json::from_str(
        &bridge
            .dispatch_json(json!({"type": "load_memory", "query": null, "limit": 10}).to_string()),
    )
    .unwrap();
    assert_eq!(
        res_load["ok"], true,
        "Non-mutating read actions must remain accessible during recovery"
    );
}

/// `T1_F20_04`: AcknowledgeUnknown clears recovery_required.
///
/// Dispatching `MobileAction::AcknowledgeUnknown` marks the uncertain operation as read,
/// clears `recovery_required`, and unblocks future mutating actions.
#[test]
fn test_t1_f20_04_acknowledge_unknown_clears_recovery_required() {
    let harness = CheckpointHarness::new().expect("checkpoint harness");
    let bridge = MobileBridge::new();

    let checkpoint = json!({
        "schema_version": 2,
        "draft": "Prêt",
        "next_request_id": 3,
        "uncertain_operations": [
            {"request_id": 2, "kind": "submit", "phase": "unknown"}
        ],
        "acknowledged_operations": []
    });
    harness.write_atomic_fsync(&checkpoint.to_string()).unwrap();
    harness.restore_into_bridge(&bridge).unwrap();

    let snap_before: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap_before["recovery_required"], true);

    // Acknowledge the unknown operation
    let ack_res: serde_json::Value = serde_json::from_str(
        &bridge.dispatch_json(
            json!({
                "type": "acknowledge_unknown",
                "request_id": 2
            })
            .to_string(),
        ),
    )
    .unwrap();

    assert_eq!(ack_res["ok"], true);

    let snap_after: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(
        snap_after["recovery_required"], false,
        "Recovery flag must be cleared"
    );
    assert_eq!(
        snap_after["uncertain_operations"].as_array().unwrap().len(),
        0
    );
    assert_eq!(
        snap_after["acknowledged_operations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(snap_after["acknowledged_operations"][0]["request_id"], 2);
}

/// `T1_F20_05`: Corrupted checkpoint recovery rejection.
///
/// A truncated or malformed checkpoint file on disk is strictly rejected with
/// an error, preventing partial or invalid state from corrupting the bridge.
#[test]
fn test_t1_f20_05_corrupted_checkpoint_recovery_rejection() {
    let harness = CheckpointHarness::new().expect("checkpoint harness");
    let bridge = MobileBridge::new();

    // Valid checkpoint initially
    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Données intactes"
        })
        .to_string(),
    );
    harness.save_draft_from_bridge(&bridge).unwrap();

    // Corrupt checkpoint bytes on disk (truncate halfway)
    harness.truncate_checkpoint(20).unwrap();

    // Attempt restoration
    let fresh_bridge = MobileBridge::new();
    let res = harness.restore_into_bridge(&fresh_bridge).unwrap();

    assert_eq!(
        res["ok"], false,
        "Corrupted checkpoint must fail to restore"
    );
    assert!(res["error"].is_string());

    // Bridge remains in initial clean state without crashing
    let snap: serde_json::Value = serde_json::from_str(&fresh_bridge.snapshot_json()).unwrap();
    assert_eq!(snap["draft"], "");
    assert_eq!(snap["recovery_required"], false);
}
