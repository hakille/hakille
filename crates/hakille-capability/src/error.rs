use thiserror::Error;

/// Core error types for the Hakille Capability subsystem.
#[derive(Debug, Error)]
pub enum CapabilityError {
    #[error("Validation failed: {0}")]
    Validation(#[from] ValidationError),

    #[error("Serialization / deserialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Core system error: {0}")]
    Core(#[from] hakille_core::HakilleError),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Execution failed for tool '{tool_name}': {message}")]
    ExecutionFailed { tool_name: String, message: String },

    #[error("Unauthorized execution: missing required scope {scope:?}")]
    UnauthorizedScope {
        scope: hakille_core::CapabilityScope,
    },

    #[error("Execution timed out after {timeout_ms} ms")]
    Timeout { timeout_ms: u64 },

    #[error("Fuel exhausted (limit: {limit})")]
    FuelExhausted { limit: u64 },

    #[error("Translator error: {0}")]
    Translator(#[from] crate::translators::TranslatorError),

    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Security violation: {reason}")]
    SecurityViolation { reason: String },

    #[error("Custom error: {0}")]
    Custom(String),
}

/// Validation errors for tools, schemas, manifests, and descriptors.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ValidationError {
    #[error("Invalid JSON Schema for parameters in tool '{tool_name}': {reason}")]
    InvalidParametersSchema { tool_name: String, reason: String },

    #[error("Schema strict mode violation in tool '{tool_name}': {reason}")]
    StrictModeViolation { tool_name: String, reason: String },

    #[error("Invalid tool arguments for '{tool_name}': {reason}")]
    InvalidArguments { tool_name: String, reason: String },

    #[error("Duplicate tool name '{name}' detected in capability '{capability_id}'")]
    DuplicateToolName { capability_id: String, name: String },

    #[error("Invalid capability identifier '{id}': {reason}")]
    InvalidIdentifier { id: String, reason: String },

    #[error("Invalid tool name '{name}': {reason}")]
    InvalidToolName { name: String, reason: String },

    #[error("Empty required field '{field}' in '{context}'")]
    EmptyField {
        field: &'static str,
        context: String,
    },

    #[error("Invalid version string '{version}': {reason}")]
    InvalidVersion { version: String, reason: String },
}

pub type Result<T> = std::result::Result<T, CapabilityError>;
