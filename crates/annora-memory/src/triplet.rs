use annora_core::{NodeId, PredicateId, Timestamp, TripletId, TrustLevel};
use serde::{Deserialize, Serialize};
use crate::context_partition::ContextPartition;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeValue {
    Entity(NodeId),
    Text(String),
    Number(f64),
    Boolean(bool),
    Timestamp(Timestamp),
    Embedding(Vec<f32>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TripletSource {
    User,
    System,
    Inferred,
    External(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TripletMetadata {
    pub weight: f64,
    pub created_at: Timestamp,
    pub last_activated: Timestamp,
    pub source: TripletSource,
    pub confidence: f64,
    pub context: ContextPartition,
    pub trust_level: TrustLevel,
    pub decay_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Triplet {
    pub id: TripletId,
    pub subject: NodeId,
    pub predicate: PredicateId,
    pub object: NodeValue,
    pub metadata: TripletMetadata,
}

#[derive(Debug, Clone)]
pub struct TriplePattern {
    pub subject: Option<NodeId>,
    pub predicate: Option<PredicateId>,
    pub object: Option<NodeValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    Fact,
    Entity,
    Objective,
    Preference,
    Skill,
}

#[derive(Debug, Clone)]
pub enum InsertResult {
    Inserted(TripletId),
    ContradictionResolved { old: TripletId, new: TripletId },
    NeedsUserResolution { existing: Triplet, incoming: Triplet },
}
