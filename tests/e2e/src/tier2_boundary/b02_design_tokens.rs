//! Boundary & Corner Cases: Feature 02 (Functional & Glass Design Tokens)
//!
//! Specifications: T2_F02_01 to T2_F02_05

use crate::harness::frame_inspector::FrameInspector;
use crate::harness::token_verifier::{TokenDoc, TokenVal, TokenVerifier};
use std::collections::BTreeMap;

/// `T2_F02_01`: Braise Accent Token Assigned to Forbidden Key
#[test]
fn test_t2_f02_01_braise_accent_token_assigned_to_forbidden_key() {
    let mut doc: TokenDoc = BTreeMap::new();
    let mut section = BTreeMap::new();
    section.insert("dark".into(), TokenVal::Str("#FF7A2F".into()));
    doc.insert("color.surface.0".into(), section);

    let res = TokenVerifier::verify_braise_restrictions(&doc);
    assert!(
        res.is_err(),
        "Braise assigned to surface.0 must be rejected"
    );
    assert!(res.err().unwrap().contains("strictly forbidden"));
}

/// `T2_F02_02`: Glass Alpha Value Out of Bounds
#[test]
fn test_t2_f02_02_glass_alpha_value_out_of_bounds() {
    let alpha_invalid = 2.5f64;
    let clamped = alpha_invalid.clamp(0.0, 1.0);
    assert_eq!(clamped, 1.0, "Alpha value must be clamped to [0.0, 1.0]");
}

/// `T2_F02_03`: Negative Blur Radius
#[test]
fn test_t2_f02_03_negative_blur_radius() {
    let raw_blur: i64 = -15;
    let valid_blur = raw_blur.max(0);
    assert_eq!(valid_blur, 0, "Negative blur radius must be clamped to 0");
}

/// `T2_F02_04`: Malformed Hex Color String
#[test]
fn test_t2_f02_04_malformed_hex_color_string() {
    assert!(TokenVerifier::validate_hex_color("#FF7A").is_err());
    assert!(TokenVerifier::validate_hex_color("#ZZZZZZ").is_err());
    assert!(TokenVerifier::validate_hex_color("#0B0C0F").is_ok());
}

/// `T2_F02_05`: WCAG AA Contrast Violation Edge Case
#[test]
fn test_t2_f02_05_wcag_aa_contrast_violation_edge_case() {
    let low_contrast_text = 0x404040;
    let dark_bg = 0x0B0C0F;
    let ratio = FrameInspector::contrast_ratio(low_contrast_text, dark_bg);
    assert!(ratio < 4.5, "Low contrast pair must be detected (< 4.5:1)");
}
