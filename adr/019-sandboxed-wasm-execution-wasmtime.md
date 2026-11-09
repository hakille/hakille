# ADR-019: Sandboxed WASM Architecture and Agent Execution Boundary

- **Status**: Ratified
- **Date**: 2026-11-07
- **Deciders**: Systems Engineer, Runtime Sandboxing Lead
- **Target Module**: `crates/hakille-agents` & `crates/hakille-capability`

## Context & Problem Statement

Hakille executes untrusted third-party capability plugins and declarative agent packages locally without risking host crashes, memory leaks, or denial-of-service via infinite computation loops.
In accordance with Hakille's Contrat d'honnêteté (`AGENTS.md`), execution boundaries must be clearly delineated:
1. WASM bytecode execution and fuel metering live in `crates/hakille-agents` (`wasm_runner.rs`, `runtime.rs`).
2. Capability discovery, schema normalization, and Biscuit authorization live in `crates/hakille-capability`.
3. Native tools, MCP stdio/SSE adapters, and `SKILL.md` loaders are handled directly in `crates/hakille-capability/src/adapters/`.

## Decision Drivers

- **Crash Sandboxing**: A WASM trap or guest failure must never panic the host daemon.
- **Deterministic Resource Bounding**: Strict memory caps and CPU fuel limits.
- **Fail-Closed Security**: Missing capabilities, invalid tokens, or unauthorized scopes are rejected immediately.
- **Contract Honesty**: No simulated execution or fake sandbox claims.

## Considered Alternatives

1. **Embedding full Wasmtime inside `crates/hakille-capability`**:
   - *Cons*: Duplicate runtime dependencies with `crates/hakille-agents`, larger binary footprint, fragmented execution isolation.
   - *Verdict*: Rejected. Sandboxed WASM execution remains consolidated in `crates/hakille-agents`.
2. **Subprocesses for All Capabilities**:
   - *Cons*: Heavy memory overhead (~20–50 MiB/process), high startup latency.
   - *Verdict*: Reserved for external MCP servers via stdio.
3. **Layered Separation (Capability Registry + Agent WASM Runtime)**:
   - *Verdict*: **Adopted**.

## Decision & Technical Specification

1. `crates/hakille-capability` manages the canonical tool contracts (`CanonicalToolDefinition`, `CanonicalToolCall`, `CanonicalToolResult`), the universal registry, and Biscuit v5 authorization verification.
2. Sandboxed agent execution is delegated to `crates/hakille-agents`, using fuel-metered execution and strict capability scopes.
3. Native operations in `crates/hakille-capability/src/adapters/native.rs` perform bounded, validated operations (e.g. max 64 KiB reads within workspace boundaries) without mocks.

## Consequences

- **Positive**: Clean separation of concerns; no duplicate WASM engines; strict compliance with the Contrat d'honnêteté.
- **Negative**: WASM capabilities require inter-crate coordination with `hakille-agents`.
