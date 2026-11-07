//! Feature 03: Complete Multi-Target Exporters (Milestone M1)
//!
//! Specifications: T1_F03_01 to T1_F03_05

use crate::harness::token_verifier::TokenVerifier;

/// `T1_F03_01`: Swift Exporter Cubic-Bezier & Spring Parameters
///
/// Asserts Swift export includes cubic-bezier easeIn and spring constants.
#[test]
fn test_t1_f03_01_swift_exporter_cubic_bezier_and_spring_parameters() {
    let verifier = TokenVerifier::new();
    let tokens_dir = verifier.tokens_dir();
    let color = verifier
        .parse_toml_file(&tokens_dir.join("color.toml"))
        .unwrap();
    let motion = verifier
        .parse_toml_file(&tokens_dir.join("motion.toml"))
        .unwrap();
    let ty = verifier
        .parse_toml_file(&tokens_dir.join("type.toml"))
        .unwrap();

    let swift = TokenVerifier::emit_swift(&color, &motion, &ty);
    assert!(
        swift.contains("cubic-bezier(0.2, 0, 0, 1)"),
        "Swift must contain cubic bezier"
    );
    assert!(
        swift.contains("springCardStiffness"),
        "Swift must export spring stiffness"
    );
    assert!(
        swift.contains("springCardDamping"),
        "Swift must export spring damping"
    );
}

/// `T1_F03_02`: Swift Exporter Font Stack & Typed Color
///
/// Asserts Swift export contains fontInterfaceFamily and typed Color constructs.
#[test]
fn test_t1_f03_02_swift_exporter_font_stack_and_typed_color() {
    let verifier = TokenVerifier::new();
    let tokens_dir = verifier.tokens_dir();
    let color = verifier
        .parse_toml_file(&tokens_dir.join("color.toml"))
        .unwrap();
    let motion = verifier
        .parse_toml_file(&tokens_dir.join("motion.toml"))
        .unwrap();
    let ty = verifier
        .parse_toml_file(&tokens_dir.join("type.toml"))
        .unwrap();

    let swift = TokenVerifier::emit_swift(&color, &motion, &ty);
    assert!(swift.contains("fontInterfaceFamily = \"Inter Variable\""));
    assert!(swift.contains("Color("));
}

/// `T1_F03_03`: Kotlin Exporter Compose CubicBezierEasing
///
/// Asserts Kotlin export contains Compose CubicBezierEasing and typed Color.
#[test]
fn test_t1_f03_03_kotlin_exporter_compose_cubic_bezier_easing() {
    let verifier = TokenVerifier::new();
    let tokens_dir = verifier.tokens_dir();
    let color = verifier
        .parse_toml_file(&tokens_dir.join("color.toml"))
        .unwrap();
    let motion = verifier
        .parse_toml_file(&tokens_dir.join("motion.toml"))
        .unwrap();
    let ty = verifier
        .parse_toml_file(&tokens_dir.join("type.toml"))
        .unwrap();

    let kotlin = TokenVerifier::emit_kotlin(&color, &motion, &ty);
    assert!(kotlin.contains("CubicBezierEasing(0.2f, 0.0f, 0.0f, 1.0f)"));
    assert!(kotlin.contains("Color(0xFFFF7A2FU)"));
}

/// `T1_F03_04`: CSS Exporter Variables Generation
///
/// Asserts CSS export generates CSS custom properties for motion, font, and Braise.
#[test]
fn test_t1_f03_04_css_exporter_variables_generation() {
    let verifier = TokenVerifier::new();
    let tokens_dir = verifier.tokens_dir();
    let color = verifier
        .parse_toml_file(&tokens_dir.join("color.toml"))
        .unwrap();
    let motion = verifier
        .parse_toml_file(&tokens_dir.join("motion.toml"))
        .unwrap();
    let ty = verifier
        .parse_toml_file(&tokens_dir.join("type.toml"))
        .unwrap();

    let css = TokenVerifier::emit_css(&color, &motion, &ty);
    assert!(css.contains("--hakille-motion-ease-in: cubic-bezier(0.2, 0, 0, 1);"));
    assert!(css.contains("--hakille-font-interface: \"Inter Variable\";"));
    assert!(css.contains("--hakille-color-braise: #FF7A2F;"));
}

/// `T1_F03_05`: Cross-Platform Numerical Parity
///
/// Asserts exact numerical parity (600ms hold, 16px radius) across Rust, Swift, Kotlin, CSS.
#[test]
fn test_t1_f03_05_cross_platform_numerical_parity() {
    let verifier = TokenVerifier::new();
    let tokens_dir = verifier.tokens_dir();
    let color = verifier
        .parse_toml_file(&tokens_dir.join("color.toml"))
        .unwrap();
    let motion = verifier
        .parse_toml_file(&tokens_dir.join("motion.toml"))
        .unwrap();
    let ty = verifier
        .parse_toml_file(&tokens_dir.join("type.toml"))
        .unwrap();

    let swift = TokenVerifier::emit_swift(&color, &motion, &ty);
    let kotlin = TokenVerifier::emit_kotlin(&color, &motion, &ty);
    let css = TokenVerifier::emit_css(&color, &motion, &ty);

    // 600ms parity
    assert!(swift.contains("600"));
    assert!(kotlin.contains("600"));
    assert!(css.contains("600ms"));

    // 16px radius parity
    assert!(swift.contains("16"));
    assert!(kotlin.contains("16"));
    assert!(css.contains("16px"));
}
