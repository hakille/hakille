use annora_core::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// An intermediate hypothesis or thought in working memory (§28a).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingHypothesis {
    pub id: String,
    pub premise: String,
    pub confidence: f64,
    pub verified: bool,
}

/// A transient tool result or observation during cognitive reasoning (§28a).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntermediateObservation {
    pub step_index: u32,
    pub action_type: String,
    pub output_summary: String,
    pub timestamp: Timestamp,
}

/// Distillation candidate: fact worthy of persistence in the long-term Knowledge Graph (§28a).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistilledFact {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub confidence: f64,
}

/// Working Memory buffer holding short-lived transient state (§28a).
/// Distinct from long-term memory graph; prevents pollution of the knowledge graph
/// with temporary execution noise.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingMemory {
    pub task_id: String,
    pub scratchpad_notes: Vec<String>,
    pub hypotheses: Vec<WorkingHypothesis>,
    pub observations: Vec<IntermediateObservation>,
    pub variables: HashMap<String, String>,
    pub distillation_queue: Vec<DistilledFact>,
    pub created_at: Timestamp,
    pub last_updated: Timestamp,
}

impl WorkingMemory {
    pub fn new(task_id: impl Into<String>) -> Self {
        let now = Timestamp::now();
        Self {
            task_id: task_id.into(),
            scratchpad_notes: Vec::new(),
            hypotheses: Vec::new(),
            observations: Vec::new(),
            variables: HashMap::new(),
            distillation_queue: Vec::new(),
            created_at: now.clone(),
            last_updated: now,
        }
    }

    pub fn add_note(&mut self, note: impl Into<String>) {
        self.scratchpad_notes.push(note.into());
        self.last_updated = Timestamp::now();
    }

    pub fn record_observation(&mut self, step_index: u32, action: impl Into<String>, summary: impl Into<String>) {
        self.observations.push(IntermediateObservation {
            step_index,
            action_type: action.into(),
            output_summary: summary.into(),
            timestamp: Timestamp::now(),
        });
        self.last_updated = Timestamp::now();
    }

    pub fn set_variable(&mut self, key: impl Into<String>, val: impl Into<String>) {
        self.variables.insert(key.into(), val.into());
        self.last_updated = Timestamp::now();
    }

    pub fn get_variable(&self, key: &str) -> Option<&String> {
        self.variables.get(key)
    }

    pub fn add_hypothesis(&mut self, id: impl Into<String>, premise: impl Into<String>, confidence: f64) {
        self.hypotheses.push(WorkingHypothesis {
            id: id.into(),
            premise: premise.into(),
            confidence,
            verified: false,
        });
        self.last_updated = Timestamp::now();
    }

    pub fn queue_for_distillation(&mut self, subject: impl Into<String>, predicate: impl Into<String>, object: impl Into<String>, confidence: f64) {
        self.distillation_queue.push(DistilledFact {
            subject: subject.into(),
            predicate: predicate.into(),
            object: object.into(),
            confidence,
        });
        self.last_updated = Timestamp::now();
    }

    /// Complete task and drain facts worthy of permanent sovereign graph storage.
    /// Wipes all ephemeral scratchpad and transient tool outputs (§28a).
    pub fn distill_and_clear(&mut self) -> Vec<DistilledFact> {
        let facts = std::mem::take(&mut self.distillation_queue);
        self.scratchpad_notes.clear();
        self.hypotheses.clear();
        self.observations.clear();
        self.variables.clear();
        facts
    }
}
