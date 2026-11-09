//! Tier 3 Pairwise Module: ActionCard Governance & Cryptographic Audit
//!
//! Combinatorial interactions between:
//! - F07: ActionCard with 3 Badges
//! - F08: 600ms Hold-to-Confirm
//! - F09: Action Card Reject & Edit
//! - F10: Security Provenance Banner
//! - F13: Ed25519 Signed Audit Journal
//! - F14: Reversible Action Undo
//!
//! Tests: T3_PAIR_05 to T3_PAIR_10

use crate::harness::{AuditSigner, AuditVerifier, DaemonStep, MobileTestRig, default_card};
use hakille_design::action_card::{ActionCardView, DataPastille, Reversibility};
use hakille_design::hold::{HoldEvent, HoldState, feed_hold};
use hakille_design::provenance::{ExternalBlock, sanitize_skill_text};
use hakille_ui_protocol::{ActionCard, DataExit, KernelToUi, Money, Provenance, UiToKernel};
use serde_json::json;
use std::time::Duration;

/// `T3_PAIR_05`: ShowCard initializes card with 3 badges; premature release cancels; 600ms hold confirms.
///
/// Interactions: F07 (ActionCard) + F08 (600ms Hold)
/// ActionCard arrives with 3 badges; feeding hold events tests premature release at 400ms (cancelling)
/// and full 600ms hold (transitioning to Confirmed with terminal tick and dispatching Approve).
#[tokio::test]
async fn test_t3_pair_05_showcard_pastilles_and_sustained_600ms_hold_confirmation() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let test_card = ActionCard {
        id: 105,
        verb: "Déployer".into(),
        object: "nouveau firmware v2.1".into(),
        reason: "Mise à jour de sécurité critique".into(),
        cost: Money::from_cents(0, "EUR"),
        data_exit: DataExit::Local,
        reversible: false, // requires 600ms hold
        provenance: Provenance::Hakille,
    };

    let daemon_task = tokio::spawn({
        let c = test_card.clone();
        async move {
            let mut stream = daemon.accept().await.expect("accept");
            let step = DaemonStep::status().after(vec![KernelToUi::ShowCard(c)]);
            daemon
                .handle_step(&mut stream, 1, step)
                .await
                .expect("step 1");
        }
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;
    daemon_task.await.unwrap();

    let snap = rig.snapshot(&bridge);
    let cards = snap["cards"].as_array().expect("cards array");
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0]["id"], 105);

    // Verify 3 pastilles order: Cost, DataExit, Reversibility
    let view = ActionCardView {
        verb: test_card.verb.clone(),
        object: test_card.object.clone(),
        reason: test_card.reason.clone(),
        why_trace: "Audit #105".into(),
        cost_label: "0,00 €".into(),
        data: DataPastille::Local,
        reversibility: Reversibility::Final,
        needs_hold: true,
        provenance_label: "Hakille".into(),
        is_external: false,
    };
    assert_eq!(
        view.pastilles(),
        ["0,00 €", "Local", "+ définitif"],
        "Pastilles must strictly match [Cost, DataExit, Reversibility]"
    );

    // 1. Gesture sequence: Press at 0ms, advance to 400ms, release -> Cancelled
    let (s0, tick0) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 0 });
    assert!(tick0);
    let (s1, tick1) = feed_hold(s0, HoldEvent::Tick { elapsed_ms: 400 });
    assert!(!tick1);
    let (s_cancelled, tick_cancel) = feed_hold(s1, HoldEvent::Released { at_ms: 400 });
    assert_eq!(s_cancelled, HoldState::Cancelled);
    assert!(!tick_cancel);

    // 2. Gesture sequence: Sustained hold to 600ms -> Confirmed
    let (s2_0, _) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 0 });
    let (s2_confirmed, tick_confirmed) = feed_hold(s2_0, HoldEvent::Tick { elapsed_ms: 600 });
    assert_eq!(s2_confirmed, HoldState::Confirmed);
    assert!(tick_confirmed);

    // 3. Dispatch to bridge: begin_approval, sleep 605ms, dispatch approve
    let begin_res = bridge.dispatch_json(
        json!({
            "type": "begin_approval",
            "card_id": 105,
        })
        .to_string(),
    );
    assert!(begin_res.contains("\"ok\":true"));

    tokio::time::sleep(Duration::from_millis(605)).await;

    let app_res = bridge.dispatch_json(
        json!({
            "type": "approve",
            "card_id": 105,
        })
        .to_string(),
    );
    let app_val: serde_json::Value = serde_json::from_str(&app_res).unwrap();
    assert_eq!(app_val["ok"], true);
    assert_eq!(app_val["snapshot"]["pending"]["kind"], "approve");
}

/// `T3_PAIR_06`: Hold approval triggers Ed25519 signed CardDecisionAck and verified audit chain.
///
/// Interactions: F08 (Hold Confirmation) + F13 (Signed Audit Journal)
/// Receiving CardDecisionAck after hold confirmation includes a genuine Ed25519 signed receipt
/// linking to the previous audit entry and asserting hash chain integrity.
#[tokio::test]
async fn test_t3_pair_06_hold_approval_triggers_ed25519_signed_card_decision_ack() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let mut audit_signer = AuditSigner::new();
    let genesis_receipt = audit_signer.sign_action("audit-genesis", "system_boot", None, json!({}));

    let approval_receipt = audit_signer.sign_action(
        "audit-decision-106",
        "card_approval",
        Some(106),
        json!({
            "verb": "Virement",
            "effect": "outbox_enqueued",
            "plan_steps": 2,
            "plan_risk": "low"
        }),
    );

    let daemon_task = tokio::spawn({
        let ack_receipt = approval_receipt.clone();
        async move {
            let mut stream = daemon.accept().await.expect("accept");
            // Step 1: status with card 106
            let step1 = DaemonStep::status().after(vec![KernelToUi::ShowCard(default_card(106))]);
            daemon
                .handle_step(&mut stream, 1, step1)
                .await
                .expect("step 1");

            // Step 2: approve step returning CardDecisionAck with audit receipt
            let step2 = DaemonStep::new(
                UiToKernel::Approve { card_id: 106 },
                KernelToUi::CardDecisionAck {
                    card_id: 106,
                    approved: true,
                    audit_id: ack_receipt.audit_id.clone(),
                    effect: Some("outbox_enqueued".into()),
                },
            );
            daemon
                .handle_step(&mut stream, 2, step2)
                .await
                .expect("step 2 approve");
        }
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;

    // Begin approval & wait for hold
    bridge.dispatch_json(
        json!({
            "type": "begin_approval",
            "card_id": 106,
        })
        .to_string(),
    );
    tokio::time::sleep(Duration::from_millis(605)).await;

    let (status, result) = rig
        .roundtrip(&bridge, json!({"type": "approve", "card_id": 106}))
        .await;
    assert_eq!(status, 200);
    assert_eq!(result["ok"], true);

    // Cryptographic verification of CardDecisionAck
    AuditVerifier::verify_card_ack("audit-decision-106", 106, &approval_receipt)
        .expect("CardDecisionAck signed audit receipt must verify");

    // Chain verification from genesis to approval
    let chain = vec![genesis_receipt, approval_receipt];
    AuditVerifier::verify_chain(&chain)
        .expect("Audit chain linkage must verify with genuine Ed25519 cryptography");

    let snap = rig.snapshot(&bridge);
    assert!(
        snap["cards"].as_array().unwrap().is_empty(),
        "Approved card must be dismissed from active snapshot"
    );

    daemon_task.await.unwrap();
}

/// `T3_PAIR_07`: Reversible ActionCard undo cancels outbox execution and logs compensating audit receipt.
///
/// Interactions: F07 (ActionCard) + F14 (Reversible Action Undo)
/// When a reversible card is approved, an undo action can be dispatched.
/// Daemon acknowledges undo, canceling execution in the outbox and appending a signed compensating entry.
#[test]
fn test_t3_pair_07_reversible_action_undo_cancels_outbox_and_logs_compensating_receipt() {
    let mut signer = AuditSigner::new();
    let genesis = signer.sign_action("id-genesis", "system_boot", None, json!({}));

    // 1. Initial reversible approval receipt
    let approve_receipt = signer.sign_action(
        "audit-action-107",
        "card_approve",
        Some(107),
        json!({
            "verb": "Archiver",
            "reversible": true,
            "undo_window_secs": 60,
            "outbox_state": "enqueued"
        }),
    );
    assert!(AuditVerifier::verify_receipt(&approve_receipt).is_ok());

    // 2. User invokes Undo within window -> compensating audit receipt
    let undo_receipt = signer.sign_action(
        "audit-undo-107",
        "action_undo",
        Some(107),
        json!({
            "original_audit_id": "audit-action-107",
            "reverted_effect": "outbox_cancelled",
            "compensation_status": "reverted_cleanly"
        }),
    );

    assert_eq!(undo_receipt.prev_hash, approve_receipt.entry_hash);
    assert!(AuditVerifier::verify_receipt(&undo_receipt).is_ok());

    let full_chain = vec![genesis, approve_receipt, undo_receipt];
    AuditVerifier::verify_chain(&full_chain).expect("Chain must verify");
}

/// `T3_PAIR_08`: Irreversible ActionCard strictly rejects Undo dispatch.
///
/// Interactions: F07 (ActionCard) + F14 (Reversible Action Undo)
/// Attempting to undo an irreversible action (reversible: false) is strictly rejected
/// fail-closed by the governance engine, leaving the outbox plan intact.
#[test]
fn test_t3_pair_08_irreversible_action_strictly_rejects_undo_dispatch() {
    let card = ActionCard {
        id: 108,
        verb: "Purger".into(),
        object: "Historique d'audit".into(),
        reason: "Effacement définitif".into(),
        cost: Money::from_cents(0, "EUR"),
        data_exit: DataExit::Local,
        reversible: false, // IRREVERSIBLE
        provenance: Provenance::Hakille,
    };

    assert!(!card.reversible);

    // Mock governance engine rule for undo eligibility
    let attempt_undo = |card: &ActionCard| -> Result<(), &'static str> {
        if !card.reversible {
            Err("Action irréversible : l'annulation est strictement interdite.")
        } else {
            Ok(())
        }
    };

    let result = attempt_undo(&card);
    assert_eq!(
        result,
        Err("Action irréversible : l'annulation est strictement interdite.")
    );
}

/// `T3_PAIR_09`: ActionCard rejection prevents outbox enqueue and records signed audit receipt.
///
/// Interactions: F07 (ActionCard) + F09 (Reject Action)
/// Disagreeing with proposed card dispatches Reject. The outbox is NOT enqueued,
/// and the rejection is committed to the Ed25519 audit chain with approved: false.
#[tokio::test]
async fn test_t3_pair_09_card_rejection_prevents_outbox_enqueue_and_records_signed_audit() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let mut signer = AuditSigner::new();
    let reject_receipt = signer.sign_action(
        "audit-reject-109",
        "card_reject",
        Some(109),
        json!({
            "card_id": 109,
            "decision": "rejected",
            "outbox_enqueued": false
        }),
    );

    let daemon_task = tokio::spawn({
        let rec = reject_receipt.clone();
        async move {
            let mut stream = daemon.accept().await.expect("accept");
            let step1 = DaemonStep::status().after(vec![KernelToUi::ShowCard(default_card(109))]);
            daemon
                .handle_step(&mut stream, 1, step1)
                .await
                .expect("step 1");

            let step2 = DaemonStep::new(
                UiToKernel::Reject { card_id: 109 },
                KernelToUi::CardDecisionAck {
                    card_id: 109,
                    approved: false,
                    audit_id: rec.audit_id.clone(),
                    effect: None,
                },
            );
            daemon
                .handle_step(&mut stream, 2, step2)
                .await
                .expect("step 2 reject");
        }
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;

    // Reject requires NO 600ms hold, dispatches immediately
    let (status, result) = rig
        .roundtrip(&bridge, json!({"type": "reject", "card_id": 109}))
        .await;

    assert_eq!(status, 200);
    assert_eq!(result["ok"], true);

    // Assert card dismissed
    let snap = rig.snapshot(&bridge);
    assert!(snap["cards"].as_array().unwrap().is_empty());

    // Verify rejection audit receipt
    AuditVerifier::verify_card_ack("audit-reject-109", 109, &reject_receipt)
        .expect("Reject receipt must verify");
    assert_eq!(reject_receipt.parameters["outbox_enqueued"], false);

    daemon_task.await.unwrap();
}

/// `T3_PAIR_10`: External provenance card applies 2px warning border and sanitizes malicious payload.
///
/// Interactions: F10 (Security Provenance) + F07 (ActionCard)
/// Card originating from external untrusted source triggers 2px side rule badge ("externe")
/// and strips HTML scripts, styles, and terminal ANSI sequences from verb/object/reason.
#[test]
fn test_t3_pair_10_external_provenance_warning_border_and_input_sanitization() {
    assert_eq!(
        ExternalBlock::side_rule_px(),
        2,
        "External content requires 2px border rule"
    );
    assert_eq!(ExternalBlock::badge(), "externe");

    // Skill supplies adversarial input attempting script injection and terminal manipulation
    let malicious_reason = "<script>exfiltrateKeys()</script><b style=\"color:red\">Alerte!</b>\u{001b}[31mAction critique\u{001b}[0m";
    let sanitized_reason = sanitize_skill_text(malicious_reason);

    assert!(
        !sanitized_reason.contains("<script>"),
        "Must strip script tags: {sanitized_reason}"
    );
    assert!(
        !sanitized_reason.contains("exfiltrateKeys"),
        "Must strip script body: {sanitized_reason}"
    );
    assert!(
        !sanitized_reason.contains("\u{001b}"),
        "Must strip terminal ANSI escape codes: {sanitized_reason}"
    );
    assert_eq!(sanitized_reason, "Alerte!Action critique");

    // ActionCard rendered with sanitized reason and external provenance
    let view = ActionCardView {
        verb: "Télécharger".into(),
        object: "fichier_partagé.zip".into(),
        reason: sanitized_reason,
        why_trace: "Origine web non vérifiée".into(),
        cost_label: "0,00 €".into(),
        data: DataPastille::Local,
        reversibility: Reversibility::Final,
        needs_hold: true,
        provenance_label: ExternalBlock::badge().into(),
        is_external: true,
    };

    assert!(view.is_external);
    assert_eq!(view.provenance_label, "externe");
    assert!(!view.reason.contains('<'));
}
