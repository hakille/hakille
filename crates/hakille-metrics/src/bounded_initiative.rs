use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitiativeRecord {
    pub proposal_id: String,
    pub within_capability_bounds: bool,
    pub user_accepted: bool,
}

/// Snapshot sérialisable de [`BoundedInitiativeTracker`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundedInitiativeSnapshot {
    pub records: Vec<InitiativeRecord>,
    pub compliance_rate: f64,
    pub acceptance_rate: f64,
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

    pub fn has_measurements(&self) -> bool {
        !self.records.is_empty()
    }

    pub fn compliance_rate_checked(&self) -> Option<f64> {
        if self.records.is_empty() {
            return None;
        }
        let strictly_bounded = self
            .records
            .iter()
            .filter(|r| r.within_capability_bounds)
            .count();
        Some(strictly_bounded as f64 / self.records.len() as f64)
    }

    pub fn compliance_rate(&self) -> f64 {
        self.compliance_rate_checked().unwrap_or(1.0)
    }

    pub fn acceptance_rate_checked(&self) -> Option<f64> {
        if self.records.is_empty() {
            return None;
        }
        let accepted = self.records.iter().filter(|r| r.user_accepted).count();
        Some(accepted as f64 / self.records.len() as f64)
    }

    pub fn acceptance_rate(&self) -> f64 {
        self.acceptance_rate_checked().unwrap_or(1.0)
    }

    /// Snapshot mémoire simple (sérialisable en JSON).
    pub fn to_snapshot(&self) -> BoundedInitiativeSnapshot {
        BoundedInitiativeSnapshot {
            records: self.records.clone(),
            compliance_rate: self.compliance_rate(),
            acceptance_rate: self.acceptance_rate(),
        }
    }

    /// Persistance JSON atomique via fichier temporaire.
    pub fn save_to_file(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(&self.to_snapshot())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        crate::atomic_write_json(path, &json)
    }

    pub fn load_from_file(path: &std::path::Path) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let snap: BoundedInitiativeSnapshot = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Self {
            records: snap.records,
        })
    }
}

impl Default for BoundedInitiativeTracker {
    fn default() -> Self {
        Self::new()
    }
}
