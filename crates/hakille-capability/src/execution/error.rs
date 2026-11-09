//! Structured error payloads for tool domain execution failures.

use crate::types::{CanonicalContentBlock, CanonicalToolResult, ExecutionMetadata, RiskTier};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Structured tool domain error envelope.
///
/// In alignment with Model Context Protocol (MCP) and Hakille Canonical Tool Contracts:
/// - Tool execution errors (e.g., file not found, bad SQL, API 404) return `CanonicalToolResult` with
///   `is_error: true` containing a `StructuredToolError` payload.
/// - System transport / infrastructure faults return `Err(CapabilityError)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredToolError {
    /// Domain error code (e.g., "FILE_NOT_FOUND", "RATE_LIMITED", "INVALID_INPUT").
    pub error_code: String,
    /// Human and model-readable explanation of the failure.
    pub message: String,
    /// Whether the error is transient and eligible for retry.
    pub is_transient: bool,
    /// Suggested wait time before retrying in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    /// Optional structured debugging details.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl StructuredToolError {
    /// Creates a non-transient structured tool error.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error_code: code.into(),
            message: message.into(),
            is_transient: false,
            retry_after_ms: None,
            details: None,
        }
    }

    /// Creates a transient structured tool error with suggested retry delay.
    pub fn transient(
        code: impl Into<String>,
        message: impl Into<String>,
        retry_after_ms: u64,
    ) -> Self {
        Self {
            error_code: code.into(),
            message: message.into(),
            is_transient: true,
            retry_after_ms: Some(retry_after_ms),
            details: None,
        }
    }

    /// Attaches structured error details.
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    /// Converts this error into a `CanonicalToolResult` with `is_error: true`.
    pub fn to_tool_result(&self, call_id: &str, risk_tier: RiskTier) -> CanonicalToolResult {
        let json_str = serde_json::to_string(self).unwrap_or_else(|_| self.message.clone());
        CanonicalToolResult {
            call_id: call_id.to_string(),
            content: vec![CanonicalContentBlock::text(json_str)],
            is_error: true,
            metadata: ExecutionMetadata::new(0, risk_tier),
        }
    }

    /// Attempts to parse a `StructuredToolError` from a `CanonicalToolResult`'s text content.
    pub fn from_tool_result(result: &CanonicalToolResult) -> Option<Self> {
        if !result.is_error {
            return None;
        }
        for block in &result.content {
            if let Some(text) = block.as_text()
                && let Ok(err) = serde_json::from_str::<Self>(text)
            {
                return Some(err);
            }
        }
        None
    }
}
