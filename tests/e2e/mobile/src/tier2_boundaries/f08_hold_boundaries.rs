//! Feature 08: 600ms Hold-to-Confirm (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F08_01 to T2_F08_05 (5 tests total).

#![allow(unused_imports)]

use crate::harness::{DaemonStep, MobileTestRig, default_card};
use hakille_design::action_card::{ActionCardView, DataPastille, Reversibility};
use hakille_design::autonomy::{Crank, DomainAutonomy, ENVELOPE_EXPIRY_DAYS, always_validated};
use hakille_design::hold::{HoldEvent, HoldState, feed_hold, hold_progress};
use hakille_design::provenance::{ExternalBlock, ProvenanceBand, sanitize_skill_text};
use hakille_design::why::{FactRef, WhySheet};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{ActionCard, DataExit, KernelToUi, Money, Provenance, UiToKernel};
use serde_json::json;

/// `T2_F08_01`: Hold boundary: 599ms released -> cancelled, 600ms -> approved.
///
/// At 599ms release cancels without confirmation tick.
/// At 600ms hold confirms with terminal confirmation tick.
#[test]
fn test_t2_f08_01_hold_boundary_599ms_vs_600ms() {
    // 599 ms -> Cancelled
    let (s0, _) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 0 });
    let (s_599, _) = feed_hold(s0, HoldEvent::Tick { elapsed_ms: 599 });
    let (s_cancel, tick_cancel) = feed_hold(s_599, HoldEvent::Released { at_ms: 599 });
    assert_eq!(s_cancel, HoldState::Cancelled);
    assert!(!tick_cancel, "599ms release must emit no confirmation tick");

    // 600 ms -> Confirmed
    let (s0_b, _) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 0 });
    let (s_confirm, tick_confirm) = feed_hold(s0_b, HoldEvent::Tick { elapsed_ms: 600 });
    assert_eq!(s_confirm, HoldState::Confirmed);
    assert!(tick_confirm, "600ms hold must emit confirmation tick");
}

/// `T2_F08_02`: Rapid touch bounce (jitter < 50ms).
///
/// Rapid touch jitter (press at 0ms, release at 20ms, press at 30ms) resets hold
/// rather than accumulating previous jitter time.
#[test]
fn test_t2_f08_02_rapid_touch_bounce_jitter() {
    let (s0, _) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 0 });
    let (s1, _) = feed_hold(s0, HoldEvent::Tick { elapsed_ms: 20 });
    let (s2, _) = feed_hold(s1, HoldEvent::Released { at_ms: 20 });
    assert_eq!(s2, HoldState::Cancelled);

    // New press restarts from zero
    let (s3, _) = feed_hold(s2, HoldEvent::Pressed { at_ms: 30 });
    assert_eq!(s3, HoldState::Holding { elapsed_ms: 0 });

    // Tick of 570ms from new press does NOT confirm (elapsed is 570, not 570+20)
    let (s4, _) = feed_hold(s3, HoldEvent::Tick { elapsed_ms: 570 });
    assert_eq!(s4, HoldState::Holding { elapsed_ms: 570 });
}

/// `T2_F08_03`: Background event at 550ms cancels hold.
///
/// Moving app to background while holding cancels the gesture and clears approval.
#[test]
fn test_t2_f08_03_background_at_550ms_cancels_hold() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());

    // Inject card
    let req_res = bridge.dispatch_json(json!({"type": "refresh_status"}).to_string());
    let req_id = serde_json::from_str::<serde_json::Value>(&req_res).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();
    let reply = json!({
        "session_id": "sess-bg-hold",
        "frames": [
            Frame::reply(req_id, KernelToUi::StatusReply { status: crate::harness::default_status() }),
            Frame::reply(0, KernelToUi::ShowCard(default_card(55)))
        ]
    });
    bridge.receive_json(req_id, reply.to_string());

    // Begin hold
    bridge.dispatch_json(json!({"type": "begin_approval", "card_id": 55}).to_string());
    let snap_hold: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert!(snap_hold["approval"].is_object());

    // Background app
    bridge.dispatch_json(json!({"type": "set_foreground", "active": false}).to_string());
    let snap_bg: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert!(
        snap_bg["approval"].is_null(),
        "Approval must be cancelled on backgrounding"
    );
}

/// `T2_F08_04`: Multiple simultaneous holds switches target cleanly.
///
/// Invoking begin_approval on a second card cancels the hold on the first card.
#[test]
fn test_t2_f08_04_multiple_simultaneous_holds_switches_target() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());

    let req_res = bridge.dispatch_json(json!({"type": "refresh_status"}).to_string());
    let req_id = serde_json::from_str::<serde_json::Value>(&req_res).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();
    let reply = json!({
        "session_id": "sess-multi-hold",
        "frames": [
            Frame::reply(req_id, KernelToUi::StatusReply { status: crate::harness::default_status() }),
            Frame::reply(0, KernelToUi::ShowCard(default_card(10))),
            Frame::reply(0, KernelToUi::ShowCard(default_card(20)))
        ]
    });
    bridge.receive_json(req_id, reply.to_string());

    // Hold card 10
    bridge.dispatch_json(json!({"type": "begin_approval", "card_id": 10}).to_string());
    let snap1: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap1["approval"]["card_id"], 10);

    // Switch to hold card 20
    bridge.dispatch_json(json!({"type": "begin_approval", "card_id": 20}).to_string());
    let snap2: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap2["approval"]["card_id"], 20);
}

/// `T2_F08_05`: Hold progress clamped [0.0, 1.0].
///
/// hold_progress clamps properly: Idle/Cancelled -> 0.0; Confirmed -> 1.0; 600ms+ -> 1.0.
#[test]
fn test_t2_f08_05_hold_progress_clamped_bounds() {
    assert_eq!(hold_progress(HoldState::Idle), 0.0);
    assert_eq!(hold_progress(HoldState::Cancelled), 0.0);
    assert_eq!(hold_progress(HoldState::Confirmed), 1.0);
    assert_eq!(hold_progress(HoldState::Holding { elapsed_ms: 0 }), 0.0);
    assert_eq!(hold_progress(HoldState::Holding { elapsed_ms: 600 }), 1.0);
    assert_eq!(hold_progress(HoldState::Holding { elapsed_ms: 1000 }), 1.0);
}
