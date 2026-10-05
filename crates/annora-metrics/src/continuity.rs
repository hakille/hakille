use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContinuityRecord {
    pub task_id: String,
    pub required_re_explanation: bool,
    pub turns_count: usize,
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

    pub fn record_task(&mut self, task_id: impl Into<String>, required_re_explanation: bool, turns_count: usize) {
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
}

impl Default for ContinuityTracker {
    fn default() -> Self {
        Self::new()
    }
}
