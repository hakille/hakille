use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContinuityRecord {
    pub task_id: String,
    pub required_re_explanation: bool,
    pub turns_count: usize,
}

/// Snapshot sérialisable de [`ContinuityTracker`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuitySnapshot {
    pub records: Vec<TaskContinuityRecord>,
    pub continuity_rate: f64,
    pub total_tasks: usize,
}

pub struct ContinuityTracker {
    records: Vec<TaskContinuityRecord>,
}

impl ContinuityTracker {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    pub fn record_task(
        &mut self,
        task_id: impl Into<String>,
        required_re_explanation: bool,
        turns_count: usize,
    ) {
        self.records.push(TaskContinuityRecord {
            task_id: task_id.into(),
            required_re_explanation,
            turns_count,
        });
    }

    pub fn continuity_rate(&self) -> f64 {
        if self.records.is_empty() {
            return 1.0;
        }

        let without_re_explanation = self
            .records
            .iter()
            .filter(|r| !r.required_re_explanation)
            .count();

        without_re_explanation as f64 / self.records.len() as f64
    }

    pub fn total_tasks(&self) -> usize {
        self.records.len()
    }

    /// Snapshot mémoire simple (sérialisable en JSON).
    pub fn to_snapshot(&self) -> ContinuitySnapshot {
        ContinuitySnapshot {
            records: self.records.clone(),
            continuity_rate: self.continuity_rate(),
            total_tasks: self.total_tasks(),
        }
    }

    /// Persistance JSON simple via `std::fs`.
    pub fn save_to_file(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(&self.to_snapshot())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    pub fn load_from_file(path: &std::path::Path) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let snap: ContinuitySnapshot = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Self {
            records: snap.records,
        })
    }
}

impl Default for ContinuityTracker {
    fn default() -> Self {
        Self::new()
    }
}
