//! Moteur Dialectique Bicaméral (§34) : Débat contradictoire parallèle
//! entre l'Avocat (Catalyseur) et le Critique (Sceptique / Avocat du Diable)
//! avec arbitrage bayésien et dosage strict de carburant (fuel metering).
//!
//! Principes souverains de Hakille :
//! - 100% Rust natif, zéro dépendance externe d'IA cloud.
//! - Débat déterministe avec budget de carburant fini (anti-hang, fail-closed).
//! - Invariant de souveraineté : tout risque de fuite cloud ou d'action irréversible
//!   déclenche un veto immédiat ou exige une validation utilisateur 600ms.

use serde::{Deserialize, Serialize};

/// Verdict issu de l'arbitrage dialectique bayésien.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DialecticVerdict {
    /// Action approuvée automatiquement (confiance bayésienne élevée, aucun veto critique).
    Approved,
    /// Action nécessitant la présentation d'une ActionCard avec maintien 600ms à l'utilisateur.
    RequiresUserHold,
    /// Action formellement rejetée (fail-closed, risque de fuite ou dommage non mitigé).
    Vetoed,
}

/// Point d'argumentation avancé par le Catalyseur.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalystArgument {
    pub claim: String,
    pub weight: f32,     // [0.0, 1.0]
    pub confidence: f32, // [0.0, 1.0]
    pub benefit_category: String,
}

/// Risque ou contre-argument soulevé par le Sceptique (Avocat du Diable).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkepticRisk {
    pub vulnerability: String,
    pub severity: f32,   // [0.0, 1.0]
    pub confidence: f32, // [0.0, 1.0]
    pub is_critical_cloud_leak: bool,
    pub is_irreversible: bool,
}

/// Tour de joute dialectique entre les deux agents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DialecticTurn {
    pub turn_index: usize,
    pub catalyst_thesis: String,
    pub skeptic_antithesis: String,
    pub synthesis_delta: Option<String>,
    pub fuel_consumed: u64,
}

/// Synthèse finale consolidée de l'arbitrage dialectique.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BicameralSynthesis {
    pub verdict: DialecticVerdict,
    pub prior_confidence: f32,
    pub posterior_confidence: f32,
    pub consensus_action: String,
    pub catalyst_score: f32,
    pub skeptic_score: f32,
    pub turns: Vec<DialecticTurn>,
    pub total_fuel_consumed: u64,
    pub veto_reason: Option<String>,
}

/// Configuration du Moteur Dialectique Bicaméral.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BicameralConfig {
    pub max_turns: usize,
    pub fuel_budget_per_turn: u64,
    pub auto_approve_threshold: f32,
    pub user_hold_threshold: f32,
}

impl Default for BicameralConfig {
    fn default() -> Self {
        Self {
            max_turns: 3,
            fuel_budget_per_turn: 25_000,
            auto_approve_threshold: 0.85,
            user_hold_threshold: 0.50,
        }
    }
}

/// Moteur dialectique bicaméral supervisant les décisions autonomes.
#[derive(Debug, Clone)]
pub struct BicameralEngine {
    config: BicameralConfig,
}

impl BicameralEngine {
    pub fn new(config: BicameralConfig) -> Self {
        Self { config }
    }

    pub fn with_default_config() -> Self {
        Self::new(BicameralConfig::default())
    }

    /// Exécute un arbitrage dialectique bayésien déterministe en Rust natif sur une
    /// proposition d'action (sans simulation d'exécution WASM).
    ///
    /// - Validation fail-closed : toute valeur `NaN` ou non finie dans `prior_soundness`,
    ///   `arguments` ou `risks` déclenche un `DialecticVerdict::Vetoed` immédiat.
    /// - Tout risque `is_critical_cloud_leak` déclenche un veto souverain immédiat.
    /// - `fuel_consumed` reflète le coût déterministe des propositions et risques
    ///   réellement évalués à chaque palier (borné par `fuel_budget_per_turn`).
    pub fn arbitrate(
        &self,
        action_intent: &str,
        prior_soundness: f32,
        arguments: &[CatalystArgument],
        risks: &[SkepticRisk],
    ) -> BicameralSynthesis {
        let has_non_finite = !prior_soundness.is_finite()
            || arguments
                .iter()
                .any(|a| !a.weight.is_finite() || !a.confidence.is_finite())
            || risks
                .iter()
                .any(|r| !r.severity.is_finite() || !r.confidence.is_finite());
        if has_non_finite {
            return BicameralSynthesis {
                verdict: DialecticVerdict::Vetoed,
                prior_confidence: 0.0,
                posterior_confidence: 0.0,
                consensus_action: action_intent.to_string(),
                catalyst_score: 0.0,
                skeptic_score: 1.0,
                turns: Vec::new(),
                total_fuel_consumed: 0,
                veto_reason: Some(
                    "Violation fail-closed : paramètre non fini (NaN/Inf) détecté".into(),
                ),
            };
        }

        let p0 = prior_soundness.clamp(0.05, 0.95);
        let mut turns = Vec::new();
        let mut total_fuel = 0u64;

        // 1. Analyse fail-closed des risques rédhibitoires (Règle d'or de Souveraineté)
        for (idx, risk) in risks.iter().enumerate() {
            if risk.is_critical_cloud_leak {
                let eval_cost = ((idx + 1) as u64).saturating_mul(64);
                return BicameralSynthesis {
                    verdict: DialecticVerdict::Vetoed,
                    prior_confidence: p0,
                    posterior_confidence: 0.0,
                    consensus_action: action_intent.to_string(),
                    catalyst_score: 0.0,
                    skeptic_score: 1.0,
                    turns: vec![DialecticTurn {
                        turn_index: 1,
                        catalyst_thesis: format!("Proposition initiale : {action_intent}"),
                        skeptic_antithesis: format!(
                            "VETO IMMÉDIAT : Risque de fuite cloud détecté ({})",
                            risk.vulnerability
                        ),
                        synthesis_delta: Some("Rejet souverain strict sans négociation".into()),
                        fuel_consumed: eval_cost,
                    }],
                    total_fuel_consumed: eval_cost,
                    veto_reason: Some(format!("Violation fail-closed : {}", risk.vulnerability)),
                };
            }
        }

        // 2. Évaluation déterministe des paliers dialectiques sur les arguments/risques réels
        let num_turns = self.config.max_turns.max(1);
        let fuel_per_turn = (self.config.fuel_budget_per_turn / (num_turns as u64)).max(1);

        for i in 1..=num_turns {
            let arg_summary = arguments
                .get(i - 1)
                .or_else(|| arguments.last())
                .map(|a| {
                    format!(
                        "[{}] {} (w={:.2}, c={:.2})",
                        a.benefit_category, a.claim, a.weight, a.confidence
                    )
                })
                .unwrap_or_else(|| "aucun argument catalyseur".to_string());
            let risk_summary = risks
                .get(i - 1)
                .or_else(|| risks.last())
                .map(|r| {
                    format!(
                        "{} (s={:.2}, c={:.2}, irrév={})",
                        r.vulnerability, r.severity, r.confidence, r.is_irreversible
                    )
                })
                .unwrap_or_else(|| "aucun risque sceptique".to_string());

            // Coût d'évaluation déterministe proportionnel aux éléments inspectés
            let turn_fuel = ((arguments.len() + risks.len()).max(1) as u64)
                .saturating_mul(64)
                .min(fuel_per_turn);
            total_fuel = total_fuel.saturating_add(turn_fuel);

            turns.push(DialecticTurn {
                turn_index: i,
                catalyst_thesis: format!("Palier {i} ({action_intent}) : {arg_summary}"),
                skeptic_antithesis: format!("Palier {i} : {risk_summary}"),
                synthesis_delta: Some(format!(
                    "Évaluation bayésienne palier {i}/{num_turns} ({} argument(s), {} risque(s))",
                    arguments.len(),
                    risks.len()
                )),
                fuel_consumed: turn_fuel,
            });
        }

        // 3. Calcul bayésien de vraisemblance (Likelihood Ratios)
        let c_sum: f32 = arguments
            .iter()
            .map(|a| a.weight.clamp(0.0, 1.0) * a.confidence.clamp(0.0, 1.0))
            .sum();
        let l_c = 1.0 + c_sum;

        let s_sum: f32 = risks
            .iter()
            .map(|r| {
                r.severity.clamp(0.0, 1.0)
                    * r.confidence.clamp(0.0, 1.0)
                    * (if r.is_irreversible { 2.0 } else { 1.0 })
            })
            .sum();
        let l_s = 1.0 + s_sum;

        // Formule de Bayes : P(Safe | Debat) = (P0 * L_C) / (P0 * L_C + (1 - P0) * L_S)
        let numerator = p0 * l_c;
        let denominator = numerator + (1.0 - p0) * l_s;
        let posterior = if denominator > 0.0001 {
            (numerator / denominator).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // 4. Détermination du verdict
        let (verdict, veto_reason) = if posterior >= self.config.auto_approve_threshold
            && s_sum < 0.8
        {
            (DialecticVerdict::Approved, None)
        } else if posterior >= self.config.user_hold_threshold {
            (
                DialecticVerdict::RequiresUserHold,
                Some("Incertitude ou irréversibilité partielle : validation utilisateur 600ms requise".into()),
            )
        } else {
            (
                DialecticVerdict::Vetoed,
                Some(format!(
                    "Rejet dialectique : score sceptique trop élevé ({:.2})",
                    s_sum
                )),
            )
        };

        BicameralSynthesis {
            verdict,
            prior_confidence: p0,
            posterior_confidence: posterior,
            consensus_action: action_intent.to_string(),
            catalyst_score: c_sum,
            skeptic_score: s_sum,
            turns,
            total_fuel_consumed: total_fuel,
            veto_reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cloud_leak_triggers_immediate_fail_closed_veto() {
        let engine = BicameralEngine::with_default_config();
        let risks = vec![SkepticRisk {
            vulnerability: "Export de contacts vers serveur tiers non autorisé".into(),
            severity: 0.9,
            confidence: 0.95,
            is_critical_cloud_leak: true,
            is_irreversible: true,
        }];
        let args = vec![CatalystArgument {
            claim: "Sauvegarde rapide de commodité".into(),
            weight: 0.9,
            confidence: 0.9,
            benefit_category: "Convenience".into(),
        }];

        let synthesis = engine.arbitrate("backup_contacts", 0.8, &args, &risks);
        assert_eq!(synthesis.verdict, DialecticVerdict::Vetoed);
        assert_eq!(synthesis.posterior_confidence, 0.0);
        assert!(
            synthesis
                .veto_reason
                .unwrap()
                .contains("Violation fail-closed")
        );
    }

    #[test]
    fn test_safe_local_action_auto_approved() {
        let engine = BicameralEngine::with_default_config();
        let risks = vec![SkepticRisk {
            vulnerability: "Faible consommation CPU locale (0.2%)".into(),
            severity: 0.05,
            confidence: 0.2,
            is_critical_cloud_leak: false,
            is_irreversible: false,
        }];
        let args = vec![
            CatalystArgument {
                claim: "Indexation locale en mémoire RAM".into(),
                weight: 0.95,
                confidence: 0.95,
                benefit_category: "Search".into(),
            },
            CatalystArgument {
                claim: "Gain de temps utilisateur immédiat".into(),
                weight: 0.85,
                confidence: 0.9,
                benefit_category: "Productivity".into(),
            },
        ];

        let synthesis = engine.arbitrate("index_notes", 0.85, &args, &risks);
        assert_eq!(synthesis.verdict, DialecticVerdict::Approved);
        assert!(synthesis.posterior_confidence >= 0.85);
        assert!(synthesis.veto_reason.is_none());
        assert_eq!(synthesis.turns.len(), 3);
        assert!(synthesis.total_fuel_consumed > 0);
    }

    #[test]
    fn test_irreversible_action_requires_user_hold() {
        let engine = BicameralEngine::with_default_config();
        let risks = vec![SkepticRisk {
            vulnerability: "Suppression locale définitive de fichiers temporaires".into(),
            severity: 0.65,
            confidence: 0.8,
            is_critical_cloud_leak: false,
            is_irreversible: true,
        }];
        let args = vec![CatalystArgument {
            claim: "Libération de 2 Go d'espace disque".into(),
            weight: 0.8,
            confidence: 0.85,
            benefit_category: "Storage".into(),
        }];

        let synthesis = engine.arbitrate("purge_temp_cache", 0.6, &args, &risks);
        assert_eq!(synthesis.verdict, DialecticVerdict::RequiresUserHold);
        assert!(synthesis.posterior_confidence >= 0.50 && synthesis.posterior_confidence < 0.85);
        assert!(synthesis.veto_reason.unwrap().contains("600ms"));
    }
}
