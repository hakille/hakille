use crate::graph::KnowledgeGraph;
use crate::triplet::Triplet;
use annora_core::Timestamp;
use serde::{Deserialize, Serialize};

pub enum ExportFormat {
    Json,
    Cbor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportMetadata {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphExport {
    pub version: String,
    pub exported_at: Timestamp,
    pub triplets: Vec<Triplet>,
    pub metadata: ExportMetadata,
}

pub fn export_graph(_graph: &KnowledgeGraph, _format: ExportFormat) -> Vec<u8> {
    vec![]
}
