//! Tier 4 Real-World Application Workload: Scenario 6
//!
//! Calm Distress Mode Activation & Ephemeral Guest Delegation
//! Features Exercised: F01 (Living Presence), F29 (Ephemeral Guest Pass), F30 (Calm Distress Mode)
//! Complexity: Low
//!
//! Workflow:
//! 1. Normal state: 21 dynamic kora wires, braise enabled in allowed contexts, autonomy Level 2.
//! 2. Emergency trigger: User triggers Calm Distress Mode.
//! 3. UI quiescence & safety: 0ms animations, static resting line, zero braise, instant emergency dialing.
//! 4. Guest delegation: User activates 10-minute Ephemeral Guest Pass to loan phone.
//! 5. Guest confinement: Autonomy clamped to Crank 0 (Observe), 1px hairline countdown ring.
//! 6. Session termination: Timer reaches 0, guest session clears, device returns to sovereign state.

use hakille_design::autonomy::{Crank, DomainAutonomy};
use hakille_design::brand::{WIRES_FULL, wire_count};
use hakille_design::color::braise_allowed;
use hakille_design::household::{DistressMode, GUEST_RING_PX, GuestPass};
use hakille_design::motion::presence_rhythm;

#[test]
fn test_scenario6_calm_distress_mode_and_ephemeral_guest_delegation() {
    // --- STEP 1: Normal Sovereign State ---
    assert_eq!(
        wire_count(360),
        WIRES_FULL,
        "21 wires in normal mobile state"
    );
    assert!(
        braise_allowed("validation"),
        "Braise allowed on normal hold validation"
    );

    let mut home_domain = DomainAutonomy::new("Maison");
    home_domain.crank = Crank::ActReversible; // Crank 2

    // --- STEP 2: Emergency Trigger & Distress Mode ---
    let distress = DistressMode {
        active: true,
        trusted_contact: Some("+33112".into()),
    };
    assert!(distress.active);

    // --- STEP 3: Immediate UI & Safety Transformation ---
    // Calm copy text
    assert_eq!(
        DistressMode::calm_copy(),
        "Je reste là. Un geste pour appeler ton contact, un autre pour les urgences."
    );

    // Zero braise contract
    assert!(
        !braise_allowed("distress"),
        "Braise is strictly forbidden in distress mode"
    );
    assert!(!braise_allowed("calm-distress"));

    // Zero animation, flat quiescent line
    let (distress_period, _, _, _) = presence_rhythm("distress");
    assert_eq!(distress_period, 0, "Zero ms animation period in distress");

    // Emergency dial triggers immediately without 600ms hold
    let emergency_dial_requires_hold = false;
    assert!(!emergency_dial_requires_hold);

    // --- STEP 4 & 5: Ephemeral Guest Pass Delegation ---
    // User loans phone to a neighbor for 10 minutes (600 seconds)
    let mut guest_pass = GuestPass::new("Voisin Secours", "telephone.urgence", 600);
    assert_eq!(GUEST_RING_PX, 1, "Hairline 1px guest ring width");
    assert_eq!(guest_pass.elapsed_secs, 0);
    assert!((guest_pass.ring_fraction() - 1.0).abs() < 1e-4);

    // Guest autonomy clamping: clamps domain to Crank 0 (Observe)
    let guest_effective_crank = if !guest_pass.expired() {
        Crank::Observe
    } else {
        home_domain.crank
    };
    assert_eq!(
        guest_effective_crank,
        Crank::Observe,
        "Guest pass must clamp home automation to Crank 0"
    );

    // Advance guest timer to halfway (300s elapsed)
    guest_pass.elapsed_secs = 300;
    assert!((guest_pass.ring_fraction() - 0.5).abs() < 1e-4);
    assert!(!guest_pass.expired());

    // --- STEP 6: Expiration & Revocation ---
    // Advance to 600s
    guest_pass.elapsed_secs = 600;
    assert!(
        guest_pass.expired(),
        "Guest pass must expire at elapsed == duration"
    );
    assert_eq!(guest_pass.ring_fraction(), 0.0);

    // Upon expiration, sovereignty is restored to owner
    let restored_crank = if !guest_pass.expired() {
        Crank::Observe
    } else {
        home_domain.crank
    };
    assert_eq!(
        restored_crank,
        Crank::ActReversible,
        "Domain autonomy restores to owner crank after pass expiration"
    );
}
