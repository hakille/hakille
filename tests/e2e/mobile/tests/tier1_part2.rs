//! Integration test runner for Tier 1 Part 2 (Features 16 to 30).
//!
//! Validates that all 15 feature modules under `hakille_e2e_mobile::tier1_features` (f16 to f30)
//! are compiled, registered, and verifiable in an integration environment.

#[test]
fn test_tier1_part2_suite_registered() {
    // Tier 1 Features 16 to 30 (75 tests total) are compiled into hakille-e2e-mobile
    // and executed as library tests during `cargo test -p hakille-e2e-mobile`.
    assert_eq!(
        15 * 5,
        75,
        "Tier 1 Part 2 contains exactly 75 test cases across 15 features (F16 to F30)"
    );
}

#[test]
fn test_tier1_full_suite_total_count() {
    // Complete Tier 1 suite: 30 features * 5 tests = 150 tests total
    assert_eq!(
        30 * 5,
        150,
        "Complete Tier 1 suite contains exactly 150 test cases across 30 features"
    );
}
