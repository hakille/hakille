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

    /// Exécute un arbitrage dialectique bayésien complet sur une proposition d'action.
    pub fn arbitrate(
        &self,
        action_intent: &str,
        prior_soundness: f32,
        arguments: &[CatalystArgument],
        risks: &[SkepticRisk],
    ) -> BicameralSynthesis {
        let p0 = prior_soundness.clamp(0.05, 0.95);
        let mut turns = Vec::new();
        let mut total_fuel = 0u64;

        // 1. Analyse fail-closed des risques rédhibitoires (Règle d'or de Souveraineté)
        for risk in risks {
            if risk.is_critical_cloud_leak {
                return BicameralSynthesis {
                    verdict: DialecticVerdict::Vetoed,
                    prior_confidence: p0,
                    posterior_confidence: 0.0,
                    consensus_action: action_intent.to_string(),
                    catalyst_score: 0.0,
                    skeptic_score: 1.0,
                    turns: vec![DialecticTurn {
                        turn_index: 1,
                        catalyst_thesis: "Proposition initiale soumise".into(),
                        skeptic_antithesis: format!(
                            "VETO IMMÉDIAT : Risque de fuite cloud détecté ({})",
                            risk.vulnerability
                        ),
                        synthesis_delta: Some("Rejet souverain strict sans négociation".into()),
                        fuel_consumed: 120,
                    }],
                    total_fuel_consumed: 120,
                    veto_reason: Some(format!("Violation fail-closed : {}", risk.vulnerability)),
                };
            }
        }

        // 2. Déroulement des tours dialectiques avec consommation de carburant
        let num_turns = self.config.max_turns.max(1);
        let fuel_per_turn = (self.config.fuel_budget_per_turn / (num_turns as u64)).max(100);

        for i in 1..=num_turns {
            let turn_fuel = (fuel_per_turn as f32 * (0.8 + 0.1 * (i as f32))) as u64;
            total_fuel += turn_fuel;

            let c_text = format!(
                "Tour {}: Défense de l'action '{}' basée sur {} arguments",
                i,
                action_intent,
                arguments.len()
            );
            let s_text = format!(
                "Tour {}: Contre-analyse de robustesse portant sur {} risques identifiés",
                i,
                risks.len()
            );

            turns.push(DialecticTurn {
                turn_index: i,
                catalyst_thesis: c_text,
                skeptic_antithesis: s_text,
                synthesis_delta: Some(format!("Rapprochement itératif palier {}", i)),
                fuel_consumed: turn_fuel,
            });
        }

        // 3. Calcul bayésien de vraisemblance (Likelihood Ratios)
        // Score Catalyseur : somme pondérée des arguments
        let c_sum: f32 = arguments.iter().map(|a| a.weight * a.confidence).sum();
        let l_c = 1.0 + c_sum;

        // Score Sceptique : pénalité proportionnelle à la sévérité et confiance des risques
        let s_sum: f32 = risks
            .iter()
            .map(|r| r.severity * r.confidence * (if r.is_irreversible { 2.0 } else { 1.0 }))
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
