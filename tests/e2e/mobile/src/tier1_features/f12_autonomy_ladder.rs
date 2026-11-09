//! Feature 12: Autonomy Ladder by Domain (Milestone M1 / Spec §3, §24)
//!
//! Specifications: T1_F12_01 to T1_F12_05
//!
//! Verifies:
//! - 5 discrete cranks (0: Observe to 4: Delegate) with explicit French labels.
//! - Default starting point at Crank 1 (Prepare) for every newly tracked domain.
//! - Promotion qualification requiring 20 validated actions without correction.
//! - Immediate demotion upon user undo/revert of an autonomous action.
//! - High-stakes actions that are always validated regardless of autonomy crank.

use hakille_design::autonomy::{
    Crank, DomainAutonomy, ENVELOPE_EXPIRY_DAYS, PROMOTION_VALIDATED_ACTIONS,
    PROMOTION_WINDOW_DAYS, always_validated,
};

/// `T1_F12_01`: Five discrete cranks and descriptive labels.
///
/// Verifies each crank level: 0: Observer, 1: Préparer, 2: Agir réversible,
/// 3: Agir enveloppe, 4: Déléguer.
#[test]
fn test_t1_f12_01_five_cranks_labels_and_values() {
    assert_eq!(Crank::Observe as u8, 0);
    assert_eq!(Crank::Prepare as u8, 1);
    assert_eq!(Crank::ActReversible as u8, 2);
    assert_eq!(Crank::ActEnvelope as u8, 3);
    assert_eq!(Crank::Delegate as u8, 4);

    assert_eq!(Crank::Observe.label(), "Cran 0 sur 4, observer");
    assert_eq!(Crank::Prepare.label(), "Cran 1 sur 4, préparer");
    assert_eq!(
        Crank::ActReversible.label(),
        "Cran 2 sur 4, agir sur le réversible"
    );
    assert_eq!(
        Crank::ActEnvelope.label(),
        "Cran 3 sur 4, agir dans une enveloppe"
    );
    assert_eq!(Crank::Delegate.label(), "Cran 4 sur 4, déléguer");
}

/// `T1_F12_02`: Default domain starts at Crank 1 (Prepare).
///
/// Every domain is isolated and starts strictly at Crank 1. No domain starts
/// at autonomous execution (Crank 2+) out of the box.
#[test]
fn test_t1_f12_02_default_domain_starts_at_crank_1() {
    let mail_autonomy = DomainAutonomy::new("Mail");
    assert_eq!(mail_autonomy.crank, Crank::Prepare);
    assert_eq!(mail_autonomy.domain.0, "Mail");
    assert_eq!(mail_autonomy.validated_without_correction, 0);
    assert!(mail_autonomy.envelope.is_none());

    let finance_autonomy = DomainAutonomy::new("Finance");
    assert_eq!(finance_autonomy.crank, Crank::Prepare);
}

/// `T1_F12_03`: Promotion qualification requires 20 validated actions.
///
/// Promotion to the next crank is never imposed; it is offerable only after 20
/// actions are validated without user correction within the 30-day window.
#[test]
fn test_t1_f12_03_promotion_requires_20_validated_actions() {
    assert_eq!(PROMOTION_VALIDATED_ACTIONS, 20);
    assert_eq!(PROMOTION_WINDOW_DAYS, 30);

    let mut autonomy = DomainAutonomy::new("Agenda");

    // 19 actions validated: not yet offerable
    for _ in 0..19 {
        autonomy.record_validated();
    }
    assert_eq!(autonomy.validated_without_correction, 19);
    assert!(
        !autonomy.promotion_offerable(),
        "19 validations must not trigger promotion offer"
    );

    // 20th action validated: promotion becomes offerable
    autonomy.record_validated();
    assert_eq!(autonomy.validated_without_correction, 20);
    assert!(
        autonomy.promotion_offerable(),
        "20 validations must trigger promotion offer"
    );
}

/// `T1_F12_04`: Immediate demotion upon user undo/reversal.
///
/// When an autonomous action is reverted or cancelled by the user, the domain
/// immediately drops one crank level and resets validated count to 0.
#[test]
fn test_t1_f12_04_instant_demotion_on_undo_reversal() {
    let mut autonomy = DomainAutonomy::new("Maison");
    autonomy.crank = Crank::ActEnvelope; // Crank 3
    for _ in 0..15 {
        autonomy.record_validated();
    }

    // User reverts an action
    autonomy.record_reverted();

    assert_eq!(
        autonomy.crank,
        Crank::ActReversible,
        "Crank 3 must demote to Crank 2"
    );
    assert_eq!(
        autonomy.validated_without_correction, 0,
        "Validation counter must be zeroed"
    );

    // Another revert
    autonomy.record_reverted();
    assert_eq!(
        autonomy.crank,
        Crank::Prepare,
        "Crank 2 must demote to Crank 1"
    );

    // Revert down to Observe
    autonomy.record_reverted();
    assert_eq!(autonomy.crank, Crank::Observe);

    // Floor is Crank 0 (cannot drop below 0)
    autonomy.record_reverted();
    assert_eq!(autonomy.crank, Crank::Observe);
}

/// `T1_F12_05`: High-stakes actions that are always validated.
///
/// Four critical actions require explicit user confirmation regardless of crank level:
/// new_power, share_memory, hard_delete, and spend_over_envelope.
#[test]
fn test_t1_f12_05_always_validated_high_stakes_actions() {
    assert!(
        always_validated("new_power"),
        "Adding new capability must be validated"
    );
    assert!(
        always_validated("share_memory"),
        "Sharing sovereign memory must be validated"
    );
    assert!(
        always_validated("hard_delete"),
        "Definitive deletion must be validated"
    );
    assert!(
        always_validated("spend_over_envelope"),
        "Budget overshoot must be validated"
    );

    // Regular low-stakes actions can be autonomous
    assert!(!always_validated("archive_email"));
    assert!(!always_validated("set_alarm"));
    assert!(!always_validated("read_calendar"));

    assert_eq!(
        ENVELOPE_EXPIRY_DAYS, 90,
        "Autonomous envelope expires after 90 days"
    );
}
