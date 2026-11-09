use hakille_core::{
    AuthorizationContext, CapabilityAction, CapabilityAuthority, CapabilityScope, CapabilityToken,
    Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Moderate,
    High,
    Irreversible,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedStep {
    pub step_id: u32,
    pub description: String,
    pub required_scope: CapabilityScope,
    pub required_action: CapabilityAction,
    pub is_irreversible: bool,
    pub fallback_strategy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub plan_id: String,
    pub goal_description: String,
    pub steps: Vec<PlannedStep>,
    pub estimated_risk: RiskLevel,
    pub created_at: Timestamp,
    pub requires_human_confirmation: bool,
}

impl ExecutionPlan {
    pub fn new(plan_id: impl Into<String>, goal: impl Into<String>) -> Self {
        Self {
            plan_id: plan_id.into(),
            goal_description: goal.into(),
            steps: Vec::new(),
            estimated_risk: RiskLevel::Low,
            created_at: Timestamp::now(),
            requires_human_confirmation: false,
        }
    }

    pub fn add_step(
        &mut self,
        description: impl Into<String>,
        required_scope: CapabilityScope,
        required_action: CapabilityAction,
        is_irreversible: bool,
    ) {
        let step_id = self.steps.len() as u32;
        if is_irreversible {
            self.estimated_risk = RiskLevel::Irreversible;
            self.requires_human_confirmation = true;
        } else if self.estimated_risk == RiskLevel::Low && required_scope == CapabilityScope::Wallet
        {
            self.estimated_risk = RiskLevel::High;
            self.requires_human_confirmation = true;
        }

        self.steps.push(PlannedStep {
            step_id,
            description: description.into(),
            required_scope,
            required_action,
            is_irreversible,
            fallback_strategy: None,
        });
    }

    /// Pre-execution capability verification (§28b, §3ter).
    /// Proves mathematically and cryptographically that all required capabilities are held
    /// BEFORE triggering the first step of the plan.
    pub fn verify_capabilities(
        &self,
        token: &CapabilityToken,
        authority: &CapabilityAuthority,
    ) -> Result<(), String> {
        for step in &self.steps {
            authority
                .verify(
                    token,
                    step.required_scope.clone(),
                    step.required_action.clone(),
                )
                .map_err(|e| {
                    format!("Plan step {} failed capability check: {}", step.step_id, e)
                })?;
        }
        Ok(())
    }

    /// Compatibilité d'API : l'absence de token est un refus, jamais une
    /// création implicite de droits réseau par le planificateur.
    pub fn verify_capabilities_or_short_token(
        &self,
        token: Option<&CapabilityToken>,
        authority: &CapabilityAuthority,
    ) -> Result<(), String> {
        let token = token.ok_or_else(|| {
            "capability explicite requise ; aucun droit forgé par le planificateur".to_string()
        })?;
        self.verify_capabilities(token, authority)
    }

    /// Vérification pré-exécution avec [`AuthorizationContext`] : valide le
    /// contexte (ressource normalisée, expiry, audience, quota, nonce,
    /// délégation), exige la couverture scope/action du contexte pour chaque
    /// pas (scope insuffisant = refus explicite), puis vérifie le token
    /// Biscuit épinglé (`verify_pinned` + expiry). Zéro exécution ici.
    pub fn verify_capabilities_with_context(
        &self,
        token: &CapabilityToken,
        authority: &CapabilityAuthority,
        ctx: &AuthorizationContext,
        expected_audience: &str,
    ) -> Result<(), String> {
        ctx.validate()
            .map_err(|e| format!("contexte d'autorisation invalide: {e}"))?;
        ctx.check_audience(expected_audience)
            .map_err(|e| format!("contexte d'autorisation rejeté: {e}"))?;
        if ctx.is_expired() {
            return Err("contexte d'autorisation expiré".to_string());
        }
        if token.is_expired() {
            return Err("token de capability expiré".to_string());
        }
        for step in &self.steps {
            if !ctx.covers(&step.required_scope, &step.required_action) {
                return Err(format!(
                    "Plan step {} scope insuffisant: contexte {:?}/{:?} ne couvre pas {:?}/{:?} (ressource '{}')",
                    step.step_id,
                    ctx.scope,
                    ctx.action,
                    step.required_scope,
                    step.required_action,
                    ctx.resource
                ));
            }
            authority
                .verify_pinned(
                    token,
                    step.required_scope.clone(),
                    step.required_action.clone(),
                )
                .map_err(|e| {
                    format!("Plan step {} failed capability check: {}", step.step_id, e)
                })?;
        }
        Ok(())
    }
}

/// Reflection outcome evaluated after executing a step or full plan (§28b).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanReflection {
    pub plan_id: String,
    pub all_steps_succeeded: bool,
    pub unexpected_side_effects: Vec<String>,
    pub confidence_delta: f64,
    pub summary: String,
}

/// Cognitive Planner implementing the Plan-Execute-Reflect loop (§28b).
pub struct DeliberatePlanner;

/// Reçu associé à une étape réelle. Une étape seulement proposée n'en a pas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepReceipt {
    pub step_id: u32,
    pub succeeded: bool,
    pub evidence_ref: String,
}

impl DeliberatePlanner {
    pub fn reflect_receipts(
        plan: &ExecutionPlan,
        receipts: &[StepReceipt],
    ) -> hakille_core::Result<PlanReflection> {
        let mut seen = std::collections::HashSet::new();
        for receipt in receipts {
            if !plan.steps.iter().any(|s| s.step_id == receipt.step_id)
                || !seen.insert(receipt.step_id)
                || receipt.evidence_ref.trim().is_empty()
            {
                return Err(hakille_core::HakilleError::Internal(
                    "reçu d'exécution invalide, dupliqué ou sans preuve".into(),
                ));
            }
        }
        let succeeded = receipts.iter().filter(|r| r.succeeded).count();
        let mut reflection = Self::evaluate_reflection(plan, succeeded, plan.steps.len());
        // Réussir une opération ne prouve pas la vérité d'une réponse LLM.
        reflection.confidence_delta = 0.0;
        if receipts.is_empty() {
            reflection.summary = format!(
                "Plan '{}' proposé : {} étapes sans reçu, aucune exécution vérifiée.",
                plan.plan_id,
                plan.steps.len()
            );
        }
        Ok(reflection)
    }

    pub fn evaluate_reflection(
        plan: &ExecutionPlan,
        steps_succeeded: usize,
        total_steps: usize,
    ) -> PlanReflection {
        let all_succeeded =
            total_steps > 0 && total_steps == plan.steps.len() && steps_succeeded == total_steps;
        let delta = if all_succeeded { 0.05 } else { -0.15 };
        let summary = if all_succeeded {
            format!(
                "Plan '{}' completed all {} steps successfully.",
                plan.plan_id, total_steps
            )
        } else {
            format!(
                "Plan '{}' stalled: {}/{} steps completed.",
                plan.plan_id, steps_succeeded, total_steps
            )
        };

        PlanReflection {
            plan_id: plan.plan_id.clone(),
            all_steps_succeeded: all_succeeded,
            unexpected_side_effects: Vec::new(),
            confidence_delta: delta,
            summary,
        }
    }
}

#[cfg(test)]
mod receipt_tests {
    use super::*;
    #[test]
    fn reflection_never_counts_proposals_or_duplicate_receipts_as_execution() {
        let mut plan = ExecutionPlan::new("p", "Lire et vérifier");
        plan.add_step(
            "lire",
            CapabilityScope::Network,
            CapabilityAction::Read,
            false,
        );
        plan.add_step(
            "vérifier",
            CapabilityScope::ModelInference,
            CapabilityAction::Read,
            false,
        );
        let proposed = DeliberatePlanner::reflect_receipts(&plan, &[]).unwrap();
        assert!(!proposed.all_steps_succeeded);
        assert_eq!(proposed.confidence_delta, 0.0);
        let receipt = StepReceipt {
            step_id: 0,
            succeeded: true,
            evidence_ref: "episode:tool-1".into(),
        };
        assert!(
            !DeliberatePlanner::reflect_receipts(&plan, std::slice::from_ref(&receipt))
                .unwrap()
                .all_steps_succeeded
        );
        assert!(
            DeliberatePlanner::reflect_receipts(&plan, &[receipt.clone(), receipt.clone()])
                .is_err()
        );
        let second = StepReceipt {
            step_id: 1,
            succeeded: true,
            evidence_ref: "episode:tool-2".into(),
        };
        let done = DeliberatePlanner::reflect_receipts(&plan, &[receipt, second]).unwrap();
        assert!(done.all_steps_succeeded);
        assert_eq!(done.confidence_delta, 0.0);
        assert!(
            !DeliberatePlanner::evaluate_reflection(&ExecutionPlan::new("empty", ""), 0, 0)
                .all_steps_succeeded
        );
    }
}
