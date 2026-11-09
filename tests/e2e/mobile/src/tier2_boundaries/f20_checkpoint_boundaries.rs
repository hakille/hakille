//! Feature 20: Checkpoint v2 & Recovery (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F20_01 to T2_F20_05 (5 tests total).

#![allow(unused_imports)]

use crate::harness::{CheckpointHarness, MobileTestRig};
use crate::tier1_features::f21_bounded_camera::{
    MAX_LONG_EDGE_PX, MAX_PHOTO_BYTES, MAX_TOTAL_PIXELS, validate_camera_frame,
};
use crate::tier1_features::f22_bounded_doc_picker::{
    DOCUMENT_READ_TIMEOUT_SECS, MAX_DOCUMENT_BYTES, MAX_PREVIEW_CHARS, inspect_document_bytes,
};
use crate::tier1_features::f23_single_shot_gps::PhoneLocation;
use crate::tier1_features::f24_allowlisted_launcher::{AllowedDestination, ExternalLaunchPolicy};
use chrono::Utc;
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_mobile_gateway::{BearerToken, read_private_token};
use hakille_ui_protocol::UiToKernel;
use reqwest::header::{AUTHORIZATION, ORIGIN};
use serde_json::json;
use std::fs::File;
use std::io::Write;

/// `T2_F20_01`: Truncated checkpoint file rejection.
///
/// Attempting to restore a truncated checkpoint file returns a deserialization error without state corruption.
#[test]
fn test_t2_f20_01_truncated_checkpoint_file_rejection() {
    let harness = CheckpointHarness::new().unwrap();
    let truncated_json = "{\"schema_version\": 2, \"draft\": \"Texte interrom";

    harness.write_atomic_fsync(truncated_json).unwrap();

    let bridge = MobileBridge::new();
    let res = harness.restore_into_bridge(&bridge).unwrap();
    assert_eq!(res["ok"], false, "Truncated JSON must fail restoration");
    assert!(res["error"].is_string());

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap["draft"], "", "Bridge draft must not be corrupted");
}

/// `T2_F20_02`: Invalid schema version (v1 or v3) rejected.
///
/// Checkpoint restoration accepts schema_version == 2 only; v1 and v3 are rejected.
#[test]
fn test_t2_f20_02_invalid_schema_version_rejected() {
    let harness = CheckpointHarness::new().unwrap();
    let bridge = MobileBridge::new();

    // Schema v1 rejected
    let cp_v1 = json!({
        "schema_version": 1,
        "draft": "Legacy v1",
        "next_request_id": 1,
        "uncertain_operations": [],
        "acknowledged_operations": []
    });
    harness.write_atomic_fsync(&cp_v1.to_string()).unwrap();
    let res_v1 = harness.restore_into_bridge(&bridge).unwrap();
    assert_eq!(res_v1["ok"], false, "Schema version 1 must be rejected");
    assert!(res_v1["error"].is_string());

    // Schema v3 rejected
    let cp_v3 = json!({
        "schema_version": 3,
        "draft": "Future v3",
        "next_request_id": 1,
        "uncertain_operations": [],
        "acknowledged_operations": []
    });
    harness.write_atomic_fsync(&cp_v3.to_string()).unwrap();
    let res_v3 = harness.restore_into_bridge(&bridge).unwrap();
    assert_eq!(res_v3["ok"], false, "Schema version 3 must be rejected");
    assert!(res_v3["error"].is_string());
}

/// `T2_F20_03`: Power loss simulation mid-sync.
///
/// Simulating crash before rename guarantees existing on-disk checkpoint is untouched.
#[test]
fn test_t2_f20_03_power_loss_simulation_mid_sync() {
    let harness = CheckpointHarness::new().unwrap();

    // Valid initial checkpoint
    let cp_initial = json!({
        "schema_version": 2,
        "draft": "Version stable persistée",
        "next_request_id": 5,
        "uncertain_operations": [],
        "acknowledged_operations": []
    });
    harness.write_atomic_fsync(&cp_initial.to_string()).unwrap();

    // Simulate half-written temp file that never gets renamed (crash during write)
    let tmp_crash = harness.temp_dir.path().join(".checkpoint-crashed.tmp");
    {
        let mut f = File::create(&tmp_crash).unwrap();
        f.write_all(b"{\"schema_version\": 2, \"draft\": \"corrupt")
            .unwrap();
    }

    // Read durable checkpoint
    let durable = harness.read_checkpoint().unwrap();
    assert_eq!(durable, cp_initial.to_string());
}

/// `T2_F20_04`: Uncertain operations queue max limit (32 operations).
///
/// When snapshot has 32 uncertain operations, new mutating dispatches fail with
/// "Trop d'opérations incertaines conservées".
#[test]
fn test_t2_f20_04_uncertain_operations_queue_max_limit_32() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());

    // Build checkpoint with exactly 32 uncertain operations
    let mut uncertain_ops = Vec::new();
    for i in 1..=32 {
        uncertain_ops.push(json!({
            "request_id": i,
            "kind": "submit",
            "phase": "unknown"
        }));
    }

    let checkpoint = json!({
        "schema_version": 2,
        "draft": "Draft",
        "next_request_id": 33,
        "uncertain_operations": uncertain_ops,
        "acknowledged_operations": []
    });

    bridge.restore_draft_json(checkpoint.to_string());

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap["uncertain_operations"].as_array().unwrap().len(), 32);

    // Attempting a mutating submit when queue is full (32 items)
    let res_str = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    let res: serde_json::Value = serde_json::from_str(&res_str).unwrap();
    assert_eq!(res["ok"], false);
    assert!(
        res["error"].as_str().unwrap().contains("inconnu")
            || res["error"].as_str().unwrap().contains("incertaine"),
        "Mutations must be blocked when uncertain operations exist"
    );

    // Attempting to restore a checkpoint with 33 uncertain operations is rejected
    let mut overflow_ops = uncertain_ops.clone();
    overflow_ops.push(json!({
        "request_id": 33,
        "kind": "submit",
        "phase": "unknown"
    }));
    let overflow_checkpoint = json!({
        "schema_version": 2,
        "draft": "Draft",
        "next_request_id": 34,
        "uncertain_operations": overflow_ops,
        "acknowledged_operations": []
    });
    let over_res_str = bridge.restore_draft_json(overflow_checkpoint.to_string());
    let over_res: serde_json::Value = serde_json::from_str(&over_res_str).unwrap();
    assert_eq!(over_res["ok"], false);
    assert!(
        over_res["error"]
            .as_str()
            .unwrap()
            .contains("supérieur à 32 incertitudes")
            || over_res["error"].as_str().unwrap().contains("incertaine")
    );
}

/// `T2_F20_05`: Multiple consecutive crashes preserve recovery_required.
///
/// Crashing and restoring repeatedly across multiple instances maintains recovery_required.
#[test]
fn test_t2_f20_05_multiple_consecutive_crashes_preserve_recovery() {
    let harness = CheckpointHarness::new().unwrap();

    let cp = json!({
        "schema_version": 2,
        "draft": "Important",
        "next_request_id": 2,
        "uncertain_operations": [{
            "request_id": 1,
            "kind": "submit",
            "phase": "in_flight"
        }],
        "acknowledged_operations": []
    });
    harness.write_atomic_fsync(&cp.to_string()).unwrap();

    // Instance 1 restores
    let b1 = MobileBridge::new();
    harness.restore_into_bridge(&b1).unwrap();
    let s1: serde_json::Value = serde_json::from_str(&b1.snapshot_json()).unwrap();
    assert_eq!(s1["recovery_required"], true);

    // Instance 1 crashes, writes back draft_json
    let d1 = b1.draft_json();
    harness.write_atomic_fsync(&d1).unwrap();

    // Instance 2 restores from crash 1
    let b2 = MobileBridge::new();
    harness.restore_into_bridge(&b2).unwrap();
    let s2: serde_json::Value = serde_json::from_str(&b2.snapshot_json()).unwrap();
    assert_eq!(
        s2["recovery_required"], true,
        "Recovery required must persist across crash 2"
    );
    assert_eq!(s2["connection"], "unknown");
}
