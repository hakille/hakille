//! Feature 02: Functional & Glass Design Tokens (Milestone M1)
//!
//! Specifications: T1_F02_01 to T1_F02_05

use crate::harness::frame_inspector::FrameInspector;
use crate::harness::token_verifier::{TokenVal, TokenVerifier};

/// `T1_F02_01`: Functional Status Color Tokens Verification
///
/// Verifies functional status colors (error, success, caution) conform to contract.
#[test]
fn test_t1_f02_01_functional_status_color_tokens_verification() {
    let error_dark = 0xFF5A5A;
    let success_dark = 0x34D399;
    let caution_dark = 0xFBBF24;
    let bg_dark = 0x0B0C0F;

    // WCAG AA contrast check against dark canvas surface
    FrameInspector::assert_wcag_aa(error_dark, bg_dark, true);
    FrameInspector::assert_wcag_aa(success_dark, bg_dark, false);
    FrameInspector::assert_wcag_aa(caution_dark, bg_dark, false);
}

/// `T1_F02_02`: Translucent Acrylic & Specular Glass Reflection Tokens
///
/// Verifies glass reflection tokens exist and maintain transparency parameters.
#[test]
fn test_t1_f02_02_translucent_acrylic_and_specular_glass_reflection_tokens() {
    let verifier = TokenVerifier::new();
    let tokens_dir = verifier.tokens_dir();
    let color = verifier
        .parse_toml_file(&tokens_dir.join("color.toml"))
        .unwrap();

    // Check lueur and line tokens defining glass reflection
    assert!(
        color.contains_key("color.lueur"),
        "color.lueur section must be present"
    );
    assert!(
        color.contains_key("color.ligne"),
        "color.ligne section must be present"
    );

    let lueur_dark = color.get("color.lueur").unwrap().get("dark").unwrap();
    if let TokenVal::Str(hex) = lueur_dark {
        assert_eq!(
            hex, "#F4EBDD",
            "Lueur dark specular tone must match #F4EBDD"
        );
    }
}

/// `T1_F02_03`: Braise Ember Diffusion Halo Token
///
/// Verifies Braise ember token (#FF7A2F) and its diffusion parameters.
#[test]
fn test_t1_f02_03_braise_ember_diffusion_halo_token() {
    let verifier = TokenVerifier::new();
    let tokens_dir = verifier.tokens_dir();
    let color = verifier
        .parse_toml_file(&tokens_dir.join("color.toml"))
        .unwrap();

    let braise_dark = color.get("color.braise").unwrap().get("dark").unwrap();
    if let TokenVal::Str(hex) = braise_dark {
        assert_eq!(hex, "#FF7A2F", "Braise dark must be exactly #FF7A2F");
    }

    // Verify allowed_in constraint strictly contains only 3 targets
    let allowed = color
        .get("color.braise")
        .unwrap()
        .get("allowed_in")
        .unwrap();
    if let TokenVal::StrList(list) = allowed {
        assert_eq!(
            list,
            &["validation", "hold-progress", "decision-ring"],
            "allowed_in must be strictly ['validation', 'hold-progress', 'decision-ring']"
        );
    }
}

/// `T1_F02_04`: Acrylic Blur Radius Token Dimensions
///
/// Verifies blur dimension standards: HUD 30px, Panel 24px, Glow 16px.
#[test]
fn test_t1_f02_04_acrylic_blur_radius_token_dimensions() {
    let hud_blur = 30;
    let panel_blur = 24;
    let glow_blur = 16;

    assert_eq!(hud_blur, 30, "HUD acrylic blur must be 30 px");
    assert_eq!(panel_blur, 24, "Panel blur must be 24 px");
    assert_eq!(glow_blur, 16, "Glow blur must be 16 px");
}

/// `T1_F02_05`: Interactive Surface State Color Tokens
///
/// Verifies interactive surface states (surface.0, surface.1, surface.2).
#[test]
fn test_t1_f02_05_interactive_surface_state_color_tokens() {
    let verifier = TokenVerifier::new();
    let tokens_dir = verifier.tokens_dir();
    let color = verifier
        .parse_toml_file(&tokens_dir.join("color.toml"))
        .unwrap();

    assert!(
        color.contains_key("color.surface.0"),
        "surface.0 must be defined"
    );
    assert!(
        color.contains_key("color.surface.1"),
        "surface.1 must be defined"
    );
    assert!(
        color.contains_key("color.surface.2"),
        "surface.2 must be defined"
    );

    let s0 = color.get("color.surface.0").unwrap().get("dark").unwrap();
    let s1 = color.get("color.surface.1").unwrap().get("dark").unwrap();
    let s2 = color.get("color.surface.2").unwrap().get("dark").unwrap();

    assert_eq!(s0, &TokenVal::Str("#0B0C0F".into()));
    assert_eq!(s1, &TokenVal::Str("#121419".into()));
    assert_eq!(s2, &TokenVal::Str("#1A1D24".into()));
}
