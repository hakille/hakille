use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostState {
    pub total_cents: u64,
    pub daily_budget_cents: u64,
    pub currency: String,
    pub alert_threshold_ratio: f32,
}

impl Default for CostState {
    fn default() -> Self {
        Self {
            total_cents: 0,
            daily_budget_cents: 500, // 5.00 EUR / day
            currency: "EUR".to_string(),
            alert_threshold_ratio: 0.8,
        }
    }
}

pub struct CostCounter {
    state: CostState,
}

impl CostCounter {
    pub fn new(daily_budget_cents: u64, currency: impl Into<String>) -> Self {
        Self {
            state: CostState {
                total_cents: 0,
                daily_budget_cents,
                currency: currency.into(),
                alert_threshold_ratio: 0.8,
            },
        }
    }

    pub fn record_cost(&mut self, cents: u64) {
        self.state.total_cents += cents;
    }

    pub fn formatted_total(&self) -> String {
        let euros = self.state.total_cents as f64 / 100.0;
        format!("{:.2} {}", euros, self.state.currency)
    }

    pub fn is_budget_exceeded(&self) -> bool {
        self.state.total_cents >= self.state.daily_budget_cents
    }

    pub fn is_near_budget_limit(&self) -> bool {
        let ratio = self.state.total_cents as f32 / self.state.daily_budget_cents.max(1) as f32;
        ratio >= self.state.alert_threshold_ratio
    }

    pub fn state(&self) -> &CostState {
        &self.state
    }
}

impl Default for CostCounter {
    fn default() -> Self {
        Self {
            state: CostState::default(),
        }
    }
}
