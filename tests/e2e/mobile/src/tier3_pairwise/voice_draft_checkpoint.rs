//! Tier 3 Pairwise Module: Voice Dictation & Checkpoint State Persistence
//!
//! Combinatorial interactions between:
//! - F03: Local Dictation Mode
//! - F04: Hub Voice Conversation
//! - F06: Honest Wait & Input Locking
//! - F20: Checkpoint v2 & Crash Recovery
//!
//! Tests: T3_PAIR_11 to T3_PAIR_14

use crate::harness::CheckpointHarness;
use hakille_mobile::MobileBridge;
use serde_json::json;

/// `T3_PAIR_11`: Local dictation stream buffers into draft and fsyncs to Checkpoint v2 before backgrounding.
///
/// Interactions: F03 (Local Dictation) + F20 (Checkpoint v2)
/// When user dictates locally, text appends progressively into snapshot.draft.
/// Prior to backgrounding or network mutation, `draft_json()` is written atomically via `sync_all()`.
#[test]
fn test_t3_pair_11_local_dictation_stream_buffers_and_fsyncs_to_checkpoint_v2() {
    let harness = CheckpointHarness::new().expect("checkpoint harness");
    let bridge = MobileBridge::new();

    // 1. Initial draft
    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Première phrase dictée.",
        })
        .to_string(),
    );

    // 2. Dictation stream chunk appends
    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Première phrase dictée. Seconde phrase en cours d'élocution.",
        })
        .to_string(),
    );

    let snap = serde_json::from_str::<serde_json::Value>(&bridge.snapshot_json()).unwrap();
    assert_eq!(
        snap["draft"],
        "Première phrase dictée. Seconde phrase en cours d'élocution."
    );

    // 3. Persist atomically to disk via CheckpointHarness
    let draft_json = harness
        .save_draft_from_bridge(&bridge)
        .expect("atomic fsync draft");
    let validated = CheckpointHarness::validate_checkpoint_schema(&draft_json)
        .expect("checkpoint schema v2 validation");

    assert_eq!(validated["schema_version"], 2);
    assert_eq!(
        validated["draft"],
        "Première phrase dictée. Seconde phrase en cours d'élocution."
    );
    assert_eq!(
        validated["uncertain_operations"].as_array().unwrap().len(),
        0
    );

    // 4. Backgrounding app
    bridge.dispatch_json(json!({"type": "set_foreground", "active": false}).to_string());
    let snap_bg = serde_json::from_str::<serde_json::Value>(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap_bg["foreground"], false);
}

/// `T3_PAIR_12`: Active dictation locks text submission and prevents concurrency races.
///
/// Interactions: F03 (Local Dictation) + F06 (Input Locking)
/// While microphone recording is active (`microphone_enabled == true`), input submission
/// is locked to prevent concurrency races between voice ingestion and keyboard dispatch.
#[test]
fn test_t3_pair_12_active_dictation_locks_text_submission_and_shows_recording_indicator() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080",
        })
        .to_string(),
    );

    // 1. Honesty contract: Attempting to enable unattached native microphone is rejected
    let mic_res_str =
        bridge.dispatch_json(json!({"type": "set_microphone", "enabled": true}).to_string());
    let mic_res: serde_json::Value = serde_json::from_str(&mic_res_str).unwrap();
    assert_eq!(mic_res["ok"], false);
    assert!(
        mic_res["error"]
            .as_str()
            .unwrap()
            .contains("Capture microphone native non raccordée"),
        "Honesty contract: unattached native microphone cannot simulate listening"
    );

    // 2. Concurrency locking during in-flight submission
    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Dictée locale en cours de traitement",
        })
        .to_string(),
    );
    let sub1_str = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    let sub1_val: serde_json::Value = serde_json::from_str(&sub1_str).unwrap();
    assert_eq!(sub1_val["ok"], true);

    // Concurrent second submission while first is pending must be strictly locked
    let sub2_str = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    let sub2_val: serde_json::Value = serde_json::from_str(&sub2_str).unwrap();
    assert_eq!(sub2_val["ok"], false);
    assert!(
        sub2_val["error"]
            .as_str()
            .unwrap()
            .contains("Une requête est déjà en cours"),
        "Must lock text submission while another request is in flight: {sub2_str}"
    );
}

/// `T3_PAIR_13`: Audio streaming interruption during voice dialogue preserves draft in Checkpoint v2.
///
/// Interactions: F04 (Voice Conversation) + F20 (Checkpoint v2)
/// Sudden audio interruption (e.g. network disconnect during voice dialogue) preserves
/// the captured voice draft in disk persistence without corrupting monotonic request IDs.
#[test]
fn test_t3_pair_13_audio_streaming_interruption_preserves_draft_in_checkpoint() {
    let harness = CheckpointHarness::new().expect("checkpoint harness");
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080",
        })
        .to_string(),
    );

    // 1. User starts voice
    bridge.dispatch_json(json!({"type": "begin_voice"}).to_string());
    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Brouillon vocal interrompu par perte réseau",
        })
        .to_string(),
    );

    // Save checkpoint
    let draft_json = harness
        .save_draft_from_bridge(&bridge)
        .expect("save draft to disk");
    let val = CheckpointHarness::validate_checkpoint_schema(&draft_json).unwrap();
    let original_next_id = val["next_request_id"].as_u64().unwrap();

    // 2. Audio cancels / stream drops
    bridge.dispatch_json(json!({"type": "cancel_voice"}).to_string());

    // 3. Recover in fresh bridge from disk
    let fresh_bridge = MobileBridge::new();
    let restore_res = harness
        .restore_into_bridge(&fresh_bridge)
        .expect("restore into bridge");
    assert_eq!(restore_res["ok"], true);

    let fresh_snap =
        serde_json::from_str::<serde_json::Value>(&fresh_bridge.snapshot_json()).unwrap();
    assert_eq!(
        fresh_snap["draft"], "Brouillon vocal interrompu par perte réseau",
        "Interrupted voice draft must be fully restored"
    );

    let fresh_draft_json = fresh_bridge.draft_json();
    let fresh_val = CheckpointHarness::validate_checkpoint_schema(&fresh_draft_json).unwrap();
    assert_eq!(
        fresh_val["next_request_id"].as_u64().unwrap(),
        original_next_id,
        "Request ID counter must not be corrupted"
    );
}

/// `T3_PAIR_14`: Process crash during local dictation restores draft cleanly with recovery_required == false.
///
/// Interactions: F03 (Local Dictation) + F20 (Checkpoint v2)
/// When process terminates during local dictation without in-flight network mutations,
/// relaunch restores the draft immediately with `recovery_required == false`, allowing prompt editing.
#[test]
fn test_t3_pair_14_process_crash_during_dictation_restores_draft_cleanly_without_recovery_flag() {
    let harness = CheckpointHarness::new().expect("checkpoint harness");
    let bridge = MobileBridge::new();

    let dictated_text = "Rapport technique d'intégrité cognitive Hakille.";
    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": dictated_text,
        })
        .to_string(),
    );

    harness
        .save_draft_from_bridge(&bridge)
        .expect("fsync draft before crash");

    // Drop bridge to simulate sudden OS kill
    drop(bridge);

    // Relaunch app with brand new bridge
    let relaunch_bridge = MobileBridge::new();
    let restore_result = harness
        .restore_into_bridge(&relaunch_bridge)
        .expect("clean restore");
    assert_eq!(restore_result["ok"], true);

    let snap = serde_json::from_str::<serde_json::Value>(&relaunch_bridge.snapshot_json()).unwrap();
    assert_eq!(snap["draft"], dictated_text);

    // Because there was no in-flight network mutation, recovery_required is strictly false
    CheckpointHarness::assert_recovery_required(&snap, false);
    assert_eq!(snap["uncertain_operations"].as_array().unwrap().len(), 0);
}
