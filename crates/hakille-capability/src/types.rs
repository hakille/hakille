//! Universal Capability Contract & Canonical Data Structures.
//!
//! This module defines the vendor-neutral contracts for AI tool execution,
//! structured content blocks, execution telemetry, 4-tier risk classification,
//! and capability manifest / descriptor schemas.

use crate::error::ValidationError;
use crate::validation::{validate_capability_id, validate_parameters_schema, validate_tool_name};
pub use hakille_core::CapabilityScope;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fmt;

// ============================================================================
// 1. Risk Tier (4-Tier Security Governance)
// ============================================================================

/// 4-tier risk classification for Hakille capabilities.
///
/// Enforces graduated security governance:
/// - Tier 0: Pure informational, idempotent, read-only queries. Auto-granted.
/// - Tier 1: Local state-mutating operations within safe workspace bounds.
/// - Tier 2: Outbound network calls and external service integrations.
/// - Tier 3: Critical system operations (shell, file deletion, financial, biometrics).
///   Requires mandatory physical user confirmation (600ms hold-to-confirm progress ring).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum RiskTier {
    /// Tier 0: Pure informational, idempotent, read-only queries. Auto-granted.
    #[serde(alias = "tier0", alias = "tier0_read_only", alias = "Tier0ReadOnly")]
    #[default]
    Tier0ReadOnly = 0,

    /// Tier 1: Local state-mutating operations within safe workspace bounds.
    #[serde(
        alias = "tier1",
        alias = "tier1_state_mutating",
        alias = "Tier1StateMutating"
    )]
    Tier1StateMutating = 1,

    /// Tier 2: Outbound network calls and external service integrations.
    #[serde(
        alias = "tier2",
        alias = "tier2_external_network",
        alias = "Tier2ExternalNetwork"
    )]
    Tier2ExternalNetwork = 2,

    /// Tier 3: Critical system operations (arbitrary code, shell, file deletion, financial, biometrics).
    /// Requires mandatory physical user confirmation (600ms hold-to-confirm progress ring).
    #[serde(
        alias = "tier3",
        alias = "tier3_critical_execution",
        alias = "Tier3CriticalExecution"
    )]
    Tier3CriticalExecution = 3,
}

impl RiskTier {
    /// Returns the integer level (0 to 3).
    pub const fn level(&self) -> u8 {
        *self as u8
    }

    /// Creates a RiskTier from its integer level (0 to 3).
    pub fn from_level(level: u8) -> Option<Self> {
        match level {
            0 => Some(RiskTier::Tier0ReadOnly),
            1 => Some(RiskTier::Tier1StateMutating),
            2 => Some(RiskTier::Tier2ExternalNetwork),
            3 => Some(RiskTier::Tier3CriticalExecution),
            _ => None,
        }
    }

    /// Whether this risk tier requires physical confirmation from the user (ActionCard 600ms).
    pub const fn requires_user_confirmation(&self) -> bool {
        matches!(self, RiskTier::Tier3CriticalExecution)
    }

    /// Whether this tier involves external network communication.
    pub const fn has_network_egress(&self) -> bool {
        matches!(
            self,
            RiskTier::Tier2ExternalNetwork | RiskTier::Tier3CriticalExecution
        )
    }

    /// Whether this tier mutates state (local or external).
    pub const fn is_mutating(&self) -> bool {
        !matches!(self, RiskTier::Tier0ReadOnly)
    }

    /// Static string identifier for serialization or telemetry.
    pub const fn as_str(&self) -> &'static str {
        match self {
            RiskTier::Tier0ReadOnly => "tier0_read_only",
            RiskTier::Tier1StateMutating => "tier1_state_mutating",
            RiskTier::Tier2ExternalNetwork => "tier2_external_network",
            RiskTier::Tier3CriticalExecution => "tier3_critical_execution",
        }
    }
}

impl fmt::Display for RiskTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// 2. Capability Type
// ============================================================================

/// Underlying runtime and integration protocol of a capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityType {
    /// Pure-Rust in-process tool executed with minimal overhead.
    Native,

    /// Model Context Protocol over stdio (child process pipes).
    McpStdio,

    /// Model Context Protocol over Server-Sent Events / HTTP.
    McpSse,

    /// Sandboxed WebAssembly component / module (Wasmtime 48).
    Wasm,

    /// Declarative cognitive skill (SKILL.md) with progressive prompt injection.
    Skill,

    /// External REST / HTTP OpenAPI endpoint.
    Rest,
}

impl CapabilityType {
    /// Whether this capability is backed by the Model Context Protocol (stdio or SSE).
    pub const fn is_mcp(&self) -> bool {
        matches!(self, CapabilityType::McpStdio | CapabilityType::McpSse)
    }

    /// Whether this capability executes inside a sandboxed environment (Wasm or isolated subproc).
    pub const fn is_sandboxed(&self) -> bool {
        matches!(self, CapabilityType::Wasm | CapabilityType::McpStdio)
    }

    /// Static string identifier.
    pub const fn as_str(&self) -> &'static str {
        match self {
            CapabilityType::Native => "native",
            CapabilityType::McpStdio => "mcp_stdio",
            CapabilityType::McpSse => "mcp_sse",
            CapabilityType::Wasm => "wasm",
            CapabilityType::Skill => "skill",
            CapabilityType::Rest => "rest",
        }
    }
}

impl fmt::Display for CapabilityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// 3. Canonical Tool Definition
// ============================================================================

/// Canonical tool definition exposed to models and registries.
///
/// Uses JSON Schema draft 2020-12 for input parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalToolDefinition {
    /// Unique name of the tool (alphanumeric, underscores, hyphens).
    pub name: String,

    /// Comprehensive documentation of what the tool accomplishes.
    pub description: String,

    /// Input parameters formatted as a JSON Schema draft 2020-12 object.
    pub parameters: Value,

    /// Whether schema strict mode is enforced (mandating additionalProperties: false
    /// and exhaustive required array).
    pub strict: bool,
}

impl CanonicalToolDefinition {
    /// Creates a new canonical tool definition with strict mode disabled by default.
    pub fn new(name: impl Into<String>, description: impl Into<String>, parameters: Value) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
            strict: false,
        }
    }

    /// Enables or disables schema strict mode.
    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// Returns a default empty parameters schema (`{"type": "object", "properties": {}}`).
    pub fn empty_parameters_schema() -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    /// Validates the tool name and its parameters schema.
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_tool_name(&self.name)?;
        if self.description.trim().is_empty() {
            return Err(ValidationError::EmptyField {
                field: "description",
                context: format!("tool '{}'", self.name),
            });
        }
        validate_parameters_schema(&self.name, &self.parameters, self.strict)?;
        Ok(())
    }
}

// ============================================================================
// 4. Canonical Tool Call
// ============================================================================

/// Normalized tool invocation call emitted by cognitive engines or AI models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalToolCall {
    /// Unique identifier for this invocation instance (e.g. "call_12345").
    pub call_id: String,

    /// Name of the target tool to invoke.
    pub tool_name: String,

    /// Parsed invocation arguments as a JSON value (typically an Object).
    pub arguments: Value,
}

impl CanonicalToolCall {
    /// Creates a new canonical tool call.
    pub fn new(call_id: impl Into<String>, tool_name: impl Into<String>, arguments: Value) -> Self {
        Self {
            call_id: call_id.into(),
            tool_name: tool_name.into(),
            arguments,
        }
    }

    /// Constructs a tool call by parsing arguments from a raw JSON string (e.g. from OpenAI responses).
    pub fn from_json_str(
        call_id: impl Into<String>,
        tool_name: impl Into<String>,
        raw_json_args: &str,
    ) -> std::result::Result<Self, serde_json::Error> {
        let parsed: Value = if raw_json_args.trim().is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(raw_json_args)?
        };
        Ok(Self::new(call_id, tool_name, parsed))
    }

    /// Deserializes arguments into a strongly-typed structure.
    pub fn parse_arguments<T: DeserializeOwned>(
        &self,
    ) -> std::result::Result<T, serde_json::Error> {
        serde_json::from_value(self.arguments.clone())
    }
}

// ============================================================================
// 5. Canonical Content Block
// ============================================================================

/// Content block in a tool's execution result.
///
/// Unifies MCP and frontier provider formats (text, image, resources).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CanonicalContentBlock {
    /// Standard plain-text or Markdown content.
    Text { text: String },

    /// Base64-encoded image content with standard MIME type.
    Image {
        mime_type: String,
        data_base64: String,
    },

    /// Resource reference or embedded payload.
    Resource {
        uri: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blob_base64: Option<String>,
    },
}

impl CanonicalContentBlock {
    /// Creates a text content block.
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// Creates an image content block with base64 data.
    pub fn image(mime_type: impl Into<String>, data_base64: impl Into<String>) -> Self {
        Self::Image {
            mime_type: mime_type.into(),
            data_base64: data_base64.into(),
        }
    }

    /// Creates a text resource content block.
    pub fn resource_text(
        uri: impl Into<String>,
        mime_type: Option<String>,
        text: impl Into<String>,
    ) -> Self {
        Self::Resource {
            uri: uri.into(),
            mime_type,
            text: Some(text.into()),
            blob_base64: None,
        }
    }

    /// Creates a binary blob resource content block.
    pub fn resource_blob(
        uri: impl Into<String>,
        mime_type: Option<String>,
        blob_base64: impl Into<String>,
    ) -> Self {
        Self::Resource {
            uri: uri.into(),
            mime_type,
            text: None,
            blob_base64: Some(blob_base64.into()),
        }
    }

    /// Returns the text reference if this block is of type Text.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            _ => None,
        }
    }

    /// Whether this block is Text.
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text { .. })
    }

    /// Whether this block is Image.
    pub fn is_image(&self) -> bool {
        matches!(self, Self::Image { .. })
    }

    /// Whether this block is Resource.
    pub fn is_resource(&self) -> bool {
        matches!(self, Self::Resource { .. })
    }
}

// ============================================================================
// 6. Execution Metadata
// ============================================================================

/// Execution telemetry and security metadata recorded during a tool invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionMetadata {
    /// Duration of the execution in milliseconds.
    pub duration_ms: u64,

    /// Wasmtime fuel consumed if executed in a WebAssembly sandbox.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fuel_consumed: Option<u64>,

    /// Peak memory consumed during execution in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<usize>,

    /// Evaluated risk tier of the executed action.
    pub risk_tier: RiskTier,

    /// Corresponding entry ID in `hakille-audit` ledger, if logged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_record_id: Option<String>,
}

impl Default for ExecutionMetadata {
    fn default() -> Self {
        Self {
            duration_ms: 0,
            fuel_consumed: None,
            memory_bytes: None,
            risk_tier: RiskTier::Tier0ReadOnly,
            audit_record_id: None,
        }
    }
}

impl ExecutionMetadata {
    /// Creates new execution metadata with duration and evaluated risk tier.
    pub fn new(duration_ms: u64, risk_tier: RiskTier) -> Self {
        Self {
            duration_ms,
            fuel_consumed: None,
            memory_bytes: None,
            risk_tier,
            audit_record_id: None,
        }
    }

    /// Sets consumed WebAssembly fuel.
    pub fn with_fuel(mut self, fuel: u64) -> Self {
        self.fuel_consumed = Some(fuel);
        self
    }

    /// Sets peak memory in bytes.
    pub fn with_memory(mut self, bytes: usize) -> Self {
        self.memory_bytes = Some(bytes);
        self
    }

    /// Sets associated cryptographic audit record identifier.
    pub fn with_audit_record_id(mut self, audit_id: impl Into<String>) -> Self {
        self.audit_record_id = Some(audit_id.into());
        self
    }
}

// ============================================================================
// 7. Canonical Tool Result
// ============================================================================

/// Complete execution result envelope returned to cognitive orchestrators and callers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalToolResult {
    /// Correlating invocation call ID.
    pub call_id: String,

    /// Execution output blocks (text, image, resource).
    pub content: Vec<CanonicalContentBlock>,

    /// Whether the execution encountered an error.
    /// In accordance with MCP, execution failures (e.g. file not found)
    /// return `is_error: true` with error details in content rather than an RPC fault.
    pub is_error: bool,

    /// Execution telemetry, fuel, memory, and audit linkage.
    pub metadata: ExecutionMetadata,
}

impl CanonicalToolResult {
    /// Creates a successful tool result with multiple content blocks.
    pub fn success(
        call_id: impl Into<String>,
        content: Vec<CanonicalContentBlock>,
        metadata: ExecutionMetadata,
    ) -> Self {
        Self {
            call_id: call_id.into(),
            content,
            is_error: false,
            metadata,
        }
    }

    /// Creates a successful tool result with a single text output.
    pub fn success_text(
        call_id: impl Into<String>,
        text: impl Into<String>,
        metadata: ExecutionMetadata,
    ) -> Self {
        Self::success(call_id, vec![CanonicalContentBlock::text(text)], metadata)
    }

    /// Creates an execution error tool result with a text error message.
    pub fn error(
        call_id: impl Into<String>,
        error_message: impl Into<String>,
        metadata: ExecutionMetadata,
    ) -> Self {
        Self {
            call_id: call_id.into(),
            content: vec![CanonicalContentBlock::text(error_message)],
            is_error: true,
            metadata,
        }
    }

    /// Creates an execution error tool result with custom content blocks.
    pub fn error_with_content(
        call_id: impl Into<String>,
        content: Vec<CanonicalContentBlock>,
        metadata: ExecutionMetadata,
    ) -> Self {
        Self {
            call_id: call_id.into(),
            content,
            is_error: true,
            metadata,
        }
    }

    /// Convenience check for success.
    pub fn is_success(&self) -> bool {
        !self.is_error
    }

    /// Extracts concatenated text from all text content blocks.
    pub fn text_summary(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| c.as_text())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// ============================================================================
// 8. Capability Manifest & Capability Descriptor
// ============================================================================

/// Static declarative specification of a capability package (e.g. from `manifest.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityManifest {
    /// Canonical capability identifier (e.g. "hakille.fs", "mcp:github/repos").
    pub id: String,

    /// Human-readable display name.
    pub name: String,

    /// SemVer 2.0 version of the capability.
    pub version: semver::Version,

    /// Detailed description of the capability's purpose and scope.
    pub description: String,

    /// Author or publisher identity.
    pub author: String,

    /// Underlying capability execution type.
    pub capability_type: CapabilityType,

    /// Biscuit capability scopes required for execution.
    #[serde(default)]
    pub scopes_required: Vec<CapabilityScope>,

    /// Exposed tools defined by this capability.
    #[serde(default)]
    pub tools: Vec<CanonicalToolDefinition>,

    /// Extensible metadata (tags, repository, documentation, license, etc.).
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
}

impl CapabilityManifest {
    /// Creates a new capability manifest.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        version: semver::Version,
        description: impl Into<String>,
        author: impl Into<String>,
        capability_type: CapabilityType,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version,
            description: description.into(),
            author: author.into(),
            capability_type,
            scopes_required: Vec::new(),
            tools: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Adds a required capability scope.
    pub fn with_scope(mut self, scope: CapabilityScope) -> Self {
        self.scopes_required.push(scope);
        self
    }

    /// Adds a tool definition.
    pub fn with_tool(mut self, tool: CanonicalToolDefinition) -> Self {
        self.tools.push(tool);
        self
    }

    /// Adds a metadata key-value pair.
    pub fn with_metadata(mut self, key: impl Into<String>, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Validates the manifest and all contained tool definitions.
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_capability_id(&self.id)?;
        if self.name.trim().is_empty() {
            return Err(ValidationError::EmptyField {
                field: "name",
                context: format!("capability '{}'", self.id),
            });
        }
        if self.description.trim().is_empty() {
            return Err(ValidationError::EmptyField {
                field: "description",
                context: format!("capability '{}'", self.id),
            });
        }

        // Validate uniqueness and correctness of tool definitions
        let mut tool_names = HashSet::new();
        for tool in &self.tools {
            if !tool_names.insert(&tool.name) {
                return Err(ValidationError::DuplicateToolName {
                    capability_id: self.id.clone(),
                    name: tool.name.clone(),
                });
            }
            tool.validate()?;
        }

        Ok(())
    }

    /// Finds a tool by name.
    pub fn find_tool(&self, name: &str) -> Option<&CanonicalToolDefinition> {
        self.tools.iter().find(|t| t.name == name)
    }

    /// Converts the manifest into a runtime `CapabilityDescriptor`.
    pub fn to_descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::from_manifest(self.clone())
    }

    /// Parses a manifest from a TOML string.
    pub fn from_toml(toml_str: &str) -> std::result::Result<Self, toml::de::Error> {
        toml::from_str(toml_str)
    }

    /// Parses a manifest from a JSON string.
    pub fn from_json(json_str: &str) -> std::result::Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }
}

/// Runtime descriptor representing a loaded, active capability in the registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityDescriptor {
    /// Canonical capability identifier.
    pub id: String,

    /// Human-readable display name.
    pub name: String,

    /// SemVer version.
    pub version: semver::Version,

    /// Detailed description.
    pub description: String,

    /// Author or publisher identity.
    pub author: String,

    /// Underlying capability execution type.
    pub capability_type: CapabilityType,

    /// Biscuit capability scopes required for execution.
    #[serde(default)]
    pub scopes_required: Vec<CapabilityScope>,

    /// Exposed tools defined by this capability.
    #[serde(default)]
    pub tools: Vec<CanonicalToolDefinition>,

    /// Extensible metadata.
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
}

impl CapabilityDescriptor {
    /// Constructs a descriptor from an evaluated manifest.
    pub fn from_manifest(manifest: CapabilityManifest) -> Self {
        Self {
            id: manifest.id,
            name: manifest.name,
            version: manifest.version,
            description: manifest.description,
            author: manifest.author,
            capability_type: manifest.capability_type,
            scopes_required: manifest.scopes_required,
            tools: manifest.tools,
            metadata: manifest.metadata,
        }
    }

    /// Validates descriptor integrity.
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_capability_id(&self.id)?;
        if self.name.trim().is_empty() {
            return Err(ValidationError::EmptyField {
                field: "name",
                context: format!("capability descriptor '{}'", self.id),
            });
        }

        let mut tool_names = HashSet::new();
        for tool in &self.tools {
            if !tool_names.insert(&tool.name) {
                return Err(ValidationError::DuplicateToolName {
                    capability_id: self.id.clone(),
                    name: tool.name.clone(),
                });
            }
            tool.validate()?;
        }

        Ok(())
    }

    /// Finds a tool by name.
    pub fn find_tool(&self, name: &str) -> Option<&CanonicalToolDefinition> {
        self.tools.iter().find(|t| t.name == name)
    }

    /// Checks whether this capability requires a specific Biscuit scope.
    pub fn requires_scope(&self, scope: &CapabilityScope) -> bool {
        self.scopes_required.contains(scope)
    }
}
