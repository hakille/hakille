//! Feature 17: Quiescence & Battery Energy Budget (Milestone M4)
//!
//! Specifications: T1_F17_01 to T1_F17_05

use hakille_ui::{CostCounter, EphemeralHud, PresenceIndicator, SilenceBudgetTracker};
use hakille_ui_desktop::runtime::{FrameBudget, HeadlessRuntime, RuntimeEvent};
use hakille_ui_model::StableSnapshot;
use std::time::{Duration, Instant};

/// `T1_F17_01`: Hidden Window Zero Frame Invariant
#[test]
fn test_t1_f17_01_hidden_window_zero_frame_invariant() {
    let mut runtime = HeadlessRuntime::new();
    runtime.visible = false;
    assert!(!runtime.visible);
    assert!(!runtime.needs_frame());

    for _ in 0..10 {
        runtime.push(RuntimeEvent::FrameDue);
        let _ = runtime.tick();
    }
    assert_eq!(
        runtime.frames_emitted, 0,
        "Hidden window must emit 0 frames"
    );
}

/// `T1_F17_02`: Idle Presence Zero Frame Invariant
#[test]
fn test_t1_f17_02_idle_presence_zero_frame_invariant() {
    let mut runtime = HeadlessRuntime::new();
    runtime.visible = true;
    let presence = PresenceIndicator::new();
    let hud = EphemeralHud::new();
    let cost = CostCounter::new(500, "EUR");
    let silence = SilenceBudgetTracker::new(3);
    let snap = StableSnapshot::from_states(&presence, &hud, &cost, &silence, 0, 0);
    runtime.snapshot = Some(snap);
    assert!(
        !runtime.needs_frame(),
        "Idle presence must not request frames"
    );
}

/// `T1_F17_03`: Battery Throttling 30 FPS Cap
#[test]
fn test_t1_f17_03_battery_throttling_30_fps_cap() {
    let mut budget = FrameBudget::battery_saver();
    assert_eq!(budget.max_fps, 30);
    assert_eq!(budget.min_frame_interval, Duration::from_millis(33));

    let start = Instant::now();
    let mut frames = 0;
    // Simulate 1000ms loop
    for i in 0..1000 {
        let t = start + Duration::from_millis(i);
        if budget.poll(t, true) {
            frames += 1;
        }
    }
    assert!(
        frames <= 31,
        "Battery saver must throttle to <= 31 fps (actual: {frames})"
    );
}

/// `T1_F17_04`: Full Power 120 FPS High Refresh
#[test]
fn test_t1_f17_04_full_power_120_fps_high_refresh() {
    let budget = FrameBudget::full();
    assert_eq!(budget.max_fps, 120);
    assert_eq!(budget.min_frame_interval, Duration::from_millis(8));
}

/// `T1_F17_05`: Single Wakeup Frame on Global Hotkey
#[test]
fn test_t1_f17_05_single_wakeup_frame_on_global_hotkey() {
    let mut runtime = HeadlessRuntime::new();
    runtime.visible = false;
    let initial_wakeups = runtime.wakeups;
    runtime.push(RuntimeEvent::HotkeyPressed);
    let actions = runtime.tick();
    assert!(runtime.visible);
    assert_eq!(runtime.wakeups, initial_wakeups + 1);
    assert!(actions.contains(&RuntimeEvent::ShowHud));
}
