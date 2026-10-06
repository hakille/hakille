use annora_core::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Blocked,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskPriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStep {
    pub step_index: u32,
    pub description: String,
    pub capability_required: Option<String>,
    pub completed: bool,
    pub completed_at: Option<Timestamp>,
    pub result_summary: Option<String>,
}

/// A short-to-medium term bounded action sequence (§28e).
/// Unlike long-term [`Objective`], a `Task` has explicit operational boundaries,
/// step sequences, and immediate completion conditions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub parent_objective_id: Option<String>,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub steps: Vec<TaskStep>,
    pub created_at: Timestamp,
    pub deadline: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
}

impl Task {
    pub fn new(id: impl Into<String>, title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: description.into(),
            parent_objective_id: None,
            status: TaskStatus::Pending,
            priority: TaskPriority::Normal,
            steps: Vec::new(),
            created_at: Timestamp::now(),
            deadline: None,
            completed_at: None,
        }
    }

    pub fn with_parent_objective(mut self, objective_id: impl Into<String>) -> Self {
        self.parent_objective_id = Some(objective_id.into());
        self
    }

    pub fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_deadline(mut self, deadline: Timestamp) -> Self {
        self.deadline = Some(deadline);
        self
    }

    pub fn add_step(&mut self, description: impl Into<String>, capability_required: Option<String>) -> u32 {
        let step_index = self.steps.len() as u32;
        self.steps.push(TaskStep {
            step_index,
            description: description.into(),
            capability_required,
            completed: false,
            completed_at: None,
            result_summary: None,
        });
        step_index
    }

    pub fn complete_step(&mut self, index: usize, summary: Option<String>) -> bool {
        if let Some(step) = self.steps.get_mut(index) {
            step.completed = true;
            step.completed_at = Some(Timestamp::now());
            step.result_summary = summary;
            
            // Auto-advance task state
            if self.steps.iter().all(|s| s.completed) {
                self.status = TaskStatus::Completed;
                self.completed_at = Some(Timestamp::now());
            } else {
                self.status = TaskStatus::InProgress;
            }
            true
        } else {
            false
        }
    }

    pub fn progress_ratio(&self) -> f64 {
        if self.steps.is_empty() {
            return if self.status == TaskStatus::Completed { 1.0 } else { 0.0 };
        }
        let completed = self.steps.iter().filter(|s| s.completed).count();
        completed as f64 / self.steps.len() as f64
    }
}
