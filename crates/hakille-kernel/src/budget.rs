use hakille_core::{HakilleError, Result};
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

impl PipelineBudgetConfig {
    /// Config dimensionnée pour un décodage LLM RÉEL (System 1/2, Candle CPU) :
    /// seuls les budgets `inference` et `total` sont dérivés du timeout LLM
    /// effectif (`HAKILLE_QWEN_TIMEOUT_MS`, défaut 600 s — voir
    /// `System1Engine::llm_timeout_ms`) ; les autres stages gardent leurs
    /// budgets par défaut (< 100 ms, pipeline interactif).
    ///
    /// Marge : +30 s sur `inference` (le premier tour inclut le chargement
    /// lazy du GGUF, ~10-60 s, dans le même `spawn_blocking`) et +30 s sur le
    /// total. Le dépassement redevient possible au-delà, et reste alors une
    /// erreur honnête — jamais de succès feint.
    ///
    /// Encadré (contrat) : le fast-path System 1 (100 ms) reste l'invariant
    /// produit documenté (`docs/COGNITION.md`) ; cette config est la
    /// configuration de PRODUCTION pour un vrai décodeur local, alignée sur le
    /// précédent des tests lents (`crates/hakille-kernel/src/lib.rs:90-98`).
    pub fn aligned_with_llm_timeout(llm_timeout_ms: u64) -> Self {
        let inference = llm_timeout_ms.saturating_add(30_000);
        Self {
            max_inference_ms: inference,
            max_total_ms: inference.saturating_add(30_000),
            ..Self::default()
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
            return Err(HakilleError::Internal(format!(
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Verrou de non-régression : les budgets du fast-path interactif
    /// (spec `docs/COGNITION.md`) ne changent pas.
    #[test]
    fn test_default_budgets_unchanged() {
        let c = PipelineBudgetConfig::default();
        assert_eq!(c.max_perception_ms, 60, "perception par défaut = 60 ms");
        assert_eq!(c.max_routing_ms, 20, "routing par défaut = 20 ms");
        assert_eq!(c.max_inference_ms, 100, "inférence par défaut = 100 ms");
        assert_eq!(c.max_action_ms, 40, "action par défaut = 40 ms");
        assert_eq!(c.max_total_ms, 220, "total par défaut = 220 ms");
    }

    #[test]
    fn test_aligned_with_llm_timeout_covers_real_decode() {
        let c = PipelineBudgetConfig::aligned_with_llm_timeout(600_000);
        assert_eq!(
            c.max_inference_ms, 630_000,
            "inférence = timeout LLM + marge 30 s"
        );
        assert_eq!(c.max_total_ms, 660_000, "total = inférence + marge 30 s");
        assert_eq!(c.max_perception_ms, 60, "perception inchangée");
        assert_eq!(c.max_routing_ms, 20, "routing inchangé");
        assert_eq!(c.max_action_ms, 40, "action inchangée");
    }

    #[test]
    fn test_aligned_config_accepts_measured_real_turn() {
        // Mesure réelle constatée en release sur i7-1165G7 : 27 428 ms pour
        // 64 tokens Qwen 0.5B Q4_K_M CPU. La config produit doit l'accepter.
        let tracker =
            LatencyTracker::start(PipelineBudgetConfig::aligned_with_llm_timeout(600_000));
        let real_turn = Duration::from_millis(27_428);
        assert!(
            tracker.check_stage("inference", real_turn).is_ok(),
            "un tour Qwen réel mesuré (27.4 s) doit passer la config alignée"
        );
        let mut tracker = tracker;
        tracker.record_stage("inference", real_turn);
        assert!(
            tracker.is_within_budget(),
            "le total d'un tour réel doit rester dans le budget aligné"
        );
    }

    #[test]
    fn test_aligned_config_still_refuses_beyond_timeout() {
        let tracker =
            LatencyTracker::start(PipelineBudgetConfig::aligned_with_llm_timeout(600_000));
        assert!(
            tracker
                .check_stage("inference", Duration::from_millis(631_000))
                .is_err(),
            "au-delà de timeout + marge, le dépassement reste une erreur honnête"
        );
    }

    #[test]
    fn test_aligned_with_llm_timeout_saturates() {
        let c = PipelineBudgetConfig::aligned_with_llm_timeout(u64::MAX);
        assert_eq!(c.max_inference_ms, u64::MAX, "saturation sans panic");
        assert_eq!(c.max_total_ms, u64::MAX, "saturation sans panic");
    }

    /// Le contrat des tests de refus (budget nul ⇒ abort dès la première
    /// étape) reste intact avec la nouvelle fabrique.
    #[test]
    fn test_aligned_config_derives_from_defaults_not_zero() {
        let c = PipelineBudgetConfig::aligned_with_llm_timeout(0);
        assert_eq!(c.max_inference_ms, 30_000, "timeout 0 ⇒ 0 + marge 30 s");
        assert_eq!(c.max_total_ms, 60_000, "total = inférence + marge 30 s");
        assert!(c.max_routing_ms > 0, "routing reste strict (20 ms)");
    }
}
