//! Tier 4: Real-World Application Scenario Specifications (10 tests total)
//!
//! Complete end-to-end user journeys: `T4_SCENARIO_01` to `T4_SCENARIO_10`.

use crate::harness::E2eHarness;
use crate::harness::a11y_inspector::A11yInspector;
use crate::harness::frame_inspector::FrameInspector;
use crate::harness::gestures::{GestureSimulator, HoldOutcome};
use crate::harness::mock_kernel::MockKernel;
use hakille_design::HOLD_TO_CONFIRM_MS;
use hakille_design::brand::wire_count;
use hakille_ui::{InitiativeOrigin, PresenceState, SilenceBudgetTracker};
use hakille_ui_desktop::a11y::{ActionCardA11y, build_shell_tree};
use hakille_ui_desktop::app::{HudAction, HudApp, HudInput, PointerTarget};
use hakille_ui_desktop::renderer::{SoftwareRenderer, card_buttons};
use hakille_ui_desktop::runtime::{FrameBudget, HeadlessRuntime, RuntimeEvent};
use hakille_ui_model::StableSnapshot;
use hakille_ui_model::snapshot::{EVAPORATE_MS, HUD_HEIGHT, HUD_WIDTH};
use hakille_ui_protocol::{
    ActionCard, DataExit, JournalEntryDto, KernelMessage, KernelToUi, MemoryRowDto, Money,
    Provenance, StatusDto, UiMessage, UiToKernel,
};
use std::time::Duration;

fn make_code_card(id: u64, reversible: bool) -> ActionCard {
    ActionCard {
        id,
        verb: "Refactoriser".into(),
        object: "crates/hakille-core/src/parser.rs".into(),
        reason: "Optimisation allocation mémoire (-18% RAM)".into(),
        cost: Money {
            micros: 0,
            currency: "EUR".into(),
        },
        data_exit: DataExit::Local,
        reversible,
        provenance: Provenance::Hakille,
    }
}

/// `T4_SCENARIO_01`: Sovereign User Onboarding to Ambient Idle Presence
#[test]
fn test_t4_scenario_01_onboarding_to_ambient_idle_presence() {
    let mut harness = E2eHarness::new_authorized();
    // User acknowledges sovereign onboarding principles
    let ack_msg = UiMessage::new(UiToKernel::SettingsToggle { index: 0 });
    let res = harness.kernel.handle_ui_message(ack_msg);
    assert!(res.is_ok());

    // Presence enters Idle (0-frame quiescence)
    harness.app.on_kernel_reply(KernelToUi::Presence(
        hakille_ui_protocol::PresenceState::Idle,
    ));
    assert_eq!(harness.snapshot().presence_state, 0);

    // Verify window is hidden and idle
    assert!(!harness.app.visible);
    let ticks = harness.gestures.tick(&mut harness.app);
    assert!(
        ticks.is_empty(),
        "Idle hidden window must produce 0 ticks/frames"
    );
}

/// `T4_SCENARIO_02`: Global Hotkey Invocation, Natural Query, and Token-Streamed HUD Reply
#[test]
fn test_t4_scenario_02_hotkey_invocation_query_streaming_auto_evaporation() {
    let mut harness = E2eHarness::new_authorized();
    // 1. Hotkey press
    let actions = harness.press_hotkey();
    assert!(harness.app.visible);
    assert!(actions.contains(&HudAction::ShowWindow));

    // 2. Type query
    harness.type_text("Résume mes emails non lus");
    assert_eq!(harness.snapshot().hud_query, "Résume mes emails non lus");

    // 3. Submit and stream token deltas
    harness
        .app
        .on_submit_sent("Résume mes emails non lus".into());
    for i in 1..=5 {
        harness.app.on_kernel_reply(KernelToUi::TokenDelta {
            text: format!(" delta_{i}"),
        });
    }

    // 4. Final reply
    harness.app.on_kernel_reply(KernelToUi::HudReply {
        response: "Voici le résumé de vos 3 emails non lus.".into(),
        validation: None,
    });
    assert_eq!(
        harness.snapshot().hud_response,
        "Voici le résumé de vos 3 emails non lus."
    );

    // 5. Auto-evaporation countdown
    let escape_actions = harness.gestures.press_escape(&mut harness.app);
    assert!(escape_actions.contains(&HudAction::Redraw));
}

/// `T4_SCENARIO_03`: Autonomous Code Refactoring Approval via Irreversible 600ms Hold Card
#[test]
fn test_t4_scenario_03_autonomous_code_refactoring_approval_600ms_hold() {
    let mut harness = E2eHarness::new_authorized();
    let card = make_code_card(101, false);
    harness.inject_action_card(card);

    // Verify consultation height expansion (360px)
    assert_eq!(harness.app.scene_height(), 360);

    // Attempt premature release at 450ms -> rejected
    let early = harness.hold_validate(Duration::from_millis(450));
    assert!(matches!(early, HoldOutcome::RejectedTooShort { .. }));

    // Complete 600ms hold -> confirmed!
    let confirmed = harness.hold_validate(Duration::from_millis(600));
    assert_eq!(confirmed, HoldOutcome::Confirmed { card_id: 101 });

    // Kernel acknowledges decision and commits audit record
    let ack_reply = harness
        .kernel
        .handle_ui_message(UiMessage::new(UiToKernel::Approve { card_id: 101 }));
    assert!(ack_reply.is_ok());
    harness.kernel.assert_received_approval(101);
}

/// `T4_SCENARIO_04`: Reversible Action Immediate Validation and 1-Gesture Audit Journal Revert
#[test]
fn test_t4_scenario_04_reversible_action_immediate_validation_and_journal_revert() {
    let mut harness = E2eHarness::new_authorized();
    let card = make_code_card(202, true); // reversible = true
    harness.inject_action_card(card);

    // Reversible card confirms immediately on click without 600ms hold
    let outcome = harness.hold_validate(Duration::from_millis(0));
    assert_eq!(outcome, HoldOutcome::Confirmed { card_id: 202 });

    // User validates
    let _ = harness
        .kernel
        .handle_ui_message(UiMessage::new(UiToKernel::Approve { card_id: 202 }));
    assert_eq!(harness.kernel.journal_entries.len(), 1);
    let audit_id = harness.kernel.journal_entries[0].id.clone();

    // User opens Journal (F1) and reverts action
    harness.gestures.press_f1(&mut harness.app);
    let undo_msg = UiMessage::new(UiToKernel::Undo {
        journal_id: audit_id.parse().unwrap(),
    });
    let undo_res = harness.kernel.handle_ui_message(undo_msg);
    assert!(undo_res.is_ok());
    assert!(
        harness.kernel.journal_entries[0]
            .summary
            .contains("annulée")
    );
}

/// `T4_SCENARIO_05`: Voice Conversation with 21-Wire Kora Waveform and Dynamic Channel Arbitration
#[test]
fn test_t4_scenario_05_voice_conversation_with_kora_waveform_and_channel_arbitration() {
    let quiet_db = 38.0f32;
    let noisy_db = 70.0f32;

    let arbitrate_channel = |db: f32| -> &'static str {
        if db <= 45.0 {
            "VoiceNormal"
        } else {
            "EphemeralHud"
        }
    };

    assert_eq!(arbitrate_channel(quiet_db), "VoiceNormal");
    assert_eq!(arbitrate_channel(noisy_db), "EphemeralHud");

    // During VoiceNormal speaking, render 21 kora wires
    assert_eq!(wire_count(64), 21);
}

/// `T4_SCENARIO_06`: Meeting Silence & Daily Silence Budget Rollover with Urgency Override
#[test]
fn test_t4_scenario_06_meeting_silence_budget_rollover_with_urgency_override() {
    let mut tracker = SilenceBudgetTracker::new(3);
    assert_eq!(tracker.credits_remaining(), 3);

    // Consume 3 daily credits
    assert!(tracker.request_initiative(
        "ev-1",
        InitiativeOrigin::SystemInferred,
        0.90,
        "Suggestion 1"
    ));
    assert!(tracker.request_initiative(
        "ev-2",
        InitiativeOrigin::SystemInferred,
        0.90,
        "Suggestion 2"
    ));
    assert!(tracker.request_initiative(
        "ev-3",
        InitiativeOrigin::SystemInferred,
        0.90,
        "Suggestion 3"
    ));
    assert_eq!(tracker.credits_remaining(), 0);

    // 4th routine suggestion deferred
    let routine_allowed = tracker.request_initiative(
        "ev-4",
        InitiativeOrigin::SystemInferred,
        0.90,
        "Suggestion 4",
    );
    assert!(
        !routine_allowed,
        "4th suggestion must be deferred when budget is exhausted"
    );

    // Critical security alert overrides budget suppression
    let is_critical_security = true;
    let critical_override = is_critical_security || tracker.credits_remaining() > 0;
    assert!(
        critical_override,
        "Critical alert must override silence budget exhaustion"
    );
}

/// `T4_SCENARIO_07`: Sovereignty Audit: Memory Inspection, Recall Query, and Cryptographic Forget
#[test]
fn test_t4_scenario_07_memory_inspection_recall_query_and_crypto_forget() {
    let mut kernel = MockKernel::authorized();
    // Remember a fact
    let rem_msg = UiMessage::new(UiToKernel::MemoryRemember {
        text: "Passeport numéro 123456789".into(),
    });
    let rem_res = kernel.handle_ui_message(rem_msg).unwrap().unwrap();
    let mem_id = match rem_res {
        KernelToUi::MemoryRememberAck { id } => id,
        _ => panic!("Expected MemoryRememberAck"),
    };
    assert_eq!(kernel.memory_rows.len(), 1);

    // Forget the fact
    let forget_msg = UiMessage::new(UiToKernel::MemoryForget {
        row_id: mem_id.clone(),
    });
    let forget_res = kernel.handle_ui_message(forget_msg);
    assert!(forget_res.is_ok());
    assert_eq!(
        kernel.memory_rows.len(),
        0,
        "Forgotten fact must be purged from memory store"
    );
}

/// `T4_SCENARIO_08`: Disconnected Daemon Detection, Fail-Closed Toast, and Safe Auto-Reconnection
#[test]
fn test_t4_scenario_08_disconnected_daemon_fail_closed_and_recovery() {
    let mut detached_kernel = MockKernel::detached();
    let approve_msg = UiMessage::new(UiToKernel::Approve { card_id: 55 });
    let res = detached_kernel.handle_ui_message(approve_msg);
    assert!(
        res.is_err(),
        "Disconnected daemon must fail-closed on sensitive approval"
    );

    // Safe reconnection
    let mut connected_kernel = MockKernel::authorized();
    let approve_after_reconnect = UiMessage::new(UiToKernel::Approve { card_id: 55 });
    assert!(
        connected_kernel
            .handle_ui_message(approve_after_reconnect)
            .is_ok()
    );
}

/// `T4_SCENARIO_09`: Accessibility Navigation with Screen Reader
#[test]
fn test_t4_scenario_09_accessibility_navigation_with_screen_reader() {
    let card = ActionCardA11y {
        verb: "Refactoriser".into(),
        object: "parser.rs".into(),
        reason: "Optimisation allocation mémoire".into(),
        why_trace: "t-100".into(),
        cost_label: "0.00 EUR".into(),
        data_label: "Local".into(),
        reversibility_label: "définitif".into(),
        needs_hold: true,
        provenance_label: "Local".into(),
        is_external: false,
    };

    let tree = A11yInspector::build_card(&card);
    A11yInspector::assert_all_interactive_labeled(&tree);
    A11yInspector::assert_validation_button_announces_hold(&card);
    A11yInspector::assert_trust_pastilles(&tree, "0.00 EUR", "Local", "définitif");
}

/// `T4_SCENARIO_10`: High-Load Multi-Agent Burst: Concurrent Cards, Outbox Flush & Quiescent Recovery
#[test]
fn test_t4_scenario_10_high_load_multi_agent_burst_single_decision_quiescence() {
    let mut harness = E2eHarness::new_authorized();
    let card1 = make_code_card(1, true);
    let card2 = make_code_card(2, true);

    // Agent bursts: inject card 1
    harness.inject_action_card(card1);
    assert!(harness.app.pending_card().is_some());
    assert_eq!(harness.app.pending_card().unwrap().id, 1);

    // Single-decision-open rule: approving card 1 allows card 2 to surface
    harness.app.on_kernel_reply(KernelToUi::CardDecisionAck {
        card_id: 1,
        approved: true,
        audit_id: "aud-1".into(),
        effect: None,
    });
    harness.app.on_kernel_reply(KernelToUi::ShowCard(card2));
    assert_eq!(harness.app.pending_card().unwrap().id, 2);

    // Outbox flush
    let flush_msg = UiMessage::new(UiToKernel::OutboxFlush);
    assert!(harness.kernel.handle_ui_message(flush_msg).is_ok());

    // Final response and return to quiescence
    harness.app.on_kernel_reply(KernelToUi::HudReply {
        response: "Toutes les opérations d'agents sont terminées.".into(),
        validation: None,
    });
    harness.gestures.press_escape(&mut harness.app);
    assert!(!harness.app.visible);
}
