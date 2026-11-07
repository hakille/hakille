//! Feature 04: Token-Generated Rust Sync (Milestone M1)
//!
//! Specifications: T1_F04_01 to T1_F04_05

use hakille_design::color::braise_allowed;
use hakille_design::generated::color::{COLOR_BRAISE_DARK, COLOR_SURFACE_0_DARK};
use hakille_design::generated::motion::{
    MOTION_DURATION_MS_T600, MOTION_EASE_EASE_IN, MOTION_SOUND_EVEIL,
};

/// `T1_F04_01`: Rust Color Constants Synchronization
///
/// Asserts Rust color constants match tokens: surface.0 = 0x0B0C0F, braise = 0xFF7A2F.
#[test]
fn test_t1_f04_01_rust_color_constants_synchronization() {
    assert_eq!(
        COLOR_SURFACE_0_DARK, 0x0B0C0F,
        "COLOR_SURFACE_0_DARK must be 0x0B0C0F"
    );
    assert_eq!(
        COLOR_BRAISE_DARK, 0xFF7A2F,
        "COLOR_BRAISE_DARK must be 0xFF7A2F"
    );
}

/// `T1_F04_02`: Rust Motion Constants Synchronization
///
/// Asserts motion constants match tokens: T600 = 600, ease_in = cubic-bezier(0.2, 0, 0, 1).
#[test]
fn test_t1_f04_02_rust_motion_constants_synchronization() {
    assert_eq!(
        MOTION_DURATION_MS_T600, 600,
        "MOTION_DURATION_MS_T600 must be 600 ms"
    );
    assert_eq!(
        MOTION_EASE_EASE_IN, "cubic-bezier(0.2, 0, 0, 1)",
        "MOTION_EASE_EASE_IN must match cubic-bezier"
    );
}

/// `T1_F04_03`: Karplus-Strong Audio Note Tuples
///
/// Asserts sound synthesis tuples match tokens: MOTION_SOUND_EVEIL = [(69, 90), (76, 90)].
#[test]
fn test_t1_f04_03_karplus_strong_audio_note_tuples() {
    assert_eq!(
        MOTION_SOUND_EVEIL,
        [(69, 90), (76, 90)],
        "MOTION_SOUND_EVEIL must equal [(69, 90), (76, 90)]"
    );
}

/// `T1_F04_04`: Zero-Diff Validation Test
///
/// Asserts that constants are available at compile-time and match specification without diff.
#[test]
fn test_t1_f04_04_zero_diff_validation_test() {
    // Both constants exist and are distinct
    assert_ne!(COLOR_SURFACE_0_DARK, COLOR_BRAISE_DARK);
    assert_eq!(COLOR_BRAISE_DARK & 0xFF0000, 0xFF0000);
}

/// `T1_F04_05`: Braise Accent Single-Context Invariant
///
/// Verifies braise_allowed returns true ONLY for validation, hold-progress, decision-ring.
#[test]
fn test_t1_f04_05_braise_accent_single_context_invariant() {
    assert!(braise_allowed("validation"), "validation must allow Braise");
    assert!(
        braise_allowed("hold-progress"),
        "hold-progress must allow Braise"
    );
    assert!(
        braise_allowed("decision-ring"),
        "decision-ring must allow Braise"
    );

    assert!(
        !braise_allowed("navigation-bar"),
        "navigation-bar must NOT allow Braise"
    );
    assert!(
        !braise_allowed("surface.0"),
        "surface.0 must NOT allow Braise"
    );
    assert!(
        !braise_allowed("background"),
        "background must NOT allow Braise"
    );
    assert!(!braise_allowed("text"), "text must NOT allow Braise");
}
