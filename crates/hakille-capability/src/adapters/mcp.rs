//! Sovereign Model Context Protocol (MCP) Client & Server Adapter.
//!
//! Implements JSON-RPC 2.0 wire framing over stdio and duplex pipes
//! conforming to the MCP 2024-11-05 specification.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};
use tokio::sync::Mutex;

use crate::error::CapabilityError;
use crate::execution::{CapabilityExecutor, ExecutionContext};
use crate::types::{
    CanonicalContentBlock, CanonicalToolCall, CanonicalToolDefinition, CanonicalToolResult,
    CapabilityManifest, CapabilityType, ExecutionMetadata, RiskTier,
};

/// JSON-RPC 2.0 Request envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 Response envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 Notification envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 Error payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// MCP Initialize result data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInitializeResult {
    pub protocol_version: String,
    pub capabilities: Value,
    pub server_info: McpServerInfo,
}

/// MCP Server Info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

/// Trait abstracting bidirectional framed I/O lines for MCP transports.
#[async_trait]
pub trait McpTransportStream: Send + Sync {
    async fn send_line(&mut self, line: &str) -> Result<(), CapabilityError>;
    async fn read_line(&mut self) -> Result<Option<String>, CapabilityError>;
}

/// In-memory Duplex transport stream.
pub struct DuplexTransport {
    reader: BufReader<tokio::io::ReadHalf<DuplexStream>>,
    writer: tokio::io::WriteHalf<DuplexStream>,
}

impl DuplexTransport {
    pub fn new(stream: DuplexStream) -> Self {
        let (read_half, write_half) = tokio::io::split(stream);
        Self {
            reader: BufReader::new(read_half),
            writer: write_half,
        }
    }
}

#[async_trait]
impl McpTransportStream for DuplexTransport {
    async fn send_line(&mut self, line: &str) -> Result<(), CapabilityError> {
        self.writer
            .write_all(line.as_bytes())
            .await
            .map_err(|e| CapabilityError::Transport(e.to_string()))?;
        self.writer
            .write_all(b"\n")
            .await
            .map_err(|e| CapabilityError::Transport(e.to_string()))?;
        self.writer
            .flush()
            .await
            .map_err(|e| CapabilityError::Transport(e.to_string()))?;
        Ok(())
    }

    async fn read_line(&mut self) -> Result<Option<String>, CapabilityError> {
        let mut line = String::new();
        match self.reader.read_line(&mut line).await {
            Ok(0) => Ok(None),
            Ok(_) => {
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                Ok(Some(line))
            }
            Err(e) => Err(CapabilityError::Transport(e.to_string())),
        }
    }
}

/// Child Process Stdio Transport Stream.
pub struct StdioTransport {
    child: tokio::process::Child,
    reader: BufReader<tokio::process::ChildStdout>,
    writer: tokio::process::ChildStdin,
}

impl StdioTransport {
    pub fn spawn(
        command: &str,
        args: &[&str],
        env_vars: &HashMap<String, String>,
    ) -> Result<Self, CapabilityError> {
        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .envs(env_vars);

        let mut child = cmd
            .spawn()
            .map_err(|e| CapabilityError::Transport(e.to_string()))?;
        let stdout = child.stdout.take().ok_or_else(|| {
            CapabilityError::Transport("Failed to capture child stdout".to_string())
        })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            CapabilityError::Transport("Failed to capture child stdin".to_string())
        })?;

        Ok(Self {
            child,
            reader: BufReader::new(stdout),
            writer: stdin,
        })
    }
}

#[async_trait]
impl McpTransportStream for StdioTransport {
    async fn send_line(&mut self, line: &str) -> Result<(), CapabilityError> {
        self.writer
            .write_all(line.as_bytes())
            .await
            .map_err(|e| CapabilityError::Transport(e.to_string()))?;
        self.writer
            .write_all(b"\n")
            .await
            .map_err(|e| CapabilityError::Transport(e.to_string()))?;
        self.writer
            .flush()
            .await
            .map_err(|e| CapabilityError::Transport(e.to_string()))?;
        Ok(())
    }

    async fn read_line(&mut self) -> Result<Option<String>, CapabilityError> {
        let mut line = String::new();
        match self.reader.read_line(&mut line).await {
            Ok(0) => Ok(None),
            Ok(_) => {
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                Ok(Some(line))
            }
            Err(e) => Err(CapabilityError::Transport(e.to_string())),
        }
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

/// Asynchronous Tokio MCP Client Adapter.
pub struct McpClientAdapter {
    transport: Arc<Mutex<Box<dyn McpTransportStream>>>,
    request_counter: AtomicU64,
    server_info: Option<McpServerInfo>,
    server_capabilities: Option<Value>,
}

impl McpClientAdapter {
    /// Creates a new MCP client over any boxed transport stream.
    pub fn new(transport: Box<dyn McpTransportStream>) -> Self {
        Self {
            transport: Arc::new(Mutex::new(transport)),
            request_counter: AtomicU64::new(1),
            server_info: None,
            server_capabilities: None,
        }
    }

    /// Creates an MCP client connected over an in-memory Duplex stream.
    pub fn from_duplex(stream: DuplexStream) -> Self {
        Self::new(Box::new(DuplexTransport::new(stream)))
    }

    /// Next unique JSON-RPC request ID.
    fn next_id(&self) -> u64 {
        self.request_counter.fetch_add(1, Ordering::SeqCst)
    }

    /// Sends a JSON-RPC request and awaits the correlating response.
    pub async fn send_request(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, CapabilityError> {
        let req_id = self.next_id();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(req_id),
            method: method.to_string(),
            params,
        };

        let raw_req = serde_json::to_string(&req).map_err(CapabilityError::Serialization)?;

        let mut lock = self.transport.lock().await;
        lock.send_line(&raw_req).await?;

        loop {
            let line = lock.read_line().await?.ok_or_else(|| {
                CapabilityError::Transport("MCP server closed connection unexpectedly".to_string())
            })?;

            if line.trim().is_empty() {
                continue;
            }

            let resp: JsonRpcResponse = serde_json::from_str(&line).map_err(|e| {
                CapabilityError::Custom(format!("Invalid JSON-RPC response: {e} ({line})"))
            })?;

            if resp.id == json!(req_id) {
                if let Some(err) = resp.error {
                    return Err(CapabilityError::ExecutionFailed {
                        tool_name: method.to_string(),
                        message: format!("MCP error {}: {}", err.code, err.message),
                    });
                }
                return Ok(resp.result.unwrap_or(Value::Null));
            }
        }
    }

    /// Sends a JSON-RPC notification (no response expected).
    pub async fn send_notification(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<(), CapabilityError> {
        let notif = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
        };

        let raw = serde_json::to_string(&notif).map_err(CapabilityError::Serialization)?;

        let mut lock = self.transport.lock().await;
        lock.send_line(&raw).await?;
        Ok(())
    }

    /// Performs the mandatory MCP `initialize` handshake.
    pub async fn initialize(&mut self) -> Result<McpInitializeResult, CapabilityError> {
        let init_params = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "roots": { "listChanged": true },
                "sampling": {}
            },
            "clientInfo": {
                "name": "hakille-kernel",
                "version": "0.1.0"
            }
        });

        let result = self.send_request("initialize", Some(init_params)).await?;
        let init_result: McpInitializeResult = serde_json::from_value(result)
            .map_err(|e| CapabilityError::Custom(format!("Malformed initialize result: {e}")))?;

        self.server_info = Some(init_result.server_info.clone());
        self.server_capabilities = Some(init_result.capabilities.clone());

        // Send notifications/initialized as required by MCP spec
        self.send_notification("notifications/initialized", None)
            .await?;

        Ok(init_result)
    }

    /// Discovers tools exposed by the MCP server (`tools/list`).
    pub async fn list_tools(&self) -> Result<Vec<CanonicalToolDefinition>, CapabilityError> {
        let res = self.send_request("tools/list", None).await?;
        let tools_val = res.get("tools").and_then(|v| v.as_array()).ok_or_else(|| {
            CapabilityError::Custom("MCP 'tools/list' response missing 'tools' array".to_string())
        })?;

        let mut definitions = Vec::new();
        for item in tools_val {
            let name = item
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let desc = item
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let schema = item
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object", "properties": {}}));

            definitions.push(CanonicalToolDefinition::new(name, desc, schema));
        }

        Ok(definitions)
    }

    /// Calls an MCP tool (`tools/call`).
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<CanonicalToolResult, CapabilityError> {
        let params = json!({
            "name": name,
            "arguments": arguments
        });

        let res = self.send_request("tools/call", Some(params)).await?;
        let is_error = res
            .get("isError")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut content_blocks = Vec::new();
        if let Some(content_arr) = res.get("content").and_then(|v| v.as_array()) {
            for block in content_arr {
                let b_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("text");
                match b_type {
                    "text" => {
                        let text = block
                            .get("text")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default();
                        content_blocks.push(CanonicalContentBlock::text(text));
                    }
                    "image" => {
                        let data = block
                            .get("data")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default();
                        let mime = block
                            .get("mimeType")
                            .and_then(|v| v.as_str())
                            .unwrap_or("image/png");
                        content_blocks.push(CanonicalContentBlock::image(mime, data));
                    }
                    "resource" => {
                        if let Some(res_obj) = block.get("resource") {
                            let uri = res_obj
                                .get("uri")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default();
                            let text = res_obj
                                .get("text")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            let mime = res_obj
                                .get("mimeType")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            content_blocks.push(CanonicalContentBlock::Resource {
                                uri: uri.to_string(),
                                mime_type: mime,
                                text,
                                blob_base64: None,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        let metadata = ExecutionMetadata::new(0, RiskTier::Tier2ExternalNetwork);

        Ok(CanonicalToolResult {
            call_id: format!("mcp_{}", self.next_id()),
            content: content_blocks,
            is_error,
            metadata,
        })
    }

    /// Pings the MCP server (`ping`).
    pub async fn ping(&self) -> Result<(), CapabilityError> {
        self.send_request("ping", None).await?;
        Ok(())
    }

    /// Converts this MCP connection into a `CapabilityManifest`.
    pub async fn to_manifest(
        &self,
        id: &str,
        name: &str,
    ) -> Result<CapabilityManifest, CapabilityError> {
        let tools = self.list_tools().await?;
        let s_info = self
            .server_info
            .as_ref()
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "MCP Server".to_string());

        let mut manifest = CapabilityManifest::new(
            id,
            name,
            semver::Version::new(1, 0, 0),
            format!("MCP-backed capability hosted by '{s_info}'"),
            s_info,
            CapabilityType::McpStdio,
        );

        for tool in tools {
            manifest = manifest.with_tool(tool);
        }

        Ok(manifest)
    }
}

#[async_trait]
impl CapabilityExecutor for McpClientAdapter {
    async fn execute(
        &self,
        call: &CanonicalToolCall,
        _context: &ExecutionContext,
    ) -> Result<CanonicalToolResult, CapabilityError> {
        let mut result = self
            .call_tool(&call.tool_name, call.arguments.clone())
            .await?;
        result.call_id = call.call_id.clone();
        Ok(result)
    }
}

// ============================================================================
// Embedded Mock MCP Server (for tests, benchmarks and self-contained demos)
// ============================================================================

/// Runs a mock MCP server on the server half of a Duplex stream.
pub async fn run_mock_mcp_server(stream: DuplexStream) {
    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    while let Ok(n) = reader.read_line(&mut line).await {
        if n == 0 {
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }

        if let Ok(req) = serde_json::from_str::<JsonRpcRequest>(trimmed) {
            let resp = match req.method.as_str() {
                "initialize" => json!({
                    "jsonrpc": "2.0",
                    "id": req.id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {
                            "tools": { "listChanged": false }
                        },
                        "serverInfo": {
                            "name": "mock-mcp-server",
                            "version": "1.0.0"
                        }
                    }
                }),
                "tools/list" => json!({
                    "jsonrpc": "2.0",
                    "id": req.id,
                    "result": {
                        "tools": [
                            {
                                "name": "mcp_echo",
                                "description": "Echoes back the input message with timestamp",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "message": { "type": "string" }
                                    },
                                    "required": ["message"]
                                }
                            },
                            {
                                "name": "mcp_get_weather",
                                "description": "Retrieves simulated weather conditions for a city",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "city": { "type": "string" }
                                    },
                                    "required": ["city"]
                                }
                            }
                        ]
                    }
                }),
                "tools/call" => {
                    let tool_name = req
                        .params
                        .as_ref()
                        .and_then(|p| p.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    let args = req
                        .params
                        .as_ref()
                        .and_then(|p| p.get("arguments"))
                        .cloned()
                        .unwrap_or(Value::Null);

                    match tool_name {
                        "mcp_echo" => {
                            let msg = args.get("message").and_then(|v| v.as_str()).unwrap_or("");
                            json!({
                                "jsonrpc": "2.0",
                                "id": req.id,
                                "result": {
                                    "content": [
                                        { "type": "text", "text": format!("Echo from MCP: {msg}") }
                                    ],
                                    "isError": false
                                }
                            })
                        }
                        "mcp_get_weather" => {
                            let city = args
                                .get("city")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Unknown");
                            json!({
                                "jsonrpc": "2.0",
                                "id": req.id,
                                "result": {
                                    "content": [
                                        { "type": "text", "text": format!("Weather in {city}: 21°C, Clear skies, Humidity 45%") }
                                    ],
                                    "isError": false
                                }
                            })
                        }
                        _ => json!({
                            "jsonrpc": "2.0",
                            "id": req.id,
                            "error": {
                                "code": -32601,
                                "message": format!("Tool not found: {tool_name}")
                            }
                        }),
                    }
                }
                "ping" => json!({
                    "jsonrpc": "2.0",
                    "id": req.id,
                    "result": {}
                }),
                _ => json!({
                    "jsonrpc": "2.0",
                    "id": req.id,
                    "error": {
                        "code": -32601,
                        "message": format!("Method not found: {}", req.method)
                    }
                }),
            };

            let out_str = serde_json::to_string(&resp).unwrap();
            let _ = write_half.write_all(out_str.as_bytes()).await;
            let _ = write_half.write_all(b"\n").await;
            let _ = write_half.flush().await;
        }

        line.clear();
    }
}
