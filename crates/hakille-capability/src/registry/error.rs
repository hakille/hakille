//! Registry error definitions.

use thiserror::Error;

/// Errors arising from capability registration, activation, or lifecycle management.
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("Capability '{id}' version {version} already registered")]
    AlreadyExists {
        id: String,
        version: semver::Version,
    },

    #[error("Capability '{id}' not found")]
    NotFound { id: String },

    #[error("No version of capability '{id}' satisfies requirement '{requirement}'")]
    VersionMismatch { id: String, requirement: String },

    #[error(
        "Tool name collision: tool '{tool_name}' already exposed by capability '{existing_capability_id}'"
    )]
    ToolCollision {
        tool_name: String,
        existing_capability_id: String,
    },

    #[error("Invalid lifecycle transition for '{id}' from {current} to {target}")]
    InvalidTransition {
        id: String,
        current: String,
        target: String,
    },

    #[error("Capability '{id}' is currently active; must be disabled before unregistering")]
    ActiveUnregisterBlocked { id: String },

    #[error("Validation failed: {0}")]
    Validation(#[from] crate::error::ValidationError),
}
