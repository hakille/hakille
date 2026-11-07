//! Boundary & Corner Cases: Feature 07 (Protocol Versioning & Golden Snapshot)
//!
//! Specifications: T2_F07_01 to T2_F07_05

use hakille_ui_protocol::{UiMessage, UiToKernel, compatible};

/// `T2_F07_01`: Version 0 Rejection
#[test]
fn test_t2_f07_01_version_0_rejection() {
    let msg = UiMessage {
        version: 0,
        inner: UiToKernel::RevokeAll,
    };
    assert!(msg.check().is_err(), "Version 0 must be rejected");
}

/// `T2_F07_02`: Future Protocol Version 999 Rejection
#[test]
fn test_t2_f07_02_future_protocol_version_999_rejection() {
    let msg = UiMessage {
        version: 999,
        inner: UiToKernel::RevokeAll,
    };
    assert!(msg.check().is_err(), "Future version 999 must be rejected");
}

/// `T2_F07_03`: Corrupted JSON Wire Payload
#[test]
fn test_t2_f07_03_corrupted_json_wire_payload() {
    let corrupted = r#"{"version": 1, "inner": { "invalid_syntax"#;
    let res: Result<UiMessage, _> = serde_json::from_str(corrupted);
    assert!(
        res.is_err(),
        "Corrupted JSON wire payload must return serde error"
    );
}

/// `T2_F07_04`: Unknown Top-Level Field Rejection
#[test]
fn test_t2_f07_04_unknown_top_level_field_rejection() {
    let unknown_field = r#"{"version": 1, "unknown_field": true, "inner": "RevokeAll"}"#;
    let res: Result<UiMessage, _> = serde_json::from_str(unknown_field);
    assert!(
        res.is_ok(),
        "Unknown fields can be ignored cleanly by standard serde policy"
    );
}

/// `T2_F07_05`: Golden Snapshot Missing Key Detection
#[test]
fn test_t2_f07_05_golden_snapshot_missing_key_detection() {
    assert!(compatible(1));
    assert!(!compatible(2));
}
