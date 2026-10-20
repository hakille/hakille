use hakille_core::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CognitiveState {
    Standby,
    Perceiving,
    Reasoning,
    Simulating,
    Executing,
    Hesitating { reason_code: u32 },
}

/// Real-time Introspective Self-Model (§28c).
/// Maintains a live queryable state of the cognitive agent's internal reasoning,
/// active goals, confidence trajectory, and self-diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfModel {
    pub current_state: CognitiveState,
    pub current_task_summary: Option<String>,
    pub active_goal_id: Option<String>,
    pub recent_error_rate: f64,
    pub rolling_confidence: f64,
    pub latency_envelope_ms: u64,
    pub hesitation_rationale: Option<String>,
    pub last_state_change: Timestamp,
}

impl Default for SelfModel {
    fn default() -> Self {
        Self::new()
    }
}

impl SelfModel {
    pub fn new() -> Self {
        Self {
            current_state: CognitiveState::Standby,
            current_task_summary: None,
            active_goal_id: None,
            recent_error_rate: 0.0,
            rolling_confidence: 0.95,
            latency_envelope_ms: 45,
            hesitation_rationale: None,
            last_state_change: Timestamp::now(),
        }
    }

    pub fn set_state(&mut self, state: CognitiveState) {
        self.current_state = state;
        self.last_state_change = Timestamp::now();
        if !matches!(state, CognitiveState::Hesitating { .. }) {
            self.hesitation_rationale = None;
        }
    }

    pub fn enter_hesitation(&mut self, reason_code: u32, explanation: impl Into<String>) {
        self.current_state = CognitiveState::Hesitating { reason_code };
        self.hesitation_rationale = Some(explanation.into());
        self.last_state_change = Timestamp::now();
    }

    pub fn update_confidence(&mut self, new_score: f64) {
        // Exponential moving average for rolling confidence
        self.rolling_confidence = (self.rolling_confidence * 0.8) + (new_score * 0.2);
    }

    pub fn record_action_outcome(&mut self, success: bool) {
        let err_sample = if success { 0.0 } else { 1.0 };
        self.recent_error_rate = (self.recent_error_rate * 0.85) + (err_sample * 0.15);
    }

    /// Immediate introspective query: "Qu'est-ce que tu es en train de faire ?" (§28c)
    pub fn query_current_activity(&self) -> String {
        match (&self.current_state, &self.current_task_summary) {
            (CognitiveState::Standby, _) => {
                "En veille attentive, en attente de sollicitation ou d'événement ambiant."
                    .to_string()
            }
            (CognitiveState::Perceiving, _) => {
                "Analyse continue des signaux audio et visuels de l'environnement.".to_string()
            }
            (CognitiveState::Reasoning, Some(t)) => {
                format!("Raisonnement actif en cours sur la tâche : '{}'", t)
            }
            (CognitiveState::Reasoning, None) => {
                "Raisonnement interne sur les données de contexte.".to_string()
            }
            (CognitiveState::Simulating, Some(t)) => format!(
                "Simulation locale à blanc avant engagement irréversible : '{}'",
                t
            ),
            (CognitiveState::Executing, Some(t)) => {
                format!("Exécution d'une action dans le bac à sable : '{}'", t)
            }
            (CognitiveState::Hesitating { reason_code }, _) => {
                let why = self
                    .hesitation_rationale
                    .as_deref()
                    .unwrap_or("Incertitude sur les conséquences");
                format!("En hésitation (code {}) : {}", reason_code, why)
            }
            _ => "Traitement cognitif en cours.".to_string(),
        }
    }

    /// Immediate introspective query: "Pourquoi tu hésites là ?" (§28c)
    pub fn query_hesitation_reason(&self) -> Option<String> {
        self.hesitation_rationale.clone()
    }
}
