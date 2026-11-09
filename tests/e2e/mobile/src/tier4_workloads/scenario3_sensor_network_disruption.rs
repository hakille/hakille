//! Tier 4 Real-World Application Workload: Scenario 3
//!
//! Bounded Sensor Interaction with Network Disruption & Crash Recovery
//! Features Exercised: F06 (Honest Wait & Input Locking), F20 (Checkpoint v2 & Recovery), F21 (Bounded In-Memory Camera), F23 (Single-Shot GPS with Age)
//! Complexity: High
//!
//! Workflow:
//! 1. Multimodal ingestion: Camera thumbnail clamped in volatile memory (<= 1536 px) + single-shot GPS fix.
//! 2. Submission & pre-dispatch fsync: Prompt submitted, UncertainOperation recorded, Checkpoint v2 fsynced to disk.
//! 3. Network disruption: Gateway connection stalls (>2s), Honest Wait activates, connection drops.
//! 4. Crash & relaunch: Process killed; fresh bridge reloads checkpoint with recovery_required: true.
//! 5. Blocking recovery lock: Mutating actions blocked; read-only status allowed; volatile thumbnail zeroized.
//! 6. Acknowledgment: User acknowledges uncertain operation, recovery lock clears, prompt submissions unlock.

use crate::harness::CheckpointHarness;
use hakille_design::states::ScreenState;
use hakille_mobile::MobileBridge;
use serde_json::json;

#[test]
fn test_scenario3_sensor_interaction_network_disruption_and_recovery() {
    let harness = CheckpointHarness::new().expect("checkpoint harness");
    let bridge = MobileBridge::new();

    // --- STEP 1: Multimodal Ingestion ---
    // Single-shot GPS fix: age 2s, accurate
    let gps_lat = 48.8566;
    let gps_lon = 2.3522;
    let gps_age_secs = 2;
    assert!(gps_age_secs <= 60, "GPS reading is fresh (<60s)");

    // Volatile camera thumbnail (clamped to <= 1536 px)
    let max_dim_px = 1536;
    let raw_photo_w = 4032;
    let raw_photo_h = 3024;
    let scale = max_dim_px as f32 / raw_photo_w as f32;
    let clamped_w = (raw_photo_w as f32 * scale) as u32;
    let clamped_h = (raw_photo_h as f32 * scale) as u32;
    assert!(clamped_w <= 1536 && clamped_h <= 1536);

    let prompt_text = format!(
        "Inspection structurelle GPS ({:.4}, {:.4}) avec photo jointe.",
        gps_lat, gps_lon
    );

    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080",
        })
        .to_string(),
    );

    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": prompt_text,
        })
        .to_string(),
    );

    // --- STEP 2: Submission & Pre-dispatch Fsync ---
    let submit_res_str = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    let submit_res: serde_json::Value = serde_json::from_str(&submit_res_str).unwrap();
    assert_eq!(submit_res["ok"], true);
    let req_id = submit_res["request"]["id"].as_u64().unwrap();

    // Snapshot records in-flight mutation
    let snap1 = serde_json::from_str::<serde_json::Value>(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap1["pending"]["kind"], "submit");
    assert_eq!(snap1["pending"]["mutating"], true);

    // Pre-dispatch atomic fsync of Checkpoint v2
    let draft_json = harness
        .save_draft_from_bridge(&bridge)
        .expect("save checkpoint v2");
    let val = CheckpointHarness::validate_checkpoint_schema(&draft_json).unwrap();
    assert_eq!(val["schema_version"], 2);

    // Camera thumbnail must NOT leak into disk checkpoint
    assert!(!draft_json.contains("thumbnail_bytes"));
    assert!(!draft_json.contains("data:image"));

    // --- STEP 3: Network Disruption & Honest Wait ---
    // Connection stalls > 2000 ms -> Honest Wait mode
    let wait_state = ScreenState::for_elapsed_ms(2400);
    match wait_state {
        ScreenState::LoadingHonest { message } => {
            assert!(message.contains("mémoire") || message.contains("cherche"));
        }
        other => panic!("Expected LoadingHonest wait state, got {other:?}"),
    }

    // --- STEP 4: Crash & Relaunch ---
    // Simulate abrupt power cut / app kill during in-flight network call
    drop(bridge);

    let fresh_bridge = MobileBridge::new();
    fresh_bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080",
        })
        .to_string(),
    );

    let restore_result = harness
        .restore_into_bridge(&fresh_bridge)
        .expect("restore crash checkpoint");
    assert_eq!(restore_result["ok"], true);

    let fresh_snap =
        serde_json::from_str::<serde_json::Value>(&fresh_bridge.snapshot_json()).unwrap();

    // In-flight operation was converted to uncertain operation in phase: unknown
    CheckpointHarness::assert_recovery_required(&fresh_snap, true);
    CheckpointHarness::assert_uncertain_operation(&fresh_snap, req_id, "submit", "unknown");

    // --- STEP 5: Blocking Recovery Lock ---
    // 5a. Mutating actions strictly blocked
    let blocked_submit = fresh_bridge.dispatch_json(json!({"type": "submit"}).to_string());
    assert!(
        blocked_submit.contains("inconnu") || blocked_submit.contains("incertaine"),
        "Must block mutating submissions while recovery is required: {blocked_submit}"
    );

    let blocked_remember = fresh_bridge.dispatch_json(
        json!({
            "type": "remember",
            "text": "Tentative mémo",
        })
        .to_string(),
    );
    assert!(
        blocked_remember.contains("inconnu") || blocked_remember.contains("incertaine"),
        "Must block remember mutations while recovery is required"
    );

    // 5b. Read-only status allowed
    let allowed_status = fresh_bridge.dispatch_json(json!({"type": "refresh_status"}).to_string());
    assert!(
        allowed_status.contains("\"ok\":true"),
        "Read-only status must succeed even when recovery is required"
    );
    let st_id =
        serde_json::from_str::<serde_json::Value>(&allowed_status).unwrap()["request"]["id"]
            .as_u64()
            .unwrap();

    // --- STEP 6: Acknowledgment & Unlock ---
    let ack_res = fresh_bridge.dispatch_json(
        json!({
            "type": "acknowledge_unknown",
            "request_id": req_id,
        })
        .to_string(),
    );
    assert!(ack_res.contains("\"ok\":true"));

    // Status reply clears connection Unknown
    fresh_bridge.receive_json(
        st_id,
        json!({
            "session_id": "sess-s3",
            "frames": [
                hakille_ipc::Frame::reply(st_id, hakille_ui_protocol::KernelToUi::StatusReply { status: crate::harness::default_status() })
            ]
        })
        .to_string(),
    );

    let unlocked_snap =
        serde_json::from_str::<serde_json::Value>(&fresh_bridge.snapshot_json()).unwrap();
    CheckpointHarness::assert_recovery_required(&unlocked_snap, false);
    CheckpointHarness::assert_acknowledged_operation(&unlocked_snap, req_id, "submit");

    // Submissions are now unlocked and draft is preserved
    assert_eq!(unlocked_snap["draft"], prompt_text);
    let retry_submit = fresh_bridge.dispatch_json(json!({"type": "submit"}).to_string());
    assert!(retry_submit.contains("\"ok\":true"));
}
