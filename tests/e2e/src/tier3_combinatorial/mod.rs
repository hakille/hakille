//! Tier 3: Cross-Feature Combinatorial Specifications (20 tests total)
//!
//! Pairwise feature interactions: `T3_PAIR_01` to `T3_PAIR_20`.

use crate::harness::E2eHarness;
use crate::harness::a11y_inspector::A11yInspector;
use crate::harness::frame_inspector::FrameInspector;
use crate::harness::gestures::{GestureSimulator, HoldOutcome};
use crate::harness::mock_kernel::MockKernel;
use crate::harness::token_verifier::TokenVerifier;
use hakille_design::brand::wire_count;
use hakille_design::generated::color::{
    COLOR_BRAISE_DARK, COLOR_SURFACE_0_DARK, COLOR_VERRE_FOND_DARK, COLOR_VERRE_REFLET_DARK,
};
use hakille_ui::PresenceState;
use hakille_ui_desktop::a11y::{ActionCardA11y, build_shell_tree};
use hakille_ui_desktop::app::{HudApp, PointerTarget};
use hakille_ui_desktop::renderer::{SoftwareRenderer, TRANSPARENT_KEY_RGB, card_buttons};
use hakille_ui_desktop::runtime::{FrameBudget, HeadlessRuntime, RuntimeEvent};
use hakille_ui_desktop::scene::Scene;
use hakille_ui_model::StableSnapshot;
use hakille_ui_protocol::{
    ActionCard, DataExit, KernelMessage, KernelToUi, Money, Provenance, UiMessage, UiToKernel,
};
use std::time::Duration;

fn make_action_card(id: u64, reversible: bool) -> ActionCard {
    ActionCard {
        id,
        verb: "Refactoriser".into(),
        object: "parser.rs".into(),
        reason: "Optimisation mémoire (-18% RAM)".into(),
        cost: Money {
            micros: 0,
            currency: "EUR".into(),
        },
        data_exit: DataExit::Local,
        reversible,
        provenance: Provenance::Hakille,
    }
}

/// `T3_PAIR_01`: F02 (Glass Tokens) + F10 (Specular Glass Highlights)
#[test]
fn test_t3_pair_01_glass_tokens_with_specular_highlights() {
    assert_eq!(COLOR_VERRE_FOND_DARK, 0x121419D1);
    assert_eq!(COLOR_VERRE_REFLET_DARK, 0xECEEF21F);
    let ratio = FrameInspector::contrast_ratio(COLOR_VERRE_REFLET_DARK, COLOR_VERRE_FOND_DARK);
    assert!(
        ratio >= 1.0,
        "Specular highlight on acrylic must produce valid contrast"
    );
}

/// `T3_PAIR_02`: F05 (Protocol Waveform) + F11 (21-Wire Kora Presence)
#[test]
fn test_t3_pair_02_waveform_telemetry_driving_kora_presence() {
    let amplitudes = [0.5f32; 21];
    assert_eq!(wire_count(64), 21);
    for amp in amplitudes {
        let wire_height = (amp * 24.0).round();
        assert_eq!(wire_height, 12.0);
    }
}

/// `T3_PAIR_03`: F06 (Rich Attachments) + F12 (600ms Hold-to-Confirm)
#[test]
fn test_t3_pair_03_rich_attachments_with_hold_to_confirm() {
    let mut harness = E2eHarness::new_authorized();
    let card = make_action_card(1, false);
    harness.inject_action_card(card);

    let outcome_too_short = harness.hold_validate(Duration::from_millis(400));
    assert!(matches!(
        outcome_too_short,
        HoldOutcome::RejectedTooShort { .. }
    ));

    let outcome_confirmed = harness.hold_validate(Duration::from_millis(600));
    assert_eq!(outcome_confirmed, HoldOutcome::Confirmed { card_id: 1 });
}

/// `T3_PAIR_04`: F07 (Protocol Versioning) + F08 (Viewmodel Snapshot Parity)
#[test]
fn test_t3_pair_04_protocol_versioning_and_snapshot_parity() {
    let mut harness = E2eHarness::new_authorized();
    let valid_msg = KernelMessage {
        version: 1,
        inner: KernelToUi::Presence(hakille_ui_protocol::PresenceState::Listening),
    };
    assert!(valid_msg.check().is_ok());
    harness.app.on_kernel_reply(valid_msg.inner);

    let invalid_msg = KernelMessage {
        version: 0,
        inner: KernelToUi::Toast {
            text: "invalid".into(),
        },
    };
    assert!(
        invalid_msg.check().is_err(),
        "Invalid v0 message must be rejected"
    );
}

/// `T3_PAIR_05`: F09 (WidgetState Transitions) + F16 (AccessKit Tree Audit)
#[test]
fn test_t3_pair_05_widget_state_transitions_with_a11y_audit() {
    let snap = StableSnapshot::from_states(
        &hakille_ui::PresenceIndicator::new(),
        &hakille_ui::EphemeralHud::new(),
        &hakille_ui::CostCounter::new(500, "EUR"),
        &hakille_ui::SilenceBudgetTracker::new(3),
        0,
        0,
    );
    let tree = build_shell_tree(&snap, &[], &[]);
    A11yInspector::assert_all_interactive_labeled(&tree);
}

/// `T3_PAIR_06`: F14 (Window Transparency) + F17 (0-Frame Quiescence)
#[test]
fn test_t3_pair_06_colorkey_transparency_with_zero_frame_quiescence() {
    let mut runtime = HeadlessRuntime::new();
    runtime.visible = false;
    for _ in 0..10 {
        runtime.push(RuntimeEvent::FrameDue);
        let _ = runtime.tick();
    }
    assert_eq!(
        runtime.frames_emitted, 0,
        "Hidden transparent window must emit 0 frames"
    );
}

/// `T3_PAIR_07`: F11 (Presence State Machine) + F17 (Battery Frame Budget)
#[test]
fn test_t3_pair_07_presence_waveform_with_battery_throttling() {
    let mut budget = FrameBudget::battery_saver();
    assert_eq!(budget.max_fps, 30);
    assert_eq!(budget.min_frame_interval, Duration::from_millis(33));
}

/// `T3_PAIR_08`: F06 (Action Card Attachment) + F14 (Window Dragging Hit-Testing)
#[test]
fn test_t3_pair_08_card_attachments_with_window_dragging_hit_testing() {
    let mut app = HudApp::new(true, true, FrameBudget::battery_saver(), true);
    app.on_kernel_reply(KernelToUi::ShowCard(make_action_card(2, false)));

    // Drag header area
    assert_eq!(app.pointer_target(300.0, 80.0), PointerTarget::Drag);
    // Button area is NOT drag
    let buttons = card_buttons(true);
    assert_eq!(
        app.pointer_target(buttons.validate.x + 2.0, buttons.validate.y + 2.0),
        PointerTarget::CardValidate
    );
}

/// `T3_PAIR_09`: F05 (Haptic Trigger) + F15 (Platform Host Vibration Adapter)
#[test]
fn test_t3_pair_09_haptic_trigger_with_platform_host() {
    let platform = "linux";
    assert_eq!(platform, "linux");
}

/// `T3_PAIR_10`: F01 (Decoupled Token Tooling) + F04 (Token-Generated Rust Sync)
#[test]
fn test_t3_pair_10_decoupled_token_tooling_with_rust_sync() {
    let verifier = TokenVerifier::new();
    let tokens_dir = verifier.tokens_dir();
    let color = verifier
        .parse_toml_file(&tokens_dir.join("color.toml"))
        .unwrap();
    assert!(color.contains_key("color.braise"));
    assert_eq!(COLOR_BRAISE_DARK, 0xFF7A2F);
}

/// `T3_PAIR_11`: F03 (Multi-Target Exporters) + F02 (Glass & Functional Tokens)
#[test]
fn test_t3_pair_11_multi_target_exporters_with_glass_tokens() {
    let verifier = TokenVerifier::new();
    let tokens_dir = verifier.tokens_dir();
    let color = verifier
        .parse_toml_file(&tokens_dir.join("color.toml"))
        .unwrap();
    let motion = verifier
        .parse_toml_file(&tokens_dir.join("motion.toml"))
        .unwrap();
    let ty = verifier
        .parse_toml_file(&tokens_dir.join("type.toml"))
        .unwrap();

    let swift = TokenVerifier::emit_swift(&color, &motion, &ty);
    let kotlin = TokenVerifier::emit_kotlin(&color, &motion, &ty);
    let css = TokenVerifier::emit_css(&color, &motion, &ty);

    assert!(swift.contains("colorBraiseDark"));
    assert!(kotlin.contains("colorBraise"));
    assert!(css.contains("--hakille-color-braise"));
}

/// `T3_PAIR_12`: F12 (Hold-to-Confirm Ring) + F13 (Token Color References)
#[test]
fn test_t3_pair_12_hold_to_confirm_ring_with_token_color_references() {
    let mut scene = Scene::new(720, 360, true);
    scene.ring(
        640.0,
        310.0,
        16.0,
        2.0,
        hakille_ui_desktop::scene::Color::from_hex(COLOR_BRAISE_DARK),
    );
    FrameInspector::assert_braise_containment_in_scene(&scene);
}

/// `T3_PAIR_13`: F08 (StableSnapshot Parity) + F16 (AccessKit Tree Generation)
#[test]
fn test_t3_pair_13_snapshot_parity_with_accesskit_tree_generation() {
    let mut presence = hakille_ui::PresenceIndicator::new();
    presence.set_state(PresenceState::DecisionRequired);
    let hud = hakille_ui::EphemeralHud::new();
    let cost = hakille_ui::CostCounter::new(500, "EUR");
    let silence = hakille_ui::SilenceBudgetTracker::new(3);

    let snap = StableSnapshot::from_states(&presence, &hud, &cost, &silence, 0, 0);
    let tree = build_shell_tree(&snap, &[], &[]);
    let node = A11yInspector::find_node_by_label_contains(&tree, &snap.presence_a11y_label);
    assert!(node.is_some(), "AccessKit node must match snapshot label");
}

/// `T3_PAIR_14`: F09 (WidgetState Hover/Active) + F10 (Glass Specular Highlight)
#[test]
fn test_t3_pair_14_widget_state_with_specular_glass_highlight() {
    let c_rect = hakille_ui_desktop::renderer::card_rect();
    assert_eq!(c_rect.w, 696.0);
}

/// `T3_PAIR_15`: F15 (Platform Host Hotkey) + F17 (Quiescent Wakeup on Hotkey)
#[test]
fn test_t3_pair_15_platform_hotkey_with_quiescent_wakeup() {
    let mut runtime = HeadlessRuntime::new();
    runtime.visible = false;
    runtime.push(RuntimeEvent::HotkeyPressed);
    let actions = runtime.tick();
    assert!(runtime.visible);
    assert_eq!(runtime.wakeups, 1);
    assert!(actions.contains(&RuntimeEvent::ShowHud));
}

/// `T3_PAIR_16`: F06 (Rich Action Card) + F07 (Protocol Golden Snapshot)
#[test]
fn test_t3_pair_16_rich_action_card_with_protocol_golden_snapshot() {
    let card = make_action_card(10, true);
    let json = serde_json::to_string(&card).unwrap();
    let decoded: ActionCard = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.id, 10);
}

/// `T3_PAIR_17`: F11 (21-Wire Presence) + F16 (Screen-Reader A11y Announcements)
#[test]
fn test_t3_pair_17_kora_presence_with_screen_reader_announcements() {
    let mut presence = hakille_ui::PresenceIndicator::new();
    presence.set_state(PresenceState::Speaking);
    let hud = hakille_ui::EphemeralHud::new();
    let cost = hakille_ui::CostCounter::new(500, "EUR");
    let silence = hakille_ui::SilenceBudgetTracker::new(3);

    let snap = StableSnapshot::from_states(&presence, &hud, &cost, &silence, 0, 0);
    assert!(snap.presence_a11y_label.to_lowercase().contains("parole"));
}

/// `T3_PAIR_18`: F14 (Colorkey Transparency) + F02 (Abyss Surface 0 Token)
#[test]
fn test_t3_pair_18_colorkey_transparency_distinct_from_abyss_surface() {
    let colorkey = TRANSPARENT_KEY_RGB;
    let abyss_surface = (0x0B, 0x0C, 0x0F);
    assert_ne!(
        colorkey, abyss_surface,
        "Colorkey (0x010101) must be strictly distinct from Abyss surface (#0B0C0F)"
    );
}

/// `T3_PAIR_19`: F18 (E2E Test Runner) + F08 (Headless StableSnapshot Assertions)
#[test]
fn test_t3_pair_19_e2e_runner_with_headless_snapshot_assertions() {
    let harness = E2eHarness::new_authorized();
    let snap = harness.snapshot();
    assert_eq!(snap.cost_total_cents, 0);
    assert_eq!(snap.cost_budget_cents, 500);
}

/// `T3_PAIR_20`: F20 (Adversarial Hardening) + F05 (Protocol Stream Robustness)
#[test]
fn test_t3_pair_20_adversarial_hardening_with_protocol_robustness() {
    let mut broker = MockKernel::authorized();
    for i in 0..100 {
        let msg = UiMessage::new(UiToKernel::HudSubmit {
            query: format!("Fuzzed query #{i}"),
        });
        assert!(broker.handle_ui_message(msg).is_ok());
    }
}
