use annora_core::{Timestamp, UserId};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEntry {
    pub timestamp: Timestamp,
    pub speaker: String,
    pub content: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveGoal {
    pub id: String,
    pub description: String,
    pub created_at: Timestamp,
    pub progress: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionContext {
    pub user_id: UserId,
    pub conversation_stream: VecDeque<ContextEntry>,
    pub max_stream_entries: usize,
    pub active_goals: Vec<ActiveGoal>,
    pub current_focus: Option<String>,
    pub last_interaction: Timestamp,
}

impl InteractionContext {
    pub fn new(user_id: UserId, max_stream_entries: usize) -> Self {
        Self {
            user_id,
            conversation_stream: VecDeque::with_capacity(max_stream_entries),
            max_stream_entries,
            active_goals: Vec::new(),
            current_focus: None,
            last_interaction: Timestamp::now(),
        }
    }

    pub fn record_interaction(&mut self, speaker: &str, content: &str, tags: Vec<String>) {
        if self.conversation_stream.len() >= self.max_stream_entries {
            self.conversation_stream.pop_front();
        }
        self.conversation_stream.push_back(ContextEntry {
            timestamp: Timestamp::now(),
            speaker: speaker.to_string(),
            content: content.to_string(),
            tags,
        });
        self.last_interaction = Timestamp::now();
    }

    pub fn add_goal(&mut self, id: impl Into<String>, description: impl Into<String>) {
        self.active_goals.push(ActiveGoal {
            id: id.into(),
            description: description.into(),
            created_at: Timestamp::now(),
            progress: 0.0,
        });
    }

    pub fn set_focus(&mut self, focus: impl Into<String>) {
        self.current_focus = Some(focus.into());
    }

    pub fn recent_exchanges(&self, count: usize) -> Vec<&ContextEntry> {
        self.conversation_stream.iter().rev().take(count).collect()
    }
}
