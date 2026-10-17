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
    /// Liaison Biscuit F4 : SHA256 des octets sérialisés du token de
    /// capability présenté à l'exécution (anti-substitution : prouve QUEL
    /// token exact a autorisé l'action). `None` = action sans token ou
    /// entrée legacy. `#[serde(default)]` : les JSON antérieurs sans ce
    /// champ se relisent (compatibilité ascendante).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_hash: Option<[u8; 32]>,
}

/// SHA256 des octets d'un token (à stocker dans `token_hash`).
/// `sha2` est déjà une dépendance de `annora-audit`.
pub fn sha256_token_hash(token_bytes: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(token_bytes);
    h.finalize().into()
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
