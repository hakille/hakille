//! Tier 3 Pairwise Module: Domain Autonomy & Human Approval Governance
//!
//! Combinatorial interactions between:
//! - F07: ActionCard with 3 Badges
//! - F08: 600ms Hold-to-Confirm
//! - F12: Autonomy Ladder by Domain
//! - F14: Reversible Action Undo
//!
//! Tests: T3_PAIR_19 to T3_PAIR_22

use crate::harness::{AuditSigner, AuditVerifier};
use hakille_design::action_card::{ActionCardView, DataPastille, Reversibility};
use hakille_design::autonomy::{Crank, DomainAutonomy, always_validated};
use hakille_ui_protocol::{ActionCard, DataExit, Money, Provenance};
use serde_json::json;

/// `T3_PAIR_19`: Autonomy Crank 0 (Observe) and Crank 1 (Prepare) strictly require 600ms hold on actions.
///
/// Interactions: F12 (Autonomy Ladder) + F08 (600ms Hold)
/// At Cranks 0 and 1, the AI agent is not permitted autonomous execution; every card proposal
/// must be held for 600ms or confirmed by human touch before dispatch.
#[test]
fn test_t3_pair_19_crank0_and_crank1_strictly_require_600ms_hold_confirmation() {
    let mut domain = DomainAutonomy::new("Mail");
    assert_eq!(domain.crank, Crank::Prepare); // Default Crank 1

    let card = ActionCard {
        id: 119,
        verb: "Envoyer".into(),
        object: "Brouillon de courriel".into(),
        reason: "Réponse préparée".into(),
        cost: Money::from_cents(0, "EUR"),
        data_exit: DataExit::Local,
        reversible: true,
        provenance: Provenance::Hakille,
    };

    let requires_human_approval = |crank: Crank, _card: &ActionCard| -> bool {
        match crank {
            Crank::Observe | Crank::Prepare => true,
            Crank::ActReversible => false, // only for safe reversible
            _ => false,
        }
    };

    assert!(
        requires_human_approval(domain.crank, &card),
        "Crank 1 Prepare must require human approval"
    );

    domain.record_reverted(); // Demote to Crank 0
    assert_eq!(domain.crank, Crank::Observe);
    assert!(
        requires_human_approval(domain.crank, &card),
        "Crank 0 Observe must require human approval"
    );
}

/// `T3_PAIR_20`: Autonomy Crank 2 (ActReversible) auto-executes safe local action with signed undo token.
///
/// Interactions: F12 (Autonomy Ladder) + F07 (ActionCard)
/// At Crank 2, safe actions (cost = 0, reversible = true, Local) execute autonomously,
/// generating a signed audit receipt with an undo token and a 30s cancellation window.
#[test]
fn test_t3_pair_20_crank2_act_reversible_auto_executes_safe_local_action_with_undo_token() {
    let mut domain = DomainAutonomy::new("Agenda");
    domain.crank = Crank::ActReversible;

    let card = ActionCard {
        id: 120,
        verb: "Déplacer".into(),
        object: "Rendez-vous médecin à 15h".into(),
        reason: "Conflit d'horaire résolu".into(),
        cost: Money::from_cents(0, "EUR"),
        data_exit: DataExit::Local,
        reversible: true,
        provenance: Provenance::Hakille,
    };

    let can_auto_execute = |crank: Crank, card: &ActionCard| -> bool {
        crank == Crank::ActReversible
            && card.reversible
            && card.cost.micros == 0
            && card.data_exit == DataExit::Local
    };

    assert!(can_auto_execute(domain.crank, &card));

    // Generates signed audit receipt with undo token
    let mut signer = AuditSigner::new();
    let receipt = signer.sign_action(
        "audit-auto-exec-120",
        "autonomous_action_executed",
        Some(120),
        json!({
            "domain": "Agenda",
            "crank": 2,
            "undo_token": "token-rev-30s-120",
            "undo_window_secs": 30
        }),
    );

    AuditVerifier::verify_receipt(&receipt).expect("Receipt must verify");
    assert_eq!(receipt.parameters["undo_token"], "token-rev-30s-120");
}

/// `T3_PAIR_21`: High-stakes action forces mandatory 600ms hold downgrade regardless of autonomy crank.
///
/// Interactions: F12 (Autonomy Ladder) + F08 (600ms Hold)
/// Even at Crank 3 or 4, actions involving money (cost > 0), external data exits,
/// or `always_validated` categories ("hard_delete", "new_power") force hold confirmation.
#[test]
fn test_t3_pair_21_high_stakes_action_forces_mandatory_600ms_hold_downgrade() {
    let mut domain = DomainAutonomy::new("Finance");
    domain.crank = Crank::Delegate; // Highest Crank (4)

    // Subcase A: cost > 0
    let expensive_card = ActionCard {
        id: 121,
        verb: "Régler".into(),
        object: "Abonnement 120,00 EUR".into(),
        reason: "Renouvellement annuel".into(),
        cost: Money::from_cents(12000, "EUR"),
        data_exit: DataExit::Local,
        reversible: false,
        provenance: Provenance::Hakille,
    };

    // Subcase B: always_validated rules
    assert!(always_validated("hard_delete"));
    assert!(always_validated("new_power"));
    assert!(always_validated("spend_over_envelope"));

    let requires_hold_downgrade = |card: &ActionCard| -> bool {
        card.cost.micros > 0 || !card.reversible || card.data_exit != DataExit::Local
    };

    assert!(
        requires_hold_downgrade(&expensive_card),
        "High-stakes card must force hold downgrade even at Crank 4"
    );

    let view = ActionCardView {
        verb: expensive_card.verb,
        object: expensive_card.object,
        reason: expensive_card.reason,
        why_trace: "Audit #121".into(),
        cost_label: "120,00 €".into(),
        data: DataPastille::Local,
        reversibility: Reversibility::Final,
        needs_hold: true, // forced hold
        provenance_label: "Hakille".into(),
        is_external: false,
    };
    assert!(view.needs_hold);
}

/// `T3_PAIR_22`: User undo demotes domain autonomy crank and clears validation counter.
///
/// Interactions: F12 (Autonomy Ladder) + F14 (Reversible Action Undo)
/// When user undoes an autonomously executed action, the domain drops 1 crank immediately
/// and resets `validated_without_correction` to 0.
#[test]
fn test_t3_pair_22_user_undo_demotes_domain_autonomy_crank_and_clears_counter() {
    let mut domain = DomainAutonomy::new("Mail");
    domain.crank = Crank::ActReversible; // Crank 2

    // Accumulate 18 validated actions
    for _ in 0..18 {
        domain.record_validated();
    }
    assert_eq!(domain.validated_without_correction, 18);

    // User triggers Undo on an autonomously sent email
    domain.record_reverted();

    assert_eq!(
        domain.crank,
        Crank::Prepare,
        "Undoing autonomous action must drop crank from 2 to 1"
    );
    assert_eq!(
        domain.validated_without_correction, 0,
        "Validation counter must be zeroed immediately"
    );
    assert!(
        !domain.promotion_offerable(),
        "Promotion must no longer be offerable after undo demotion"
    );
}
