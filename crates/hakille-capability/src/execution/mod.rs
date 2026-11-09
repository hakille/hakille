//! Universal Execution Engine with timeout preemption and cooperative cancellation.

pub mod context;
pub mod error;
pub mod traits;

use crate::error::CapabilityError;
use crate::types::{CanonicalToolCall, CanonicalToolResult};
pub use context::ExecutionContext;
pub use error::StructuredToolError;
use std::sync::Arc;
use std::time::Instant;
pub use traits::CapabilityExecutor;

/// Universal execution dispatcher enforcing timeout limits and cancellation.
pub struct UniversalExecutionEngine;

impl UniversalExecutionEngine {
    /// Executes a tool call across an executor adapter with timeout preemption,
    /// cooperative cancellation, telemetry tracking, and failure taxonomy mapping.
    pub async fn execute_guarded(
        executor: Arc<dyn CapabilityExecutor>,
        call: &CanonicalToolCall,
        context: &ExecutionContext,
    ) -> Result<CanonicalToolResult, CapabilityError> {
        if context.cancellation_token.is_cancelled() {
            return Err(CapabilityError::ExecutionFailed {
                tool_name: call.tool_name.clone(),
                message: "Execution aborted by cooperative cancellation token".to_string(),
            });
        }

        let start_time = Instant::now();

        tokio::select! {
            biased;

            _ = context.cancellation_token.cancelled() => {
                Err(CapabilityError::ExecutionFailed {
                    tool_name: call.tool_name.clone(),
                    message: "Execution aborted by cooperative cancellation token".to_string(),
                })
            }
            res = tokio::time::timeout(context.timeout, executor.execute(call, context)) => {
                let duration_ms = start_time.elapsed().as_millis() as u64;
                match res {
                    Ok(Ok(mut tool_result)) => {
                        tool_result.metadata.duration_ms = duration_ms;
                        tool_result.metadata.risk_tier = context.risk_tier;
                        Ok(tool_result)
                    }
                    Ok(Err(err)) => Err(err),
                    Err(_) => Err(CapabilityError::Timeout {
                        timeout_ms: context.timeout.as_millis() as u64,
                    }),
                }
            }
        }
    }
}
