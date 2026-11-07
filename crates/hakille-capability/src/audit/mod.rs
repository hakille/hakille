//! Cryptographic Audit Adapter bridging tool executions to the `hakille-audit` redb ledger.

use crate::error::CapabilityError;
use crate::types::{CanonicalToolCall, CanonicalToolResult, RiskTier};
use hakille_audit::{ActionRecord, AuditEntry, AuditJournal, ReasoningTrace, sha256_token_hash};
use hakille_core::{ActorId, CapabilityScope, CapabilityToken, EntryId, Timestamp};
use std::sync::Arc;

/// Cryptographic audit adapter committing executions into an immutable Merkle hash chain in redb.
pub struct CapabilityAuditAdapter {
    journal: Arc<AuditJournal>,
}

impl CapabilityAuditAdapter {
    /// Creates a new audit adapter wrapping the audit journal.
    pub fn new(journal: Arc<AuditJournal>) -> Self {
        Self { journal }
    }

    /// Returns a reference to the wrapped audit journal.
    pub fn journal(&self) -> &Arc<AuditJournal> {
        &self.journal
    }

    /// Logs an executed tool invocation to redb with Merkle chaining and token binding.
    pub async fn log_invocation(
        &self,
        call: &CanonicalToolCall,
        result: &CanonicalToolResult,
        token: Option<&CapabilityToken>,
        required_scope: Option<CapabilityScope>,
        actor_id: ActorId,
    ) -> Result<String, CapabilityError> {
        let token_hash = token.and_then(|t| t.to_vec().ok().map(|bytes| sha256_token_hash(&bytes)));

        let sanitized_params = sanitize_parameters(&call.arguments);

        let action = ActionRecord {
            action_type: format!("capability:{}", call.tool_name),
            description: format!(
                "Execution of tool '{}' (error: {})",
                call.tool_name, result.is_error
            ),
            parameters: sanitized_params,
            reversible: result.metadata.risk_tier == RiskTier::Tier1StateMutating,
            reversed: false,
        };

        let entry = AuditEntry {
            id: EntryId(uuid::Uuid::now_v7()),
            timestamp: Timestamp::now(),
            actor: actor_id,
            action,
            capability_used: required_scope,
            reasoning_trace: ReasoningTrace {
                steps: Vec::new(),
                model_used: None,
                duration_ms: result.metadata.duration_ms,
            },
            graph_nodes_used: Vec::new(),
            confidence: 1.0,
            cost: None,
            signature: Vec::new(), // Signed automatically by AuditJournal::append
            prev_hash: Vec::new(), // Chained automatically by AuditJournal::append
            token_hash,
        };

        let entry_id =
            self.journal
                .append(entry)
                .map_err(|e| CapabilityError::ExecutionFailed {
                    tool_name: call.tool_name.clone(),
                    message: format!("Audit journal append failed: {e}"),
                })?;

        Ok(entry_id.0.to_string())
    }
}

/// Recursively replaces sensitive parameter keys with "[REDACTED]".
pub fn sanitize_parameters(val: &serde_json::Value) -> serde_json::Value {
    match val {
        serde_json::Value::Object(map) => {
            let mut sanitized = serde_json::Map::new();
            for (k, v) in map {
                let lower = k.to_lowercase();
                if lower.contains("token")
                    || lower.contains("key")
                    || lower.contains("secret")
                    || lower.contains("password")
                    || lower.contains("auth")
                    || lower.contains("credential")
                    || lower.contains("bearer")
                {
                    sanitized.insert(
                        k.clone(),
                        serde_json::Value::String("[REDACTED]".to_string()),
                    );
                } else {
                    sanitized.insert(k.clone(), sanitize_parameters(v));
                }
            }
            serde_json::Value::Object(sanitized)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(sanitize_parameters).collect())
        }
        other => other.clone(),
    }
}
