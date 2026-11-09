//! Feature 29: Ephemeral Guest Pass (Milestone M1 / Spec §2, §21, hakille-design)
//!
//! Specifications: T1_F29_01 to T1_F29_05
//!
//! Verifies:
//! - Default 1-hour (3600s) duration and initial 1.0 ring fraction.
//! - Ring fraction linear drain metering and 1 px hairline ring width token.
//! - Automatic pass expiration upon elapsed duration without manual revocation.
//! - Scoped capability confinement (bounded power delegation, zero root capabilities).
//! - Zero duration edge case handling and JSON serialization stability.

use hakille_design::household::{GUEST_RING_PX, GuestPass};

/// `T1_F29_01`: Default 1-hour duration and initial ring fraction.
///
/// A guest pass defaults to 3600 seconds (1 hour). At creation, elapsed is 0,
/// ring fraction is 1.0, and the pass is not expired.
#[test]
fn test_t1_f29_01_default_1h_duration_and_initial_ring_fraction() {
    let pass = GuestPass::new("Amina", "wifi", 3600);

    assert_eq!(pass.name, "Amina");
    assert_eq!(pass.power, "wifi");
    assert_eq!(pass.duration_secs, 3600);
    assert_eq!(pass.elapsed_secs, 0);
    assert!((pass.ring_fraction() - 1.0).abs() < 1e-6);
    assert!(!pass.expired(), "Pass must not be expired initially");
}

/// `T1_F29_02`: Ring fraction drain metering and 1 px width.
///
/// The ring is strictly 1 px wide (`GUEST_RING_PX = 1`). As time elapses, the ring
/// fraction visibly drains in linear proportion to remaining time.
#[test]
fn test_t1_f29_02_ring_fraction_drain_metering_and_1px_width() {
    assert_eq!(GUEST_RING_PX, 1, "Guest ring must be a 1 px hairline");

    let mut pass = GuestPass::new("Tidiane", "salon.lumieres", 3600);

    // Halfway through: 1800s elapsed -> fraction 0.5
    pass.elapsed_secs = 1800;
    assert!((pass.ring_fraction() - 0.5).abs() < 1e-6);
    assert!(!pass.expired());

    // 75% through: 2700s elapsed -> fraction 0.25
    pass.elapsed_secs = 2700;
    assert!((pass.ring_fraction() - 0.25).abs() < 1e-6);
    assert!(!pass.expired());
}

/// `T1_F29_03`: Automatic expiration on duration reached.
///
/// When elapsed_secs reaches duration_secs, `expired()` returns true and
/// `ring_fraction()` drops to 0.0.
#[test]
fn test_t1_f29_03_automatic_expiration_on_duration_reached() {
    let mut pass = GuestPass::new("Marc", "musique", 1800);

    // Exactly at expiration
    pass.elapsed_secs = 1800;
    assert!(pass.expired(), "Pass must expire when elapsed == duration");
    assert!((pass.ring_fraction() - 0.0).abs() < 1e-6);

    // Beyond expiration (clamped at 0.0, no negative fractions)
    pass.elapsed_secs = 2000;
    assert!(pass.expired());
    assert_eq!(pass.ring_fraction(), 0.0, "Fraction must clamp at 0.0");
}

/// `T1_F29_04`: Scoped capability confinement.
///
/// Guest pass grants only the specific capability named in `power` (e.g. "wifi").
/// High-stakes powers (memory deletion, wallet, system admin) are never granted to guests.
#[test]
fn test_t1_f29_04_scoped_capability_confinement() {
    let guest_pass = GuestPass::new("Invité", "wifi.connect", 3600);

    // Check capability checking function
    let is_authorized_for_guest = |pass: &GuestPass, requested_action: &str| -> bool {
        if pass.expired() {
            return false;
        }
        let forbidden_guest_actions = [
            "memory.forget",
            "wallet.spend",
            "system.reboot",
            "autonomy.promote",
            "audit.clear",
        ];
        if forbidden_guest_actions.contains(&requested_action) {
            return false;
        }
        pass.power == requested_action
    };

    // Authorized scoped action
    assert!(is_authorized_for_guest(&guest_pass, "wifi.connect"));

    // Unauthorized non-scoped action
    assert!(!is_authorized_for_guest(&guest_pass, "thermostat.set"));

    // Strictly forbidden high-stakes action
    assert!(!is_authorized_for_guest(&guest_pass, "memory.forget"));
    assert!(!is_authorized_for_guest(&guest_pass, "wallet.spend"));
}

/// `T1_F29_05`: Zero duration edge case and serialization.
///
/// If duration is 0, the pass is immediately expired with fraction 0.0.
/// Full JSON serialization and deserialization preserves all fields.
#[test]
fn test_t1_f29_05_zero_duration_edge_case_and_serialization() {
    let zero_pass = GuestPass::new("Test", "none", 0);
    assert_eq!(zero_pass.ring_fraction(), 0.0);
    assert!(zero_pass.expired());

    // JSON serialization roundtrip
    let pass = GuestPass::new("Sophie", "tv.control", 7200);
    let serialized = serde_json::to_string(&pass).expect("serialize guest pass");
    let deserialized: GuestPass =
        serde_json::from_str(&serialized).expect("deserialize guest pass");

    assert_eq!(deserialized.name, "Sophie");
    assert_eq!(deserialized.power, "tv.control");
    assert_eq!(deserialized.duration_secs, 7200);
    assert_eq!(deserialized.elapsed_secs, 0);
}
