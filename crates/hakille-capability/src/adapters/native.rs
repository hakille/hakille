//! Native in-process capability adapter.
//!
//! Provides zero-overhead in-process capability execution with panic isolation
//! and structured error reporting.

use async_trait::async_trait;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::CapabilityError;
use crate::execution::{CapabilityExecutor, ExecutionContext};
use crate::types::{
    CanonicalContentBlock, CanonicalToolCall, CanonicalToolDefinition, CanonicalToolResult,
    CapabilityManifest, CapabilityScope, CapabilityType, ExecutionMetadata, RiskTier,
};

/// Type definition for a synchronous native tool handler.
pub type NativeHandlerFn = Arc<dyn Fn(Value) -> Result<Value, String> + Send + Sync + 'static>;

/// Represents a pure-Rust in-process tool.
#[derive(Clone)]
pub struct NativeTool {
    /// Canonical tool definition schema.
    pub definition: CanonicalToolDefinition,
    /// Risk tier associated with this native tool.
    pub risk_tier: RiskTier,
    /// Required security scope, if any.
    pub required_scope: Option<CapabilityScope>,
    /// Execution callback closure.
    pub handler: NativeHandlerFn,
}

/// Native capability adapter implementing [`CapabilityExecutor`].
#[derive(Clone, Default)]
pub struct NativeToolsAdapter {
    tools: HashMap<String, NativeTool>,
}

impl NativeToolsAdapter {
    /// Creates a new empty native adapter.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Creates an adapter pre-populated with standard system and cognitive tools.
    pub fn with_standard_tools() -> Self {
        let mut adapter = Self::new();

        // 1. System Info (Tier 0 ReadOnly)
        adapter.register_tool(
            CanonicalToolDefinition::new(
                "system_info",
                "Retrieves host operating system information, architecture, and timestamp",
                json!({
                    "type": "object",
                    "properties": {}
                }),
            ),
            RiskTier::Tier0ReadOnly,
            None,
            |_args| {
                Ok(json!({
                    "os": std::env::consts::OS,
                    "arch": std::env::consts::ARCH,
                    "family": std::env::consts::FAMILY,
                    "status": "operational"
                }))
            },
        );

        // 2. Calculator (Tier 0 ReadOnly)
        adapter.register_tool(
            CanonicalToolDefinition::new(
                "calculate_expression",
                "Evaluates a basic arithmetic expression (addition, subtraction, multiplication, division)",
                json!({
                    "type": "object",
                    "properties": {
                        "op": {
                            "type": "string",
                            "enum": ["add", "sub", "mul", "div"]
                        },
                        "a": { "type": "number" },
                        "b": { "type": "number" }
                    },
                    "required": ["op", "a", "b"]
                }),
            ),
            RiskTier::Tier0ReadOnly,
            None,
            |args| {
                let op = args.get("op").and_then(|v| v.as_str()).ok_or("Missing 'op'")?;
                let a = args.get("a").and_then(|v| v.as_f64()).ok_or("Missing 'a'")?;
                let b = args.get("b").and_then(|v| v.as_f64()).ok_or("Missing 'b'")?;

                let res = match op {
                    "add" => a + b,
                    "sub" => a - b,
                    "mul" => a * b,
                    "div" => {
                        if b == 0.0 {
                            return Err("Division by zero".to_string());
                        }
                        a / b
                    }
                    _ => return Err(format!("Unsupported operator '{op}'")),
                };

                Ok(json!({ "result": res }))
            },
        );

        // 3. VFS Workspace Read (Tier 0 ReadOnly)
        adapter.register_tool(
            CanonicalToolDefinition::new(
                "vfs_read",
                "Reads metadata or content from a virtual filesystem workspace path",
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            ),
            RiskTier::Tier0ReadOnly,
            Some(CapabilityScope::Filesystem),
            |args| {
                let path = args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'path'")?;
                if path.contains("..") {
                    return Err("Path traversal forbidden: '..' not permitted".to_string());
                }
                Ok(json!({
                    "path": path,
                    "exists": true,
                    "size_bytes": 1024,
                    "content": format!("[VFS mock content for {path}]")
                }))
            },
        );

        // 4. Critical Action (Tier 3 CriticalExecution)
        adapter.register_tool(
            CanonicalToolDefinition::new(
                "emergency_shred",
                "Executes digital twilight cryptographic shredding on memory vaults",
                json!({
                    "type": "object",
                    "properties": {
                        "vault_id": { "type": "string" },
                        "confirm": { "type": "boolean" }
                    },
                    "required": ["vault_id", "confirm"]
                }),
            ),
            RiskTier::Tier3CriticalExecution,
            Some(CapabilityScope::FullSystem),
            |args| {
                let vault_id = args
                    .get("vault_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'vault_id'")?;
                let confirm = args
                    .get("confirm")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if !confirm {
                    return Err("Shredding aborted: explicit confirm=true required".to_string());
                }
                Ok(json!({
                    "shredded_vault": vault_id,
                    "entropy_overwritten": true,
                    "status": "purged"
                }))
            },
        );

        adapter
    }

    /// Registers a new native tool handler.
    pub fn register_tool<F>(
        &mut self,
        definition: CanonicalToolDefinition,
        risk_tier: RiskTier,
        required_scope: Option<CapabilityScope>,
        handler: F,
    ) where
        F: Fn(Value) -> Result<Value, String> + Send + Sync + 'static,
    {
        let name = definition.name.clone();
        self.tools.insert(
            name,
            NativeTool {
                definition,
                risk_tier,
                required_scope,
                handler: Arc::new(handler),
            },
        );
    }

    /// Returns all registered tool definitions.
    pub fn definitions(&self) -> Vec<CanonicalToolDefinition> {
        self.tools.values().map(|t| t.definition.clone()).collect()
    }

    /// Generates a capability manifest representing these native tools.
    pub fn to_manifest(&self, id: &str, name: &str) -> CapabilityManifest {
        let mut manifest = CapabilityManifest::new(
            id,
            name,
            semver::Version::new(1, 0, 0),
            "Pure-Rust native capability provider for sovereign system tools",
            "Hakille Systems",
            CapabilityType::Native,
        );

        for tool in self.tools.values() {
            manifest = manifest.with_tool(tool.definition.clone());
            if let Some(scope) = &tool.required_scope
                && !manifest.scopes_required.contains(scope)
            {
                manifest = manifest.with_scope(scope.clone());
            }
        }

        manifest
    }
}

#[async_trait]
impl CapabilityExecutor for NativeToolsAdapter {
    async fn execute(
        &self,
        call: &CanonicalToolCall,
        _context: &ExecutionContext,
    ) -> Result<CanonicalToolResult, CapabilityError> {
        let tool = self
            .tools
            .get(&call.tool_name)
            .ok_or_else(|| CapabilityError::ToolNotFound(call.tool_name.clone()))?;

        // Guard execution in panic-safe boundary
        let handler = Arc::clone(&tool.handler);
        let args = call.arguments.clone();
        let call_id = call.call_id.clone();
        let risk_tier = tool.risk_tier;

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || handler(args)));

        let metadata = ExecutionMetadata::new(0, risk_tier);

        match result {
            Ok(Ok(val)) => Ok(CanonicalToolResult::success(
                call_id,
                vec![CanonicalContentBlock::text(
                    serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string()),
                )],
                metadata,
            )),
            Ok(Err(err_msg)) => Ok(CanonicalToolResult::error(call_id, err_msg, metadata)),
            Err(_) => Err(CapabilityError::ExecutionFailed {
                tool_name: call.tool_name.clone(),
                message: "Native tool execution panicked inside boundary".to_string(),
            }),
        }
    }
}
