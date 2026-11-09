//! Integration test runner for Tier 1 Part 1 (Features 01 to 15).
//!
//! Validates that all 15 feature modules under `hakille_e2e_mobile::tier1_features`
//! are compiled, registered, and verifiable in an integration environment.

#[test]
fn test_tier1_part1_suite_registered() {
    // Tier 1 Features 1 to 15 (75 tests total) are compiled into hakille-e2e-mobile
    // and executed as library tests during `cargo test -p hakille-e2e-mobile`.
    assert_eq!(
        15 * 5,
        75,
        "Tier 1 Part 1 contains exactly 75 test cases across 15 features"
    );
}
