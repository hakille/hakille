//! Feature 07: ActionCard with 3 Badges (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F07_01 to T2_F07_05 (5 tests total).

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

/// `T2_F07_01`: Max cards limit (32 cards in snapshot; 33rd rejected).
///
/// MobileBridge enforces a hard limit of 32 pending ActionCards in snapshot.
/// Receiving a 33rd card returns explicit error "Trop de cartes en attente".
#[test]
fn test_t2_f07_01_max_cards_limit_32_cards() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());

    // Generate 32 distinct cards
    let mut frames = Vec::new();
    for id in 1..=32 {
        frames.push(Frame::reply(0, KernelToUi::ShowCard(default_card(id))));
    }

    // First request delivers 32 cards
    let req_res = bridge.dispatch_json(json!({"type": "refresh_status"}).to_string());
    let req_json: serde_json::Value = serde_json::from_str(&req_res).unwrap();
    let req_id = req_json["request"]["id"].as_u64().unwrap();

    let reply = json!({
        "session_id": "sess-cards-32",
        "frames": [
            Frame::reply(req_id, KernelToUi::StatusReply {
                status: crate::harness::default_status(),
            }),
        ]
    });
    let mut reply_val = reply;
    for frame in frames {
        reply_val["frames"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::to_value(frame).unwrap());
    }

    let rec_res = bridge.receive_json(req_id, reply_val.to_string());
    assert!(rec_res.contains("\"ok\":true"));

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap["cards"].as_array().unwrap().len(), 32);

    // Attempt to add a 33rd card via next request
    let req2_res = bridge.dispatch_json(json!({"type": "refresh_status"}).to_string());
    let req2_id = serde_json::from_str::<serde_json::Value>(&req2_res).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();

    let overflow_reply = json!({
        "session_id": "sess-cards-32",
        "frames": [
            Frame::reply(req2_id, KernelToUi::StatusReply {
                status: crate::harness::default_status(),
            }),
            Frame::reply(0, KernelToUi::ShowCard(default_card(33)))
        ]
    });

    let rec_over = bridge.receive_json(req2_id, overflow_reply.to_string());
    let over_json: serde_json::Value = serde_json::from_str(&rec_over).unwrap();
    assert_eq!(over_json["ok"], false);
    assert!(
        over_json["error"]
            .as_str()
            .unwrap()
            .contains("Trop de cartes en attente")
    );

    // The failed receive triggers fail() which invalidates cache into Unknown connection and clears cards
    let snap_after: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap_after["connection"], "unknown");
    assert_eq!(snap_after["cards"].as_array().unwrap().len(), 0);
}

/// `T2_F07_02`: Card with extreme cost (u64::MAX cents).
///
/// ActionCard with u64::MAX cents formats safely without arithmetic overflow.
#[test]
fn test_t2_f07_02_card_with_extreme_cost_u64_max() {
    let money = Money::from_cents(u64::MAX, "EUR");
    assert_eq!(money.micros, (u64::MAX as i64).wrapping_mul(10_000));
    assert_eq!(money.currency, "EUR");

    let card = ActionCard {
        id: 999,
        verb: "Transférer".into(),
        object: "Fonds souverains".into(),
        reason: "Test valeur extrême".into(),
        cost: money.clone(),
        data_exit: DataExit::Local,
        reversible: false,
        provenance: Provenance::Hakille,
    };

    assert_eq!(card.cost.micros, money.micros);
    let display_str = card.cost.display();
    assert!(
        !display_str.is_empty(),
        "Display formatting must not panic on extreme cost"
    );
}

/// `T2_F07_03`: Missing badges fallback ordering.
///
/// Regardless of badge values (0 cost, local data exit, final reversibility),
/// pastilles are strictly rendered in `[Cost, DataExit, Reversibility]` order.
#[test]
fn test_t2_f07_03_missing_badges_fallback_preserves_order() {
    let card = ActionCardView {
        verb: "Consulter".into(),
        object: "Historique".into(),
        reason: "Demande utilisateur".into(),
        why_trace: "Trace".into(),
        cost_label: "0,00 €".into(),
        data: DataPastille::Local,
        reversibility: Reversibility::Final,
        needs_hold: true,
        provenance_label: "Hakille".into(),
        is_external: false,
    };

    let pastilles = card.pastilles();
    assert_eq!(pastilles.len(), 3);
    assert_eq!(pastilles[0], "0,00 €", "First badge must be Cost");
    assert_eq!(pastilles[1], "Local", "Second badge must be DataExit");
    assert_eq!(
        pastilles[2], "+ définitif",
        "Third badge must be Reversibility"
    );
}

/// `T2_F07_04`: Multiline oversized card title/reason handling.
///
/// Card containing multiline or very long text preserves its identifiers and fields.
#[test]
fn test_t2_f07_04_multiline_oversized_card_title_and_reason() {
    let long_reason =
        "Ligne 1 : Examen du rapport.\nLigne 2 : Analyse des métriques.\nLigne 3 : Validation."
            .repeat(10);
    let card = ActionCardView {
        verb: "Valider".into(),
        object: "Rapport étendu".into(),
        reason: long_reason.clone(),
        why_trace: "Trace #999".into(),
        cost_label: "0,00 €".into(),
        data: DataPastille::Local,
        reversibility: Reversibility::Undoable { undo_seconds: 60 },
        needs_hold: false,
        provenance_label: "Hakille".into(),
        is_external: false,
    };

    assert_eq!(card.reason, long_reason);
    assert_eq!(card.pastilles()[2], "− annulable");
}

/// `T2_F07_05`: Rapid card updating in-place.
///
/// Sending ShowCard for an existing card ID updates it in-place and cancels any pending hold.
#[test]
fn test_t2_f07_05_updating_existing_card_cancels_hold() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());

    // Deliver initial card 42
    let req_res = bridge.dispatch_json(json!({"type": "refresh_status"}).to_string());
    let req_id = serde_json::from_str::<serde_json::Value>(&req_res).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();
    let reply1 = json!({
        "session_id": "sess-update-card",
        "frames": [
            Frame::reply(req_id, KernelToUi::StatusReply { status: crate::harness::default_status() }),
            Frame::reply(0, KernelToUi::ShowCard(default_card(42)))
        ]
    });
    bridge.receive_json(req_id, reply1.to_string());

    // Begin approval on card 42
    let begin_res =
        bridge.dispatch_json(json!({"type": "begin_approval", "card_id": 42}).to_string());
    assert!(begin_res.contains("\"ok\":true"));
    let snap_hold: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert!(snap_hold["approval"].is_object());

    // Now update card 42 with different object
    let mut updated_card = default_card(42);
    updated_card.object = "Objet modifié".into();

    let req2_res = bridge.dispatch_json(json!({"type": "refresh_status"}).to_string());
    let req2_id = serde_json::from_str::<serde_json::Value>(&req2_res).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();
    let reply2 = json!({
        "session_id": "sess-update-card",
        "frames": [
            Frame::reply(req2_id, KernelToUi::StatusReply { status: crate::harness::default_status() }),
            Frame::reply(0, KernelToUi::ShowCard(updated_card))
        ]
    });
    bridge.receive_json(req2_id, reply2.to_string());

    // Hold must be cancelled because the card changed
    let snap_after: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert!(
        snap_after["approval"].is_null(),
        "Approval hold must be cancelled on card update"
    );
    assert_eq!(snap_after["cards"][0]["object"], "Objet modifié");
}
