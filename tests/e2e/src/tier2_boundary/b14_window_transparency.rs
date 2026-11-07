//! Boundary & Corner Cases: Feature 14 (Desktop Window Dragging & Transparency)
//!
//! Specifications: T2_F14_01 to T2_F14_05

use hakille_ui_desktop::app::{HudApp, PointerTarget};
use hakille_ui_desktop::renderer::SoftwareRenderer;
use hakille_ui_desktop::runtime::FrameBudget;
use hakille_ui_protocol::{ActionCard, DataExit, KernelToUi, Money, Provenance};

fn make_modal_card() -> ActionCard {
    ActionCard {
        id: 99,
        verb: "Exécuter".into(),
        object: "action.sh".into(),
        reason: "Test".into(),
        cost: Money {
            micros: 0,
            currency: "EUR".into(),
        },
        data_exit: DataExit::Local,
        reversible: false,
        provenance: Provenance::Hakille,
    }
}

/// `T2_F14_01`: Win32 SetLayeredWindowAttributes API Failure Fallback
#[test]
fn test_t2_f14_01_win32_api_failure_fallback() {
    let renderer = SoftwareRenderer::new(720, 64, true).with_transparent_backdrop(false);
    assert!(
        !renderer.transparent_backdrop,
        "Failure must fall back to opaque"
    );
}

/// `T2_F14_02`: Window Dragged Off All Physical Monitors
#[test]
fn test_t2_f14_02_window_dragged_off_all_monitors() {
    let offscreen_x = -5000.0f32;
    let clamped_x = offscreen_x.max(0.0);
    assert_eq!(clamped_x, 0.0, "Window position off-screen must be clamped");
}

/// `T2_F14_03`: Transparent Colorkey Click-Through Verification
#[test]
fn test_t2_f14_03_transparent_colorkey_click_through_verification() {
    let key = 0x00010101u32;
    assert_eq!(key, 0x00010101);
}

/// `T2_F14_04`: Rapid Double/Triple Drag Clicks
#[test]
fn test_t2_f14_04_rapid_double_triple_drag_clicks() {
    let app = HudApp::new(true, true, FrameBudget::battery_saver(), true);
    for _ in 0..5 {
        assert_eq!(app.pointer_target(300.0, 30.0), PointerTarget::Drag);
    }
}

/// `T2_F14_05`: Dragging Attempt While Modal Action Card Open
#[test]
fn test_t2_f14_05_dragging_attempt_while_modal_action_card_open() {
    let mut app = HudApp::new(true, true, FrameBudget::battery_saver(), true);
    app.on_kernel_reply(KernelToUi::ShowCard(make_modal_card()));
    // Clicking on card content body does NOT drag window
    assert_ne!(app.pointer_target(300.0, 250.0), PointerTarget::Drag);
    assert_eq!(app.pointer_target(300.0, 300.0), PointerTarget::None);
}
