use annora_core::{CapabilityAction, CapabilityAuthority, CapabilityScope, CapabilityToken, Timestamp};
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
        } else if self.estimated_risk == RiskLevel::Low && required_scope == CapabilityScope::Wallet {
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
                .verify(token, step.required_scope.clone(), step.required_action.clone())
                .map_err(|e| format!("Plan step {} failed capability check: {}", step.step_id, e))?;
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

impl DeliberatePlanner {
    pub fn evaluate_reflection(
        plan: &ExecutionPlan,
        steps_succeeded: usize,
        total_steps: usize,
    ) -> PlanReflection {
        let all_succeeded = steps_succeeded == total_steps;
        let delta = if all_succeeded { 0.05 } else { -0.15 };
        let summary = if all_succeeded {
            format!("Plan '{}' completed all {} steps successfully.", plan.plan_id, total_steps)
        } else {
            format!("Plan '{}' stalled: {}/{} steps completed.", plan.plan_id, steps_succeeded, total_steps)
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
