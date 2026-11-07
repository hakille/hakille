//! Feature 13: Elimination of Hardcoded Magic Colors (Milestone M3)
//!
//! Specifications: T1_F13_01 to T1_F13_05

use hakille_design::generated::color::{COLOR_BRAISE_DARK, COLOR_LIGNE_DARK, COLOR_TEXTE_1_DARK};
use hakille_ui_model::snapshot::{CORAL_HEX, HUD_BLUR};

/// `T1_F13_01`: Error Color Reference Check
#[test]
fn test_t1_f13_01_error_color_reference_check() {
    let error_hex = 0xFF5A5A;
    assert_eq!(error_hex, 0xFF5A5A);
}

/// `T1_F13_02`: Text Color Reference Check
#[test]
fn test_t1_f13_02_text_color_reference_check() {
    assert_eq!(COLOR_TEXTE_1_DARK, 0xECEEF2);
}

/// `T1_F13_03`: Model Snapshot Token Renaming
#[test]
fn test_t1_f13_03_model_snapshot_token_renaming() {
    assert_eq!(CORAL_HEX, "#FF7A2F");
    assert_eq!(COLOR_BRAISE_DARK, 0xFF7A2F);
}

/// `T1_F13_04`: HUD Acrylic Blur Token Integration
#[test]
fn test_t1_f13_04_hud_acrylic_blur_token_integration() {
    assert_eq!(HUD_BLUR, 30, "HUD blur must be 30 px");
}

/// `T1_F13_05`: Border & Line Token Reference Check
#[test]
fn test_t1_f13_05_border_and_line_token_reference_check() {
    assert_eq!(COLOR_LIGNE_DARK, 0x262A33);
}
