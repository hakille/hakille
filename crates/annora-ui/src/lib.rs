pub mod accessibility;
pub mod channel_selector;
pub mod cost_display;
pub mod ephemeral_hud;
pub mod journal_view;
pub mod onboarding;
pub mod platform_adapter;
pub mod presence;
pub mod silence_budget;

pub use accessibility::*;
pub use channel_selector::*;
pub use cost_display::*;
pub use ephemeral_hud::*;
pub use journal_view::*;
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
        assert_eq!(presence.pulse_period_ms(), 2000);

        presence.set_state(PresenceState::Acting);
        assert_eq!(presence.pulse_period_ms(), 600);

        // Validation accent theme rule: Coral #FF5722 is strictly reserved
        assert_eq!(presence.theme().validation_accent.r, 255);
        assert_eq!(presence.theme().validation_accent.g, 87);
        assert_eq!(presence.theme().validation_accent.b, 34);
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
        assert_eq!(hud.hotkey(), "Alt+Space");

        // Open prompt
        hud.open_prompt();
        hud.insert_char('H');
        hud.insert_char('i');
        assert_eq!(*hud.state(), HudState::Prompting { query: "Hi".to_string(), cursor_pos: 2 });

        // Cursor movement & deletion
        hud.move_cursor_left();
        assert_eq!(*hud.state(), HudState::Prompting { query: "Hi".to_string(), cursor_pos: 1 });
        hud.insert_char('a');
        assert_eq!(*hud.state(), HudState::Prompting { query: "Hai".to_string(), cursor_pos: 2 });
        hud.delete(); // deletes 'i'
        assert_eq!(*hud.state(), HudState::Prompting { query: "Ha".to_string(), cursor_pos: 2 });
        hud.clear();
        assert_eq!(*hud.state(), HudState::Prompting { query: "".to_string(), cursor_pos: 0 });

        // Re-type and stream response
        hud.insert_text("Hi");
        hud.set_streaming("Hi", "Bonjour ! Comment puis-je vous aider ?", true);
        let ascii = hud.render_ascii();
        assert!(ascii.contains("Bonjour !"));

        // Validation required (Coral accent reserved)
        hud.request_validation("wallet-spend-12", "Envoyer 25.00 EUR à Pierre", Some(2500), "Financial");
        let val_ascii = hud.render_ascii();
        assert!(val_ascii.contains("#FF5722 CORAL"));
        assert!(val_ascii.contains("25.00 EUR"));

        // Evaporate
        hud.start_evaporation();
        match hud.state() {
            HudState::Evaporating { remaining_ms, .. } => assert_eq!(*remaining_ms, 1500),
            _ => panic!("Expected evaporating state"),
        }
    }

    #[test]
    fn test_silence_budget_quota_and_replenish() {
        let mut budget = SilenceBudgetTracker::new(2);
        assert_eq!(budget.credits_remaining(), 2);

        // 1. User-scheduled reminder -> costs 0 credits
        let allowed1 = budget.request_initiative("remind-1", InitiativeOrigin::UserScheduled, 0.5, "Rappel RDV");
        assert!(allowed1);
        assert_eq!(budget.credits_remaining(), 2);

        // 2. System-inferred suggestion with high confidence -> costs 1 credit
        let allowed2 = budget.request_initiative("sugg-1", InitiativeOrigin::SystemInferred, 0.92, "Proposer pause");
        assert!(allowed2);
        assert_eq!(budget.credits_remaining(), 1);

        // 3. System-inferred with low confidence (< 0.85) -> rejected, no credit spent
        let allowed3 = budget.request_initiative("sugg-low", InitiativeOrigin::SystemInferred, 0.60, "Proposition faible");
        assert!(!allowed3);
        assert_eq!(budget.credits_remaining(), 1);

        // 4. Spend second credit
        let allowed4 = budget.request_initiative("sugg-2", InitiativeOrigin::SystemInferred, 0.95, "Sauvegarder travail");
        assert!(allowed4);
        assert_eq!(budget.credits_remaining(), 0);

        // 5. Budget exhausted -> rejected even with 1.0 confidence
        let allowed5 = budget.request_initiative("sugg-3", InitiativeOrigin::SystemInferred, 1.0, "Autre proposition");
        assert!(!allowed5);

        // 6. User rewards helpful suggestion -> +1 credit
        budget.reward_helpful_initiative();
        assert_eq!(budget.credits_remaining(), 1);
    }

    #[test]
    fn test_platform_adapter_cross_os() {
        let adapter = PlatformAdapter::auto_detect();
        let tray_spec = adapter.create_tray_spec(PresenceState::Listening, 0);
        assert_eq!(tray_spec.pulse_period_ms, 2000);
        assert!(!tray_spec.has_attention_badge);

        let acting_spec = adapter.create_tray_spec(PresenceState::Acting, 2);
        assert_eq!(acting_spec.pulse_period_ms, 600);
        assert!(acting_spec.has_attention_badge);

        assert!(!adapter.os_integration_details().is_empty());
    }
}

