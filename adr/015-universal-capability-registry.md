# ADR 015: Universal Capability Registry & Canonical Tool Contract

- **Status**: Ratified
- **Date**: 2026-11-07
- **Deciders**: Principal Systems Architect, AI Infrastructure Lead, Security Engineer
- **Target Crate**: `crates/hakille-capability`

## Context & Problem Statement

Hakille previously relied on hardcoded action classifications in `crates/hakille-kernel/src/orchestrator.rs` (`infer_action_type` using fragile string heuristics like `"payer"`, `"supprimer"`), while agents in `agents/` were isolated sub-crates not dynamically discoverable or invocable by LLMs.
To establish a sovereign, extensible platform, Hakille requires:
1. A single canonical, vendor-neutral representation of any tool or capability regardless of backing implementation (Native Rust, WebAssembly component, MCP server via stdio/SSE, declarative Agent Skill `SKILL.md`, or external REST endpoint).
2. A thread-safe, centralized capability registry indexing tools, managing versioning, resolving scopes, and tracking active lifecycles.
3. Lossless bidirectional schema translation between this canonical model and the wire formats of frontier APIs (OpenAI, Anthropic, Gemini) and local GGUFs (ChatML).

## Decision Drivers

- **Zero Ambient Authority**: Capabilities must carry explicit required scopes and risk tier definitions.
- **Provider Agnosticism**: Kernel and orchestration logic must remain 100% decoupled from LLM provider quirks.
- **Strict Input Validation**: Enforce JSON Schema Draft 2020-12 to intercept malformed arguments before runtime execution.
- **Zero Runtime Reflection Overhead**: All core contracts must be statically typed in Rust with zero-copy deserialization where possible (`serde`).

## Considered Alternatives

1. **Direct Adoption of Anthropic MCP Schemas as Internal Core**:
   - *Pros*: Out-of-the-box alignment with external MCP tools.
   - *Cons*: Leaky abstraction. MCP schemas lack representation of Biscuit token requirements, Wasm fuel limits, risk tiers, and execution modes. OpenAI Strict mode features (nullable unions, `additionalProperties: false`) cannot be expressed without lossy hacks.
   - *Verdict*: Rejected as primary internal contract. MCP is treated as an adapter, not the core data model.
2. **Raw JSON / Dynamic Value Dispatch**:
   - *Pros*: Low upfront engineering cost.
   - *Cons*: Untyped errors, inability to statically analyze tool dependencies, high risk of panics, no compile-time contract safety.
   - *Verdict*: Rejected.
3. **Dedicated Canonical Tool Contract (`CanonicalToolDefinition`)**:
   - *Pros*: Full sovereignty, strictly typed, lossless mappings to all major providers, native integration with Hakille's security architecture.
   - *Verdict*: **Adopted**.

## Decision & Technical Specification

We establish `crates/hakille-capability` encapsulating:
- `CanonicalToolDefinition`: Normalized name, description, JSON Schema Draft 2020-12 parameters, and `strict` boolean flag.
- `CanonicalToolCall`: Universal tool invocation structure (`call_id`, `tool_name`, `arguments: Value`).
- `CanonicalToolResult`: Output envelope (`call_id`, `content: Vec<CanonicalContentBlock>`, `is_error: bool`, `metadata: ExecutionMetadata`).
- `CanonicalContentBlock`: Multimodal content blocks (`Text`, `Image`, `Resource`).
- `ExecutionMetadata`: Telemetry (`duration_ms`, `fuel_consumed`, `memory_bytes`, `risk_tier`, `audit_record_id`).
- `RiskTier`: 4-tier security classification (`Tier0ReadOnly`, `Tier1StateMutating`, `Tier2ExternalNetwork`, `Tier3CriticalExecution`).
- `CapabilityType`: Runtime type (`Native`, `McpStdio`, `McpSse`, `Wasm`, `Skill`, `Rest`).
- `CapabilityManifest` & `CapabilityDescriptor`: Declarative packaging and runtime registry representation.
- `UniversalCapabilityRegistry`: Thread-safe, concurrent registry indexing capabilities by ID, tool name, tag, and required Biscuit scope.

## Consequences

- **Positive**: Complete decoupling of cognitive models from capabilities; rigorous schema validation; foundation for sovereign multi-tool orchestration.
- **Negative**: Adds a translation step when interfacing with external model APIs.
