pub mod accessibility;
pub mod cost_display;
pub mod journal_view;
pub mod onboarding;
pub mod presence;

pub use accessibility::*;
pub use cost_display::*;
pub use journal_view::*;
pub use onboarding::*;
pub use presence::*;

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
}
