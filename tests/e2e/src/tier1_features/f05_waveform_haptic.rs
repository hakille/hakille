//! Feature 05: Protocol Waveform & Haptic Schemas (Milestone M2)
//!
//! Specifications: T1_F05_01 to T1_F05_05

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct VoiceWaveformPayload {
    pub amplitudes: [f32; 21],
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum HapticPattern {
    ShortPulse,
    DoubleTap,
    SubtleVibe,
    UrgentBuzz,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HapticTriggerPayload {
    pub pattern: HapticPattern,
    pub intensity_pct: u8,
}

/// `T1_F05_01`: Waveform Telemetry Serialization
///
/// Asserts serialization produces JSON containing 21 amplitude floats.
#[test]
fn test_t1_f05_01_waveform_telemetry_serialization() {
    let payload = VoiceWaveformPayload {
        amplitudes: [0.5; 21],
    };
    let json = serde_json::to_string(&payload).expect("Serialization failed");
    assert!(
        json.contains("amplitudes"),
        "JSON must contain amplitudes key"
    );

    let val: serde_json::Value = serde_json::from_str(&json).unwrap();
    let arr = val["amplitudes"]
        .as_array()
        .expect("amplitudes must be array");
    assert_eq!(arr.len(), 21, "Must contain exactly 21 floats");
}

/// `T1_F05_02`: Haptic Trigger Serialization
///
/// Asserts serialization of HapticTrigger with pattern UrgentBuzz and intensity 100%.
#[test]
fn test_t1_f05_02_haptic_trigger_serialization() {
    let payload = HapticTriggerPayload {
        pattern: HapticPattern::UrgentBuzz,
        intensity_pct: 100,
    };
    let json = serde_json::to_string(&payload).expect("Serialization failed");
    assert!(json.contains("UrgentBuzz"));
    assert!(json.contains("100"));
}

/// `T1_F05_03`: Waveform Telemetry Deserialization
///
/// Asserts JSON array of 21 floats successfully deserializes into VoiceWaveform.
#[test]
fn test_t1_f05_03_waveform_telemetry_deserialization() {
    let raw = r#"{"amplitudes": [0.0, 0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.45, 0.5, 0.55, 0.6, 0.65, 0.7, 0.75, 0.8, 0.85, 0.9, 0.95, 1.0]}"#;
    let payload: VoiceWaveformPayload = serde_json::from_str(raw).expect("Deserialization failed");
    assert_eq!(payload.amplitudes.len(), 21);
    assert!((payload.amplitudes[0] - 0.0).abs() < 0.001);
    assert!((payload.amplitudes[20] - 1.0).abs() < 0.001);
}

/// `T1_F05_04`: Haptic Pattern Variants Exhaustiveness
///
/// Asserts all 4 haptic patterns serialize and deserialize losslessly.
#[test]
fn test_t1_f05_04_haptic_pattern_variants_exhaustiveness() {
    let patterns = [
        HapticPattern::ShortPulse,
        HapticPattern::DoubleTap,
        HapticPattern::SubtleVibe,
        HapticPattern::UrgentBuzz,
    ];

    for pattern in patterns {
        let payload = HapticTriggerPayload {
            pattern: pattern.clone(),
            intensity_pct: 80,
        };
        let json = serde_json::to_string(&payload).unwrap();
        let decoded: HapticTriggerPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.pattern, pattern);
        assert_eq!(decoded.intensity_pct, 80);
    }
}

/// `T1_F05_05`: Precision Preservation in Waveform Floats
///
/// Asserts float precision is preserved to at least 4 decimal places across wire roundtrip.
#[test]
fn test_t1_f05_05_precision_preservation_in_waveform_floats() {
    let mut original = [0.0f32; 21];
    for (i, val) in original.iter_mut().enumerate() {
        *val = (i as f32 * 0.0476).min(1.0);
    }
    let payload = VoiceWaveformPayload {
        amplitudes: original,
    };
    let json = serde_json::to_string(&payload).unwrap();
    let decoded: VoiceWaveformPayload = serde_json::from_str(&json).unwrap();

    for i in 0..21 {
        let diff = (original[i] - decoded.amplitudes[i]).abs();
        assert!(
            diff < 0.0001,
            "Precision loss at index {}: original {}, decoded {}",
            i,
            original[i],
            decoded.amplitudes[i]
        );
    }
}
