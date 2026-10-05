use crate::graph::KnowledgeGraph;

pub struct DecayEngine {
    pub rate: f64,
}

pub struct DecayReport {
    pub nodes_decayed: u64,
    pub min_weight: f64,
    pub avg_weight: f64,
}

impl DecayEngine {
    pub fn apply_decay(_graph: &mut KnowledgeGraph, _cutoff_weight: f64) -> DecayReport {
        DecayReport {
            nodes_decayed: 0,
            min_weight: 0.0,
            avg_weight: 0.0,
        }
    }
}
