use annora_core::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Objective {
    pub id: String,
    pub title: String,
    pub description: String,
    pub milestones: Vec<Milestone>,
    pub created_at: Timestamp,
    pub target_date: Option<Timestamp>,
    pub status: ObjectiveStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    pub id: String,
    pub description: String,
    pub completed: bool,
    pub completed_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectiveStatus {
    Active,
    Paused,
    Completed,
    Abandoned,
}
