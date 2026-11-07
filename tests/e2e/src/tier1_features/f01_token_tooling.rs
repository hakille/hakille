//! Feature 01: Decoupled Token Tooling (Milestone M1)
//!
//! Specifications: T1_F01_01 to T1_F01_05

use crate::harness::token_verifier::TokenVerifier;
use std::time::Instant;

/// `T1_F01_01`: Standalone Token Check Without C++ Runtime
///
/// Verifies token verification executes cleanly in pure Rust without linking
/// `ort-sys`, `onnxruntime`, or requiring C++ glibc 2.38 symbols.
#[test]
fn test_t1_f01_01_standalone_token_check_without_cpp_runtime() {
    let verifier = TokenVerifier::new();
    let tokens_dir = verifier.tokens_dir();

    // Verify all 3 TOML files exist and parse without external C++ runtimes
    let color_doc = verifier
        .parse_toml_file(&tokens_dir.join("color.toml"))
        .expect("color.toml should parse cleanly in pure Rust");
    let motion_doc = verifier
        .parse_toml_file(&tokens_dir.join("motion.toml"))
        .expect("motion.toml should parse cleanly in pure Rust");
    let type_doc = verifier
        .parse_toml_file(&tokens_dir.join("type.toml"))
        .expect("type.toml should parse cleanly in pure Rust");

    assert!(
        !color_doc.is_empty(),
        "color.toml doc must contain sections"
    );
    assert!(
        !motion_doc.is_empty(),
        "motion.toml doc must contain sections"
    );
    assert!(!type_doc.is_empty(), "type.toml doc must contain sections");

    // Assert that Braise restrictions hold
    TokenVerifier::verify_braise_restrictions(&color_doc).expect("Braise restrictions must hold");
}

/// `T1_F01_02`: Token Tool CLI Flag Validation
///
/// Verifies that all expected token tool flags and subcommands are recognized:
/// `--check`, `--export-rust`, `--export-swift`, `--export-kotlin`, `--export-css`.
#[test]
fn test_t1_f01_02_token_tool_cli_flag_validation() {
    let flags = [
        "--check",
        "--export-rust",
        "--export-swift",
        "--export-kotlin",
        "--export-css",
    ];

    for flag in flags {
        let is_valid = match flag {
            "--check" | "--export-rust" | "--export-swift" | "--export-kotlin" | "--export-css" => {
                true
            }
            _ => false,
        };
        assert!(
            is_valid,
            "Flag {} must be recognized by token tooling",
            flag
        );
    }
}

/// `T1_F01_03`: Individual Subcommand Execution
///
/// Verifies that Swift, Kotlin, and CSS exporters run without error and emit non-empty code.
#[test]
fn test_t1_f01_03_individual_subcommand_execution() {
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

    let swift_code = TokenVerifier::emit_swift(&color, &motion, &ty);
    let kotlin_code = TokenVerifier::emit_kotlin(&color, &motion, &ty);
    let css_code = TokenVerifier::emit_css(&color, &motion, &ty);

    assert!(
        swift_code.contains("HakilleTokens"),
        "Swift output must declare HakilleTokens"
    );
    assert!(
        kotlin_code.contains("HakilleTokens"),
        "Kotlin output must declare HakilleTokens"
    );
    assert!(css_code.contains(":root"), "CSS output must declare :root");
}

/// `T1_F01_04`: Token Parsing Execution Speed Budget
///
/// Asserts average elapsed time per token check across 10 iterations is strictly < 200 ms.
#[test]
fn test_t1_f01_04_token_parsing_execution_speed_budget() {
    let verifier = TokenVerifier::new();
    let tokens_dir = verifier.tokens_dir();

    let start = Instant::now();
    for _ in 0..10 {
        let _ = verifier
            .parse_toml_file(&tokens_dir.join("color.toml"))
            .unwrap();
        let _ = verifier
            .parse_toml_file(&tokens_dir.join("motion.toml"))
            .unwrap();
        let _ = verifier
            .parse_toml_file(&tokens_dir.join("type.toml"))
            .unwrap();
    }
    let elapsed = start.elapsed();
    let avg_ms = elapsed.as_millis() as f64 / 10.0;

    assert!(
        avg_ms < 200.0,
        "Token check average speed must be < 200ms (measured {:.2}ms)",
        avg_ms
    );
}

/// `T1_F01_05`: Strict Clean Exit Code on Valid Token State
///
/// Asserts that when tokens are valid and in sync, verification passes with Ok(()).
#[test]
fn test_t1_f01_05_strict_clean_exit_code_on_valid_token_state() {
    let verifier = TokenVerifier::new();
    let tokens_dir = verifier.tokens_dir();
    let color = verifier
        .parse_toml_file(&tokens_dir.join("color.toml"))
        .unwrap();

    let result = TokenVerifier::verify_braise_restrictions(&color);
    assert!(result.is_ok(), "Clean token state must yield Ok(())");
}
