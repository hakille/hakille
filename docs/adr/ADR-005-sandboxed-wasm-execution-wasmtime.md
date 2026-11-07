# ADR-005: Sandboxed WASM Execution via Wasmtime 48 with Fuel Metering & 5ms Epoch Preemption

- **Status**: Ratified
- **Date**: 2026-11-07
- **Deciders**: Systems Engineer, Runtime Sandboxing Lead
- **Target Module**: `crates/hakille-capability/src/adapters/wasm/`

## Context & Problem Statement

Hakille requires the ability to execute untrusted third-party capability plugins locally on user machines without risking host crashes, memory leaks, or denial-of-service via infinite computation loops.
Existing WASM execution in `crates/hakille-agents` included minimal runner harnesses. To support modern, modular plugins, Hakille requires:
1. WebAssembly Component Model (WASI Preview 2) support.
2. Fuel metering to deterministically bound CPU instructions.
3. Cooperative epoch interruption to preempt non-yielding guest loops.
4. Linear memory ceilings enforced by hardware MMU guard pages.
5. Anti-SSRF network validation at the socket level.

## Decision Drivers

- **Crash Sandboxing**: A WASM trap must never panic the host daemon.
- **Sub-50 µs Cold Starts**: Ahead-Of-Time (AOT) precompilation to native machine code (`.cwasm`).
- **Deterministic Resource Bounding**: Strict memory caps (64 MiB default) and CPU fuel limits.
- **Hardware Isolation**: MMU guard pages preventing out-of-bounds pointer tampering.

## Considered Alternatives

1. **Subprocesses for Every Plugin (`fork`/`exec`)**:
   - *Cons*: Heavy memory overhead (~20–50 MiB/process), high startup latency (~10–30 ms), complex IPC.
   - *Verdict*: Reserved for external MCP servers; rejected for lightweight plugins.
2. **Embedded JS Engines (v8 / QuickJS)**:
   - *Cons*: Large C++ dependencies or lack of CPU fuel bounding, memory safety concerns.
   - *Verdict*: Rejected.
3. **Wasmtime 48 Component Model with Fuel & Epoch Preemption**:
   - *Verdict*: **Adopted**.

## Decision & Technical Specification

The WASM execution engine is configured with:
1. **Hardware Memory Protection**:
   - Static linear memory maximum: 64 MiB (`StoreLimits`).
   - Static memory guard size: 2 MiB guard zone catching out-of-bounds access with zero CPU overhead.
2. **Dual Compute Bounding**:
   - Fuel Metering: `consume_fuel(true)`. Traps with `CapabilityError::FuelExhausted` when fuel reaches 0.
   - Epoch Preemption: Background timer ticks engine epoch every 5 ms. Interrupts tight infinite loops even if guest bypasses fuel branches.
3. **WASI Preview 2 WIT Interface**:
   - Standard WIT package `hakille:capability@0.1.0` exporting `describe()` and `execute(req) -> resp`.
4. **Cranelift AOT Precompilation**:
   - Compiles components to `.cwasm` at installation time, enabling sub-50 µs invocation latency.

## Consequences

- **Positive**: Complete hardware/software sandboxing; microsecond cold-start latency; deterministic resource bounding.
- **Negative**: Background thread ticking epoch counter; requires Cranelift compilation on target host.
