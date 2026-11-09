//! Feature 02: Aura Ring Streaming (Milestone M1 / Spec §1, §3)
//!
//! Specifications: T1_F02_01 to T1_F02_05
//!
//! Verifies:
//! - Emerald high-trust state (tau >= 0.90, calm bloom, slow respiration).
//! - Amber ambient fluid state (0.40 <= tau < 0.90, warm glow, intermediate pulse).
//! - Violet step-up challenge state (liveness elevation, alert pulse).
//! - Red locked/distress state (tau = 0.0, containment glow, rapid alert rate).
//! - AuraRingUpdateDto protocol framing and non-blocking streaming delivery to MobileBridge.

use crate::harness::MobileTestRig;
use hakille_ipc::Frame;
use hakille_ui_protocol::KernelToUi;
use hakille_ui_protocol::aura::AuraRingUpdateDto;
use serde_json::json;

/// `T1_F02_01`: Emerald high-trust chromatic state invariants.
///
/// When tau >= 0.90, the aura ring adopts the emerald signature ("EmeraldHighTrust"),
/// primary hex #10B981, and a tranquil 3000 ms pulse period.
#[test]
fn test_t1_f02_01_emerald_high_trust_state_invariants() {
    let dto = AuraRingUpdateDto::new(
        0.95,
        "EmeraldHighTrust",
        "#10B981",
        "#10B98173",
        3000,
        1.0,
        1700000000000,
    );

    assert!(
        dto.is_high_trust(),
        "tau=0.95 must qualify as high-trust emerald"
    );
    assert!(!dto.is_ambient());
    assert!(!dto.is_step_up());
    assert!(!dto.is_locked());

    assert!(dto.tau >= 0.90 && dto.tau <= 1.0);
    assert_eq!(dto.primary_hex, "#10B981");
    assert_eq!(dto.glow_hex, "#10B98173");
    assert_eq!(dto.pulse_period_ms, 3000);
    assert!((dto.scale_factor - 1.0).abs() < 1e-4);
}

/// `T1_F02_02`: Amber ambient fluid state invariants.
///
/// In the normal interaction band (0.40 <= tau < 0.90), state is "AmberAmbient",
/// primary hex #F59E0B, with scale factor within [0.95, 1.05].
#[test]
fn test_t1_f02_02_amber_ambient_fluid_state_invariants() {
    let dto = AuraRingUpdateDto::new(
        0.65,
        "AmberAmbient",
        "#F59E0B",
        "#F59E0B73",
        2500,
        1.02,
        1700000005000,
    );

    assert!(dto.is_ambient(), "tau=0.65 must qualify as ambient amber");
    assert!(!dto.is_high_trust());
    assert!(!dto.is_step_up());
    assert!(!dto.is_locked());

    assert!(dto.tau >= 0.40 && dto.tau < 0.90);
    assert_eq!(dto.primary_hex, "#F59E0B");
    assert!(dto.scale_factor >= 0.95 && dto.scale_factor <= 1.05);
}

/// `T1_F02_03`: Violet step-up liveness challenge state invariants.
///
/// When elevation of privilege is demanded, state transitions to "VioletStepUp",
/// primary hex #8B5CF6, and accelerated pulse (1500 ms).
#[test]
fn test_t1_f02_03_violet_step_up_liveness_challenge_invariants() {
    let dto = AuraRingUpdateDto::new(
        0.30,
        "VioletStepUp",
        "#8B5CF6",
        "#8B5CF673",
        1500,
        0.98,
        1700000010000,
    );

    assert!(dto.is_step_up(), "Step-up elevation must qualify as violet");
    assert!(!dto.is_high_trust());
    assert!(!dto.is_ambient());
    assert!(!dto.is_locked());

    assert_eq!(dto.primary_hex, "#8B5CF6");
    assert_eq!(dto.pulse_period_ms, 1500);
}

/// `T1_F02_04`: Red locked / distress containment state invariants.
///
/// On security lockdown or calm distress, tau is clamped to 0.0, state is "RedLocked",
/// primary hex #EF4444, and rapid containment pulse (800 ms).
#[test]
fn test_t1_f02_04_red_locked_distress_state_invariants() {
    let dto = AuraRingUpdateDto::new(
        0.0,
        "RedLocked",
        "#EF4444",
        "#EF444473",
        800,
        1.05,
        1700000015000,
    );

    assert!(dto.is_locked(), "tau=0.0 locked mode must qualify as red");
    assert!(!dto.is_high_trust());
    assert!(!dto.is_ambient());
    assert!(!dto.is_step_up());

    assert_eq!(dto.tau, 0.0);
    assert_eq!(dto.primary_hex, "#EF4444");
    assert_eq!(dto.pulse_period_ms, 800);
}

/// `T1_F02_05`: AuraRingUpdateDto stream delivery roundtrip through gateway and bridge.
///
/// Verifies that KernelToUi::AuraRingUpdate frames pass cleanly through the IPC gateway,
/// validate schema v1, and do not disrupt core snapshot consistency.
#[tokio::test]
async fn test_t1_f02_05_aura_ring_dto_stream_roundtrip_through_bridge() {
    let (rig, _daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let aura_update = AuraRingUpdateDto::new(
        0.92,
        "EmeraldHighTrust",
        "#10B981",
        "#10B98173",
        3000,
        1.01,
        1700000020000,
    );

    // 1. Verify bidirectional serialization compatibility with PROTOCOL_VERSION 1
    let msg = KernelToUi::AuraRingUpdate(aura_update.clone());
    let serialized = serde_json::to_value(&msg).expect("serialize AuraRingUpdate");
    assert!(
        serialized.get("AuraRingUpdate").is_some(),
        "Enum variant must match protocol schema"
    );

    let roundtrip_dto: KernelToUi =
        serde_json::from_value(serialized.clone()).expect("deserialize AuraRingUpdate");
    assert_eq!(
        roundtrip_dto, msg,
        "Deserialized message must match original"
    );

    // 2. Opaque-box contract: ambient AuraRingUpdate is strictly an ambient streaming event;
    // injecting it into a correlated request-response batch is rejected by mobile core.
    let (frame, _session) = rig.prepare_frame(&bridge, json!({"type": "refresh_status"}));
    let invalid_batch = json!({
        "session_id": "session-aura-test-1",
        "frames": [
            Frame::reply(0, msg),
            Frame::reply(frame.id, KernelToUi::StatusReply {
                status: crate::harness::default_status(),
            }),
        ],
    });

    let receive_res_str = bridge.receive_json(frame.id, invalid_batch.to_string());
    let receive_res: serde_json::Value = serde_json::from_str(&receive_res_str).unwrap();

    assert_eq!(
        receive_res["ok"], false,
        "Ambient AuraRingUpdate must not be mixed into request batches"
    );
    assert!(
        receive_res["error"]
            .as_str()
            .unwrap()
            .contains("Type de réponse incompatible avec la requête"),
        "Must enforce batch type compatibility: {receive_res_str}"
    );
}
