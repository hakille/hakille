# Project: Hakille Universal Capability, Tools & AI Integrations Architecture

## Architecture
Hakille Universal Capability Architecture unifies all forms of tool and agent extensibility into a sovereign, high-performance, cryptographically secure Rust platform. It bridges local cognitive models (System 2 Qwen 2.5, Candle, ONNX Runtime), external frontier APIs (OpenAI, Anthropic, Gemini), and diverse extension ecosystems without modifying the kernel core.

The architecture comprises 5 foundational layers:
1. **Universal Capability Contract & Data Model (`crates/hakille-capability`)**:
   - Canonical Tool Contract (`CanonicalToolDefinition`, `CanonicalToolCall`, `CanonicalToolResult`, `CanonicalContentBlock`, `ExecutionMetadata`).
   - Bidirectional lossless schema translation for OpenAI Strict Function Calling, Anthropic Thinking Blocks & Tool Use, Gemini Function Declarations, and local ChatML GGUF.
   - Capability Descriptor & Manifest schema (`CapabilityDescriptor`, `CapabilityType`, `CapabilityManifest`, `Version`, `Author`).
2. **Core Capability Engines (`crates/hakille-capability`)**:
   - `UniversalCapabilityRegistry`: Thread-safe, versioned repository managing registration, provenance, metadata, dependencies, availability, and lifecycle.
   - `CapabilityDiscoveryEngine`: Hybrid search combining semantic HNSW vector retrieval (MiniLM 384-dim embeddings via `hakille-memory`) and structured metadata/scope filtering to eliminate context window saturation.
   - `UniversalExecutionEngine`: Sandboxed execution orchestrator with fuel metering, timeout cancellation, concurrency limits, and error classification.
   - `PermissionSecurityEngine`: Sovereign defense-in-depth model combining Biscuit v5 Datalog monotonic capability attenuation, 4-tier risk classification (Tier 0 Read, Tier 1 Mutating, Tier 2 Network, Tier 3 Critical/CodeExec), anti-SSRF IP filtering, and zero-knowledge credential injection.
3. **Capability Adapters**:
   - `McpClientAdapter`: Zero-bloat, asynchronous Tokio-based Model Context Protocol client supporting stdio (child process pipes) and SSE transports, complete JSON-RPC 2.0 lifecycle, tools/resources/prompts primitives, and connection resilience.
   - `SkillsLoader`: Open standard `SKILL.md` parser with YAML frontmatter catalog indexing, 3-tier progressive loading, and instruction injection.
   - `NativeToolsAdapter`: Pure-Rust native tools executing in-process with minimal overhead for system, VFS filesystem, and memory graph operations.
   - `WasmCapabilityAdapter`: Wasmtime 48 sandboxed execution with fuel limits and epoch preemption.
4. **Kernel & Daemon Integration (`crates/hakille-kernel` & `crates/hakille-daemon`)**:
   - Seamless integration with `System2Engine` and `orchestrator.rs`: Tool discovery before inference, prompt augmentation, function calling loop, result feedback, and multi-turn tool chaining.
   - UI ActionCard integration: High-risk (Tier 3) capability invocations prompt the user with 600ms hold-to-confirm physical verification.
   - Cryptographic Audit Trail: Every invocation logged to `hakille-audit` redb ledger with SHA-256 Merkle chaining and Biscuit token hash.
5. **Requirement-Driven E2E Test & Benchmark Harness (`tests/`)**:
   - Comprehensive multi-tier test suite (Tiers 1–4) validating discovery, execution, permissions, MCP servers, skills, error recovery, and performance benchmarks.

---

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Architectural Decision Records (ADRs) | Produce ADR-001 through ADR-007 documenting core architectural choices, tradeoffs, and rejected alternatives | M1 | ORIGINAL_REQUEST §10.D |
| 2 | Canonical Tool Contract & Data Model | Define `CanonicalToolDefinition`, `CanonicalToolCall`, `CanonicalToolResult`, `CanonicalContentBlock`, and `ExecutionMetadata` in Rust | M1 | Spec Miner §5 |
| 3 | Lossless Provider Schema Translators | Implement bidirectional schema and call translators for OpenAI, Anthropic, Gemini, MCP, and GGUF ChatML | M1 | Spec Miner §4 |
| 4 | Capability Descriptor & Manifest Specification | Define universal descriptor, manifest format, provenance metadata, semantic tags, and version constraints | M1 | ORIGINAL_REQUEST §4, §5 |
| 5 | Universal Capability Registry | Centralized, thread-safe registry with registration, query, lifecycle state, and version resolution | M2 | ORIGINAL_REQUEST §5 |
| 6 | Hybrid Capability Discovery Engine | Multi-modal search engine combining HNSW vector similarity (MiniLM 384-dim) and structured attribute/scope filtering | M2 | ORIGINAL_REQUEST §5, §6 |
| 7 | Universal Execution Engine | Asynchronous tool execution runtime with timeouts, cancellation tokens, resource bounds, and error categorization | M2 | ORIGINAL_REQUEST §5 |
| 8 | Sovereign Permission & Security Engine | Biscuit v5 Datalog token enforcement, 4-tier risk classification, least-privilege checks, and anti-SSRF network validation | M2 | ORIGINAL_REQUEST §5, §7 |
| 9 | Zero-Knowledge Secret Egress Proxy | Safe credential management injecting API keys into outbound headers without exposing secrets to tool memory | M2 | WASM Explorer §4 |
| 10 | Cryptographic Audit Log Integration | Automatic logging of capability invocations to `hakille-audit` with SHA-256 Merkle chaining and token binding | M2 | Codebase Auditor §3 |
| 11 | Sovereign Tokio MCP Client Adapter | Full JSON-RPC 2.0 MCP client supporting stdio and SSE transports, tool calling, resource reading, and prompts | M3 | ORIGINAL_REQUEST §3.B, §8 |
| 12 | MCP Lifecycle, Reconnection & Error Resilience | MCP `initialize`, `ping`, graceful cancellation, exponential backoff reconnects, and `isError` payload handling | M3 | Spec Miner §2 |
| 13 | Agent Skills Loader & Progressive Loading | `SKILL.md` parser with frontmatter catalog indexing, 3-tier progressive context injection, and resource access | M3 | ORIGINAL_REQUEST §3.C, §8 |
| 14 | Native Rust Capability Adapter | Direct in-process adapter for foundational tools (VFS inspection, system info, memory graph queries) | M3 | ORIGINAL_REQUEST §4, §8 |
| 15 | Wasmtime Sandboxed Capability Adapter | WASI sandbox runner with fuel metering, epoch interruption, and memory limits | M3 | WASM Explorer §2 |
| 16 | Kernel & System 2 Function Calling Orchestration | Integrate capability discovery, tool calling, and result synthesis into `hakille-kernel`'s multi-turn reasoning loop | M3 | ORIGINAL_REQUEST §8 |
| 17 | UI ActionCard & 600ms Hold-to-Confirm Integration | Wire Tier 3 sensitive capability executions into Hakille desktop UI confirmation flows | M3 | Codebase Auditor §2 |
| 18 | E2E Testing Infrastructure & Runner | Test harness for capability discovery, execution, permissions, MCP, and skills | E2E | ORIGINAL_REQUEST §9 |
| 19 | Tier 1 Feature Coverage Tests | ≥5 isolated unit & integration tests per feature across all capability components | E2E | Project Pattern §Dual Track |
| 20 | Tier 2 Boundary & Negative Tests | Robustness tests for empty inputs, oversized schemas, malformed JSON-RPC, timeouts, cancelled tasks, and revoked tokens | E2E | Project Pattern §Dual Track |
| 21 | Tier 3 Combinatorial & Cross-Adapter Tests | Pairwise integration tests (e.g. MCP + Skill chaining, Biscuit attenuation on MCP, HNSW search + execution) | E2E | Project Pattern §Dual Track |
| 22 | Tier 4 Real-World Application Workloads | End-to-end cognitive scenarios: user query → semantic discovery → permission gate → tool execution → audit logging → response | E2E | Project Pattern §Dual Track |
| 23 | 100% E2E Test Suite Pass | Complete passing run of all E2E capability tests | M4 | Project Pattern §Implementation Track |
| 24 | Tier 5 Adversarial Coverage Hardening | White-box adversarial testing, edge-case hardening, and fuzzing of capability inputs and JSON-RPC parsing | M4 | Project Pattern §Implementation Track |
| 25 | Performance Benchmarks | Measure and document discovery latency, execution overhead, MCP throughput, and memory consumption | M4 | ORIGINAL_REQUEST §9 |
| 26 | Comprehensive Architecture Documentation & SDK Guide | Complete technical architecture reference, developer SDK documentation, and security specifications | M5 | ORIGINAL_REQUEST §10.C, §10.E |
| 27 | Real Demonstration Suites & Examples | Working standalone examples: real MCP server demo, functional Skill demo, and native capability workflow | M5 | ORIGINAL_REQUEST §10.G |
| 28 | Final Validation & Verification Report | Detailed validation report documenting test results, benchmark metrics, security posture, and future roadmap | M5 | ORIGINAL_REQUEST §10.H |

---

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Target Architecture, ADRs, Specs & Canonical Data Model | Produce ADRs (001–007), target architecture doc, and `crates/hakille-capability` foundation (Canonical Tool Contract, descriptor, manifest, provider translators) | none | COMPLETE |
| M2 | Core Capability Engines (Registry, Discovery, Execution, Security) | Implement `UniversalCapabilityRegistry`, `CapabilityDiscoveryEngine` (HNSW + filters), `UniversalExecutionEngine`, `PermissionSecurityEngine` (Biscuit v5 + 4-tier risk), and audit logging | M1 | COMPLETE |
| M3 | Capability Adapters & Ecosystem Integration | Implement `McpClientAdapter` (stdio/duplex/SSE), `ProgressiveSkillsLoader` (`SKILL.md`), `NativeToolsAdapter`, and `IntelligentToolOrchestrator` | M1, M2 | COMPLETE |
| E2E | E2E Requirement-Driven Test Suite Track (Parallel) | Standalone test infrastructure and Tiers 1–4 test cases for all capability features | M1, M2 | COMPLETE |
| M4 | 100% E2E Test Suite Pass & Adversarial Hardening (Tier 5) | Pass all Tier 1–5 tests, adversarial stress suites, and reproducible benchmarks | M3, E2E | COMPLETE |
| M5 | Final Documentation, Working Demos & Validation Report | Complete architecture documentation, Capability SDK, runnable showcase demo, and final validation report | M4 | COMPLETE |

---

## Interface Contracts
### `crates/hakille-capability` ↔ External AI Providers & Local GGUF
- `CanonicalToolDefinition`:
  ```rust
  pub struct CanonicalToolDefinition {
      pub name: String,
      pub description: String,
      pub parameters: serde_json::Value, // JSON Schema draft 2020-12
      pub strict: bool,
  }
  ```
- `CanonicalToolCall`:
  ```rust
  pub struct CanonicalToolCall {
      pub call_id: String,
      pub tool_name: String,
      pub arguments: serde_json::Value,
  }
  ```
- `CanonicalToolResult`:
  ```rust
  pub struct CanonicalToolResult {
      pub call_id: String,
      pub content: Vec<CanonicalContentBlock>,
      pub is_error: bool,
      pub metadata: ExecutionMetadata,
  }
  ```

### `crates/hakille-capability` ↔ `crates/hakille-core`
- Invocations must present a valid `CapabilityToken` (Biscuit v5) attenuated with the required `CapabilityScope` (e.g. `Filesystem`, `Network`, `GuiInput`) and action rights (`Read`, `Write`, `Execute`).
- `PermissionSecurityEngine` evaluates presented token against pinned root authority and verifies against `RevocationStore`.

### `crates/hakille-capability` ↔ `crates/hakille-audit`
- Successful and failed executions generate an `AuditEntry` with `ActionRecord`, `token_hash`, Merkle `prev_hash`, and Ed25519 signature before returning result to caller.

### `crates/hakille-capability` ↔ `crates/hakille-memory`
- `CapabilityDiscoveryEngine` leverages `instant-distance` HNSW index with 384-dimensional vectors generated by MiniLM-L6-v2 to perform sub-millisecond semantic search over registered capability descriptions.

---

## Code Layout
- `crates/hakille-capability/`:
  - `Cargo.toml`
  - `src/lib.rs`
  - `src/types.rs`: Canonical tool contracts, descriptors, manifests, execution metadata.
  - `src/translators/`: Bidirectional schema mappings (`openai.rs`, `anthropic.rs`, `gemini.rs`, `chatml.rs`).
  - `src/registry/`: `UniversalCapabilityRegistry`, versioning, catalog indexing.
  - `src/discovery/`: `CapabilityDiscoveryEngine`, HNSW vector indexing, structured filtering.
  - `src/execution/`: `UniversalExecutionEngine`, timeout, cancellation, fuel limits.
  - `src/security/`: `PermissionSecurityEngine`, Biscuit v5 verification, 4-tier risk classification, anti-SSRF.
  - `src/adapters/`:
    - `mcp/`: Async Tokio MCP client (stdio, sse, JSON-RPC 2.0).
    - `skills/`: `SKILL.md` parser, progressive loader.
    - `native/`: In-process native tools.
    - `wasm/`: Wasmtime 48 sandboxed execution.
- `crates/hakille-kernel/src/`:
  - `orchestrator.rs`: Dynamic tool discovery and function calling loop integration.
- `crates/hakille-daemon/src/`:
  - `capability_service.rs`: Daemon IPC exposure of capability registry.
- `tests/capability_e2e.rs`: End-to-end integration test suite.
- `benches/capability_bench.rs`: Criterion / standalone reproducible benchmarks.
- `docs/capabilities/`:
  - `ARCHITECTURE.md`: Complete architectural reference.
  - `ADR/`: Architecture Decision Records (001–007).
  - `SDK.md`: Capability Development Kit guide.
  - `VALIDATION_REPORT.md`: Comprehensive validation report.
- `examples/capabilities/`:
  - `mcp_server_demo/`: Working example MCP server.
  - `skills_demo/`: Example standalone skill.
