use annora_core::{ActorId, CapabilityScope, Cost, EntryId, ModelReference, Timestamp, TripletId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: EntryId,
    pub timestamp: Timestamp,
    pub actor: ActorId,
    pub action: ActionRecord,
    pub capability_used: Option<CapabilityScope>,
    pub reasoning_trace: ReasoningTrace,
    pub graph_nodes_used: Vec<TripletId>,
    pub confidence: f64,
    pub cost: Option<Cost>,
    pub signature: Vec<u8>,
    pub prev_hash: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRecord {
    pub action_type: String,
    pub description: String,
    pub parameters: serde_json::Value,
    pub reversible: bool,
    pub reversed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningTrace {
    pub steps: Vec<ReasoningStep>,
    pub model_used: Option<ModelReference>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    pub description: String,
    pub confidence: f64,
    pub evidence: Vec<TripletId>,
}
