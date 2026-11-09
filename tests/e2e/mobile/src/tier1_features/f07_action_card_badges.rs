//! Feature 07: ActionCard with 3 Badges (Milestone M1 / Spec §3, §8)
//!
//! Specifications: T1_F07_01 to T1_F07_05
//!
//! Verifies:
//! - Strict prescribed order of the 3 badges (Cost, DataExit, Reversibility).
//! - DataExit badge label formatting (Local vs Cloud with optional weight in Ko).
//! - Reversibility badge text and sign ('− annulable' vs '+ définitif', never color alone).
//! - Single decision-required card constraint (at most one open card at a time).
//! - ActionCard delivery and attribute preservation in MobileSnapshot.

use crate::harness::{DaemonStep, MobileTestRig};
use hakille_design::action_card::{ActionCardView, DataPastille, Reversibility};
use hakille_ui_protocol::{DataExit, KernelToUi, Money, Provenance};
use serde_json::json;

/// `T1_F07_01`: Prescribed order of the three pastilles.
///
/// ActionCardView::pastilles() returns exactly `[Cost, DataExit, Reversibility]`
/// in that strict invariable order for sovereign cognitive accessibility.
#[test]
fn test_t1_f07_01_prescribed_order_of_three_pastilles() {
    let card = ActionCardView {
        verb: "Virer".into(),
        object: "50 € à Bob".into(),
        reason: "Remboursement convenu".into(),
        why_trace: "Trace d'audit #42".into(),
        cost_label: "0,00 €".into(),
        data: DataPastille::Local,
        reversibility: Reversibility::Undoable { undo_seconds: 30 },
        needs_hold: false,
        provenance_label: "Hakille".into(),
        is_external: false,
    };

    let pastilles = card.pastilles();
    assert_eq!(pastilles[0], "0,00 €", "First badge must strictly be Cost");
    assert_eq!(
        pastilles[1], "Local",
        "Second badge must strictly be DataExit"
    );
    assert_eq!(
        pastilles[2], "− annulable",
        "Third badge must strictly be Reversibility"
    );
}

/// `T1_F07_02`: DataExit pastille label formatting.
///
/// Formats "Local" for on-device processing, "Cloud : {provider}" for cloud without weight,
/// and "Cloud : {provider} (≈ {weight} Ko)" when upload weight is estimated.
#[test]
fn test_t1_f07_02_data_exit_pastille_formatting() {
    let local = DataPastille::Local;
    assert_eq!(local.label(), "Local");

    let cloud_simple = DataPastille::Cloud {
        provider: "Mistral".into(),
        weight_ko: None,
    };
    assert_eq!(cloud_simple.label(), "Cloud : Mistral");

    let cloud_weighted = DataPastille::Cloud {
        provider: "OpenAI".into(),
        weight_ko: Some(128),
    };
    assert_eq!(cloud_weighted.label(), "Cloud : OpenAI (≈ 128 Ko)");
}

/// `T1_F07_03`: Reversibility badge text and sign requirements.
///
/// To prevent accessibility failure through color blindness, reversibility
/// displays both the mathematical sign and label: "− annulable" or "+ définitif".
#[test]
fn test_t1_f07_03_reversibility_badge_text_and_sign() {
    let undoable = Reversibility::Undoable { undo_seconds: 60 };
    assert_eq!(undoable.label(), "− annulable");
    assert!(
        undoable.label().starts_with('−'),
        "Undoable must begin with minus sign"
    );

    let irreversible = Reversibility::Final;
    assert_eq!(irreversible.label(), "+ définitif");
    assert!(
        irreversible.label().starts_with('+'),
        "Final must begin with plus sign"
    );
}

/// `T1_F07_04`: Single decision-required card constraint.
///
/// The sovereign UI strictly forbids displaying multiple decision-required cards simultaneously
/// to eliminate prompt-bombing and decision fatigue.
#[test]
fn test_t1_f07_04_single_decision_card_constraint() {
    let make_card = |id: u64| ActionCardView {
        verb: format!("Action {id}"),
        object: "cible".into(),
        reason: "raison".into(),
        why_trace: "trace".into(),
        cost_label: "0 €".into(),
        data: DataPastille::Local,
        reversibility: Reversibility::Final,
        needs_hold: true,
        provenance_label: "Hakille".into(),
        is_external: false,
    };

    let empty: [ActionCardView; 0] = [];
    assert!(
        ActionCardView::single_decision_open(&empty),
        "0 open cards is permitted"
    );

    let one_card = [make_card(1)];
    assert!(
        ActionCardView::single_decision_open(&one_card),
        "Exactly 1 open decision card is permitted"
    );

    let two_cards = [make_card(1), make_card(2)];
    assert!(
        !ActionCardView::single_decision_open(&two_cards),
        ">1 open decision cards must be rejected by shell guard"
    );
}

/// `T1_F07_05`: ActionCard delivery and attribute preservation in MobileSnapshot.
///
/// Receiving ShowCard from gateway populates snapshot.cards with verbatim metadata:
/// id, verb, object, reason, cost, data_exit, reversible, and provenance.
#[tokio::test]
async fn test_t1_f07_05_action_card_arrival_in_mobile_bridge() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let custom_card = hakille_ui_protocol::ActionCard {
        id: 42,
        verb: "Supprimer".into(),
        object: "compte temporaire".into(),
        reason: "Fin de période d'essai".into(),
        cost: Money::from_cents(500, "EUR"),
        data_exit: DataExit::Cloud {
            provider: "OVHcloud".into(),
        },
        reversible: false,
        provenance: Provenance::VerifiedSkill {
            publisher: "Hakille".into(),
            name: "cleanup".into(),
        },
    };

    let daemon_task = tokio::spawn({
        let c = custom_card.clone();
        async move {
            let mut stream = daemon.accept().await.expect("accept");
            let step = DaemonStep::status().after(vec![KernelToUi::ShowCard(c)]);
            daemon
                .handle_step(&mut stream, 1, step)
                .await
                .expect("status with showcard");
        }
    });

    let (status, result) = rig
        .roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;

    assert_eq!(status, 200);
    assert_eq!(result["ok"], true);

    let snapshot = rig.snapshot(&bridge);
    let cards = snapshot["cards"].as_array().expect("cards array");
    assert_eq!(cards.len(), 1);

    let received = &cards[0];
    assert_eq!(received["id"], 42);
    assert_eq!(received["verb"], "Supprimer");
    assert_eq!(received["object"], "compte temporaire");
    assert_eq!(received["reason"], "Fin de période d'essai");
    assert_eq!(received["reversible"], false);
    assert_eq!(
        received["data_exit"],
        json!({"Cloud": {"provider": "OVHcloud"}})
    );

    daemon_task.await.unwrap();
}
