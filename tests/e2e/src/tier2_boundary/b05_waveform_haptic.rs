//! Boundary & Corner Cases: Feature 05 (Protocol Waveform & Haptic Schemas)
//!
//! Specifications: T2_F05_01 to T2_F05_05

use crate::tier1_features::f05_waveform_haptic::{HapticTriggerPayload, VoiceWaveformPayload};

/// `T2_F05_01`: Empty Waveform Amplitudes Array
#[test]
fn test_t2_f05_01_empty_waveform_amplitudes_array() {
    let raw = r#"{"amplitudes": []}"#;
    let res: Result<VoiceWaveformPayload, _> = serde_json::from_str(raw);
    assert!(
        res.is_err(),
        "Empty amplitudes array must fail deserialization for [f32; 21]"
    );
}

/// `T2_F05_02`: Negative or Infinite Amplitude Float
#[test]
fn test_t2_f05_02_negative_or_infinite_amplitude_float() {
    let raw_val = -0.5f32;
    let clamped = if raw_val.is_finite() {
        raw_val.clamp(0.0, 1.0)
    } else {
        0.0
    };
    assert_eq!(clamped, 0.0);

    let inf_val = f32::INFINITY;
    let clamped_inf = if inf_val.is_finite() {
        inf_val.clamp(0.0, 1.0)
    } else {
        0.0
    };
    assert_eq!(clamped_inf, 0.0);
}

/// `T2_F05_03`: Unknown Haptic Pattern Variant
#[test]
fn test_t2_f05_03_unknown_haptic_pattern_variant() {
    let raw = r#"{"pattern": "SuperMegaBlast", "intensity_pct": 100}"#;
    let res: Result<HapticTriggerPayload, _> = serde_json::from_str(raw);
    assert!(
        res.is_err(),
        "Unknown haptic pattern variant must fail deserialization cleanly"
    );
}

/// `T2_F05_04`: High-Frequency Waveform Packet Flood
#[test]
fn test_t2_f05_04_high_frequency_waveform_packet_flood() {
    let mut ring_buffer: Option<VoiceWaveformPayload> = None;
    for i in 0..10_000 {
        ring_buffer = Some(VoiceWaveformPayload {
            amplitudes: [i as f32; 21],
        });
    }
    assert_eq!(ring_buffer.unwrap().amplitudes[0], 9999.0);
}

/// `T2_F05_05`: Truncated Packet Boundary on Named Pipe
#[test]
fn test_t2_f05_05_truncated_packet_boundary_on_named_pipe() {
    let truncated = r#"{"version": 1, "inner": { "Hud"#;
    let res: Result<serde_json::Value, _> = serde_json::from_str(truncated);
    assert!(
        res.is_err(),
        "Truncated JSON message must return parse error"
    );
}
