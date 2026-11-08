pub mod accessibility;
pub mod aura_ring;
pub mod channel_selector;
pub mod cost_display;
pub mod ephemeral_hud;
pub mod journal_view;
pub mod notification_router;
pub mod onboarding;
pub mod platform_adapter;
pub mod presence;
pub mod silence_budget;

pub use accessibility::*;
pub use aura_ring::*;
pub use channel_selector::*;
pub use cost_display::*;
pub use ephemeral_hud::*;
pub use journal_view::*;
pub use notification_router::*;
pub use onboarding::*;
pub use platform_adapter::*;
pub use presence::*;
pub use silence_budget::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_presence_indicator_states_and_pulses() {
        let mut presence = PresenceIndicator::new();
        assert_eq!(presence.state(), PresenceState::Idle);
        assert_eq!(presence.pulse_period_ms(), 0);

        presence.set_state(PresenceState::Listening);
        assert_eq!(presence.pulse_period_ms(), 4000);

        presence.set_state(PresenceState::Acting);
        assert_eq!(presence.pulse_period_ms(), 1200);

        presence.set_state(PresenceState::Speaking);
        assert_eq!(presence.pulse_period_ms(), 16);

        presence.set_state(PresenceState::DecisionRequired);
        assert_eq!(presence.pulse_period_ms(), 0);
        assert!(presence.ring_color_for_state().is_some());

        presence.set_state(PresenceState::Urgent);
        assert_eq!(presence.pulse_period_ms(), 800);

        // Braise #FF7A2F strictement réservée à la validation (spec §3).
        assert_eq!(presence.theme().validation_accent.r, 255);
        assert_eq!(presence.theme().validation_accent.g, 122);
        assert_eq!(presence.theme().validation_accent.b, 47);
        // surface/0 #0B0C0F, texte/1 #ECEEF2.
        assert_eq!(presence.theme().background.r, 11);
        assert_eq!(presence.theme().text.r, 236);
        // Fils Parole : enveloppe → 0..24 px.
        assert_eq!(PresenceIndicator::speech_wire_height_px(0.5), 12.0);
        assert_eq!(PresenceIndicator::speech_wire_height_px(2.0), 24.0);
    }

    #[test]
    fn test_cost_counter_and_budget_alert() {
        let mut counter = CostCounter::new(500, "EUR"); // 5.00 EUR budget
        assert_eq!(counter.formatted_total(), "0.00 EUR");
        assert!(!counter.is_near_budget_limit());
        assert!(!counter.is_budget_exceeded());

        // Spend 4.10 EUR (82% of budget -> exceeds 80% alert threshold)
        counter.record_cost(410);
        assert_eq!(counter.formatted_total(), "4.10 EUR");
        assert!(counter.is_near_budget_limit());
        assert!(!counter.is_budget_exceeded());

        // Spend 1.00 EUR -> Total 5.10 EUR -> exceeds budget
        counter.record_cost(100);
        assert_eq!(counter.formatted_total(), "5.10 EUR");
        assert!(counter.is_budget_exceeded());
    }

    #[test]
    fn test_channel_selector_arbitration() {
        // 1. In meeting -> Ephemeral HUD
        let mut ctx = InteractionContext {
            in_meeting: true,
            ..Default::default()
        };
        let decision = ChannelSelector::arbitrate(&ctx);
        assert_eq!(decision.channel, OutputChannel::EphemeralHud);

        // 2. Third parties present -> Ephemeral HUD (protect privacy)
        ctx = InteractionContext {
            third_parties_present: true,
            ..Default::default()
        };
        let decision2 = ChannelSelector::arbitrate(&ctx);
        assert_eq!(decision2.channel, OutputChannel::EphemeralHud);

        // 3. User wearing headphones in third party presence -> Voice allowed
        ctx.user_wearing_headphones = true;
        let decision3 = ChannelSelector::arbitrate(&ctx);
        assert_eq!(decision3.channel, OutputChannel::VoiceNormal);

        // 4. Critical urgency -> Immediate voice or HUD with urgent vibration
        ctx = InteractionContext {
            urgency: UrgencyLevel::Critical,
            in_meeting: true,
            ..Default::default()
        };
        let decision4 = ChannelSelector::arbitrate(&ctx);
        assert_eq!(decision4.haptic_feedback, Some(HapticPattern::UrgentBuzz));

        // 5. Wearable low urgency task -> Haptic
        ctx = InteractionContext {
            device_type: DeviceType::Wearable,
            urgency: UrgencyLevel::Low,
            ..Default::default()
        };
        let decision5 = ChannelSelector::arbitrate(&ctx);
        assert_eq!(decision5.channel, OutputChannel::Haptic);
    }

    #[test]
    fn test_ephemeral_hud_lifecycle_and_validation() {
        let mut hud = EphemeralHud::new();
        assert_eq!(*hud.state(), HudState::Hidden);
        assert_eq!(hud.hotkey(), "Ctrl+Alt+Space");

        // Open prompt
        hud.open_prompt();
        hud.insert_char('H');
        hud.insert_char('i');
        assert_eq!(
            *hud.state(),
            HudState::Prompting {
                query: "Hi".to_string(),
                cursor_pos: 2
            }
        );

        // Cursor movement & deletion
        hud.move_cursor_left();
        assert_eq!(
            *hud.state(),
            HudState::Prompting {
                query: "Hi".to_string(),
                cursor_pos: 1
            }
        );
        hud.insert_char('a');
        assert_eq!(
            *hud.state(),
            HudState::Prompting {
                query: "Hai".to_string(),
                cursor_pos: 2
            }
        );
        hud.delete(); // deletes 'i'
        assert_eq!(
            *hud.state(),
            HudState::Prompting {
                query: "Ha".to_string(),
                cursor_pos: 2
            }
        );
        hud.clear();
        assert_eq!(
            *hud.state(),
            HudState::Prompting {
                query: "".to_string(),
                cursor_pos: 0
            }
        );

        // Re-type and stream response
        hud.insert_text("Hi");
        hud.set_streaming("Hi", "Bonjour ! Comment puis-je vous aider ?", true);
        let ascii = hud.render_ascii();
        assert!(ascii.contains("Bonjour !"));

        // Validation required (Coral accent reserved)
        hud.request_validation(
            "wallet-spend-12",
            "Envoyer 25.00 EUR à Pierre",
            Some(2500),
            "Financial",
        );
        let val_ascii = hud.render_ascii();
        assert!(val_ascii.contains("#FF7A2F BRAISE"));
        assert!(val_ascii.contains("25.00 EUR"));

        // Evaporate
        hud.start_evaporation();
        match hud.state() {
            HudState::Evaporating { remaining_ms, .. } => assert_eq!(*remaining_ms, 1500),
            _ => panic!("Expected evaporating state"),
        }
    }

    #[test]
    fn test_hud_editing_utf8_cjk_arabic_emoji() {
        // Spike S3 (partie CI) : curseur en BYTES, frontières de caractères.
        // CJK (3 octets), arabe (2 octets + jointures), émojis (4 octets).
        let mut hud = EphemeralHud::new();
        hud.open_prompt();
        hud.insert_text("日本語テスト"); // 6 caractères, 18 octets
        hud.insert_char(' ');
        hud.insert_text("مرحبا"); // 5 caractères arabes
        hud.insert_char(' ');
        hud.insert_text("👋🏽"); // emoji + modificateur
        let (query, cursor) = match hud.state() {
            HudState::Prompting { query, cursor_pos } => (query.clone(), *cursor_pos),
            _ => panic!("expected prompting"),
        };
        assert_eq!(cursor, query.len()); // curseur à la fin, frontière valide
        assert!(query.is_char_boundary(cursor));
        // Recul caractère par caractère : jamais de panique, jamais au milieu.
        let start_len = query.len();
        let mut hud2 = hud;
        let mut last = start_len;
        while last > 0 {
            hud2.backspace();
            let cur = match hud2.state() {
                HudState::Prompting { query, cursor_pos } => {
                    assert!(query.is_char_boundary(*cursor_pos));
                    *cursor_pos
                }
                _ => panic!("expected prompting"),
            };
            assert!(cur < last);
            last = cur;
        }
        // Déplacement gauche/droite sur frontières.
        let mut hud3 = EphemeralHud::new();
        hud3.open_prompt();
        hud3.insert_text("aé中b");
        hud3.move_cursor_left();
        hud3.move_cursor_left();
        match hud3.state() {
            HudState::Prompting { query, cursor_pos } => {
                assert!(query.is_char_boundary(*cursor_pos));
                assert_eq!(&query[..*cursor_pos], "aé");
            }
            _ => panic!("expected prompting"),
        }
    }

    #[test]
    fn test_silence_budget_quota_and_replenish() {
        let mut budget = SilenceBudgetTracker::new(2);
        assert_eq!(budget.credits_remaining(), 2);

        // 1. User-scheduled reminder -> costs 0 credits
        let allowed1 = budget.request_initiative(
            "remind-1",
            InitiativeOrigin::UserScheduled,
            0.5,
            "Rappel RDV",
        );
        assert!(allowed1);
        assert_eq!(budget.credits_remaining(), 2);

        // 2. System-inferred suggestion with high confidence -> costs 1 credit
        let allowed2 = budget.request_initiative(
            "sugg-1",
            InitiativeOrigin::SystemInferred,
            0.92,
            "Proposer pause",
        );
        assert!(allowed2);
        assert_eq!(budget.credits_remaining(), 1);

        // 3. System-inferred with low confidence (< 0.85) -> rejected, no credit spent
        let allowed3 = budget.request_initiative(
            "sugg-low",
            InitiativeOrigin::SystemInferred,
            0.60,
            "Proposition faible",
        );
        assert!(!allowed3);
        assert_eq!(budget.credits_remaining(), 1);

        // 4. Spend second credit
        let allowed4 = budget.request_initiative(
            "sugg-2",
            InitiativeOrigin::SystemInferred,
            0.95,
            "Sauvegarder travail",
        );
        assert!(allowed4);
        assert_eq!(budget.credits_remaining(), 0);

        // 5. Budget exhausted -> rejected even with 1.0 confidence
        let allowed5 = budget.request_initiative(
            "sugg-3",
            InitiativeOrigin::SystemInferred,
            1.0,
            "Autre proposition",
        );
        assert!(!allowed5);

        // 6. User rewards helpful suggestion -> +1 credit
        budget.reward_helpful_initiative();
        assert_eq!(budget.credits_remaining(), 1);
    }

    #[test]
    fn test_platform_adapter_cross_os() {
        let adapter = PlatformAdapter::auto_detect();
        let tray_spec = adapter.create_tray_spec(PresenceState::Listening, 0);
        assert_eq!(tray_spec.pulse_period_ms, 4000);
        assert!(!tray_spec.has_attention_badge);

        let acting_spec = adapter.create_tray_spec(PresenceState::Acting, 2);
        assert_eq!(acting_spec.pulse_period_ms, 1200);
        assert!(acting_spec.has_attention_badge);

        assert!(!adapter.os_integration_details().is_empty());
    }

    #[test]
    fn test_hotkey_default_and_alias_spec() {
        // Spec §8/§29 : ⌃⌥Espace par défaut modifiable dès l'onboarding.
        assert_eq!(
            EphemeralHud::default_hotkey_for(PlatformKind::MacOS),
            "Cmd+Space"
        );
        assert_eq!(
            EphemeralHud::default_hotkey_for(PlatformKind::Windows),
            "Ctrl+Alt+Space"
        );
        assert_eq!(
            EphemeralHud::default_hotkey_for(PlatformKind::Linux),
            "Ctrl+Alt+Space"
        );
        // Alias historique accepté, jamais le défaut.
        assert!(EphemeralHud::is_hotkey_alias_accepted("Alt+Space"));
        assert!(EphemeralHud::is_hotkey_alias_accepted("Ctrl+Alt+Space"));
        assert!(!EphemeralHud::is_hotkey_alias_accepted("F12"));

        let mut hud = EphemeralHud::with_platform(PlatformKind::MacOS);
        assert_eq!(hud.hotkey(), "Cmd+Space");
        hud.set_custom_hotkey("Alt+Space");
        assert_eq!(hud.hotkey(), "Alt+Space");
        assert_eq!(hud.custom_hotkey(), Some("Alt+Space"));
        hud.clear_custom_hotkey();
        assert_eq!(hud.hotkey(), "Cmd+Space");

        // PlatformAdapter utilise le même défaut.
        let adapter = PlatformAdapter::auto_detect();
        assert!(
            EphemeralHud::is_hotkey_alias_accepted(adapter.config().global_hotkey.as_str())
                || adapter.config().global_hotkey == "Cmd+Space"
                || adapter.config().global_hotkey == "Ctrl+Alt+Space"
        );
        assert_ne!(adapter.config().global_hotkey, "Alt+Space");
    }

    #[test]
    fn test_silence_default_quota_is_three() {
        // Spec §18/§29 : quota 3/jour.
        let budget = SilenceBudgetTracker::default();
        assert_eq!(budget.daily_quota(), 3);
        assert_eq!(budget.credits_remaining(), 3);
    }

    #[test]
    fn test_channel_doc_thresholds_and_defer() {
        // Doc §21a : >65dB → HUD (pas 72). 66dB sans casque → EphemeralHud.
        let ctx = InteractionContext {
            ambient_noise_db: 66.0,
            user_attentive_to_screen: false,
            ..Default::default()
        };
        let d = ChannelSelector::arbitrate(&ctx);
        assert_eq!(d.channel, OutputChannel::EphemeralHud);

        // Vision table : urgence faible + non attentif → Defer (jamais AmbientPulse).
        let ctx2 = InteractionContext {
            urgency: UrgencyLevel::Low,
            user_attentive_to_screen: false,
            device_type: DeviceType::Desktop,
            ..Default::default()
        };
        let d2 = ChannelSelector::arbitrate(&ctx2);
        assert_eq!(d2.channel, OutputChannel::Defer);

        // Wearable + Low garde son haptique (pas de régression).
        let ctx3 = InteractionContext {
            urgency: UrgencyLevel::Low,
            device_type: DeviceType::Wearable,
            ..Default::default()
        };
        assert_eq!(
            ChannelSelector::arbitrate(&ctx3).channel,
            OutputChannel::Haptic
        );
    }

    #[test]
    fn test_accessibility_reconfigures_channel_not_degrades() {
        use crate::{AccessibilityManager, AccessibilityProfile, PrimaryModality};
        let ctx = InteractionContext::default(); // attentif desktop → EphemeralHud déjà
        let base = ChannelSelector::arbitrate(&ctx);

        // Voix d'abord : aucune modification.
        let voice = AccessibilityManager::default();
        assert_eq!(voice.override_decision(&base).channel, base.channel);

        // Sourd (TextFirst) : une décision voix devient HUD silencieux.
        let mut text = AccessibilityManager::new(AccessibilityProfile {
            primary_modality: PrimaryModality::TextFirst,
            ..Default::default()
        });
        let _ = &mut text;
        let voice_decision = ChannelDecision {
            channel: OutputChannel::VoiceNormal,
            rationale: "test".into(),
            haptic_feedback: None,
        };
        let over = text.override_decision(&voice_decision);
        assert_eq!(over.channel, OutputChannel::EphemeralHud);
        assert!(over.rationale.contains("TextFirst"));

        // HapticAssisted : tout devient haptique avec DoubleTap par défaut.
        let hap = AccessibilityManager::new(AccessibilityProfile {
            primary_modality: PrimaryModality::HapticAssisted,
            ..Default::default()
        });
        let over2 = hap.override_decision(&voice_decision);
        assert_eq!(over2.channel, OutputChannel::Haptic);
        assert_eq!(over2.haptic_feedback, Some(HapticPattern::DoubleTap));
    }

    #[test]
    fn test_hud_tick_evaporates_to_hidden() {
        let mut hud = EphemeralHud::new();
        hud.open_prompt();
        hud.insert_text("test");
        hud.set_streaming("test", "réponse", true);
        hud.start_evaporation();
        match hud.state() {
            HudState::Evaporating { remaining_ms, .. } => assert_eq!(*remaining_ms, 1500),
            _ => panic!("expected evaporating"),
        }
        hud.tick(500);
        match hud.state() {
            HudState::Evaporating { remaining_ms, .. } => assert_eq!(*remaining_ms, 1000),
            _ => panic!("expected still evaporating"),
        }
        hud.tick(1000);
        assert_eq!(*hud.state(), HudState::Hidden);
    }

    #[test]
    fn test_onboarding_injects_real_platform() {
        let mut m = OnboardingManager::new();
        m.inject_platform(
            "Native",
            "Windows",
            "11 (24H2)",
            &["Filesystem".to_string(), "Sensor".to_string()],
            &["FullSystem".to_string()],
        );
        let man = m.manifest();
        assert!(
            man.guaranteed_limits
                .iter()
                .any(|l| l.title == "Plateforme hôte détectée")
        );
        assert!(man.granted_capabilities.contains(&"Sensor".to_string()));
        assert!(!m.is_ready());
        m.acknowledge_limits();
        assert!(m.is_ready());
    }

    #[test]
    fn test_journal_full_reasoning_and_nodes() {
        use hakille_core::{ActorId, EntryId, Timestamp, UserId};
        let entry = hakille_audit::entry::AuditEntry {
            id: EntryId(uuid::Uuid::now_v7()),
            timestamp: Timestamp::now(),
            actor: ActorId::User(UserId(uuid::Uuid::now_v7())),
            action: hakille_audit::entry::ActionRecord {
                action_type: "test".into(),
                description: "d".into(),
                parameters: serde_json::json!({}),
                reversible: true,
                reversed: false,
            },
            capability_used: None,
            reasoning_trace: hakille_audit::entry::ReasoningTrace {
                steps: vec![
                    hakille_audit::entry::ReasoningStep {
                        description: "step one".into(),
                        confidence: 0.9,
                        evidence: vec![],
                    },
                    hakille_audit::entry::ReasoningStep {
                        description: "step two".into(),
                        confidence: 0.8,
                        evidence: vec![],
                    },
                ],
                model_used: None,
                duration_ms: 1,
            },
            graph_nodes_used: vec![],
            confidence: 0.9,
            cost: None,
            signature: vec![],
            prev_hash: vec![],
            token_hash: None,
        };
        let full = JournalView::full_reasoning(&entry);
        assert!(full.contains("step one") && full.contains("step two"));
        assert!(JournalView::graph_node_labels(&entry).is_empty());
    }

    #[test]
    fn test_aura_ring_four_chromatic_state_transitions() {
        let mut widget = AuraRingWidget::new(0.50);
        assert_eq!(widget.visual_state(), AuraVisualState::AmberAmbient);

        // 1. EmeraldHighTrust (tau >= 0.90)
        widget.update_confidence(0.90);
        assert_eq!(widget.visual_state(), AuraVisualState::EmeraldHighTrust);
        widget.update_confidence(0.99);
        assert_eq!(widget.visual_state(), AuraVisualState::EmeraldHighTrust);
        widget.update_confidence(1.0);
        assert_eq!(widget.visual_state(), AuraVisualState::EmeraldHighTrust);

        // 2. AmberAmbient (0.40 <= tau < 0.90)
        widget.update_confidence(0.8999);
        assert_eq!(widget.visual_state(), AuraVisualState::AmberAmbient);
        widget.update_confidence(0.50);
        assert_eq!(widget.visual_state(), AuraVisualState::AmberAmbient);
        widget.update_confidence(0.40);
        assert_eq!(widget.visual_state(), AuraVisualState::AmberAmbient);

        // 3. VioletStepUp (active challenge with tau >= 0.40)
        widget.set_step_up_active(true);
        assert_eq!(widget.visual_state(), AuraVisualState::VioletStepUp);
        widget.update_confidence(0.95); // Even with high trust, step-up challenge displays Violet
        assert_eq!(widget.visual_state(), AuraVisualState::VioletStepUp);
        widget.set_step_up_active(false);
        assert_eq!(widget.visual_state(), AuraVisualState::EmeraldHighTrust);

        // 4. RedLocked (tau < 0.40, locked, distress, or revoked)
        widget.update_confidence(0.399);
        assert_eq!(widget.visual_state(), AuraVisualState::RedLocked);
        widget.update_confidence(0.0);
        assert_eq!(widget.visual_state(), AuraVisualState::RedLocked);

        // Security precedence: lock overrides step-up and high trust
        widget.update_confidence(0.95);
        assert_eq!(widget.visual_state(), AuraVisualState::EmeraldHighTrust);
        widget.set_locked(true);
        assert_eq!(widget.visual_state(), AuraVisualState::RedLocked);
        widget.set_step_up_active(true);
        assert_eq!(widget.visual_state(), AuraVisualState::RedLocked); // lock has precedence over step-up
        widget.set_locked(false);
        assert_eq!(widget.visual_state(), AuraVisualState::VioletStepUp); // step-up now visible
        widget.set_step_up_active(false);
        assert_eq!(widget.visual_state(), AuraVisualState::EmeraldHighTrust);

        // Distress overrides all
        widget.set_distress(true);
        assert_eq!(widget.visual_state(), AuraVisualState::RedLocked);
        widget.set_distress(false);
        assert_eq!(widget.visual_state(), AuraVisualState::EmeraldHighTrust);

        // Revocation overrides all
        widget.set_revoked(true);
        assert_eq!(widget.visual_state(), AuraVisualState::RedLocked);
        widget.set_revoked(false);
        assert_eq!(widget.visual_state(), AuraVisualState::EmeraldHighTrust);
    }

    #[test]
    fn test_aura_ring_exact_colors_alpha_and_pulse_periods() {
        // Emerald 500 (#10B981, 6000ms, alpha 0.45)
        let emerald = AuraVisualState::EmeraldHighTrust;
        assert_eq!(emerald.primary_hex(), "#10B981");
        assert_eq!(emerald.primary_color().r, 16);
        assert_eq!(emerald.primary_color().g, 185);
        assert_eq!(emerald.primary_color().b, 129);
        assert_eq!(emerald.primary_color().a, 1.0);
        assert_eq!(emerald.pulse_period_ms(), 6000);
        assert_eq!(emerald.glow_alpha(), 0.45);
        assert_eq!(emerald.glow_color().a, 0.45);
        assert_eq!(emerald.glow_hex(), "#10B98173");

        // Amber 500 (#F59E0B, 3500ms, alpha 0.30)
        let amber = AuraVisualState::AmberAmbient;
        assert_eq!(amber.primary_hex(), "#F59E0B");
        assert_eq!(amber.primary_color().r, 245);
        assert_eq!(amber.primary_color().g, 158);
        assert_eq!(amber.primary_color().b, 11);
        assert_eq!(amber.primary_color().a, 1.0);
        assert_eq!(amber.pulse_period_ms(), 3500);
        assert_eq!(amber.glow_alpha(), 0.30);
        assert_eq!(amber.glow_color().a, 0.30);
        assert_eq!(amber.glow_hex(), "#F59E0B4D");

        // Violet 500 (#8B5CF6, 1000ms, alpha 0.60)
        let violet = AuraVisualState::VioletStepUp;
        assert_eq!(violet.primary_hex(), "#8B5CF6");
        assert_eq!(violet.primary_color().r, 139);
        assert_eq!(violet.primary_color().g, 92);
        assert_eq!(violet.primary_color().b, 246);
        assert_eq!(violet.primary_color().a, 1.0);
        assert_eq!(violet.pulse_period_ms(), 1000);
        assert_eq!(violet.glow_alpha(), 0.60);
        assert_eq!(violet.glow_color().a, 0.60);
        assert_eq!(violet.glow_hex(), "#8B5CF699");

        // Red 500 (#EF4444, 800ms, alpha 0.70)
        let red = AuraVisualState::RedLocked;
        assert_eq!(red.primary_hex(), "#EF4444");
        assert_eq!(red.primary_color().r, 239);
        assert_eq!(red.primary_color().g, 68);
        assert_eq!(red.primary_color().b, 68);
        assert_eq!(red.primary_color().a, 1.0);
        assert_eq!(red.pulse_period_ms(), 800);
        assert_eq!(red.glow_alpha(), 0.70);
        assert_eq!(red.glow_color().a, 0.70);
        assert_eq!(red.glow_hex(), "#EF4444B3");
    }

    #[test]
    fn test_aura_ring_animation_sine_wave_scale_oscillation() {
        let widget = AuraRingWidget::new(0.95);
        let period = widget.pulse_period_ms();
        assert_eq!(period, 6000);

        // Check periodic oscillation boundaries [0.95, 1.05]
        for t in (0..=12000).step_by(50) {
            let factor = widget.animation_factor(t);
            assert!(
                (0.9499..=1.0501).contains(&factor),
                "Animation factor {factor} outside [0.95, 1.05] at t={t}"
            );
        }

        // Exact phase points for sine wave 1.0 + 0.05 * sin(2*pi*t/T)
        // t = 0 -> sin(0) = 0 -> scale = 1.0
        assert!((widget.animation_factor(0) - 1.0).abs() < 1e-5);
        // t = period / 4 -> sin(pi/2) = 1.0 -> scale = 1.05
        assert!((widget.animation_factor(period as u64 / 4) - 1.05).abs() < 1e-4);
        // t = period / 2 -> sin(pi) = 0 -> scale = 1.0
        assert!((widget.animation_factor(period as u64 / 2) - 1.0).abs() < 1e-4);
        // t = 3 * period / 4 -> sin(3pi/2) = -1.0 -> scale = 0.95
        assert!((widget.animation_factor(3 * period as u64 / 4) - 0.95).abs() < 1e-4);
        // t = period -> sin(2pi) = 0 -> scale = 1.0
        assert!((widget.animation_factor(period as u64) - 1.0).abs() < 1e-5);

        // Strict periodicity: f(t) == f(t + period)
        for t in [123u64, 456, 789, 1500, 2750, 4200] {
            let f1 = widget.animation_factor(t);
            let f2 = widget.animation_factor(t + period as u64);
            assert!((f1 - f2).abs() < 1e-5, "Periodicity failed at t={t}");
        }
    }

    #[test]
    fn test_aura_ring_widget_dto_and_serde_bincode_roundtrip() {
        let widget = AuraRingWidget::new(0.92);
        assert_eq!(widget.visual_state(), AuraVisualState::EmeraldHighTrust);

        let timestamp_ms = 1794078900000;
        let dto = widget.to_update_dto(timestamp_ms);
        assert_eq!(dto.tau, 0.92);
        assert_eq!(dto.state_name, "EmeraldHighTrust");
        assert_eq!(dto.primary_hex, "#10B981");
        assert_eq!(dto.glow_hex, "#10B98173");
        assert_eq!(dto.pulse_period_ms, 6000);
        assert_eq!(dto.timestamp_ms, timestamp_ms);
        assert!(dto.scale_factor >= 0.95 && dto.scale_factor <= 1.05);

        // JSON roundtrip
        let json = serde_json::to_string(&dto).unwrap();
        let from_json: hakille_ui_protocol::AuraRingUpdateDto =
            serde_json::from_str(&json).unwrap();
        assert_eq!(dto, from_json);

        // Bincode roundtrip
        let bin = bincode::serialize(&dto).unwrap();
        let from_bin: hakille_ui_protocol::AuraRingUpdateDto = bincode::deserialize(&bin).unwrap();
        assert_eq!(dto, from_bin);

        // Widget serde roundtrip
        let w_json = serde_json::to_string(&widget).unwrap();
        let w_from_json: AuraRingWidget = serde_json::from_str(&w_json).unwrap();
        assert_eq!(widget.tau, w_from_json.tau);
        assert_eq!(widget.visual_state, w_from_json.visual_state);

        // Clamping check
        let mut clamped = AuraRingWidget::new(-0.5);
        assert_eq!(clamped.tau(), 0.0);
        clamped.update_confidence(1.5);
        assert_eq!(clamped.tau(), 1.0);
    }
}
