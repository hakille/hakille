use hakille_core::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InitiativeOrigin {
    /// Scheduled directly by the user (cost 0 credits)
    UserScheduled,
    /// Inferred autonomously by System 2 (costs 1 interruption credit)
    SystemInferred,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterruptionEvent {
    pub id: String,
    pub timestamp: Timestamp,
    pub origin: InitiativeOrigin,
    pub confidence: f64,
    pub description: String,
    pub allowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SilenceBudgetTracker {
    daily_quota: u32,
    credits_remaining: u32,
    min_confidence_threshold: f64,
    history: Vec<InterruptionEvent>,
    last_reset_epoch_day: i64,
}

impl SilenceBudgetTracker {
    /// Create a new silence budget tracker with a daily credit limit (default 3 suggestions/day, spec §18/§29)
    pub fn new(daily_quota: u32) -> Self {
        let current_day = chrono::Utc::now().timestamp() / 86400;
        Self {
            daily_quota,
            credits_remaining: daily_quota,
            min_confidence_threshold: 0.85,
            history: Vec::new(),
            last_reset_epoch_day: current_day,
        }
    }

    pub fn daily_quota(&self) -> u32 {
        self.daily_quota
    }

    pub fn credits_remaining(&self) -> u32 {
        self.credits_remaining
    }

    pub fn min_confidence_threshold(&self) -> f64 {
        self.min_confidence_threshold
    }

    pub fn history(&self) -> &[InterruptionEvent] {
        &self.history
    }

    /// Reset daily credits if a new day has arrived
    pub fn check_daily_rollover(&mut self) {
        let current_day = chrono::Utc::now().timestamp() / 86400;
        if current_day > self.last_reset_epoch_day {
            self.credits_remaining = self.daily_quota;
            self.last_reset_epoch_day = current_day;
        }
    }

    /// Request to initiate a proactive interaction (§21d)
    pub fn request_initiative(
        &mut self,
        id: impl Into<String>,
        origin: InitiativeOrigin,
        confidence: f64,
        description: impl Into<String>,
    ) -> bool {
        self.check_daily_rollover();
        let id_str = id.into();
        let desc_str = description.into();

        match origin {
            InitiativeOrigin::UserScheduled => {
                // User-scheduled reminders NEVER consume budget
                self.history.push(InterruptionEvent {
                    id: id_str,
                    timestamp: Timestamp::now(),
                    origin,
                    confidence,
                    description: desc_str,
                    allowed: true,
                });
                true
            }
            InitiativeOrigin::SystemInferred => {
                // Autonomous suggestions must pass both confidence threshold and budget availability
                let passes_confidence = confidence >= self.min_confidence_threshold;
                let has_credits = self.credits_remaining > 0;

                let allowed = passes_confidence && has_credits;
                if allowed {
                    self.credits_remaining -= 1;
                }

                self.history.push(InterruptionEvent {
                    id: id_str,
                    timestamp: Timestamp::now(),
                    origin,
                    confidence,
                    description: desc_str,
                    allowed,
                });

                allowed
            }
        }
    }

    /// Explicitly replenish credits (e.g. if user thanks or approves of suggestions)
    pub fn reward_helpful_initiative(&mut self) {
        if self.credits_remaining < self.daily_quota {
            self.credits_remaining += 1;
        }
    }
}

impl Default for SilenceBudgetTracker {
    fn default() -> Self {
        Self::new(3)
    }
}
