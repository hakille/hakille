//! Empirical Boundary and Stress Tests for Pillar 5 Aura Ring.
//!
//! Evaluates exact numerical confidence boundaries and security priorities:
//! - Boundary confidence tau = 0.8999 vs 0.9000 (AmberAmbient vs EmeraldHighTrust)
//! - Boundary confidence tau = 0.3999 vs 0.4000 (RedLocked vs AmberAmbient)
//! - Fail-closed lock priority over step-up (RedLocked strictly dominates VioletStepUp)

use hakille_ui::aura_ring::{AuraRingWidget, AuraVisualState};

// -----------------------------------------------------------------------------
// Pillar 5: Boundary Confidence tau = 0.8999 vs 0.9000
// -----------------------------------------------------------------------------

#[test]
fn test_pillar5_boundary_confidence_tau_0_8999_vs_0_9000() {
    // Exactly 0.8999: Just below high trust threshold (0.90) -> AmberAmbient
    let widget_8999 = AuraRingWidget::new(0.8999);
    assert_eq!(
        widget_8999.visual_state(),
        AuraVisualState::AmberAmbient,
        "tau = 0.8999 must be AmberAmbient"
    );
    assert_eq!(widget_8999.primary_hex(), "#F59E0B");
    assert_eq!(widget_8999.pulse_period_ms(), 3500);

    let dto_8999 = widget_8999.to_update_dto(1000);
    assert_eq!(dto_8999.state_name, "AmberAmbient");
    assert_eq!(dto_8999.primary_hex, "#F59E0B");
    assert_eq!(dto_8999.pulse_period_ms, 3500);

    // Exactly 0.9000: High trust threshold reached -> EmeraldHighTrust
    let widget_9000 = AuraRingWidget::new(0.9000);
    assert_eq!(
        widget_9000.visual_state(),
        AuraVisualState::EmeraldHighTrust,
        "tau = 0.9000 must be EmeraldHighTrust"
    );
    assert_eq!(widget_9000.primary_hex(), "#10B981");
    assert_eq!(widget_9000.pulse_period_ms(), 6000);

    let dto_9000 = widget_9000.to_update_dto(1000);
    assert_eq!(dto_9000.state_name, "EmeraldHighTrust");
    assert_eq!(dto_9000.primary_hex, "#10B981");
    assert_eq!(dto_9000.pulse_period_ms, 6000);

    // Dynamic transition via update_confidence
    let mut widget = AuraRingWidget::new(0.8999);
    assert_eq!(widget.visual_state(), AuraVisualState::AmberAmbient);
    widget.update_confidence(0.9000);
    assert_eq!(widget.visual_state(), AuraVisualState::EmeraldHighTrust);
    widget.update_confidence(0.8999);
    assert_eq!(widget.visual_state(), AuraVisualState::AmberAmbient);
}

// -----------------------------------------------------------------------------
// Pillar 5: Boundary Confidence tau = 0.3999 vs 0.4000
// -----------------------------------------------------------------------------

#[test]
fn test_pillar5_boundary_confidence_tau_0_3999_vs_0_4000() {
    // Exactly 0.3999: Insufficient confidence (< 0.40) -> RedLocked
    let widget_3999 = AuraRingWidget::new(0.3999);
    assert_eq!(
        widget_3999.visual_state(),
        AuraVisualState::RedLocked,
        "tau = 0.3999 must be RedLocked"
    );
    assert_eq!(widget_3999.primary_hex(), "#EF4444");
    assert_eq!(widget_3999.pulse_period_ms(), 800);

    let dto_3999 = widget_3999.to_update_dto(1000);
    assert_eq!(dto_3999.state_name, "RedLocked");
    assert_eq!(dto_3999.primary_hex, "#EF4444");
    assert_eq!(dto_3999.pulse_period_ms, 800);

    // Exactly 0.4000: Minimum nominal ambient threshold reached -> AmberAmbient
    let widget_4000 = AuraRingWidget::new(0.4000);
    assert_eq!(
        widget_4000.visual_state(),
        AuraVisualState::AmberAmbient,
        "tau = 0.4000 must be AmberAmbient"
    );
    assert_eq!(widget_4000.primary_hex(), "#F59E0B");
    assert_eq!(widget_4000.pulse_period_ms(), 3500);

    let dto_4000 = widget_4000.to_update_dto(1000);
    assert_eq!(dto_4000.state_name, "AmberAmbient");
    assert_eq!(dto_4000.primary_hex, "#F59E0B");
    assert_eq!(dto_4000.pulse_period_ms, 3500);

    // Dynamic transition via update_confidence
    let mut widget = AuraRingWidget::new(0.4000);
    assert_eq!(widget.visual_state(), AuraVisualState::AmberAmbient);
    widget.update_confidence(0.3999);
    assert_eq!(widget.visual_state(), AuraVisualState::RedLocked);
    widget.update_confidence(0.4000);
    assert_eq!(widget.visual_state(), AuraVisualState::AmberAmbient);
}

// -----------------------------------------------------------------------------
// Pillar 5: Fail-Closed Lock Priority Over Step-Up
// -----------------------------------------------------------------------------

#[test]
fn test_pillar5_fail_closed_lock_priority_over_step_up() {
    let mut widget = AuraRingWidget::new(0.95);
    assert_eq!(widget.visual_state(), AuraVisualState::EmeraldHighTrust);

    // 1. Activate Step-Up (liveness challenge in progress) -> VioletStepUp
    widget.set_step_up_active(true);
    assert_eq!(
        widget.visual_state(),
        AuraVisualState::VioletStepUp,
        "Step-up active under nominal tau must show VioletStepUp"
    );
    assert_eq!(widget.primary_hex(), "#8B5CF6");
    assert_eq!(widget.pulse_period_ms(), 1000);

    // 2. Concurrently engage manual Lock -> RedLocked MUST OVERRIDE VioletStepUp
    widget.set_locked(true);
    assert_eq!(
        widget.visual_state(),
        AuraVisualState::RedLocked,
        "is_locked=true must strictly override is_step_up_active=true (fail-closed priority)"
    );
    assert_eq!(widget.primary_hex(), "#EF4444");
    assert_eq!(widget.pulse_period_ms(), 800);

    // 3. Clear Lock while step-up is still active -> Returns to VioletStepUp
    widget.set_locked(false);
    assert_eq!(
        widget.visual_state(),
        AuraVisualState::VioletStepUp,
        "Clearing lock while step-up is still active must return to VioletStepUp"
    );

    // 4. Trigger distress (Silent Distress Blink) while step-up is active -> RedLocked MUST OVERRIDE
    widget.set_distress(true);
    assert_eq!(
        widget.visual_state(),
        AuraVisualState::RedLocked,
        "is_distress=true must strictly override is_step_up_active=true"
    );
    widget.set_distress(false);
    assert_eq!(widget.visual_state(), AuraVisualState::VioletStepUp);

    // 5. Trigger cryptographic revocation while step-up is active -> RedLocked MUST OVERRIDE
    widget.set_revoked(true);
    assert_eq!(
        widget.visual_state(),
        AuraVisualState::RedLocked,
        "is_revoked=true must strictly override is_step_up_active=true"
    );
    widget.set_revoked(false);
    assert_eq!(widget.visual_state(), AuraVisualState::VioletStepUp);

    // 6. Degrade tau to 0.3999 while step-up is active -> RedLocked MUST OVERRIDE
    widget.update_confidence(0.3999);
    assert_eq!(
        widget.visual_state(),
        AuraVisualState::RedLocked,
        "tau < 0.40 must strictly override is_step_up_active=true (fail-closed priority)"
    );

    // Elevate tau back to 0.95 -> returns to VioletStepUp since step-up is still active
    widget.update_confidence(0.95);
    assert_eq!(widget.visual_state(), AuraVisualState::VioletStepUp);

    // Deactivate step-up -> returns to EmeraldHighTrust
    widget.set_step_up_active(false);
    assert_eq!(widget.visual_state(), AuraVisualState::EmeraldHighTrust);
}
