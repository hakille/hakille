//! Universal Capability Adapters.
//!
//! Provides integration adapters for Native Rust routines, Model Context Protocol
//! (MCP) client/server sessions, Agent Skills (`SKILL.md`) with progressive loading,
//! and Intelligent Tool Orchestration.

pub mod mcp;
pub mod native;
pub mod orchestrator;
pub mod skills;

pub use mcp::{
    DuplexTransport, JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse,
    McpClientAdapter, McpInitializeResult, McpServerInfo, McpTransportStream, StdioTransport,
    run_mock_mcp_server,
};
pub use native::{NativeHandlerFn, NativeTool, NativeToolsAdapter};
pub use orchestrator::{IntelligentToolOrchestrator, OrchestrationRequest, OrchestrationResult};
pub use skills::{ProgressiveSkillsLoader, SkillDescriptor, SkillParser};
