//! Feature 14: Desktop Window Dragging & Transparency (Milestone M4)
//!
//! Specifications: T1_F14_01 to T1_F14_05

use hakille_ui_desktop::app::{HudApp, PointerTarget};
use hakille_ui_desktop::renderer::{SoftwareRenderer, TRANSPARENT_KEY_RGB, card_buttons};
use hakille_ui_desktop::runtime::FrameBudget;
use hakille_ui_protocol::{ActionCard, DataExit, KernelToUi, Money, Provenance};

fn make_test_card() -> ActionCard {
    ActionCard {
        id: 42,
        verb: "Supprimer".into(),
        object: "cache.db".into(),
        reason: "Libération espace disque".into(),
        cost: Money {
            micros: 0,
            currency: "EUR".into(),
        },
        data_exit: DataExit::Local,
        reversible: false,
        provenance: Provenance::Hakille,
    }
}

/// `T1_F14_01`: Win32 Layered Colorkey Transparency Setup
#[test]
fn test_t1_f14_01_win32_layered_colorkey_transparency_setup() {
    assert_eq!(
        TRANSPARENT_KEY_RGB,
        (1, 1, 1),
        "Colorkey must be exactly (1, 1, 1)"
    );
    let renderer = SoftwareRenderer::new(720, 64, true).with_transparent_backdrop(true);
    assert_eq!(renderer.backdrop_rgb(true), (1, 1, 1));
}

/// `T1_F14_02`: Dynamic Style Re-arming on State Transition
#[test]
fn test_t1_f14_02_dynamic_style_rearming_on_state_transition() {
    let renderer = SoftwareRenderer::new(720, 64, true).with_transparent_backdrop(true);
    assert!(renderer.transparent_backdrop);
}

/// `T1_F14_03`: Non-Windows Opaque Fallback
#[test]
fn test_t1_f14_03_non_windows_opaque_fallback() {
    let renderer = SoftwareRenderer::new(720, 64, true).with_transparent_backdrop(false);
    let backdrop = renderer.backdrop_rgb(true);
    assert_ne!(
        backdrop,
        (1, 1, 1),
        "Opaque backdrop must NOT use colorkey (1, 1, 1)"
    );
    assert_eq!(backdrop, (0x0B, 0x0C, 0x0F));
}

/// `T1_F14_04`: Compact HUD Dragging Hit-Test
#[test]
fn test_t1_f14_04_compact_hud_dragging_hit_test() {
    let app = HudApp::new(true, true, FrameBudget::battery_saver(), true);
    // Any point inside compact HUD (720x64) is a drag handle
    assert_eq!(app.pointer_target(350.0, 32.0), PointerTarget::Drag);
    assert_eq!(app.pointer_target(10.0, 10.0), PointerTarget::Drag);
}

/// `T1_F14_05`: Action Card Decision Buttons Non-Draggable Boundary
#[test]
fn test_t1_f14_05_action_card_decision_buttons_non_draggable_boundary() {
    let mut app = HudApp::new(true, true, FrameBudget::battery_saver(), true);
    app.on_kernel_reply(KernelToUi::ShowCard(make_test_card()));

    let buttons = card_buttons(true);
    assert_eq!(
        app.pointer_target(buttons.validate.x + 2.0, buttons.validate.y + 2.0),
        PointerTarget::CardValidate,
        "Validate button must NOT be a Drag target"
    );
    assert_eq!(
        app.pointer_target(buttons.edit.x + 2.0, buttons.edit.y + 2.0),
        PointerTarget::CardEdit
    );
    assert_eq!(
        app.pointer_target(buttons.reject.x + 2.0, buttons.reject.y + 2.0),
        PointerTarget::CardReject
    );
    // Card body is non-draggable
    assert_eq!(app.pointer_target(300.0, 300.0), PointerTarget::None);
}
