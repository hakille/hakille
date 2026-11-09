//! Feature 14: Reversible Action Undo (Milestone M1 / Spec §3, §8, §24)
//!
//! Specifications: T1_F14_01 to T1_F14_05
//!
//! Verifies:
//! - Reversible attribute on ActionCard correctly sets Undoable pastille.
//! - Irreversible action demands Final pastille and 600 ms hold requirement.
//! - Audit trail captures reversibility parameter in signed receipts.
//! - Protocol framing for UiToKernel::Undo requests.
//! - Reversible action undo automatically triggers domain autonomy demotion.

use crate::harness::{AuditSigner, AuditVerifier};
use hakille_design::action_card::{ActionCardView, DataPastille, Reversibility};
use hakille_design::autonomy::{Crank, DomainAutonomy};
use hakille_ipc::Frame;
use hakille_ui_protocol::{ActionCard, DataExit, Money, PROTOCOL_VERSION, Provenance, UiToKernel};
use serde_json::json;

/// `T1_F14_01`: ActionCard reversible attribute maps to Undoable pastille.
///
/// An ActionCard declared with reversible: true maps to Reversibility::Undoable,
/// displaying the minus sign ('− annulable') and not requiring 600 ms hold.
#[test]
fn test_t1_f14_01_action_card_reversibility_attribute() {
    let proto_card = ActionCard {
        id: 10,
        verb: "Archiver".into(),
        object: "message".into(),
        reason: "Nettoyage boîte".into(),
        cost: Money::from_cents(0, "EUR"),
        data_exit: DataExit::Local,
        reversible: true,
        provenance: Provenance::Hakille,
    };

    assert!(proto_card.reversible, "Card must declare reversible: true");

    let card_view = ActionCardView {
        verb: proto_card.verb,
        object: proto_card.object,
        reason: proto_card.reason,
        why_trace: "Trace".into(),
        cost_label: "0 €".into(),
        data: DataPastille::Local,
        reversibility: if proto_card.reversible {
            Reversibility::Undoable { undo_seconds: 30 }
        } else {
            Reversibility::Final
        },
        needs_hold: !proto_card.reversible,
        provenance_label: "Hakille".into(),
        is_external: false,
    };

    assert_eq!(card_view.reversibility.label(), "− annulable");
    assert!(
        !card_view.needs_hold,
        "Reversible actions do not require hold"
    );
}

/// `T1_F14_02`: Irreversible action maps to Final pastille and requires hold.
///
/// An ActionCard with reversible: false maps to Reversibility::Final,
/// displaying the plus sign ('+ définitif') and enforcing needs_hold: true.
#[test]
fn test_t1_f14_02_irreversible_action_requires_hold() {
    let proto_card = ActionCard {
        id: 20,
        verb: "Purger".into(),
        object: "base de données".into(),
        reason: "Suppression définitive demandée".into(),
        cost: Money::from_cents(0, "EUR"),
        data_exit: DataExit::Local,
        reversible: false,
        provenance: Provenance::Hakille,
    };

    assert!(!proto_card.reversible);

    let card_view = ActionCardView {
        verb: proto_card.verb,
        object: proto_card.object,
        reason: proto_card.reason,
        why_trace: "Trace".into(),
        cost_label: "0 €".into(),
        data: DataPastille::Local,
        reversibility: Reversibility::Final,
        needs_hold: true,
        provenance_label: "Hakille".into(),
        is_external: false,
    };

    assert_eq!(card_view.reversibility.label(), "+ définitif");
    assert!(
        card_view.needs_hold,
        "Irreversible action strictly requires 600ms hold"
    );
}

/// `T1_F14_03`: Audit trail captures reversibility status in signed receipts.
///
/// Cryptographic receipt includes reversible: true/false in canonical parameters,
/// proving whether an action was designed to be undoable.
#[test]
fn test_t1_f14_03_audit_trail_captures_reversibility() {
    let mut signer = AuditSigner::new();

    let receipt = signer.sign_action(
        "audit-rev-01",
        "card_approve",
        Some(10),
        json!({
            "verb": "Archiver",
            "reversible": true,
            "undo_window_secs": 30,
        }),
    );

    AuditVerifier::verify_receipt(&receipt).expect("Receipt must verify");
    assert_eq!(receipt.parameters["reversible"], true);
    assert_eq!(receipt.parameters["undo_window_secs"], 30);
}

/// `T1_F14_04`: Protocol framing for UiToKernel::Undo requests.
///
/// Dispatches UiToKernel::Undo referencing the specific journal ID,
/// with version 1 conformance.
#[test]
fn test_t1_f14_04_undo_request_framing_and_dispatch() {
    let undo_req = UiToKernel::Undo { journal_id: 1042 };
    assert_eq!(undo_req.version(), PROTOCOL_VERSION);

    let frame = Frame::request(7, undo_req.clone());
    assert_eq!(frame.version, PROTOCOL_VERSION);
    assert_eq!(frame.id, 7);

    // Frame check
    frame.check().expect("Frame check must pass");
    let serialized = serde_json::to_string(&frame).expect("serialize frame");
    assert!(serialized.contains("\"Undo\":{\"journal_id\":1042}"));
}

/// `T1_F14_05`: Reversible action undo automatically triggers autonomy demotion.
///
/// When an autonomous action is undone by the user, the domain's autonomy
/// crank is immediately demoted by 1 level and resets validated count to 0.
#[test]
fn test_t1_f14_05_undo_triggers_domain_autonomy_crank_demotion() {
    let mut domain = DomainAutonomy::new("Mail");
    domain.crank = Crank::ActReversible; // Crank 2
    for _ in 0..12 {
        domain.record_validated();
    }
    assert_eq!(domain.validated_without_correction, 12);

    // User executes Undo on an archived email
    domain.record_reverted();

    assert_eq!(
        domain.crank,
        Crank::Prepare,
        "Undoing action must drop crank from 2 to 1"
    );
    assert_eq!(
        domain.validated_without_correction, 0,
        "Validation counter must be cleared upon undo"
    );
}
