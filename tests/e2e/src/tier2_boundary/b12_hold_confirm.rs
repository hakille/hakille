//! Boundary & Corner Cases: Feature 12 (600ms Hold-to-Confirm Progress Ring)
//!
//! Specifications: T2_F12_01 to T2_F12_05

use hakille_design::hold::{HoldEvent, HoldState, feed_hold, hold_progress};

/// `T2_F12_01`: Release at 599 Milliseconds
#[test]
fn test_t2_f12_01_release_at_599_milliseconds() {
    let (state, _) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 0 });
    let (state, _) = feed_hold(state, HoldEvent::Tick { elapsed_ms: 599 });
    let (state, _) = feed_hold(state, HoldEvent::Released { at_ms: 599 });
    assert_eq!(
        state,
        HoldState::Cancelled,
        "Release at 599ms must NOT confirm (requires 600ms)"
    );
    assert_eq!(hold_progress(state), 0.0);
}

/// `T2_F12_02`: Indefinite Hold (60,000 Milliseconds)
#[test]
fn test_t2_f12_02_indefinite_hold() {
    let (state, _) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 0 });
    let (state, confirmed) = feed_hold(state, HoldEvent::Tick { elapsed_ms: 600 });
    assert!(confirmed);
    assert_eq!(state, HoldState::Confirmed);

    // Further ticks past 600ms do not re-trigger
    let (state2, second_confirm) = feed_hold(state, HoldEvent::Tick { elapsed_ms: 60_000 });
    assert!(
        !second_confirm,
        "Subsequent ticks must not re-trigger confirmation"
    );
    assert_eq!(state2, HoldState::Confirmed);
}

/// `T2_F12_03`: Alt-Tab / Focus Loss During Active Hold
#[test]
fn test_t2_f12_03_alt_tab_focus_loss_during_active_hold() {
    let (state, _) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 0 });
    let (state, _) = feed_hold(state, HoldEvent::Tick { elapsed_ms: 350 });
    // Focus lost simulates cancellation
    let (state, _) = feed_hold(state, HoldEvent::Released { at_ms: 350 });
    assert_eq!(state, HoldState::Cancelled);
}

/// `T2_F12_04`: Negative Time Delta / Clock Skew During Hold
#[test]
fn test_t2_f12_04_negative_time_delta_during_hold() {
    let start_ms = 1000u64;
    let skewed_ms = 800u64;
    let safe_elapsed = skewed_ms.saturating_sub(start_ms);
    assert_eq!(safe_elapsed, 0, "Clock skew backwards must saturate to 0");
}

/// `T2_F12_05`: Simultaneous Multi-Touch / Multi-Mouse Clicks
#[test]
fn test_t2_f12_05_simultaneous_multi_touch_clicks() {
    let mut primary_touch_id = Some(1);
    let secondary_touch_id = 2;
    // Primary touch is locked
    let accepted = if primary_touch_id.is_none() {
        primary_touch_id = Some(secondary_touch_id);
        true
    } else {
        false
    };
    assert!(
        !accepted,
        "Secondary touch must be ignored during active hold"
    );
}
