//! Feature 08: 600ms Hold-to-Confirm (Milestone M1 / Spec §3, §8)
//!
//! Specifications: T1_F08_01 to T1_F08_05
//!
//! Verifies:
//! - Early release cancellation (< 600 ms cancels immediately, no confirmation tick).
//! - 600 ms hold threshold confirmation with terminal haptic/audio tick.
//! - Continuous progress metering function (0.0 to 1.0).
//! - MobileBridge approval initialization (`begin_approval` setting 600 ms threshold).
//! - Immediate premature approval rejection (< 600 ms real elapsed time rejected).

use crate::harness::{DaemonStep, MobileTestRig, default_card};
use hakille_design::hold::{HoldEvent, HoldState, feed_hold, hold_progress};
use hakille_ui_protocol::KernelToUi;
use serde_json::json;

/// `T1_F08_01`: Early release cancels without confirmation.
///
/// If touch release occurs at 599 ms (1 ms short of threshold), the gesture
/// cancels immediately and produces no confirmation action or tick.
#[test]
fn test_t1_f08_01_early_release_cancels_without_confirmation() {
    let (s0, tick0) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 0 });
    assert_eq!(s0, HoldState::Holding { elapsed_ms: 0 });
    assert!(tick0, "Initial press emits haptic onset tick");

    // Advance to 599 ms
    let (s1, tick1) = feed_hold(s0, HoldEvent::Tick { elapsed_ms: 599 });
    assert_eq!(s1, HoldState::Holding { elapsed_ms: 599 });
    assert!(!tick1, "Intermediate progression emits no tick");

    // Release at 599 ms
    let (s2, tick2) = feed_hold(s1, HoldEvent::Released { at_ms: 599 });
    assert_eq!(
        s2,
        HoldState::Cancelled,
        "Premature release cancels gesture"
    );
    assert!(!tick2, "Cancelled gesture emits no confirmation tick");
}

/// `T1_F08_02`: Full 600 ms threshold confirms with terminal tick.
///
/// Sustaining touch for exactly 600 ms transitions state to Confirmed
/// and produces a final haptic/audio feedback tick.
#[test]
fn test_t1_f08_02_full_600ms_confirms_with_tick() {
    let (s0, _) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 0 });

    // Tick to 600 ms
    let (s_confirmed, tick_confirmed) = feed_hold(s0, HoldEvent::Tick { elapsed_ms: 600 });
    assert_eq!(
        s_confirmed,
        HoldState::Confirmed,
        "600 ms hold must confirm"
    );
    assert!(
        tick_confirmed,
        "Confirmation must trigger terminal tick feedback"
    );

    // Further ticks while Confirmed remain Confirmed without extra ticks
    let (s_noop, tick_noop) = feed_hold(s_confirmed, HoldEvent::Tick { elapsed_ms: 50 });
    assert_eq!(s_noop, HoldState::Confirmed);
    assert!(!tick_noop);
}

/// `T1_F08_03`: Continuous progress metering function.
///
/// hold_progress accurately maps elapsed hold time into a normalized [0.0, 1.0]
/// fraction for rendering the braise arc.
#[test]
fn test_t1_f08_03_hold_progress_metering() {
    assert_eq!(hold_progress(HoldState::Idle), 0.0);
    assert_eq!(hold_progress(HoldState::Cancelled), 0.0);
    assert_eq!(hold_progress(HoldState::Confirmed), 1.0);

    assert_eq!(hold_progress(HoldState::Holding { elapsed_ms: 0 }), 0.0);
    assert!(
        (hold_progress(HoldState::Holding { elapsed_ms: 300 }) - 0.5).abs() < 1e-4,
        "300 ms must equal 50% progress"
    );
    assert!(
        (hold_progress(HoldState::Holding { elapsed_ms: 599 }) - (599.0 / 600.0)).abs() < 1e-4,
        "599 ms must equal 99.83% progress"
    );
}

/// `T1_F08_04`: MobileBridge approval initialization.
///
/// begin_approval initializes snapshot.approval targeting card_id with required_hold_ms 600.
#[tokio::test]
async fn test_t1_f08_04_mobile_bridge_approval_initialization() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        let step = DaemonStep::status().after(vec![KernelToUi::ShowCard(default_card(88))]);
        daemon
            .handle_step(&mut stream, 1, step)
            .await
            .expect("status with card");
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;
    daemon_task.await.unwrap();

    let begin_res_str = bridge.dispatch_json(
        json!({
            "type": "begin_approval",
            "card_id": 88,
        })
        .to_string(),
    );

    let begin_res: serde_json::Value = serde_json::from_str(&begin_res_str).unwrap();
    assert_eq!(begin_res["ok"], true);

    let snapshot = rig.snapshot(&bridge);
    let approval = &snapshot["approval"];
    assert!(!approval.is_null());
    assert_eq!(approval["card_id"], 88);
    assert_eq!(approval["required_hold_ms"], 600);
}

/// `T1_F08_05`: Immediate premature approval without elapsed hold is strictly rejected.
///
/// Dispatching approve immediately after begin_approval (elapsed << 600 ms)
/// is strictly rejected by the mobile core and resets the approval state.
#[tokio::test]
async fn test_t1_f08_05_immediate_approval_without_elapsed_hold_strictly_rejected() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        let step = DaemonStep::status().after(vec![KernelToUi::ShowCard(default_card(88))]);
        daemon
            .handle_step(&mut stream, 1, step)
            .await
            .expect("status with card");
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;
    daemon_task.await.unwrap();

    // 1. Begin approval
    bridge.dispatch_json(
        json!({
            "type": "begin_approval",
            "card_id": 88,
        })
        .to_string(),
    );

    // 2. Immediate approve without waiting 600 ms
    let approve_res_str = bridge.dispatch_json(
        json!({
            "type": "approve",
            "card_id": 88,
        })
        .to_string(),
    );

    let approve_res: serde_json::Value = serde_json::from_str(&approve_res_str).unwrap();
    assert_eq!(approve_res["ok"], false);
    assert!(
        approve_res["error"]
            .as_str()
            .unwrap()
            .contains("Maintien continu de 600 ms requis"),
        "Must enforce 600 ms hold: {approve_res_str}"
    );

    // Approval state must be cancelled
    let snapshot = rig.snapshot(&bridge);
    assert!(
        snapshot["approval"].is_null(),
        "Premature approve must cancel active approval state"
    );
}
