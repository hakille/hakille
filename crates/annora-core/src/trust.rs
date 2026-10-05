use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfidenceLevel {
    pub model_confidence: f64,
    pub graph_corroboration: f64,
    pub combined: f64,
}

impl ConfidenceLevel {
    pub fn compute(model: f64, graph: f64) -> Self {
        Self {
            model_confidence: model,
            graph_corroboration: graph,
            combined: model * 0.4 + graph * 0.6,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Domain {
    General,
    Legal,
    Financial,
    Medical,
    Technical,
    Creative,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainThresholds(pub HashMap<Domain, f64>);

impl Default for DomainThresholds {
    fn default() -> Self {
        let mut map = HashMap::new();
        map.insert(Domain::General, 0.7);
        map.insert(Domain::Legal, 0.9);
        map.insert(Domain::Financial, 0.9);
        map.insert(Domain::Medical, 0.95);
        map.insert(Domain::Technical, 0.85);
        map.insert(Domain::Creative, 0.6);
        Self(map)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConfidenceDecision {
    AutoExecute,
    InternalContradiction,
    ProposeAndWait,
    Refuse,
}

pub fn evaluate_confidence(
    confidence: &ConfidenceLevel,
    domain: &Domain,
    thresholds: &DomainThresholds,
) -> ConfidenceDecision {
    let threshold = thresholds.0.get(domain).unwrap_or(&0.7);
    if confidence.combined >= *threshold {
        ConfidenceDecision::AutoExecute
    } else if confidence.combined >= threshold * 0.8 {
        ConfidenceDecision::ProposeAndWait
    } else {
        ConfidenceDecision::Refuse
    }
}
