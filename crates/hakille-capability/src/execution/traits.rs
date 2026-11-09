//! Core trait for capability execution adapters.

use super::context::ExecutionContext;
use crate::error::CapabilityError;
use crate::types::{CanonicalToolCall, CanonicalToolResult};
use async_trait::async_trait;

/// Asynchronous capability execution adapter.
///
/// Implemented by Native, MCP (stdio/SSE), WebAssembly (Wasmtime 48), and Skill execution backends.
#[async_trait]
pub trait CapabilityExecutor: Send + Sync {
    /// Executes a tool call asynchronously within the given context bounds.
    async fn execute(
        &self,
        call: &CanonicalToolCall,
        context: &ExecutionContext,
    ) -> Result<CanonicalToolResult, CapabilityError>;
}
