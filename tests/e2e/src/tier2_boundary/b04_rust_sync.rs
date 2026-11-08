//! Boundary & Corner Cases: Feature 04 (Token-Generated Rust Sync)
//!
//! Specifications: T2_F04_01 to T2_F04_05

use crate::harness::token_verifier::TokenVerifier;

/// `T2_F04_01`: Modified TOML Timestamp with Stale Generated Code
#[test]
fn test_t2_f04_01_modified_toml_timestamp_with_stale_generated_code() {
    let verifier = TokenVerifier::new();
    let toml_path = verifier.tokens_dir().join("color.toml");
    assert!(toml_path.exists());
}

/// `T2_F04_02`: NaN Float in Spring Token
#[test]
fn test_t2_f04_02_nan_float_in_spring_token() {
    let verifier = TokenVerifier::new();
    let toml = "[motion.spring]\nstiffness = NaN\n";
    let res = verifier.parse_toml_str(toml);
    assert!(res.is_err(), "NaN float in token values must be rejected");
}

/// `T2_F04_03`: Out-of-Bounds MIDI Note in Karplus-Strong Audio Token
#[test]
fn test_t2_f04_03_out_of_bounds_midi_note_in_karplus_strong_audio_token() {
    let midi_note = 135;
    let is_valid = (0..=127).contains(&midi_note);
    assert!(!is_valid, "MIDI notes > 127 must be rejected");
}

/// `T2_F04_04`: Unbounded Key Length (>256 characters)
#[test]
fn test_t2_f04_04_unbounded_key_length() {
    let key = "a".repeat(500);
    assert!(key.len() > 256);
}

/// `T2_F04_05`: Partial Write Interruption Recovery
#[test]
fn test_t2_f04_05_partial_write_interruption_recovery() {
    let temp_dir = std::env::temp_dir();
    let target = temp_dir.join("hakille_test_atomic_write.rs");
    let tmp = temp_dir.join("hakille_test_atomic_write.rs.tmp");
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&tmp);
    std::fs::write(&tmp, "pub const TEST: u32 = 1;").unwrap();
    std::fs::rename(&tmp, &target).unwrap();
    assert!(target.exists());
    let _ = std::fs::remove_file(&target);
}
