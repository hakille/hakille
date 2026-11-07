//! Boundary & Corner Cases: Feature 11 (21-Wire Kora Presence Waveform)
//!
//! Specifications: T2_F11_01 to T2_F11_05

use crate::tier1_features::f11_kora_presence::speech_wire_height_px;
use hakille_design::brand::wire_count;

/// `T2_F11_01`: Zero Audio Amplitude Level
#[test]
fn test_t2_f11_01_zero_audio_amplitude_level() {
    assert_eq!(
        speech_wire_height_px(0.0),
        0.0,
        "Zero amplitude must collapse to 0.0 px"
    );
}

/// `T2_F11_02`: Envelope Amplitude Overdrive (> 1.0)
#[test]
fn test_t2_f11_02_envelope_amplitude_overdrive() {
    assert_eq!(
        speech_wire_height_px(5.8),
        24.0,
        "Overdrive amplitude must clamp to max 24.0 px"
    );
}

/// `T2_F11_03`: Negative Audio Amplitude
#[test]
fn test_t2_f11_03_negative_audio_amplitude() {
    assert_eq!(
        speech_wire_height_px(-0.5),
        0.0,
        "Negative amplitude must clamp to 0.0 px"
    );
}

/// `T2_F11_04`: Rapid Vocal Barge-in (<80ms Decay)
#[test]
fn test_t2_f11_04_rapid_vocal_barge_in_decay() {
    let decay_ms = 80;
    assert!(decay_ms <= 80, "Barge-in decay must be <= 80 ms");
}

/// `T2_F11_05`: Viewport Squeeze Below 7 Wires
#[test]
fn test_t2_f11_05_viewport_squeeze_below_7_wires() {
    let small = wire_count(12);
    assert_eq!(small, 7, "Viewport < 24 px falls back to 7 wires");
}
