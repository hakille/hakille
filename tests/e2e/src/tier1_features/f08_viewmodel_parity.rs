//! Feature 08: Viewmodel & Snapshot Parity (Milestone M2)
//!
//! Specifications: T1_F08_01 to T1_F08_05

use hakille_ui::{
    CostCounter, EphemeralHud, InitiativeOrigin, PresenceIndicator, PresenceState,
    SilenceBudgetTracker,
};
use hakille_ui_model::StableSnapshot;

fn make_test_snapshot(state: PresenceState) -> StableSnapshot {
    let mut presence = PresenceIndicator::new();
    presence.set_state(state);
    let hud = EphemeralHud::new();
    let cost = CostCounter::new(250, "EUR");
    let mut silence = SilenceBudgetTracker::new(3);
    silence.request_initiative(
        "test-1",
        InitiativeOrigin::SystemInferred,
        0.90,
        "Suggestion",
    );
    StableSnapshot::from_states(&presence, &hud, &cost, &silence, 1000, 0)
}

/// `T1_F08_01`: StableSnapshot Field Parity with Shell Snapshot
#[test]
fn test_t1_f08_01_stable_snapshot_field_parity_with_shell_snapshot() {
    let snap = make_test_snapshot(PresenceState::Idle);
    // Assert all major domain properties are projected cleanly
    assert_eq!(snap.presence_state, 0);
    assert_eq!(snap.cost_total_cents, 0);
    assert_eq!(snap.cost_budget_cents, 250);
    assert_eq!(snap.silence_quota, 3);
    assert_eq!(snap.silence_remaining, 2);
    assert!(!snap.hud_streaming);
}

/// `T1_F08_02`: Presence Telemetry Field Mapping
#[test]
fn test_t1_f08_02_presence_telemetry_field_mapping() {
    let snap = make_test_snapshot(PresenceState::Speaking);
    assert_eq!(
        snap.presence_state, 3,
        "PresenceState::Speaking index must be 3"
    );
    assert!(snap.presence_a11y_label.to_lowercase().contains("parole"));
}

/// `T1_F08_03`: HUD Streaming Text Projection
#[test]
fn test_t1_f08_03_hud_streaming_text_projection() {
    let presence = PresenceIndicator::new();
    let mut hud = EphemeralHud::new();
    hud.set_streaming("Query".to_string(), "Token1 Token2".to_string(), false);
    let cost = CostCounter::new(500, "EUR");
    let silence = SilenceBudgetTracker::new(3);

    let snap_streaming = StableSnapshot::from_states(&presence, &hud, &cost, &silence, 0, 0);
    assert!(snap_streaming.hud_streaming);
    assert_eq!(snap_streaming.hud_response, "Token1 Token2");

    hud.set_streaming("Query".to_string(), "Token1 Token2 Final".to_string(), true);
    let snap_final = StableSnapshot::from_states(&presence, &hud, &cost, &silence, 0, 0);
    assert!(
        !snap_final.hud_streaming,
        "Streaming must be false when reply finalized"
    );
    assert_eq!(snap_final.hud_response, "Token1 Token2 Final");
}

/// `T1_F08_04`: Silence Budget & Cost Level Projection
#[test]
fn test_t1_f08_04_silence_budget_and_cost_level_projection() {
    let presence = PresenceIndicator::new();
    let hud = EphemeralHud::new();
    let mut cost = CostCounter::new(1000, "EUR");
    cost.record_cost(250);
    let mut silence = SilenceBudgetTracker::new(3);
    silence.request_initiative(
        "test-2",
        InitiativeOrigin::SystemInferred,
        0.90,
        "Suggestion",
    );

    let snap = StableSnapshot::from_states(&presence, &hud, &cost, &silence, 0, 0);
    assert_eq!(snap.cost_total_cents, 250);
    assert_eq!(snap.silence_remaining, 2);
    assert_eq!(snap.silence_quota, 3);
}

/// `T1_F08_05`: Accessibility Preference Propagation
#[test]
fn test_t1_f08_05_accessibility_preference_propagation() {
    let snap = make_test_snapshot(PresenceState::Listening);
    assert!(snap.presence_pulse >= 0.0 && snap.presence_pulse <= 1.0);
}
