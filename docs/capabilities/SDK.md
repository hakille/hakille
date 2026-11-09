# Hakille Capability Development Kit (CDK / SDK)

**Document ID**: HAKILLE-CDK-GUIDE-001  
**MSRV**: Rust 1.95 (Edition 2024)  
**Status**: Authoritative Developer & Extensibility Guide  

---

## 1. Overview & Core Philosophy

The **Hakille Universal Capability Architecture** allows third-party developers, system engineers, and AI researchers to extend Hakille without modifying the kernel core.

Every capability in Hakille conforms to a single, vendor-neutral Canonical Contract:
- **Zero Python Dependency**: All extensions execute in pure Rust, WebAssembly (sandboxed via `hakille-agents`), or via external JSON-RPC 2.0 subprocesses/HTTP (Model Context Protocol).
- **Least-Privilege by Design**: Capabilities possess zero ambient authority. Privileges are cryptographically granted via Biscuit v5 Datalog tokens.
- **Graduated Risk Governance (4 Tiers)**:
  - **Tier 0 (ReadOnly)**: Pure informational queries. Auto-granted silently.
  - **Tier 1 (StateMutating)**: Local workspace changes. Auto-granted with ambient UI HUD indicator.
  - **Tier 2 (ExternalNetwork)**: Outbound HTTP network requests subject to domain allowlists and anti-SSRF validation.
  - **Tier 3 (CriticalExecution)**: Shell commands, financial operations, cryptographic shredding. Requires mandatory **600ms hold-to-confirm** physical interaction on UI ActionCards.
- **Lossless Multi-Provider Translation**: Capabilities declared once are automatically translated into OpenAI Strict Function Calling, Anthropic Thinking Blocks & Tool Use, Google Gemini Function Declarations, and local quantized ChatML GGUF prompts.

---

## 2. Core Concepts & Data Structures

Located in crate `hakille-capability`:

### 2.1 Canonical Tool Definition
```rust
use hakille_capability::types::CanonicalToolDefinition;
use serde_json::json;

let tool = CanonicalToolDefinition::new(
    "calculate_expression",
    "Evaluates an arithmetic expression deterministically",
    json!({
        "type": "object",
        "properties": {
            "op": { "type": "string", "enum": ["add", "sub", "mul", "div"] },
            "a": { "type": "number" },
            "b": { "type": "number" }
        },
        "required": ["op", "a", "b"]
    }),
).with_strict(true);
```

### 2.2 Canonical Tool Call & Result
```rust
use hakille_capability::types::{CanonicalContentBlock, CanonicalToolCall, CanonicalToolResult, ExecutionMetadata, RiskTier};

// Invocation
let call = CanonicalToolCall::new("call_01", "calculate_expression", json!({"op": "mul", "a": 6.0, "b": 7.0}));

// Success Result
let result = CanonicalToolResult::success(
    call.call_id,
    vec![CanonicalContentBlock::text("{\"result\": 42.0}")],
    ExecutionMetadata::new(5, RiskTier::Tier0ReadOnly),
);
```

---

## 3. Developing Capabilities

### A. Pure-Rust Native Tools
Implement the `CapabilityExecutor` trait or register directly into `NativeToolsAdapter`:

```rust
use hakille_capability::adapters::NativeToolsAdapter;
use hakille_capability::types::{CanonicalToolDefinition, RiskTier};
use serde_json::json;

let mut adapter = NativeToolsAdapter::new();
adapter.register_tool(
    CanonicalToolDefinition::new(
        "system_ping",
        "Pings the host system and returns latency metrics",
        json!({"type": "object", "properties": {}}),
    ),
    RiskTier::Tier0ReadOnly,
    None,
    |_args| {
        Ok(json!({ "status": "online", "latency_us": 12 }))
    },
);
```

### B. Model Context Protocol (MCP 2024-11-05) Server
Expose capabilities over standard JSON-RPC 2.0 lines:

```rust
use hakille_capability::adapters::McpClientAdapter;
use tokio::io::duplex;

// Connect via child process stdio pipes:
// let transport = StdioTransport::spawn("npx", &["-y", "@modelcontextprotocol/server-postgres"], &env)?;
// let client = McpClientAdapter::new(Box::new(transport));

// Or in-memory duplex:
let (client_stream, server_stream) = duplex(65536);
let mut client = McpClientAdapter::from_duplex(client_stream);

// Handshake
client.initialize().await?;

// Discover
let tools = client.list_tools().await?;

// Convert to Universal Capability Manifest
let manifest = client.to_manifest("mcp:postgres", "Postgres MCP").await?;
```

### C. Declarative Agent Skills (`SKILL.md`)
Store procedural knowledge in standard markdown with YAML frontmatter:

```markdown
---
name: kubernetes_debugger
displayName: Kubernetes Sovereign Debugger
version: 1.0.0
description: Triage CrashLoopBackOff pods, inspect container logs, and check OOMKilled events.
scope: network
---

# Kubernetes Debugging Manual

1. Run `kubectl get pods -n <namespace>` to identify crashing pods.
2. Check `kubectl describe pod <name>` for exit code 137 (OOMKilled) or 1 (Application Error).
3. Extract previous container logs using `kubectl logs <name> --previous`.
4. Suggest memory limit increases if OOMKilled was detected.
```

Load progressively with `ProgressiveSkillsLoader`:
```rust
use hakille_capability::adapters::ProgressiveSkillsLoader;

let loader = ProgressiveSkillsLoader::new();
loader.load_skill_from_str(skill_md).await?;

// Stage 1: Catalog Index (~30 tokens)
let catalog = loader.catalog_definitions().await;

// Stage 2: Heuristic Injection
let prompt_instructions = loader.inject_instructions("kubernetes_debugger").await;

// Stage 3: Bundled Resource Fetching with path-traversal protection
let runbook = loader.fetch_resource("kubernetes_debugger", "runbooks/oom.md").await?;
```

---

## 4. Security & Physical Confirmation (Tier 3)

When declaring Tier 3 actions (`emergency_shred`, file deletion, shell commands), the UI Shell intercepts the action and presents an interactive ActionCard. Execution is blocked until a cryptographic `ActionConfirmationProof` with `physical_hold_duration_ms >= 600` is presented.

```rust
use hakille_capability::security::ActionConfirmationProof;
use chrono::Utc;

let action_hash = ActionConfirmationProof::compute_action_hash("emergency_shred", &args);
let proof = ActionConfirmationProof::new(
    "card_123",
    action_hash,
    "user_root",
    650, // Physical hold >= 600ms
    Utc::now(),
);
```

---

## 5. End-to-End Orchestration & Verification

```rust
use hakille_capability::adapters::{IntelligentToolOrchestrator, OrchestrationRequest};

let mut orchestrator = IntelligentToolOrchestrator::new(registry, root_key, Some(audit_adapter));
orchestrator.register_executor("calculate_expression", native_adapter);

let req = OrchestrationRequest {
    intent: "Multiply 12 by 3".to_string(),
    caller_token: None,
    explicit_tool_name: Some("calculate_expression".to_string()),
    arguments: json!({ "op": "mul", "a": 12.0, "b": 3.0 }),
    confirmation_proof: None,
    timeout_ms: 1000,
};

let outcome = orchestrator.execute(req).await?;
println!("Result: {}", outcome.tool_result.text_summary());
println!("Merkle Audit TX: {:?}", outcome.audit_tx_id);
```
