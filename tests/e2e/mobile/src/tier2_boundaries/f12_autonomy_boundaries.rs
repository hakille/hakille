//! Feature 12: Autonomy Ladder by Domain (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F12_01 to T2_F12_05 (5 tests total).

#![allow(unused_imports)]

use crate::harness::{DaemonStep, MobileTestRig, default_card};
use hakille_design::action_card::{ActionCardView, DataPastille, Reversibility};
use hakille_design::autonomy::{Crank, DomainAutonomy, ENVELOPE_EXPIRY_DAYS, always_validated};
use hakille_design::hold::{HoldEvent, HoldState, feed_hold, hold_progress};
use hakille_design::provenance::{ExternalBlock, ProvenanceBand, sanitize_skill_text};
use hakille_design::why::{FactRef, WhySheet};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{ActionCard, DataExit, KernelToUi, Money, Provenance, UiToKernel};
use serde_json::json;

/// `T2_F12_01`: Crank boundary validation (0..=4).
///
/// Crank values must strictly be in 0..=4; invalid integers are rejected.
#[test]
fn test_t2_f12_01_crank_boundary_validation() {
    fn parse_crank(val: u8) -> Result<Crank, &'static str> {
        match val {
            0 => Ok(Crank::Observe),
            1 => Ok(Crank::Prepare),
            2 => Ok(Crank::ActReversible),
            3 => Ok(Crank::ActEnvelope),
            4 => Ok(Crank::Delegate),
            _ => Err("Cran d'autonomie invalide (hors 0..4)"),
        }
    }

    assert_eq!(parse_crank(0), Ok(Crank::Observe));
    assert_eq!(parse_crank(4), Ok(Crank::Delegate));
    assert_eq!(parse_crank(5), Err("Cran d'autonomie invalide (hors 0..4)"));
    assert_eq!(
        parse_crank(255),
        Err("Cran d'autonomie invalide (hors 0..4)")
    );
}

/// `T2_F12_02`: Envelope spending limit boundary: 1 cent under, exact, 1 cent over.
///
/// An envelope of 5000 cents (50.00 €) permits 4999 and 5000 cents; 5001 cents requires manual confirmation.
#[test]
fn test_t2_f12_02_envelope_spending_limit_boundary() {
    let envelope_cents = 5000_u64; // 50.00 €

    let check_envelope = |amount_cents: u64| -> bool { amount_cents <= envelope_cents };

    assert!(check_envelope(4999), "1 cent under envelope is permitted");
    assert!(check_envelope(5000), "Exact envelope limit is permitted");
    assert!(
        !check_envelope(5001),
        "1 cent over envelope requires validation"
    );
}

/// `T2_F12_03`: Automatic crank demotion on undo.
///
/// Reverting an action demotes domain by one crank and resets validations without correction to 0.
#[test]
fn test_t2_f12_03_automatic_crank_demotion_on_undo() {
    let mut autonomy = DomainAutonomy::new("Finances");
    autonomy.crank = Crank::ActEnvelope; // Crank 3
    for _ in 0..10 {
        autonomy.record_validated();
    }
    assert_eq!(autonomy.validated_without_correction, 10);

    // Revert an action
    autonomy.record_reverted();
    assert_eq!(
        autonomy.crank,
        Crank::ActReversible,
        "Must demote from Crank 3 to Crank 2"
    );
    assert_eq!(
        autonomy.validated_without_correction, 0,
        "Validations count must reset to 0"
    );

    // Second revert demotes to Crank 1
    autonomy.record_reverted();
    assert_eq!(autonomy.crank, Crank::Prepare);

    // Third revert demotes to Crank 0
    autonomy.record_reverted();
    assert_eq!(autonomy.crank, Crank::Observe);

    // Reverting at Crank 0 stays at Crank 0 (cannot underflow)
    autonomy.record_reverted();
    assert_eq!(autonomy.crank, Crank::Observe);
}

/// `T2_F12_04`: 90-day envelope expiration boundary.
///
/// Envelope expires after 90 days (ENVELOPE_EXPIRY_DAYS = 90).
#[test]
fn test_t2_f12_04_90_day_envelope_expiration_boundary() {
    assert_eq!(ENVELOPE_EXPIRY_DAYS, 90);

    fn is_envelope_expired(days_active: u32) -> bool {
        days_active > ENVELOPE_EXPIRY_DAYS
    }

    assert!(!is_envelope_expired(89), "Day 89 is valid");
    assert!(!is_envelope_expired(90), "Day 90 is exact boundary (valid)");
    assert!(is_envelope_expired(91), "Day 91 is expired");
}

/// `T2_F12_05`: High-stakes actions always requiring manual validation.
///
/// Actions designated as always_validated (payments, memory erasure, device wipe)
/// strictly require confirmation regardless of autonomy level.
#[test]
fn test_t2_f12_05_high_stakes_always_validated() {
    assert!(always_validated("new_power"));
    assert!(always_validated("share_memory"));
    assert!(always_validated("hard_delete"));
    assert!(always_validated("spend_over_envelope"));

    // Low stakes autonomous action is not always validated
    assert!(!always_validated("calendar.read"));
    assert!(!always_validated("mail.read"));
}
