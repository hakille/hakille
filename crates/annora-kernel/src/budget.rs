use annora_core::{AnnoraError, Result};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineBudgetConfig {
    pub max_perception_ms: u64,
    pub max_routing_ms: u64,
    pub max_inference_ms: u64,
    pub max_action_ms: u64,
    pub max_total_ms: u64,
}

impl Default for PipelineBudgetConfig {
    fn default() -> Self {
        Self {
            max_perception_ms: 60,
            max_routing_ms: 20,
            max_inference_ms: 100,
            max_action_ms: 40,
            max_total_ms: 220,
        }
    }
}

pub struct LatencyTracker {
    config: PipelineBudgetConfig,
    start_time: Instant,
    stage_durations: Vec<(&'static str, Duration)>,
}

impl LatencyTracker {
    pub fn start(config: PipelineBudgetConfig) -> Self {
        Self {
            config,
            start_time: Instant::now(),
            stage_durations: Vec::new(),
        }
    }

    pub fn record_stage(&mut self, stage: &'static str, duration: Duration) {
        self.stage_durations.push((stage, duration));
    }

    /// Budget per-stage (heuristique honnête, pas de mesure kernel réelle) :
    /// chaque nom d'étape du pipeline est comparé à son `max_*` dédié,
    /// pas seulement au total. Mapping documenté :
    /// - perception (`memory_extract`, `recall`, `context_record`) → `max_perception_ms`
    /// - `routing` → `max_routing_ms`
    /// - `inference` (S1/S2) → `max_inference_ms`
    /// - action / enregistrement final (`response_record`, `distill`,
    ///   `reflection`, `simulation`, `contradiction`, `priority`) → `max_action_ms`
    /// - inconnu → `max_action_ms` par prudence (pas de passe-droit silencieux).
    pub fn stage_budget_ms(&self, stage: &str) -> u64 {
        match stage {
            "memory_extract" | "recall" | "context_record" => self.config.max_perception_ms,
            "routing" => self.config.max_routing_ms,
            "inference" => self.config.max_inference_ms,
            "response_record" | "distill" | "reflection" | "simulation" | "contradiction"
            | "priority" | "action" => self.config.max_action_ms,
            _ => self.config.max_action_ms,
        }
    }

    /// Vérifie une étape isolée contre son budget dédié.
    /// Retourne `Err` si `duration > budget(stage)` — l'appelant doit aborter
    /// ou dégrader au lieu de continuer silencieusement.
    pub fn check_stage(&self, stage: &str, duration: Duration) -> Result<()> {
        let budget = self.stage_budget_ms(stage);
        if duration > Duration::from_millis(budget) {
            return Err(AnnoraError::Internal(format!(
                "Stage budget exceeded for '{}' (elapsed {} ms > max {} ms)",
                stage,
                duration.as_millis(),
                budget,
            )));
        }
        Ok(())
    }

    /// Étapes enregistrées (nom, durée) — exposé pour les tests de budget.
    pub fn stages(&self) -> &[(&'static str, Duration)] {
        &self.stage_durations
    }

    pub fn config(&self) -> &PipelineBudgetConfig {
        &self.config
    }

    pub fn total_elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Comparaison à la nanoseconde (et non `as_millis` tronqué) : un budget
    /// `max_total_ms = 0` aborte dès la première étape (utile pour les tests
    /// de dépassement déterministes), et un pipeline réel n'est plus déclaré
    /// « dans le budget » à 220.9 ms.
    pub fn is_within_budget(&self) -> bool {
        self.total_elapsed() <= Duration::from_millis(self.config.max_total_ms)
    }

    pub fn remaining_budget_ms(&self) -> i64 {
        let elapsed = self.total_elapsed().as_millis() as i64;
        self.config.max_total_ms as i64 - elapsed
    }
}
