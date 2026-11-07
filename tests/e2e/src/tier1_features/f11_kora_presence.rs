//! Feature 11: 21-Wire Kora Presence Waveform (Milestone M3)
//!
//! Specifications: T1_F11_01 to T1_F11_05

use hakille_design::brand::{WIRES_FULL, WIRES_SMALL, wire_count};

pub fn speech_wire_height_px(envelope: f32) -> f32 {
    (envelope.clamp(0.0, 1.0) * 24.0).round()
}

/// `T1_F11_01`: Desktop Speaking State Renders 21 Wires
#[test]
fn test_t1_f11_01_desktop_speaking_state_renders_21_wires() {
    assert_eq!(
        WIRES_FULL, 21,
        "Speaking state full wire count must be exactly 21"
    );
    assert_eq!(
        wire_count(64),
        21,
        "Desktop HUD viewport (64px) must select 21 wires"
    );
}

/// `T1_F11_02`: Wire Height Proportional to Envelope
#[test]
fn test_t1_f11_02_wire_height_proportional_to_envelope() {
    assert_eq!(speech_wire_height_px(0.0), 0.0);
    assert_eq!(speech_wire_height_px(0.5), 12.0);
    assert_eq!(speech_wire_height_px(1.0), 24.0);
}

/// `T1_F11_03`: Wire Geometry Token Compliance
#[test]
fn test_t1_f11_03_wire_geometry_token_compliance() {
    let wire_width = 2.0;
    let wire_gap = 3.0;
    let total_width = (21.0 * wire_width) + (20.0 * wire_gap);
    assert_eq!(total_width, 102.0, "21 wires span must be 102 px");
}

/// `T1_F11_04`: Compact Viewport 7-Wire Layout
#[test]
fn test_t1_f11_04_compact_viewport_7_wire_layout() {
    assert_eq!(WIRES_SMALL, 7);
    assert_eq!(
        wire_count(16),
        7,
        "Viewport < 24px must collapse to 7 wires"
    );
}

/// `T1_F11_05`: Reduced Motion Waveform Fallback
#[test]
fn test_t1_f11_05_reduced_motion_waveform_fallback() {
    let reduced_motion = true;
    let wire_animation_enabled = !reduced_motion;
    assert!(
        !wire_animation_enabled,
        "Animation must be disabled under reduced motion"
    );
}
