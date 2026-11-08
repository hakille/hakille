//! Integration and validation tests for Aura Ring HUD Ambiant (Pillar 5).
//!
//! Validates:
//! 1. Send + Sync thread safety of AuraRingWidget, AuraVisualState, AuraColorRgba, and AuraRingUpdateDto.
//! 2. Exact chromatic state transitions across the full spectrum of tau values.
//! 3. Real-world scenario lifecycle (Cold boot -> Ambient -> Step-Up -> High Trust -> Inactivity Decay -> Distress/Lock).
//! 4. Hardware animation factor bounds [0.95, 1.05] across all 4 periodic rates.
//! 5. Complete JSON & Bincode roundtrip serialization of DTOs and KernelToUi events.

use hakille_ui::aura_ring::{AuraColorRgba, AuraRingWidget, AuraVisualState};
use hakille_ui_protocol::{AuraRingUpdateDto, KernelMessage, KernelToUi};

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn test_aura_ring_thread_safety_send_sync() {
    assert_send_sync::<AuraColorRgba>();
    assert_send_sync::<AuraVisualState>();
    assert_send_sync::<AuraRingWidget>();
    assert_send_sync::<AuraRingUpdateDto>();
}

#[test]
fn test_aura_ring_full_scenario_lifecycle() {
    // Stage 1: Cold Boot - unauthenticated, tau = 0.0 -> RedLocked
    let mut ring = AuraRingWidget::new(0.0);
    assert_eq!(ring.visual_state(), AuraVisualState::RedLocked);
    assert_eq!(ring.primary_hex(), "#EF4444");
    assert_eq!(ring.pulse_period_ms(), 800);
    assert_eq!(ring.glow_alpha(), 0.70);

    // Stage 2: Passive Biometric Presence - user recognized, tau = 0.75 -> AmberAmbient
    ring.update_confidence(0.75);
    assert_eq!(ring.visual_state(), AuraVisualState::AmberAmbient);
    assert_eq!(ring.primary_hex(), "#F59E0B");
    assert_eq!(ring.pulse_period_ms(), 3500);
    assert_eq!(ring.glow_alpha(), 0.30);

    // Stage 3: Privileged Action Step-Up - liveness challenge initiated -> VioletStepUp
    ring.set_step_up_active(true);
    assert_eq!(ring.visual_state(), AuraVisualState::VioletStepUp);
    assert_eq!(ring.primary_hex(), "#8B5CF6");
    assert_eq!(ring.pulse_period_ms(), 1000);
    assert_eq!(ring.glow_alpha(), 0.60);

    // Stage 4: Challenge Passed & Privilege Escalated - tau = 0.95, challenge closed -> EmeraldHighTrust
    ring.set_step_up_active(false);
    ring.update_confidence(0.95);
    assert_eq!(ring.visual_state(), AuraVisualState::EmeraldHighTrust);
    assert_eq!(ring.primary_hex(), "#10B981");
    assert_eq!(ring.pulse_period_ms(), 6000);
    assert_eq!(ring.glow_alpha(), 0.45);

    // Stage 5: Confidence Decay - tau slowly decays to 0.60 -> AmberAmbient
    ring.update_confidence(0.60);
    assert_eq!(ring.visual_state(), AuraVisualState::AmberAmbient);

    // Stage 6: Covert Distress Sequence / Shoulder-Surfing Threat -> RedLocked
    ring.set_distress(true);
    assert_eq!(ring.visual_state(), AuraVisualState::RedLocked);

    // Distress active overrides any tau elevation
    ring.update_confidence(1.0);
    assert_eq!(ring.visual_state(), AuraVisualState::RedLocked);

    // Clear distress -> returns to evaluated trust (tau = 1.0 -> EmeraldHighTrust)
    ring.set_distress(false);
    assert_eq!(ring.visual_state(), AuraVisualState::EmeraldHighTrust);
}

#[test]
fn test_aura_ring_boundary_precision() {
    let mut widget = AuraRingWidget::new(0.5);

    // Exactly at threshold 0.90 -> EmeraldHighTrust
    widget.update_confidence(0.9000);
    assert_eq!(widget.visual_state(), AuraVisualState::EmeraldHighTrust);

    // Just below 0.90 (0.89999) -> AmberAmbient
    widget.update_confidence(0.89999);
    assert_eq!(widget.visual_state(), AuraVisualState::AmberAmbient);

    // Exactly at threshold 0.40 -> AmberAmbient
    widget.update_confidence(0.4000);
    assert_eq!(widget.visual_state(), AuraVisualState::AmberAmbient);

    // Just below 0.40 (0.39999) -> RedLocked
    widget.update_confidence(0.39999);
    assert_eq!(widget.visual_state(), AuraVisualState::RedLocked);
}

#[test]
fn test_aura_ring_all_periods_animation_factor_bounds() {
    let states = [
        AuraVisualState::EmeraldHighTrust,
        AuraVisualState::AmberAmbient,
        AuraVisualState::VioletStepUp,
        AuraVisualState::RedLocked,
    ];

    for state in states {
        let period = state.pulse_period_ms();
        assert!(period > 0);

        // Check 1000 sample points throughout multiple cycles
        for step in 0..1000 {
            let t = (step as u64) * (period as u64 / 50);
            let factor = state.animation_factor(t);
            assert!(
                (0.9499..=1.0501).contains(&factor),
                "State {:?} factor {factor} outside [0.95, 1.05] at t={t}",
                state
            );
        }

        // Check wave crest at t = period / 4
        let crest = state.animation_factor(period as u64 / 4);
        assert!(
            (crest - 1.05).abs() < 1e-4,
            "Crest expected ~1.05, got {crest} for {:?}",
            state
        );

        // Check wave trough at t = 3 * period / 4
        let trough = state.animation_factor(3 * period as u64 / 4);
        assert!(
            (trough - 0.95).abs() < 1e-4,
            "Trough expected ~0.95, got {trough} for {:?}",
            state
        );
    }
}

#[test]
fn test_aura_ring_color_formatting_css_and_hex() {
    let emerald_primary = AuraVisualState::EmeraldHighTrust.primary_color();
    assert_eq!(emerald_primary.to_hex_rgb(), "#10B981");
    assert_eq!(emerald_primary.to_hex_rgba(), "#10B981FF");
    assert_eq!(emerald_primary.to_css_rgba(), "rgba(16, 185, 129, 1.00)");

    let emerald_glow = AuraVisualState::EmeraldHighTrust.glow_color();
    assert_eq!(emerald_glow.to_hex_rgba(), "#10B98173");
    assert_eq!(emerald_glow.to_css_rgba(), "rgba(16, 185, 129, 0.45)");

    let amber_glow = AuraVisualState::AmberAmbient.glow_color();
    assert_eq!(amber_glow.to_hex_rgba(), "#F59E0B4D");
    assert_eq!(amber_glow.to_css_rgba(), "rgba(245, 158, 11, 0.30)");

    let violet_glow = AuraVisualState::VioletStepUp.glow_color();
    assert_eq!(violet_glow.to_hex_rgba(), "#8B5CF699");
    assert_eq!(violet_glow.to_css_rgba(), "rgba(139, 92, 246, 0.60)");

    let red_glow = AuraVisualState::RedLocked.glow_color();
    assert_eq!(red_glow.to_hex_rgba(), "#EF4444B3");
    assert_eq!(red_glow.to_css_rgba(), "rgba(239, 68, 68, 0.70)");
}

#[test]
fn test_kernel_to_ui_aura_ring_update_end_to_end() {
    let widget = AuraRingWidget::new(0.96);
    let now_ms = 1794078950000;
    let dto = widget.to_update_dto(now_ms);

    let kernel_msg = KernelMessage::new(KernelToUi::AuraRingUpdate(dto.clone()));
    assert!(kernel_msg.check().is_ok());

    // 1. Serde JSON end-to-end
    let json = serde_json::to_string(&kernel_msg).unwrap();
    let back_json: KernelMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(kernel_msg, back_json);

    // 2. Bincode end-to-end
    let bin = bincode::serialize(&kernel_msg).unwrap();
    let back_bin: KernelMessage = bincode::deserialize(&bin).unwrap();
    assert_eq!(kernel_msg, back_bin);

    // 3. Extract inner payload
    if let KernelToUi::AuraRingUpdate(unpacked) = back_bin.inner {
        assert_eq!(unpacked.tau, 0.96);
        assert_eq!(unpacked.state_name, "EmeraldHighTrust");
        assert_eq!(unpacked.primary_hex, "#10B981");
        assert_eq!(unpacked.glow_hex, "#10B98173");
        assert_eq!(unpacked.pulse_period_ms, 6000);
        assert_eq!(unpacked.timestamp_ms, now_ms);
        assert!(unpacked.is_high_trust());
    } else {
        panic!("Expected KernelToUi::AuraRingUpdate");
    }
}
