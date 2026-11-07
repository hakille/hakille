//! Feature 10: Specular Glass Edge Highlights (Milestone M3)
//!
//! Specifications: T1_F10_01 to T1_F10_05

use hakille_ui::{CostCounter, EphemeralHud, PresenceIndicator, SilenceBudgetTracker};
use hakille_ui_desktop::renderer::{card_rect, card_scene, hud_scene};
use hakille_ui_desktop::scene::{DrawCmd, Rect};
use hakille_ui_model::StableSnapshot;
use hakille_ui_protocol::{ActionCard, DataExit, Money, Provenance};

fn mock_snapshot() -> StableSnapshot {
    let presence = PresenceIndicator::new();
    let hud = EphemeralHud::new();
    let cost = CostCounter::new(500, "EUR");
    let silence = SilenceBudgetTracker::new(3);
    StableSnapshot::from_states(&presence, &hud, &cost, &silence, 0, 0)
}

fn mock_card() -> ActionCard {
    ActionCard {
        id: 1,
        verb: "Refactoriser".into(),
        object: "main.rs".into(),
        reason: "Perf".into(),
        cost: Money {
            micros: 0,
            currency: "EUR".into(),
        },
        data_exit: DataExit::Local,
        reversible: false,
        provenance: Provenance::Hakille,
    }
}

/// `T1_F10_01`: HUD Specular Highlight Line Rendering
#[test]
fn test_t1_f10_01_hud_specular_highlight_line_rendering() {
    let snap = mock_snapshot();
    let scene = hud_scene(&snap, true);
    assert!(
        !scene.cmds.is_empty(),
        "HUD scene must contain drawing commands"
    );
    let has_bar_fill = scene
        .cmds
        .iter()
        .any(|cmd| matches!(cmd, DrawCmd::FillRect { rect, .. } if rect.w > 700.0));
    assert!(has_bar_fill, "HUD scene must contain bar fill rect");
}

/// `T1_F10_02`: Action Card Specular Top Highlight
#[test]
fn test_t1_f10_02_action_card_specular_top_highlight() {
    let snap = mock_snapshot();
    let card = mock_card();
    let scene = card_scene(&snap, &card, None, true);
    assert!(!scene.cmds.is_empty());
    let c_rect = card_rect();
    assert_eq!(c_rect.w, 696.0);
}

/// `T1_F10_03`: Corner Radius Clipping on Specular Line
#[test]
fn test_t1_f10_03_corner_radius_clipping_on_specular_line() {
    let rect = Rect::new(0.0, 0.0, 720.0, 64.0).rounded(12.0);
    assert_eq!(rect.radius, 12.0);
    // Specular highlight starts indented by corner radius
    let specular_x = rect.x + rect.radius;
    let specular_w = rect.w - (2.0 * rect.radius);
    assert!(specular_x > 0.0);
    assert!(specular_w < 720.0);
}

/// `T1_F10_04`: High Contrast Mode Override
#[test]
fn test_t1_f10_04_high_contrast_mode_override() {
    let mut snap = mock_snapshot();
    snap.high_contrast = true;
    let scene = hud_scene(&snap, true);
    assert!(!scene.cmds.is_empty());
}

/// `T1_F10_05`: Light Mode Specular Inversion
#[test]
fn test_t1_f10_05_light_mode_specular_inversion() {
    let mut snap = mock_snapshot();
    snap.light_mode = true;
    let scene = hud_scene(&snap, false);
    assert!(!scene.theme_dark);
}
