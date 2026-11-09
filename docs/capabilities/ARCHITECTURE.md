# Hakille Universal Capability, Tools & AI Integrations Architecture

**Document ID**: HAKILLE-ARCH-CAPABILITY-001  
**Milestone**: Milestone 1 (Foundation & Canonical Contracts)  
**Status**: Authoritative Architectural Specification  
**MSRV**: Rust 1.95 (Rust Edition 2024)  

---

## 1. Architectural Philosophy & Sovereign Tenets

The Hakille Universal Capability Architecture transforms Hakille from a local AI assistant into a sovereign, extensible cognitive operating platform. Traditional AI tool-calling frameworks suffer from four systemic vulnerabilities:
1. **Context Window Exhaustion**: Monolithically stuffing hundreds of tool schemas into LLM prompts degrades model reasoning ("lost-in-the-middle"), balloons token costs, and induces latency.
2. **Ambient Authority & Confused Deputy**: Granting models ambient shell or filesystem privileges allows indirect prompt injection attacks to wipe user data or exfiltrate private credentials.
3. **Ecosystem Fragmentation**: Incompatible tool formats (OpenAI functions, Anthropic tool blocks, Gemini declarations, Model Context Protocol, local GGUFs, Agent Skills) lead to fragile glue code and vendor lock-in.
4. **Audit Deficits**: Unverifiable executions where tool invocations cannot be mathematically proven against user grants or cryptographically tied to immutable logs.

Hakille resolves these challenges by introducing a unified capability layer in pure Rust, adhering strictly to Hakille's core architectural tenets:
- **Zero-Python in Production**: No runtime Python interpreters or subprocess virtual environments.
- **Pure-Rust Sovereign Foundation**: Built on Tokio, Biscuit v5 Datalog capabilities, Wasmtime 48, redb 2.0, and Candle / ONNX Runtime.
- **Cryptographic Capability Attenuation**: Mathematical least-privilege delegation without central authorization bottlenecks.
- **Progressive Discovery & Execution**: Scalable discovery over thousands of tools with sub-millisecond retrieval and bounded context overhead.

---

## 2. Universal Capability Model & Canonical Contract

### 2.1 The Capability Concept
A Capability in Hakille unifies:
- **Native Rust Routines**: In-process high performance filesystem, system information, and spatial graph capabilities.
- **WebAssembly Components**: Sandboxed plugins executing in Wasmtime 48 (WASI Preview 2) with fuel bounding.
- **Model Context Protocol (MCP)**: Local and remote MCP servers connected via asynchronous stdio child process pipes or HTTP Server-Sent Events (SSE).
- **Agent Skills**: Procedural cognitive capabilities (`SKILL.md`) providing instructions, checklists, and scripts.
- **External REST APIs**: OpenAPI-compatible web services integrated through secure zero-knowledge egress gateways.

### 2.2 Canonical Tool Data Structures
Located in `crates/hakille-capability/src/types.rs`:
- **`CanonicalToolDefinition`**: The vendor-neutral declaration of a tool:
  - `name`: Normalized identifier (`[a-zA-Z0-9_-]`).
  - `description`: Explanatory documentation for models and human operators.
  - `parameters`: JSON Schema Draft 2020-12 input specification.
  - `strict`: Boolean indicating whether OpenAI-style strict schema enforcement is enabled.
- **`CanonicalToolCall`**: Universal invocation structure:
  - `call_id`: Unique invocation correlation ID.
  - `tool_name`: Target tool identifier.
  - `arguments`: Parsed JSON Value (always valid structured JSON).
- **`CanonicalToolResult`**: Universal result envelope:
  - `call_id`: Correlation ID matching `CanonicalToolCall`.
  - `content`: Vector of `CanonicalContentBlock` (Text, Image, Resource).
  - `is_error`: Boolean flag indicating execution failure without transport fault.
  - `metadata`: `ExecutionMetadata` (duration, Wasm fuel, memory, evaluated risk tier, audit entry ID).

### 2.3 Lossless Bidirectional Schema Normalization
Frontier and local LLM engines require disparate wire formats. The translation layer (`crates/hakille-capability/src/translators/`) provides lossless mappings:
- **OpenAI**: Maps to `{"type": "function", "function": { ... }}` with strict mode mutation (`additionalProperties: false`, required null-unions) and stringified argument parsing.
- **Anthropic**: Maps to top-level `input_schema`, direct JSON objects, typed `tool_use` and `tool_result` content blocks, preserving interleaved thinking blocks byte-for-byte.
- **Google Gemini**: Maps to `functionDeclarations` with uppercase OpenAPI types (`OBJECT`, `STRING`, etc.), object response envelopes, and synthetic call ID generation.
- **Local GGUF / ChatML**: Generates `<tools>` system prompt blocks and parses `<tool_call>` streaming delimiters with codeblock stripping and alias normalization.

---

## 3. Core Capability Engines (`crates/hakille-capability`)

```
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                        UNIVERSAL CAPABILITY LAYER (`hakille-capability`)               │
│                                                                                        │
│  ┌────────────────────────┐  ┌────────────────────────┐  ┌────────────────────────┐   │
│  │   Universal Registry   │  │   Hybrid Discovery     │  │   Execution Engine     │   │
│  │ - Versioned repository │  │ - MiniLM 384-dim HNSW  │  │ - Timeouts & Cancel    │   │
│  │ - Descriptors/Manifests│  │ - Structured filters   │  │ - Concurrency limits   │   │
│  │ - Lifecycle management │  │ - Dynamic Top-K        │  │ - Error classification │   │
│  └────────────────────────┘  └────────────────────────┘  └────────────────────────┘   │
│                                           │                                            │
│  ┌────────────────────────────────────────┴─────────────────────────────────────────┐   │
│  │                     Lossless Bidirectional Schema Translators                    │   │
│  │      OpenAI Strict  │  Anthropic Thinking  │  Gemini Decs  │  ChatML GGUF        │   │
│  └──────────────────────────────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

### 3.1 Universal Capability Registry
- Thread-safe repository indexing active capabilities by ID, tool name, semantic tags, and required Biscuit scopes.
- Manages static `CapabilityManifest` specifications and runtime `CapabilityDescriptor` state.

### 3.2 Hybrid Discovery Engine
- **Pre-Filtering**: Evaluates caller Biscuit tokens against tool requirements, eliminating inaccessible tools before vector search.
- **Semantic ANN**: Generates 384-dimensional dense vectors using MiniLM-L6-v2 ONNX embeddings. Queries an HNSW index to retrieve Top-$K$ relevant tools exceeding cosine similarity threshold $\theta \ge 0.65$.

### 3.3 Universal Execution Engine
- Enforces execution timeouts and cooperative cancellation tokens.
- Manages concurrency semaphores per tool and resource domain.
- Guarantees error classification where execution faults return `is_error: true` with error details rather than panicking the host.

---

## 4. Capability Adapters & Runtimes

1. **Native Rust In-Process Adapter**:
   - Zero-overhead direct Rust function calls executed under `std::panic::catch_unwind`.
2. **Sovereign Tokio MCP Client/Server**:
   - JSON-RPC 2.0 protocol engine over `stdio` (isolated subprocess pipes with cleared environment) and `SSE` (HTTP streams).
3. **3-Tier Progressive Skills Loader**:
   - Frontmatter catalog indexing (~40 tokens/skill) -> Runtime body injection on intent match -> Sandboxed lazy resource fetching with strict path traversal guards.
4. **Sandboxed WASM Runner**:
   - Wasmtime 48 Component Model (WASI Preview 2) with Cranelift AOT compilation, 64 MiB linear memory limits, MMU guard zones, CPU fuel metering, and 5 ms epoch preemption.

---

## 5. Security, Governance & Verification

### 5.1 Biscuit v5 Datalog Attenuation
Every invocation requires a `CapabilityToken` verified against Hakille's pinned root public key (`PinnedRootPublicKey`). Each delegation step attenuates permissions by appending Datalog caveats.

### 5.2 4-Tier Risk Classification
- **Tier 0 (ReadOnly)**: Informational, idempotent queries. Auto-granted silently.
- **Tier 1 (StateMutating)**: Local modifications within safe workspace bounds. Auto-granted in trusted workspaces with ambient HUD indicator.
- **Tier 2 (ExternalNetwork)**: Outbound HTTP network requests. Verified against domain allowlists and anti-SSRF IP filters.
- **Tier 3 (CriticalExecution)**: Shell commands, file deletion, financial transactions, camera/biometrics. Requires mandatory human consent via **600ms hold-to-confirm progress ring** on UI ActionCards.

### 5.3 Zero-Knowledge Secret Egress Proxy
Capabilities requesting access to authenticated third-party APIs never receive raw secret bytes. The host proxy injects credentials directly into socket buffers at the transport layer.

### 5.4 Anti-SSRF Socket Protection
All outbound network connections reject loopback (`127.0.0.0/8`, `::1`), link-local (`169.254.0.0/16`), and private RFC 1918 subnets.

---

## 6. Cryptographic Audit & Forensics (`crates/hakille-audit`)

Every capability invocation commits an immutable audit record to `redb 2.0`:
- **SHA-256 Merkle Chaining**: Each entry stores `prev_hash` of the preceding transaction, ensuring that any historical alteration invalidates the entire chain.
- **Token Binding**: Each entry stores `token_hash = sha256_token_hash(&biscuit_bytes)`, mathematically linking the specific authorization token to the execution result.
- **Asymmetric Host Signatures**: Entries are signed with the sovereign daemon's Ed25519 host key.
- **Non-Destructive Reversals**: Undoing an action writes a signed `ReversalFlag`, preserving complete provenance.

---

## 7. Architecture Decision Records (ADRs)

| ADR ID | Title | Status |
|---|---|---|
| [ADR-001](../adr/ADR-001-universal-capability-registry.md) | Universal Capability Registry & Canonical Tool Contract | Ratified |
| [ADR-002](../adr/ADR-002-sovereign-tokio-mcp-client-server.md) | Sovereign Tokio-Based MCP Client/Server Implementation | Ratified |
| [ADR-003](../adr/ADR-003-3-tier-progressive-skills-loading.md) | 3-Tier Progressive Skills Loading | Ratified |
| [ADR-004](../adr/ADR-004-defense-in-depth-biscuit-v5-risk-classification.md) | Defense-in-Depth with Biscuit v5 Datalog Attenuation & 4-Tier Risk Classification | Ratified |
| [ADR-005](../adr/ADR-005-sandboxed-wasm-execution-wasmtime.md) | Sandboxed WASM Execution via Wasmtime 48 with Fuel Metering & 5ms Epoch Preemption | Ratified |
| [ADR-006](../adr/ADR-006-hybrid-capability-discovery-hnsw.md) | Hybrid Capability Discovery via MiniLM 384-dim HNSW Vector Index & Structured Filtering | Ratified |
| [ADR-007](../adr/ADR-007-cryptographic-audit-trail.md) | Cryptographically Bound Audit Trail in redb with SHA-256 Merkle Chaining & Token Binding | Ratified |
