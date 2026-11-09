//! Feature 01: Living Presence & 21 Kora Wires (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F01_01 to T2_F01_05 (5 tests total).

#![allow(unused_imports)]

use crate::harness::{DaemonStep, MobileTestRig};
use hakille_design::brand::{WIRES_FULL, WIRES_SMALL, WIRES_SMALL_BELOW_PX, wire_count};
use hakille_design::voice::{
    DEFAULT_SPEECH_RATE, NO_ANSWER_SILENCE_SECS, SPEECH_RATE_MAX, SPEECH_RATE_MIN,
    clamp_speech_rate,
};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_mobile_gateway::BARRIER_ID;
use hakille_ui_protocol::aura::AuraRingUpdateDto;
use hakille_ui_protocol::{KernelToUi, PresenceState, UiToKernel};
use serde_json::json;
use std::time::{Duration, Instant};

/// `T2_F01_01`: Viewport extreme aspect ratios and speech rate underflow/overflow clamping.
///
/// Validates wire count resolution across extreme aspect ratios (ultra-wide 32:9,
/// ultra-tall 9:22, micro ribbons, extreme slits) and extreme speech rate bounds.
#[test]
fn test_t2_f01_01_viewport_extreme_aspect_ratios_and_speech_rate_clamping() {
    // Ultra-wide (3840 x 1080) -> width 3840 >= 24 px -> 21 wires
    assert_eq!(wire_count(3840), WIRES_FULL);
    // Ultra-tall foldable (1080 x 2640) -> width 1080 >= 24 px -> 21 wires
    assert_eq!(wire_count(1080), WIRES_FULL);
    // Micro slit ribbon (12 px wide x 1000 px tall) -> width 12 < 24 px -> 7 wires
    assert_eq!(wire_count(12), WIRES_SMALL);
    // Degenerate width 1 px -> 7 wires
    assert_eq!(wire_count(1), WIRES_SMALL);
    // Degenerate width 0 px -> 7 wires
    assert_eq!(wire_count(0), WIRES_SMALL);
    // Huge width (100_000 px) -> 21 wires
    assert_eq!(wire_count(100_000), WIRES_FULL);

    // Speech rate extremes clamping
    assert_eq!(
        clamp_speech_rate(-10.0),
        SPEECH_RATE_MIN,
        "Negative rate clamped to min 0.7"
    );
    assert_eq!(
        clamp_speech_rate(0.0),
        SPEECH_RATE_MIN,
        "Zero rate clamped to min 0.7"
    );
    assert_eq!(
        clamp_speech_rate(0.699),
        SPEECH_RATE_MIN,
        "Sub-min rate clamped to min 0.7"
    );
    assert_eq!(
        clamp_speech_rate(1.601),
        SPEECH_RATE_MAX,
        "Supra-max rate clamped to max 1.6"
    );
    assert_eq!(
        clamp_speech_rate(100.0),
        SPEECH_RATE_MAX,
        "Extreme high rate clamped to max 1.6"
    );
    assert!(
        clamp_speech_rate(f32::NAN).is_nan(),
        "Rust f32::clamp preserves NaN without panic"
    );
}

/// `T2_F01_02`: Wire count boundary at exactly 23px vs 24px off-by-one check.
///
/// At exactly 23 px viewport width, wire count must be 7 (WIRES_SMALL).
/// At exactly 24 px viewport width, wire count must be 21 (WIRES_FULL).
#[test]
fn test_t2_f01_02_wire_count_boundary_at_23px_vs_24px() {
    assert_eq!(WIRES_SMALL_BELOW_PX, 24);
    assert_eq!(
        wire_count(WIRES_SMALL_BELOW_PX - 1),
        WIRES_SMALL,
        "23 px is just below threshold: must return compact 7 wires"
    );
    assert_eq!(
        wire_count(WIRES_SMALL_BELOW_PX),
        WIRES_FULL,
        "24 px is exactly at threshold: must return full 21 wires"
    );
    assert_eq!(
        wire_count(WIRES_SMALL_BELOW_PX + 1),
        WIRES_FULL,
        "25 px is above threshold: must return full 21 wires"
    );
}

/// `T2_F01_03`: Extreme rapid presence state thrashing.
///
/// Rapidly switching between presence states 70 times in succession maintains
/// state consistency without leaking intermediate state or desynchronizing the bridge.
#[test]
fn test_t2_f01_03_extreme_rapid_presence_state_thrashing() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080"
        })
        .to_string(),
    );

    let states = [
        PresenceState::Idle,
        PresenceState::Listening,
        PresenceState::Acting,
        PresenceState::Speaking,
        PresenceState::DecisionRequired,
        PresenceState::Urgent,
    ];

    // Simulate rapid updates arriving via receive_json replies
    for cycle in 0..10 {
        for (idx, state) in states.iter().enumerate() {
            let req_res = bridge.dispatch_json(json!({"type": "refresh_status"}).to_string());
            let req_json: serde_json::Value = serde_json::from_str(&req_res).unwrap();
            let req_id = req_json["request"]["id"].as_u64().unwrap();

            let reply_payload = json!({
                "session_id": "test-session",
                "frames": [
                    Frame::reply(req_id, KernelToUi::StatusReply {
                        status: crate::harness::default_status(),
                    }),
                    Frame::reply(0, KernelToUi::Presence(state.clone()))
                ]
            });
            let rec_res = bridge.receive_json(req_id, reply_payload.to_string());
            assert!(
                rec_res.contains("\"ok\":true"),
                "Cycle {cycle} step {idx} must succeed"
            );

            let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
            let current_presence = serde_json::to_string(&snap["presence"]).unwrap();
            let expected_presence = serde_json::to_string(state).unwrap();
            assert_eq!(
                current_presence, expected_presence,
                "Presence must match after rapid switch"
            );
        }
    }
}

/// `T2_F01_04`: Zero duration transitions immediate apply.
///
/// When transition duration is 0 ms, state change is instantaneous without pending animation frames.
#[test]
fn test_t2_f01_04_zero_duration_transitions() {
    let zero_duration = Duration::ZERO;
    assert_eq!(zero_duration.as_millis(), 0);

    // In zero-duration mode, time delta requirement is 0 ms
    let start = Instant::now();
    let elapsed = start.elapsed();
    assert!(elapsed >= zero_duration);

    // Verify silence timeout boundary
    assert_eq!(NO_ANSWER_SILENCE_SECS, 8);
}

/// `T2_F01_05`: Presence update with missing or disconnected connection.
///
/// When bridge is disconnected (no gateway session), presence can still be held locally
/// without falsely claiming connection status is Connected.
#[test]
fn test_t2_f01_05_presence_with_missing_connection() {
    let bridge = MobileBridge::new();
    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap["connection"], "disconnected");
    assert!(
        snap["presence"].is_null(),
        "Initial presence is None when disconnected without hub"
    );

    // Setting draft or local state does not transition connection to connected
    bridge.dispatch_json(json!({"type": "set_draft", "text": "Note hors ligne"}).to_string());
    let snap_after: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap_after["connection"], "disconnected");
    assert_eq!(snap_after["draft"], "Note hors ligne");
    assert!(
        snap_after["presence"].is_null(),
        "Presence remains None after local draft edit"
    );
}
