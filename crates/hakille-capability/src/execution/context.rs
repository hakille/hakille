//! Execution context, timeout constraints, and cancellation tokens.

use crate::types::RiskTier;
use hakille_core::CapabilityToken;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Execution bounds, security tokens, and cooperative cancellation controls for an invocation.
#[derive(Clone, Debug)]
pub struct ExecutionContext {
    /// Correlating invocation call identifier.
    pub call_id: String,
    /// Biscuit capability authorization token presented by caller.
    pub caller_token: Option<CapabilityToken>,
    /// Identity of the calling agent, user, or daemon.
    pub caller_identity: Option<String>,
    /// Hard execution timeout limit.
    pub timeout: Duration,
    /// Cooperative cancellation token propagated across async tasks and child processes.
    pub cancellation_token: CancellationToken,
    /// WebAssembly execution fuel limit (if sandboxed).
    pub max_fuel: Option<u64>,
    /// Memory consumption upper bound in bytes.
    pub max_memory_bytes: Option<usize>,
    /// Evaluated risk tier of this operation.
    pub risk_tier: RiskTier,
    /// Conversation or workflow session ID.
    pub session_id: Option<String>,
}

impl ExecutionContext {
    /// Creates a new execution context with default cooperative cancellation token.
    pub fn new(call_id: impl Into<String>, timeout: Duration) -> Self {
        Self {
            call_id: call_id.into(),
            caller_token: None,
            caller_identity: None,
            timeout,
            cancellation_token: CancellationToken::new(),
            max_fuel: None,
            max_memory_bytes: None,
            risk_tier: RiskTier::Tier0ReadOnly,
            session_id: None,
        }
    }

    /// Attaches an existing cooperative cancellation token.
    pub fn with_cancellation_token(mut self, token: CancellationToken) -> Self {
        self.cancellation_token = token;
        self
    }

    /// Sets the caller authorization token.
    pub fn with_caller_token(mut self, token: CapabilityToken) -> Self {
        self.caller_token = Some(token);
        self
    }

    /// Sets the caller identity string.
    pub fn with_caller_identity(mut self, identity: impl Into<String>) -> Self {
        self.caller_identity = Some(identity.into());
        self
    }

    /// Sets the risk tier for this execution.
    pub fn with_risk_tier(mut self, tier: RiskTier) -> Self {
        self.risk_tier = tier;
        self
    }

    /// Sets the maximum execution fuel.
    pub fn with_fuel(mut self, max_fuel: u64) -> Self {
        self.max_fuel = Some(max_fuel);
        self
    }

    /// Sets the maximum peak memory limit in bytes.
    pub fn with_memory_bytes(mut self, bytes: usize) -> Self {
        self.max_memory_bytes = Some(bytes);
        self
    }

    /// Sets the session identifier.
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }
}
