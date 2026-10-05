use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitiativeRecord {
    pub proposal_id: String,
    pub within_capability_bounds: bool,
    pub user_accepted: bool,
}

pub struct BoundedInitiativeTracker {
    records: Vec<InitiativeRecord>,
}

impl BoundedInitiativeTracker {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    pub fn record_proposal(
        &mut self,
        proposal_id: impl Into<String>,
        within_bounds: bool,
        user_accepted: bool,
    ) {
        self.records.push(InitiativeRecord {
            proposal_id: proposal_id.into(),
            within_capability_bounds: within_bounds,
            user_accepted,
        });
    }

    pub fn compliance_rate(&self) -> f64 {
        if self.records.is_empty() {
            return 1.0;
        }

        let strictly_bounded = self
            .records
            .iter()
            .filter(|r| r.within_capability_bounds)
            .count();

        strictly_bounded as f64 / self.records.len() as f64
    }

    pub fn acceptance_rate(&self) -> f64 {
        if self.records.is_empty() {
            return 1.0;
        }

        let accepted = self.records.iter().filter(|r| r.user_accepted).count();
        accepted as f64 / self.records.len() as f64
    }
}

impl Default for BoundedInitiativeTracker {
    fn default() -> Self {
        Self::new()
    }
}
