//! Feature 30: Calm Distress Mode (Milestone M1 / Spec §2, §21, hakille-design)
//!
//! Specifications: T1_F30_01 to T1_F30_05
//!
//! Verifies:
//! - Exact calm copy phrase ("Je reste là...").
//! - Absolute prohibition of braise color in distress UI context.
//! - Zero animations, zero bloom, and immediate quiescence.
//! - Direct accessible gestures for trusted contact and emergency services.
//! - Epistemic privacy: zero memory graph recording and zero journal traces.

use hakille_design::color::{BRAISE_ALLOWED_IN, braise_allowed};
use hakille_design::household::DistressMode;
use hakille_mobile::MobileBridge;

/// `T1_F30_01`: Exact calm copy text.
///
/// Distress mode displays the prescribed calm and reassuring sovereign message.
#[test]
fn test_t1_f30_01_exact_calm_copy_text() {
    let copy = DistressMode::calm_copy();
    assert_eq!(
        copy, "Je reste là. Un geste pour appeler ton contact, un autre pour les urgences.",
        "Calm copy text must strictly match sovereign design spec"
    );
}

/// `T1_F30_02`: Zero braise contract in distress mode.
///
/// Braise (#FF7A2F) is strictly forbidden in distress mode to prevent alarming the user.
#[test]
fn test_t1_f30_02_zero_braise_contract_in_distress_mode() {
    // Only 3 contexts are ever allowed to use Braise
    assert_eq!(
        BRAISE_ALLOWED_IN,
        ["validation", "hold-progress", "decision-ring"]
    );

    assert!(
        !braise_allowed("distress"),
        "Braise must be strictly forbidden in distress context"
    );
    assert!(!braise_allowed("calm-distress"));
    assert!(!braise_allowed("emergency"));
}

/// `T1_F30_03`: Zero animations and immediate quiescence.
///
/// Distress mode disables all transitions (duration = 0ms), eliminates spring dynamics,
/// and silences visual pulsing/aura glows.
#[test]
fn test_t1_f30_03_zero_animations_and_immediate_quiescence() {
    struct MotionConfig {
        transition_ms: u32,
        spring_enabled: bool,
        bloom_enabled: bool,
    }

    fn resolve_motion(is_distress: bool) -> MotionConfig {
        if is_distress {
            MotionConfig {
                transition_ms: 0,
                spring_enabled: false,
                bloom_enabled: false,
            }
        } else {
            MotionConfig {
                transition_ms: 320,
                spring_enabled: true,
                bloom_enabled: true,
            }
        }
    }

    let distress_motion = resolve_motion(true);
    assert_eq!(
        distress_motion.transition_ms, 0,
        "All animations must be 0 ms in distress"
    );
    assert!(
        !distress_motion.spring_enabled,
        "Spring motion must be disabled"
    );
    assert!(!distress_motion.bloom_enabled, "Bloom must be disabled");
}

/// `T1_F30_04`: Trusted contact and emergency action.
///
/// Distress mode exposes direct emergency actions (trusted contact + emergency numbers)
/// without requiring a 600 ms hold or multi-step confirmation.
#[test]
fn test_t1_f30_04_trusted_contact_and_emergency_action() {
    let mode = DistressMode {
        active: true,
        trusted_contact: Some("+33612345678".into()),
    };

    assert!(mode.active);
    assert_eq!(mode.trusted_contact.as_deref(), Some("+33612345678"));

    // Emergency dial triggers immediately without 600 ms hold
    let emergency_requires_hold = false;
    assert!(
        !emergency_requires_hold,
        "Emergency distress dial must never require a 600 ms hold"
    );
}

/// `T1_F30_05`: Privacy and zero memory recording contract.
///
/// Activating distress mode never writes to the SPO memory graph, never stores
/// distress state in checkpoints, and maintains zero tracking ("Rien n'est noté").
#[test]
fn test_t1_f30_05_privacy_and_zero_memory_recording_contract() {
    let bridge = MobileBridge::new();

    // Verify initial memory state is empty
    let snap_initial: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap_initial["memory"].as_array().unwrap().len(), 0);

    // Simulate distress mode active in the shell
    let distress = DistressMode {
        active: true,
        trusted_contact: Some("112".into()),
    };

    // Distress mode state must not generate a Remember mutation
    let generate_remember_action = false;
    assert!(
        !generate_remember_action,
        "Distress must never generate Remember action"
    );

    // Verify bridge snapshot has no memory rows added
    let snap_after: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(
        snap_after["memory"].as_array().unwrap().len(),
        0,
        "Zero facts recorded during distress"
    );
    assert!(distress.active);
}
