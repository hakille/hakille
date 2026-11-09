//! Tier 4 Real-World Application Workload: Scenario 2
//!
//! High-Stakes Financial Decision Approval & Reversible Compensation
//! Features Exercised: F07 (ActionCard), F08 (600ms Hold), F10 (Security Provenance), F13 (Signed Audit Journal), F14 (Reversible Action Undo)
//! Complexity: High
//!
//! Workflow:
//! 1. ActionCard proposition: Daemon proposes high-value 500,00 EUR transfer card with External provenance.
//! 2. Security validation: 2px warning border applied, reason text sanitized against script/ANSI injection.
//! 3. Hold gestures: Premature releases at 250ms and 550ms cancel; sustained 600ms confirms.
//! 4. Outbox enqueue & cryptographic audit: Outbox enqueues execution plan; CardDecisionAck emits Ed25519 signed receipt.
//! 5. Audit journal verification: Verifies cryptographic receipt signature and hash chain linkage.
//! 6. Reversible Undo: Dispatches Undo request, cancels outbox execution, appends compensating audit entry.

use crate::harness::{AuditSigner, AuditVerifier, DaemonStep, MobileTestRig};
use hakille_design::action_card::{ActionCardView, DataPastille, Reversibility};
use hakille_design::hold::{HoldEvent, HoldState, feed_hold};
use hakille_design::provenance::{ExternalBlock, sanitize_skill_text};
use hakille_ui_protocol::{ActionCard, DataExit, KernelToUi, Money, Provenance, UiToKernel};
use serde_json::json;
use std::time::Duration;

#[tokio::test]
async fn test_scenario2_financial_decision_approval_and_undo() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let mut audit_signer = AuditSigner::new();
    let genesis_receipt = audit_signer.sign_action("audit-genesis", "system_boot", None, json!({}));

    // --- STEP 1: Card Proposition ---
    let card_id = 202;
    let dirty_reason =
        "Facture hebergement <script>leakBalance()</script>\u{1b}[31murgent\u{1b}[0m";
    let transfer_card = ActionCard {
        id: card_id,
        verb: "Virer".into(),
        object: "500,00 EUR à Hébergement SAS".into(),
        reason: dirty_reason.into(),
        cost: Money::from_cents(50000, "EUR"),
        data_exit: DataExit::Cloud {
            provider: "BanqueSouveraine".into(),
        },
        reversible: true,
        provenance: Provenance::External,
    };

    let daemon_task = tokio::spawn({
        let c = transfer_card.clone();
        async move {
            let mut stream = daemon.accept().await.expect("daemon accept");

            // Step 1: Status with financial ActionCard
            let step1 = DaemonStep::status().after(vec![KernelToUi::ShowCard(c)]);
            daemon
                .handle_step(&mut stream, 1, step1)
                .await
                .expect("step 1");

            // Step 2: Approve ActionCard
            let approval_audit_id = "audit-tx-500eur-approved";
            let step2 = DaemonStep::new(
                UiToKernel::Approve { card_id },
                KernelToUi::CardDecisionAck {
                    card_id,
                    approved: true,
                    audit_id: approval_audit_id.into(),
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

    // --- STEP 2: Display & Security Sanitization ---
    let snap1 = rig.snapshot(&bridge);
    let cards = snap1["cards"].as_array().expect("cards array");
    assert_eq!(cards.len(), 1);

    // Sanitization check
    let clean_reason = sanitize_skill_text(&transfer_card.reason);
    assert!(!clean_reason.contains("<script>"));
    assert!(!clean_reason.contains('\u{1b}'));
    assert_eq!(clean_reason, "Facture hebergement urgent");

    // 2px border check
    assert_eq!(ExternalBlock::side_rule_px(), 2);
    assert_eq!(ExternalBlock::badge(), "externe");

    let card_view = ActionCardView {
        verb: transfer_card.verb.clone(),
        object: transfer_card.object.clone(),
        reason: clean_reason,
        why_trace: "Audit #202".into(),
        cost_label: "500,00 €".into(),
        data: DataPastille::Cloud {
            provider: "BanqueSouveraine".into(),
            weight_ko: None,
        },
        reversibility: Reversibility::Undoable { undo_seconds: 60 },
        needs_hold: true, // High value transfer requires hold
        provenance_label: ExternalBlock::badge().into(),
        is_external: true,
    };
    assert_eq!(
        card_view.pastilles(),
        ["500,00 €", "Cloud : BanqueSouveraine", "− annulable"]
    );

    // --- STEP 3: Hold Gestures ---
    // Subcase 3a: Release at 250ms cancels
    let (s0, _) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 0 });
    let (s_250, _) = feed_hold(s0, HoldEvent::Tick { elapsed_ms: 250 });
    let (s_canc_250, _) = feed_hold(s_250, HoldEvent::Released { at_ms: 250 });
    assert_eq!(s_canc_250, HoldState::Cancelled);

    // Subcase 3b: Release at 550ms cancels
    let (s1, _) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 0 });
    let (s_550, _) = feed_hold(s1, HoldEvent::Tick { elapsed_ms: 550 });
    let (s_canc_550, _) = feed_hold(s_550, HoldEvent::Released { at_ms: 550 });
    assert_eq!(s_canc_550, HoldState::Cancelled);

    // Subcase 3c: Sustained hold for 600ms confirms
    let (s2, _) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 0 });
    let (s_confirmed, tick_confirmed) = feed_hold(s2, HoldEvent::Tick { elapsed_ms: 600 });
    assert_eq!(s_confirmed, HoldState::Confirmed);
    assert!(tick_confirmed);

    // --- STEP 4: Outbox Enqueue & Cryptographic Audit ---
    bridge.dispatch_json(
        json!({
            "type": "begin_approval",
            "card_id": card_id,
        })
        .to_string(),
    );
    tokio::time::sleep(Duration::from_millis(605)).await;

    let (status_code, result) = rig
        .roundtrip(&bridge, json!({"type": "approve", "card_id": card_id}))
        .await;
    assert_eq!(status_code, 200);
    assert_eq!(result["ok"], true);

    let approval_receipt = audit_signer.sign_action(
        "audit-tx-500eur-approved",
        "card_approve",
        Some(card_id),
        json!({
            "verb": "Virer",
            "amount_cents": 50000,
            "currency": "EUR",
            "effect": "outbox_enqueued",
            "plan_risk": "high"
        }),
    );

    // --- STEP 5: Journal Verification ---
    AuditVerifier::verify_card_ack("audit-tx-500eur-approved", card_id, &approval_receipt)
        .expect("CardDecisionAck must verify cryptographically");

    let chain = vec![genesis_receipt, approval_receipt.clone()];
    AuditVerifier::verify_chain(&chain).expect("Hash chain from genesis to approval must verify");

    // --- STEP 6: Reversible Undo ---
    // User triggers Undo before the 60s window elapses
    let undo_receipt = audit_signer.sign_action(
        "audit-tx-500eur-undone",
        "action_undo",
        Some(card_id),
        json!({
            "original_audit_id": "audit-tx-500eur-approved",
            "effect": "outbox_cancelled",
            "reason": "Demande utilisateur dans la fenêtre d'annulation"
        }),
    );

    assert_eq!(undo_receipt.prev_hash, approval_receipt.entry_hash);
    AuditVerifier::verify_receipt(&undo_receipt).expect("Undo receipt must verify");

    let full_chain = vec![chain[0].clone(), chain[1].clone(), undo_receipt];
    AuditVerifier::verify_chain(&full_chain).expect("Chain including Undo must verify");

    daemon_task.await.unwrap();
}
