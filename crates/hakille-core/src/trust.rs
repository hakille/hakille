use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfidenceLevel {
    pub model_confidence: f64,
    pub graph_corroboration: f64,
    pub combined: f64,
}

impl ConfidenceLevel {
    /// Formule COGNITION.md : 0.6·modèle + 0.4·corroboration graphe.
    /// Les entrées sont clampées dans [0,1] pour éviter les scores hors bornes.
    pub fn compute(model: f64, graph: f64) -> Self {
        let model = model.clamp(0.0, 1.0);
        let graph = graph.clamp(0.0, 1.0);
        Self {
            model_confidence: model,
            graph_corroboration: graph,
            combined: model * 0.6 + graph * 0.4,
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
    // Contradiction interne : modèle et graphe tous deux > 0.5 mais en fort
    // désaccord (écart > 0.4). Ni l'un ni l'autre ne peut être suivi aveuglément.
    let gap = (confidence.model_confidence - confidence.graph_corroboration).abs();
    if gap > 0.4 && confidence.model_confidence > 0.5 && confidence.graph_corroboration > 0.5 {
        return ConfidenceDecision::InternalContradiction;
    }
    let threshold = thresholds.0.get(domain).unwrap_or(&0.7);
    if confidence.combined >= *threshold {
        ConfidenceDecision::AutoExecute
    } else if confidence.combined >= threshold * 0.8 {
        ConfidenceDecision::ProposeAndWait
    } else {
        ConfidenceDecision::Refuse
    }
}
