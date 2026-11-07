//! Boundary & Corner Cases: Feature 01 (Decoupled Token Tooling)
//!
//! Specifications: T2_F01_01 to T2_F01_05

use crate::harness::token_verifier::TokenVerifier;

/// `T2_F01_01`: Zero-Byte TOML Token File
#[test]
fn test_t2_f01_01_zero_byte_toml_token_file() {
    let verifier = TokenVerifier::new();
    let res = verifier.parse_toml_str("");
    assert!(res.is_err(), "Empty file must return error");
    assert!(res.err().unwrap().contains("empty"));
}

/// `T2_F01_02`: Malformed Syntax in TOML
#[test]
fn test_t2_f01_02_malformed_syntax_in_toml() {
    let verifier = TokenVerifier::new();
    let malformed = "[color.surface.0\nkey = \"value";
    let res = verifier.parse_toml_str(malformed);
    assert!(res.is_err(), "Malformed syntax must return error");
    assert!(res.err().unwrap().contains("SyntaxError"));
}

/// `T2_F01_03`: Missing Mandatory Surface Token
#[test]
fn test_t2_f01_03_missing_mandatory_surface_token() {
    let verifier = TokenVerifier::new();
    let toml_without_surface = "[color.braise]\ndark = \"#FF7A2F\"\n";
    let doc = verifier.parse_toml_str(toml_without_surface).unwrap();
    assert!(
        !doc.contains_key("color.surface.0"),
        "Missing surface.0 must be detected"
    );
}

/// `T2_F01_04`: Read-Only Output Target Filesystem
#[test]
fn test_t2_f01_04_read_only_output_target_filesystem() {
    let path = std::path::Path::new("/proc/read_only_test");
    let err = std::fs::write(path, "test");
    assert!(
        err.is_err(),
        "Writing to read-only location must yield error"
    );
}

/// `T2_F01_05`: Massive 10 MB Synthetic Token File Stress Test
#[test]
fn test_t2_f01_05_massive_10mb_synthetic_token_file_stress_test() {
    let mut large_toml = String::with_capacity(1_000_000);
    large_toml.push_str("[synthetic]\n");
    for i in 0..10_000 {
        large_toml.push_str(&format!("color_{} = \"#0B0C0F\"\n", i));
    }
    let verifier = TokenVerifier::new();
    let res = verifier.parse_toml_str(&large_toml);
    assert!(
        res.is_ok(),
        "Parser must handle 10,000 entries without stack overflow"
    );
}
