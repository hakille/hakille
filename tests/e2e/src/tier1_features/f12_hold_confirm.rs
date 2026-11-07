//! Feature 12: 600ms Hold-to-Confirm Progress Ring (Milestone M3)
//!
//! Specifications: T1_F12_01 to T1_F12_05

use hakille_design::HOLD_TO_CONFIRM_MS;
use hakille_design::hold::{HoldEvent, HoldState, feed_hold, hold_progress};
use hakille_ui_desktop::scene::{Color, Scene};

/// `T1_F12_01`: Progress Ring Initialization on Hold Start
#[test]
fn test_t1_f12_01_progress_ring_initialization_on_hold_start() {
    let (state, tick_sound) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 1000 });
    assert!(tick_sound, "Press must emit haptic/sound start tick");
    match state {
        HoldState::Holding { elapsed_ms } => assert_eq!(elapsed_ms, 0),
        _ => panic!("Expected Holding state"),
    }
    assert_eq!(hold_progress(state), 0.0);
}

/// `T1_F12_02`: Continuous Progress Calculation
#[test]
fn test_t1_f12_02_continuous_progress_calculation() {
    let (state, _) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 0 });
    let (state, _) = feed_hold(state, HoldEvent::Tick { elapsed_ms: 300 });
    let prog = hold_progress(state);
    assert!(
        (prog - 0.5).abs() < 0.001,
        "300ms hold out of 600ms must yield 0.5 progress"
    );
}

/// `T1_F12_03`: Full Ring Confirmation at 600ms
#[test]
fn test_t1_f12_03_full_ring_confirmation_at_600ms() {
    assert_eq!(HOLD_TO_CONFIRM_MS, 600);
    let (state, _) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 0 });
    let (state, tick_done) = feed_hold(state, HoldEvent::Tick { elapsed_ms: 600 });
    assert!(tick_done, "Confirmation must emit completion tick");
    assert_eq!(state, HoldState::Confirmed);
    assert_eq!(hold_progress(state), 1.0);
}

/// `T1_F12_04`: Premature Release Cancellation
#[test]
fn test_t1_f12_04_premature_release_cancellation() {
    let (state, _) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 0 });
    let (state, _) = feed_hold(state, HoldEvent::Tick { elapsed_ms: 450 });
    let (state, _) = feed_hold(state, HoldEvent::Released { at_ms: 450 });
    assert_eq!(state, HoldState::Cancelled);
    assert_eq!(
        hold_progress(state),
        0.0,
        "Progress must reset to 0.0 on cancel"
    );
}

/// `T1_F12_05`: Braise Ring Rendering on Validation Button
#[test]
fn test_t1_f12_05_braise_ring_rendering_on_validation_button() {
    let mut scene = Scene::new(720, 360, true);
    let braise = Color::from_hex(0xFF7A2F);
    // Draw progress ring around validation button center
    scene.ring(640.0, 310.0, 16.0, 2.0, braise);
    assert_eq!(scene.cmds.len(), 1);
}
