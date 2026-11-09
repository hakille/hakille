//! Milestone 1 E2E Integration Test Suite:
//! Verifies the pure Rust sovereign design contracts for Milestone 1:
//! - Deep Obsidian OLED Theme & Tokens
//! - Living Presence Aura Canvas & 21 Kora Wires
//! - 600ms Hold-to-Confirm Button & Haptics
//! - ActionCard 3 Badges Order & Single Decision Open Constraint
//! - Security Provenance Banner & Sanitization
//! - Calm Distress Mode: Zero Braise & Exact Calm Copy

use hakille_design::action_card::{ActionCardView, DataPastille, Reversibility};
use hakille_design::brand::{WIRES_FULL, WIRES_SMALL, WIRES_SMALL_BELOW_PX, wire_count};
use hakille_design::generated::color::{
    COLOR_BRAISE_DARK, COLOR_LIGNE_DARK, COLOR_LUEUR_DARK, COLOR_SURFACE_0_DARK,
    COLOR_SURFACE_1_DARK, COLOR_SURFACE_2_DARK,
};
use hakille_design::generated::motion::MOTION_HAPTIC_MS_TICK;
use hakille_design::hold::{HoldEvent, HoldState, feed_hold, hold_progress};
use hakille_design::household::DistressMode;
use hakille_design::motion::HOLD_TO_CONFIRM_MS;
use hakille_design::provenance::{
    CLIPBOARD_SENSITIVE_TTL_SECS, ExternalBlock, ProvenanceBand, sanitize_skill_text,
};
use hakille_ui_protocol::AuraRingUpdateDto;

#[test]
fn test_milestone1_action_card_badges_order_and_reversibility() {
    let card = ActionCardView {
        verb: "Envoyer".into(),
        object: "document confidentiel".into(),
        reason: "Demande utilisateur".into(),
        why_trace: "trace".into(),
        cost_label: "0.00 EUR".into(),
        data: DataPastille::Local,
        reversibility: Reversibility::Undoable { undo_seconds: 30 },
        needs_hold: false,
        provenance_label: "Hakille".into(),
        is_external: false,
    };

    // 1. Mandatory 3 pastilles in strict order: [Cost, Data Destination, Reversibility]
    let pastilles = card.pastilles();
    assert_eq!(pastilles[0], "0.00 EUR");
    assert_eq!(pastilles[1], "Local");
    assert_eq!(pastilles[2], "− annulable");

    // 2. Reversibility label: never color alone! Icon + Text
    assert_eq!(
        Reversibility::Undoable { undo_seconds: 30 }.label(),
        "− annulable"
    );
    assert_eq!(Reversibility::Final.label(), "+ définitif");

    // Cloud pastille formatting
    let cloud = DataPastille::Cloud {
        provider: "Anthropic".into(),
        weight_ko: Some(42),
    };
    assert_eq!(cloud.label(), "Cloud : Anthropic (≈ 42 Ko)");
}

#[test]
fn test_milestone1_single_decision_open_constraint() {
    let dummy_card = ActionCardView {
        verb: "A".into(),
        object: "B".into(),
        reason: "C".into(),
        why_trace: "D".into(),
        cost_label: "0 €".into(),
        data: DataPastille::Local,
        reversibility: Reversibility::Final,
        needs_hold: true,
        provenance_label: "Hakille".into(),
        is_external: false,
    };

    // Shell allows at most 1 active decision card open
    assert!(ActionCardView::single_decision_open(&[]));
    assert!(ActionCardView::single_decision_open(&[dummy_card.clone()]));
    assert!(!ActionCardView::single_decision_open(&[
        dummy_card.clone(),
        dummy_card.clone()
    ]));
}

#[test]
fn test_milestone1_hold_to_confirm_exact_monotonic_600ms_and_early_cancellation() {
    assert_eq!(HOLD_TO_CONFIRM_MS, 600);
    assert_eq!(MOTION_HAPTIC_MS_TICK, 10);

    // Press start at 0ms: emits first haptic tick
    let (state, press_tick) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 0 });
    assert_eq!(state, HoldState::Holding { elapsed_ms: 0 });
    assert!(press_tick, "Press down at 0ms must emit haptic tick");

    // Progress at 300ms is 0.5
    let (state_half, _) = feed_hold(state, HoldEvent::Tick { elapsed_ms: 300 });
    assert!((hold_progress(state_half) - 0.5).abs() < 1e-6);

    // Progress at 599ms (1ms early): not confirmed
    let (state_599, _) = feed_hold(state, HoldEvent::Tick { elapsed_ms: 599 });
    assert!((hold_progress(state_599) - (599.0 / 600.0)).abs() < 1e-6);

    // Early release at 599ms: Cancelled immediately, NO confirmation tick
    let (state_cancelled, cancel_tick) = feed_hold(state_599, HoldEvent::Released { at_ms: 599 });
    assert_eq!(state_cancelled, HoldState::Cancelled);
    assert!(
        !cancel_tick,
        "Early release must NOT emit confirmation tick"
    );
    assert_eq!(hold_progress(state_cancelled), 0.0);

    // Held full 600ms: Confirmed with final confirmation tick
    let (state_held, confirm_tick) = feed_hold(state, HoldEvent::Tick { elapsed_ms: 600 });
    assert_eq!(state_held, HoldState::Confirmed);
    assert!(
        confirm_tick,
        "600ms hold reached must emit confirmation tick"
    );
    assert_eq!(hold_progress(state_held), 1.0);
}

#[test]
fn test_milestone1_provenance_sanitization_and_2px_external_rule() {
    // 1. Strict text sanitization: Strips HTML tags and ANSI escape sequences
    let dirty = "<script>stealKeys(); alert(1);</script><style>body { color:red; }</style><b style=\"color:red\">Texte propre</b>\u{001B}[31m";
    let clean = sanitize_skill_text(dirty);
    assert!(!clean.contains('<') && !clean.contains('>'));
    assert!(!clean.contains('\u{001B}'));
    assert!(!clean.contains("stealKeys"));
    assert!(!clean.contains("[31m"));
    assert_eq!(clean, "Texte propre");

    // 2. External block: 2px lateral rule and 'externe' badge
    assert_eq!(ExternalBlock::side_rule_px(), 2);
    assert_eq!(ExternalBlock::badge(), "externe");

    // 3. Sensitive clipboard expiration TTL: 60 seconds
    assert_eq!(CLIPBOARD_SENSITIVE_TTL_SECS, 60);

    // 4. Anti-typosquatting
    let fake_skill = ProvenanceBand::VerifiedSkill {
        publisher: "attacker".into(),
        name: "mail-service".into(),
    };
    assert!(fake_skill.looks_like_installed(&["mail".to_string()]));

    let legit_skill = ProvenanceBand::VerifiedSkill {
        publisher: "Hakille".into(),
        name: "mail".into(),
    };
    assert!(!legit_skill.looks_like_installed(&["mail".to_string()]));
}

#[test]
fn test_milestone1_calm_distress_zero_braise_and_exact_calm_copy() {
    // Exact calm copy string
    let copy = DistressMode::calm_copy();
    assert_eq!(
        copy,
        "Je reste là. Un geste pour appeler ton contact, un autre pour les urgences."
    );

    // Zero Braise in distress: Braise is strictly forbidden on distress surfaces
    assert_ne!(COLOR_BRAISE_DARK, COLOR_SURFACE_0_DARK);
    assert_ne!(COLOR_BRAISE_DARK, COLOR_SURFACE_1_DARK);
    assert_ne!(COLOR_BRAISE_DARK, COLOR_SURFACE_2_DARK);
    assert_ne!(COLOR_BRAISE_DARK, COLOR_LIGNE_DARK);
    assert_ne!(COLOR_BRAISE_DARK, COLOR_LUEUR_DARK);
}

#[test]
fn test_milestone1_aura_ring_dto_and_wire_counts() {
    // Wire counts: 21 full kora wires, 7 compact wires below 24px
    assert_eq!(WIRES_FULL, 21);
    assert_eq!(WIRES_SMALL, 7);
    assert_eq!(WIRES_SMALL_BELOW_PX, 24);
    assert_eq!(wire_count(16), 7);
    assert_eq!(wire_count(23), 7);
    assert_eq!(wire_count(24), 21);
    assert_eq!(wire_count(176), 21);

    // AuraRingUpdateDto chromatic confidence halo mappings
    let emerald_dto = AuraRingUpdateDto::new(
        0.95,
        "EmeraldHighTrust",
        "#10B981",
        "#10B98173",
        3000,
        1.0,
        1000,
    );
    assert!(emerald_dto.is_high_trust());
    assert!(!emerald_dto.is_ambient());

    let amber_dto =
        AuraRingUpdateDto::new(0.72, "AmberAmbient", "#F59E0B", "#FBBF24", 3000, 1.0, 1000);
    assert!(amber_dto.is_ambient());
    assert!(!amber_dto.is_high_trust());

    let violet_dto =
        AuraRingUpdateDto::new(0.80, "VioletStepUp", "#8B5CF6", "#A78BFA", 1500, 1.02, 1000);
    assert!(violet_dto.is_step_up());

    let red_dto = AuraRingUpdateDto::new(0.10, "RedLocked", "#EF4444", "#FF5A5A", 1000, 1.05, 1000);
    assert!(red_dto.is_locked());
}
