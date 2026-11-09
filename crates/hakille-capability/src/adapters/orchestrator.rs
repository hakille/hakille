//! Intelligent Tool Orchestrator.
//!
//! Coordinates discovery, security gates, physical human confirmations (Tier 3),
//! timeout preemption, and cryptographic audit trail logging across
//! heterogeneous capability backends.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use hakille_core::{
    ActorId, CapabilityAction, CapabilityScope, CapabilityToken, PinnedRootPublicKey,
};

use crate::audit::CapabilityAuditAdapter;
use crate::discovery::{CapabilityDiscoveryEngine, DiscoveredTool, DiscoveryQuery};
use crate::error::CapabilityError;
use crate::execution::{CapabilityExecutor, ExecutionContext, UniversalExecutionEngine};
use crate::registry::UniversalCapabilityRegistry;
use crate::security::{ActionConfirmationProof, BiscuitVerifier};
use crate::types::{CanonicalToolCall, CanonicalToolResult, RiskTier};

/// Request to orchestrate a capability execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationRequest {
    /// Intent or query driving tool selection.
    pub intent: String,
    /// Caller Biscuit authorization token, if available.
    #[serde(skip)]
    pub caller_token: Option<CapabilityToken>,
    /// Optional explicit tool name (if already decided by caller).
    pub explicit_tool_name: Option<String>,
    /// Tool input arguments.
    pub arguments: Value,
    /// Physical hold confirmation proof for Tier 3 actions.
    pub confirmation_proof: Option<ActionConfirmationProof>,
    /// Execution timeout in milliseconds.
    pub timeout_ms: u64,
}

/// Orchestration result summary returned to cognitive callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationResult {
    pub selected_tool: String,
    pub tool_result: CanonicalToolResult,
    pub evaluated_risk: RiskTier,
    pub audit_tx_id: Option<String>,
    pub requires_physical_confirmation: bool,
}

/// Central orchestrator coordinating registry, discovery, security, and execution.
pub struct IntelligentToolOrchestrator {
    registry: UniversalCapabilityRegistry,
    discovery: CapabilityDiscoveryEngine,
    root_public_key: Option<PinnedRootPublicKey>,
    audit: Option<CapabilityAuditAdapter>,
    executors: HashMap<String, Arc<dyn CapabilityExecutor>>,
}

impl IntelligentToolOrchestrator {
    /// Creates a new orchestrator with the provided registry and optional verification key.
    pub fn new(
        registry: UniversalCapabilityRegistry,
        root_public_key: Option<PinnedRootPublicKey>,
        audit: Option<CapabilityAuditAdapter>,
    ) -> Self {
        let discovery = CapabilityDiscoveryEngine::new(registry.clone());
        Self {
            registry,
            discovery,
            root_public_key,
            audit,
            executors: HashMap::new(),
        }
    }

    /// Registers an executor adapter for a specific tool name.
    pub fn register_executor(
        &mut self,
        tool_name: impl Into<String>,
        executor: Arc<dyn CapabilityExecutor>,
    ) {
        self.executors.insert(tool_name.into(), executor);
    }

    /// Discovers candidate tools matching the user's intent within the prompt budget.
    pub async fn discover_candidates(
        &self,
        query: &str,
        caller_token: Option<CapabilityToken>,
    ) -> Vec<DiscoveredTool> {
        let mut disc_query = DiscoveryQuery::new(query)
            .with_top_k(5)
            .with_min_similarity(0.0)
            .with_token_budget(4096);
        disc_query.caller_token = caller_token;

        let empty_embeddings = HashMap::new();
        self.discovery
            .discover(&disc_query, &empty_embeddings)
            .await
            .unwrap_or_default()
    }

    /// Orchestrates an end-to-end tool execution:
    /// 1. Discovery/Selection
    /// 2. Biscuit Security & Risk Verification
    /// 3. Tier 3 Physical Confirmation Gate (600ms hold)
    /// 4. Guarded Execution with Timeout & Cancellation
    /// 5. Cryptographic Audit Ledger Commit
    pub async fn execute(
        &self,
        req: OrchestrationRequest,
    ) -> Result<OrchestrationResult, CapabilityError> {
        // 1. Determine target tool
        let tool_name = if let Some(explicit) = req.explicit_tool_name {
            explicit
        } else {
            let candidates = self
                .discover_candidates(&req.intent, req.caller_token.clone())
                .await;
            let top = candidates
                .first()
                .ok_or_else(|| CapabilityError::ExecutionFailed {
                    tool_name: "discovery".to_string(),
                    message: format!(
                        "No suitable capability found matching intent: '{}'",
                        req.intent
                    ),
                })?;
            top.tool.name.clone()
        };

        // 2. Fetch definition from registry
        let (descriptor, def) = self
            .registry
            .find_tool(&tool_name)
            .await
            .ok_or_else(|| CapabilityError::ToolNotFound(tool_name.clone()))?;

        // 3. Evaluate risk tier
        let risk_tier = match tool_name.as_str() {
            "emergency_shred" => RiskTier::Tier3CriticalExecution,
            n if n.starts_with("mcp_") => RiskTier::Tier2ExternalNetwork,
            "vfs_write" => RiskTier::Tier1StateMutating,
            _ => RiskTier::Tier0ReadOnly,
        };

        // 4. Biscuit verification if required and root key present
        if let Some(root_key) = &self.root_public_key {
            let action = match risk_tier {
                RiskTier::Tier0ReadOnly => CapabilityAction::Read,
                RiskTier::Tier1StateMutating => CapabilityAction::Write,
                _ => CapabilityAction::Execute,
            };

            let scopes_to_check = if descriptor.scopes_required.is_empty() {
                if risk_tier > RiskTier::Tier0ReadOnly {
                    vec![CapabilityScope::Filesystem]
                } else {
                    Vec::new()
                }
            } else {
                descriptor.scopes_required.clone()
            };

            if !scopes_to_check.is_empty() {
                let token = req.caller_token.as_ref().ok_or_else(|| {
                    CapabilityError::SecurityViolation {
                        reason: "Missing caller token for secured capability".to_string(),
                    }
                })?;

                for scope in &scopes_to_check {
                    BiscuitVerifier::verify_token(token, root_key, scope, action.clone()).map_err(
                        |e| CapabilityError::SecurityViolation {
                            reason: format!(
                                "Security authorization rejected for scope {scope:?}: {e}"
                            ),
                        },
                    )?;
                }
            }
        }

        // 5. Check Tier 3 Physical Confirmation Gate
        if risk_tier == RiskTier::Tier3CriticalExecution {
            let proof =
                req.confirmation_proof
                    .ok_or_else(|| {
                        CapabilityError::SecurityViolation {
                    reason:
                        "Tier 3 Critical Execution requires physical 600ms hold confirmation proof"
                            .to_string(),
                }
                    })?;

            let expected_hash =
                ActionConfirmationProof::compute_action_hash(&tool_name, &req.arguments);
            proof
                .verify(&expected_hash)
                .map_err(|e| CapabilityError::SecurityViolation {
                    reason: format!("Physical confirmation invalid: {e}"),
                })?;
        }

        // 6. Lookup executor adapter
        let executor = self.executors.get(&tool_name).cloned().ok_or_else(|| {
            CapabilityError::ExecutionFailed {
                tool_name: tool_name.clone(),
                message: format!("No executor registered for tool '{tool_name}'"),
            }
        })?;

        // 7. Execute guarded with timeout and cancellation
        let call_id = format!("orch_{}_{}", tool_name, uuid::Uuid::new_v4().as_simple());
        let call = CanonicalToolCall::new(&call_id, &tool_name, req.arguments.clone());
        let ctx = ExecutionContext::new(&call_id, Duration::from_millis(req.timeout_ms))
            .with_risk_tier(risk_tier);

        let mut tool_result =
            UniversalExecutionEngine::execute_guarded(executor, &call, &ctx).await?;

        // 8. Cryptographic audit trail commit
        let mut audit_tx_id = None;
        if let Some(audit) = &self.audit {
            let required_scope = descriptor.scopes_required.first().cloned();
            let actor_id = ActorId::Agent(uuid::Uuid::now_v7().to_string());

            let entry_id = audit
                .log_invocation(
                    &call,
                    &tool_result,
                    req.caller_token.as_ref(),
                    required_scope,
                    actor_id,
                )
                .await?;
            audit_tx_id = Some(entry_id.clone());
            tool_result.metadata.audit_record_id = Some(entry_id);
        }

        Ok(OrchestrationResult {
            selected_tool: def.name,
            tool_result,
            evaluated_risk: risk_tier,
            audit_tx_id,
            requires_physical_confirmation: risk_tier == RiskTier::Tier3CriticalExecution,
        })
    }
}
