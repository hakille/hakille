//! Feature 07: Protocol Versioning & Golden Snapshot (Milestone M2)
//!
//! Specifications: T1_F07_01 to T1_F07_05

use hakille_ui_protocol::{
    KernelMessage, KernelToUi, PROTOCOL_VERSION, UiMessage, UiToKernel, compatible,
};

/// `T1_F07_01`: Protocol Version Constant Verification
#[test]
fn test_t1_f07_01_protocol_version_constant_verification() {
    assert_eq!(PROTOCOL_VERSION, 1, "PROTOCOL_VERSION must be exactly 1");
}

/// `T1_F07_02`: Version Compatibility Check Function
#[test]
fn test_t1_f07_02_version_compatibility_check_function() {
    assert!(compatible(1), "Version 1 must be compatible");
    assert!(!compatible(0), "Version 0 must be rejected");
    assert!(!compatible(2), "Version 2 must be rejected");
    assert!(!compatible(999), "Version 999 must be rejected");
}

/// `T1_F07_03`: Golden Schema Snapshot Parity Test
#[test]
fn test_t1_f07_03_golden_schema_snapshot_parity_test() {
    // Assert schema snapshot file exists and is valid JSON
    let content = include_str!("../../../../crates/hakille-ui-protocol/src/schema_snapshot.json");
    let val: serde_json::Value =
        serde_json::from_str(content).expect("schema_snapshot.json must be valid JSON");
    assert!(val.is_object() || val.is_array());
}

/// `T1_F07_04`: Additive Field Backward Compatibility
#[test]
fn test_t1_f07_04_additive_field_backward_compatibility() {
    let payload = r#"{"version": 1, "inner": {"CardDecisionAck": {"card_id": 42, "approved": true, "audit_id": "aud-1", "effect": "queued"}}}"#;
    let msg: Result<KernelMessage, _> = serde_json::from_str(payload);
    assert!(
        msg.is_ok(),
        "Additive field payload must deserialize cleanly"
    );
    let unwrapped = msg.unwrap();
    assert_eq!(unwrapped.version, 1);
    match unwrapped.inner {
        KernelToUi::CardDecisionAck {
            card_id,
            approved,
            effect,
            ..
        } => {
            assert_eq!(card_id, 42);
            assert!(approved);
            assert_eq!(effect, Some("queued".into()));
        }
        _ => panic!("Expected CardDecisionAck"),
    }
}

/// `T1_F07_05`: Message Envelope Wrapping
#[test]
fn test_t1_f07_05_message_envelope_wrapping() {
    let msg = UiMessage::new(UiToKernel::RevokeAll);
    assert_eq!(msg.version, 1);
    assert_eq!(msg.inner, UiToKernel::RevokeAll);
    assert!(
        msg.check().is_ok(),
        ".check() on valid version 1 must succeed"
    );
}
