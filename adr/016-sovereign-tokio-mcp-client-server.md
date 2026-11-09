# ADR 016: Sovereign Tokio-Based MCP Client/Server Implementation

- **Status**: Ratified
- **Date**: 2026-11-07
- **Deciders**: Systems Architect, Security Lead
- **Target Module**: `crates/hakille-capability/src/adapters/mcp/`

## Context & Problem Statement

The Model Context Protocol (MCP) by Anthropic is an open standard for AI tools, context resources, and prompts. Supporting MCP enables Hakille to connect to external servers (filesystem, SQLite, GitHub, Brave Search) or expose sovereign capabilities to other tools.
However, the upstream `modelcontextprotocol/rust-sdk` (`rmcp`) pulls in a heavy, unstable dependency graph:
- Pulls in `axum`, `tower`, `schemars`, and unpinned procedural macros.
- Has experienced multiple breaking changes between releases.
- Conflicts with Hakille's strict MSRV (1.95) and audited `deny.toml` license and advisory policies.
- Lacks integration with Hakille's Biscuit token attenuation and Redb cryptographic audit trail.

## Decision Drivers

- **Zero-Bloat Dependency Policy**: Avoid heavy web frameworks in core daemon runtimes.
- **Process Isolation**: Child MCP subprocesses must not inherit parent environment variables (preventing credential leakage).
- **Protocol Conformance**: Full compliance with MCP Specification 2024-11-05 (JSON-RPC 2.0, stdio and SSE transports).
- **Error Resilience**: Execution errors must be returned as `isError: true` tool results rather than RPC protocol crashes.

## Considered Alternatives

1. **Vendor Upstream `rmcp`**:
   - *Cons*: Heavy compilation overhead, transitive license violations, inability to intercept socket operations for Biscuit authentication.
   - *Verdict*: Rejected.
2. **Sovereign Tokio Implementation**:
   - *Pros*: ~1,200 lines of clean asynchronous Rust using `tokio::process`, `tokio::io`, and `reqwest`; zero external framework overhead; native Biscuit and Redb integration; 100% MSRV 1.95 compliant.
   - *Verdict*: **Adopted**.

## Decision & Technical Specification

Hakille implements a sovereign, self-contained MCP client supporting both `stdio` and `SSE` transports:
1. **Child Process Isolation**:
   - `tokio::process::Command` with `.env_clear()`.
   - Whitelist of minimal env vars (`PATH`, `HOME`, `TMPDIR`).
   - Scoped working directory.
2. **JSON-RPC 2.0 State Machine**:
   - States: Disconnected -> Initializing -> Initialized -> Active.
   - Initialization handshake (`initialize` -> `notifications/initialized`).
3. **Capability Mapping**:
   - `tools/list` -> Indexes tools as `CanonicalToolDefinition`.
   - `tools/call` -> Translates arguments, checks Biscuit Datalog caveats, returns `CanonicalToolResult`.
   - Handles `resources/*` and `prompts/*`.
4. **Error Normalization**:
   - Tool execution errors populate `content` and set `is_error: true`.

## Consequences

- **Positive**: Complete sovereignty; zero transitive dependency bloat; MSRV 1.95 and `deny.toml` compliant; memory-safe process isolation.
- **Negative**: Requires maintaining JSON-RPC 2.0 message parsing against future MCP specification updates.
