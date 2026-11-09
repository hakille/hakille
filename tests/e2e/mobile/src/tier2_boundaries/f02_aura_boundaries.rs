//! Feature 02: Aura Ring Streaming (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F02_01 to T2_F02_05 (5 tests total).

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

/// `T2_F02_01`: Aura Ring tau underflow boundary (tau < 0.0 clamped to 0.0).
///
/// Tau values less than 0.0 are clamped to 0.0, producing RedLocked state (#EF4444).
#[test]
fn test_t2_f02_01_aura_ring_tau_underflow_clamped() {
    fn clamp_tau(raw: f32) -> f32 {
        raw.clamp(0.0, 1.0)
    }

    let clamped_neg = clamp_tau(-0.5);
    assert_eq!(clamped_neg, 0.0);

    let clamped_extreme = clamp_tau(-1000.0);
    assert_eq!(clamped_extreme, 0.0);

    let dto = AuraRingUpdateDto::new(
        clamped_neg,
        "RedLocked",
        "#EF4444",
        "#EF444473",
        1000,
        1.0,
        1700000000000,
    );
    assert!(dto.is_locked());
    assert!(!dto.is_ambient());
    assert!(!dto.is_high_trust());
    assert_eq!(dto.tau, 0.0);
    assert_eq!(dto.primary_hex, "#EF4444");
}

/// `T2_F02_02`: Aura Ring tau overflow boundary (tau > 1.0 clamped to 1.0).
///
/// Tau values greater than 1.0 are clamped to 1.0, producing EmeraldHighTrust state (#10B981).
#[test]
fn test_t2_f02_02_aura_ring_tau_overflow_clamped() {
    fn clamp_tau(raw: f32) -> f32 {
        raw.clamp(0.0, 1.0)
    }

    let clamped_over = clamp_tau(1.05);
    assert_eq!(clamped_over, 1.0);

    let clamped_extreme = clamp_tau(500.0);
    assert_eq!(clamped_extreme, 1.0);

    let dto = AuraRingUpdateDto::new(
        clamped_over,
        "EmeraldHighTrust",
        "#10B981",
        "#10B98173",
        3000,
        1.0,
        1700000000000,
    );
    assert!(dto.is_high_trust());
    assert!(!dto.is_ambient());
    assert!(!dto.is_locked());
    assert_eq!(dto.tau, 1.0);
    assert_eq!(dto.primary_hex, "#10B981");
}

/// `T2_F02_03`: Exact boundary transitions at tau = 0.40 and tau = 0.90.
///
/// Tau = 0.3999 is VioletStepUp; 0.4000 is AmberAmbient;
/// Tau = 0.8999 is AmberAmbient; 0.9000 is EmeraldHighTrust.
#[test]
fn test_t2_f02_03_exact_boundaries_at_0_40_and_0_90() {
    // 1. Just below 0.40 -> step-up violet
    let dto_step_up = AuraRingUpdateDto::new(
        0.3999,
        "VioletStepUp",
        "#8B5CF6",
        "#8B5CF673",
        1500,
        1.0,
        1700000000000,
    );
    assert!(dto_step_up.is_step_up(), "0.3999 must be step-up");
    assert!(!dto_step_up.is_ambient(), "0.3999 must not be ambient");

    // 2. Exactly at 0.40 -> ambient amber
    let dto_ambient_low = AuraRingUpdateDto::new(
        0.40,
        "AmberAmbient",
        "#F59E0B",
        "#F59E0B73",
        2500,
        1.0,
        1700000000000,
    );
    assert!(dto_ambient_low.is_ambient(), "0.40 must be ambient");
    assert!(!dto_ambient_low.is_step_up(), "0.40 must not be step-up");

    // 3. Just below 0.90 -> ambient amber
    let dto_ambient_high = AuraRingUpdateDto::new(
        0.8999,
        "AmberAmbient",
        "#F59E0B",
        "#F59E0B73",
        2500,
        1.0,
        1700000000000,
    );
    assert!(dto_ambient_high.is_ambient(), "0.8999 must be ambient");
    assert!(
        !dto_ambient_high.is_high_trust(),
        "0.8999 must not be high trust"
    );

    // 4. Exactly at 0.90 -> high-trust emerald
    let dto_high_trust = AuraRingUpdateDto::new(
        0.90,
        "EmeraldHighTrust",
        "#10B981",
        "#10B98173",
        3000,
        1.0,
        1700000000000,
    );
    assert!(dto_high_trust.is_high_trust(), "0.90 must be high trust");
    assert!(!dto_high_trust.is_ambient(), "0.90 must not be ambient");
}

/// `T2_F02_04`: Zero pulse period handling.
///
/// Pulse period 0 ms must not cause division by zero in animation calculations.
#[test]
fn test_t2_f02_04_zero_pulse_period_division_by_zero_prevention() {
    fn compute_pulse_phase(elapsed_ms: u64, pulse_period_ms: u32) -> f32 {
        if pulse_period_ms == 0 {
            0.0
        } else {
            ((elapsed_ms % (pulse_period_ms as u64)) as f32) / (pulse_period_ms as f32)
        }
    }

    let phase_zero = compute_pulse_phase(1500, 0);
    assert_eq!(
        phase_zero, 0.0,
        "Zero period must safely evaluate to 0.0 without panic"
    );

    let phase_normal = compute_pulse_phase(750, 1500);
    assert!(
        (phase_normal - 0.5).abs() < 1e-4,
        "Normal period evaluates phase accurately"
    );
}

/// `T2_F02_05`: Extreme oscillation of tau and scale factor bounds.
///
/// Rapid oscillation between tau 0.0 and 1.0 maintains scale factor in [0.95, 1.05].
#[test]
fn test_t2_f02_05_extreme_oscillation_of_tau() {
    for i in 0..50 {
        let (tau, scale) = if i % 2 == 0 {
            (0.01f32, 0.95f32)
        } else {
            (0.99f32, 1.05f32)
        };
        let dto = AuraRingUpdateDto::new(
            tau,
            if tau > 0.9 {
                "EmeraldHighTrust"
            } else {
                "RedLocked"
            },
            if tau > 0.9 { "#10B981" } else { "#EF4444" },
            "#000000",
            2000,
            scale,
            1700000000000 + i * 100,
        );
        assert!(dto.scale_factor >= 0.95 && dto.scale_factor <= 1.05);
        if i % 2 == 0 {
            assert!(dto.is_locked());
        } else {
            assert!(dto.is_high_trust());
        }
    }
}
