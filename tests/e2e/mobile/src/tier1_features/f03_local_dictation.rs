//! Feature 03: Local Dictation Mode (Milestone M1 / Spec §2, ADR-011)
//!
//! Specifications: T1_F03_01 to T1_F03_05
//!
//! Verifies:
//! - Local draft updates without submission or network roundtrips.
//! - Strict draft capacity enforcement (8192 bytes bound).
//! - Sovereign offline operation (works with zero hub configuration or network access).
//! - Foreground lifecycle transitions preserving draft integrity.
//! - Checkpoint v2 durability and restoration across bridge restart.

use crate::harness::CheckpointHarness;
use hakille_mobile::MobileBridge;
use serde_json::json;

/// `T1_F03_01`: Local draft update without submission.
///
/// Dispatching `set_draft` updates `snapshot.draft` immediately, does not initiate
/// a pending request, and produces no outbound network frame.
#[test]
fn test_t1_f03_01_local_draft_update_without_submission() {
    let bridge = MobileBridge::new();
    let dictation_text = "Rédiger le rapport d'audit souverain pour la réunion de demain.";

    let res_str = bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": dictation_text,
        })
        .to_string(),
    );

    let res: serde_json::Value = serde_json::from_str(&res_str).expect("parse dispatch response");
    assert_eq!(res["ok"], true, "set_draft must succeed: {res_str}");
    assert!(
        res["request"].is_null(),
        "Local dictation must not generate a network request frame"
    );

    let snapshot: serde_json::Value =
        serde_json::from_str(&bridge.snapshot_json()).expect("parse snapshot");
    assert_eq!(snapshot["draft"], dictation_text);
    assert!(
        snapshot["pending"].is_null(),
        "No pending request must be scheduled"
    );
    assert!(
        snapshot["outcomes"].as_array().unwrap().is_empty(),
        "No request outcome must be recorded"
    );
}

/// `T1_F03_02`: Strict draft capacity limit enforcement.
///
/// MAX_DRAFT_BYTES is 8192 bytes. A text of exactly 8192 bytes succeeds;
/// 8193 bytes is strictly rejected with an explicit validation error,
/// and the draft is not modified.
#[test]
fn test_t1_f03_02_strict_draft_capacity_limit_enforcement() {
    let bridge = MobileBridge::new();

    // 1. Valid boundary: exactly 8192 ASCII characters
    let max_text = "a".repeat(8192);
    let ok_res = bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": max_text,
        })
        .to_string(),
    );
    assert!(ok_res.contains("\"ok\":true"));

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap["draft"].as_str().unwrap().len(), 8192);

    // 2. Invalid boundary: 8193 bytes
    let overflow_text = "a".repeat(8193);
    let err_res_str = bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": overflow_text,
        })
        .to_string(),
    );

    let err_res: serde_json::Value = serde_json::from_str(&err_res_str).unwrap();
    assert_eq!(err_res["ok"], false);
    assert!(
        err_res["error"]
            .as_str()
            .unwrap()
            .contains("limite 8192 octets"),
        "Error message must specify 8192 byte limit: {err_res_str}"
    );

    // Previous valid draft must remain untouched
    let snap_after: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap_after["draft"].as_str().unwrap().len(), 8192);
}

/// `T1_F03_03`: Sovereign offline and disconnected dictation availability.
///
/// Local dictation requires neither internet, nor gateway, nor a configured hub.
/// It operates purely in-memory with sovereign privacy.
#[test]
fn test_t1_f03_03_offline_and_disconnected_dictation_availability() {
    let bridge = MobileBridge::new();
    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();

    assert!(snap["hub_id"].is_null());
    assert_eq!(snap["connection"], "disconnected");

    let offline_note = "Note confidentielle rédigée hors ligne sans aucun serveur";
    let res = bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": offline_note,
        })
        .to_string(),
    );

    assert!(res.contains("\"ok\":true"));
    let snap2: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap2["draft"], offline_note);
}

/// `T1_F03_04`: Foreground lifecycle preserves draft integrity.
///
/// Moving the app to background (foreground: false) and back (foreground: true)
/// must never wipe or alter the draft.
#[test]
fn test_t1_f03_04_foreground_lifecycle_preserves_draft() {
    let bridge = MobileBridge::new();
    let draft_text = "Brouillon précieux lors d'un appel téléphonique entrant";

    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": draft_text,
        })
        .to_string(),
    );

    // Switch to background
    let bg_res = bridge.dispatch_json(
        json!({
            "type": "set_foreground",
            "active": false,
        })
        .to_string(),
    );
    assert!(bg_res.contains("\"ok\":true"));

    let snap_bg: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap_bg["foreground"], false);
    assert_eq!(
        snap_bg["draft"], draft_text,
        "Draft preserved in background"
    );

    // Return to foreground
    let fg_res = bridge.dispatch_json(
        json!({
            "type": "set_foreground",
            "active": true,
        })
        .to_string(),
    );
    assert!(fg_res.contains("\"ok\":true"));

    let snap_fg: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap_fg["foreground"], true);
    assert_eq!(snap_fg["draft"], draft_text, "Draft preserved upon resume");
}

/// `T1_F03_05`: Draft checkpoint durability across bridge restarts.
///
/// Checkpoint v2 fsyncs draft to disk before submission; restoring it into a fresh
/// bridge recovers the exact dictated text.
#[test]
fn test_t1_f03_05_draft_checkpoint_durability_across_bridge_restart() {
    let harness = CheckpointHarness::new().expect("create harness");
    let bridge1 = MobileBridge::new();

    let dictated = "Compte rendu de dictée locale prêt pour persistance atomique.";
    bridge1.dispatch_json(
        json!({
            "type": "set_draft",
            "text": dictated,
        })
        .to_string(),
    );

    // Fsync checkpoint to disk
    harness
        .save_draft_from_bridge(&bridge1)
        .expect("atomic save to disk");

    // Drop bridge1 to simulate process death
    drop(bridge1);

    // Fresh instance
    let bridge2 = MobileBridge::new();
    let restore_result = harness
        .restore_into_bridge(&bridge2)
        .expect("restore checkpoint into new bridge");

    assert_eq!(restore_result["ok"], true);
    let snap: serde_json::Value = serde_json::from_str(&bridge2.snapshot_json()).unwrap();
    assert_eq!(
        snap["draft"], dictated,
        "Restored draft must match original dictated text"
    );
}
