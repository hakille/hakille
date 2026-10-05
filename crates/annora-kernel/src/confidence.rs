use annora_core::{ConfidenceDecision, ConfidenceLevel, Domain, DomainThresholds};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedAction {
    pub action_name: String,
    pub domain: Domain,
    pub model_confidence: f64,
    pub parameters_summary: String,
}

pub struct ConfidenceEngine {
    thresholds: DomainThresholds,
    auto_act_margin: f64,
}

impl ConfidenceEngine {
    pub fn new(thresholds: DomainThresholds, auto_act_margin: f64) -> Self {
        Self {
            thresholds,
            auto_act_margin,
        }
    }

    pub fn with_default_thresholds() -> Self {
        Self {
            thresholds: DomainThresholds::default(),
            auto_act_margin: 0.05,
        }
    }

    pub fn evaluate(
        &self,
        action: &ProposedAction,
        graph_corroboration: f64,
    ) -> (ConfidenceDecision, ConfidenceLevel) {
        let level = ConfidenceLevel::compute(action.model_confidence, graph_corroboration);
        let base_threshold = *self.thresholds.0.get(&action.domain).unwrap_or(&0.7);

        let decision = if level.combined >= base_threshold + self.auto_act_margin {
            ConfidenceDecision::AutoExecute
        } else if level.combined >= base_threshold * 0.75 {
            // Borderline zone: triggers internal self-critique/contradiction check
            ConfidenceDecision::InternalContradiction
        } else if level.combined >= base_threshold * 0.5 {
            ConfidenceDecision::ProposeAndWait
        } else {
            ConfidenceDecision::Refuse
        };

        (decision, level)
    }
}
