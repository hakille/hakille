//! Lossless Bidirectional Schema Translators for Frontier and Local AI Models.
//!
//! Provides bidirectional translations between Hakille's canonical tool contracts
//! and model wire formats:
//! - OpenAI (Strict Mode Function Calling, stringified arguments, `role: "tool"`)
//! - Anthropic (Claude 3.5/3.7 `tool_use`, `tool_result`, interleaved thinking preservation)
//! - Google Gemini (`functionDeclarations`, uppercase schema types, `functionCall`/`functionResponse`)
//! - Local GGUF / ChatML (`<tools>`, `<tool_call>`, `<tool_response>` delimiter parsing)

use crate::types::{CanonicalToolCall, CanonicalToolDefinition, CanonicalToolResult};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod anthropic;
pub mod chatml;
pub mod gemini;
pub mod openai;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TranslatorError {
    #[error("Provider schema error for tool '{tool_name}': {reason}")]
    SchemaTransformationError { tool_name: String, reason: String },

    #[error(
        "Malformed stringified JSON arguments in tool call '{tool_name}': {raw_arguments}. Parse error: {details}"
    )]
    MalformedArguments {
        tool_name: String,
        raw_arguments: String,
        details: String,
    },

    #[error("Missing mandatory field '{field}' in provider payload for '{context}'")]
    MissingField {
        field: &'static str,
        context: String,
    },

    #[error("Invalid tool name '{name}': violates provider regex constraint '{pattern}'")]
    InvalidToolName { name: String, pattern: &'static str },

    #[error("Delimiter parse error in ChatML stream: {details}")]
    ChatMlDelimiterError { details: String },

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// The Universal Provider Translator Trait.
///
/// Encapsulates bidirectional serialization and deserialization between canonical
/// data structures and provider-specific wire schemas.
pub trait ProviderTranslator {
    type ProviderToolDef: Serialize + for<'de> Deserialize<'de>;
    type ProviderToolCall: Serialize + for<'de> Deserialize<'de>;
    type ProviderToolResult: Serialize + for<'de> Deserialize<'de>;

    /// Converts a canonical tool definition to the provider's tool declaration.
    fn to_provider_tool(
        tool: &CanonicalToolDefinition,
    ) -> Result<Self::ProviderToolDef, TranslatorError>;

    /// Normalizes a provider's tool declaration into the canonical definition.
    fn from_provider_tool(
        raw: &Self::ProviderToolDef,
    ) -> Result<CanonicalToolDefinition, TranslatorError>;

    /// Parses an incoming provider tool call into a CanonicalToolCall.
    fn parse_tool_call(call: &Self::ProviderToolCall)
    -> Result<CanonicalToolCall, TranslatorError>;

    /// Encodes a CanonicalToolCall into the provider's call format.
    fn encode_tool_call(
        call: &CanonicalToolCall,
    ) -> Result<Self::ProviderToolCall, TranslatorError>;

    /// Formats a CanonicalToolResult into the provider's result structure.
    fn encode_tool_result(
        result: &CanonicalToolResult,
    ) -> Result<Self::ProviderToolResult, TranslatorError>;

    /// Parses a provider's tool result envelope back into CanonicalToolResult.
    fn parse_tool_result(
        raw: &Self::ProviderToolResult,
    ) -> Result<CanonicalToolResult, TranslatorError>;
}

/// Trait for schema normalization across JSON Schema dialects.
pub trait SchemaNormalizer {
    /// Normalizes schema to provider dialect.
    fn normalize_schema(
        &self,
        schema: &mut serde_json::Value,
        tool_name: &str,
    ) -> Result<(), TranslatorError>;

    /// Denormalizes schema back to standard JSON Schema 2020-12.
    fn denormalize_schema(&self, schema: &mut serde_json::Value) -> Result<(), TranslatorError>;
}
