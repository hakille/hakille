# Project: Hakille Sovereign Cognitive OS Architecture & Experience

## Architecture
Hakille follows a strict sovereign, layered, headless-first architecture across 7 core subsystems:
1. `tokens/`: Single source of truth for design tokens (color, motion, type), checked via `cargo xtask tokens --check` and exported to Rust, Swift, Kotlin, and CSS.
2. `crates/hakille-design`: Pure Rust design system constants, palettes (Abyss dark `#0B0C0F`, Text `#ECEEF2`, Braise `#FF7A2F`), typography, WCAG AA contrast validation, Karplus-Strong sound synthesis, 21-wire kora geometry, and action card layouts.
3. `crates/hakille-core`: Foundational security, Biscuit Datalog capability tokens with monotonic attenuation, authorization contexts, VFS path normalization, Ed25519/ChaCha20-Poly1305 cryptography, confidence calculus ($0.6 \cdot \text{model} + 0.4 \cdot \text{graph}$), and domain thresholds.
4. `crates/hakille-memory`: Distributed Subject-Predicate-Object (SPO) semantic store, RAM knowledge graph (`KnowledgeGraph`), redb 2.0 persistent storage (`EdgeStore`) across 6 tables, hybrid HNSW vector indexing (`instant-distance`, 384-dim), declarative graph query DSL (`GraphQuery`), exponential decay engine, and `MemoryRuntime` facade.
5. `crates/hakille-ui` & `crates/hakille-ui-model`: Headless ambient presence (6 states), Spotlight HUD, environmental channel selector, silence budget, immutable snapshots (`StableSnapshot`), and 18 validated UI events.
6. `crates/hakille-ui-protocol` & `crates/hakille-ipc`: Versioned (v1) communication wire schema between kernel, daemon, and UI. Secure local-only transport (1 MiB framing, CSPRNG token file with 0600 permissions, constant-time verification, named pipe / domain socket separation, and `TokenDelta` streaming).
7. `crates/hakille-ui-platform` & `crates/hakille-ui-desktop`: Cross-platform host abstraction (`DesktopHost`) and native desktop shell running on `winit 0.30` + `wgpu 30` with `softbuffer 0.4` software fallback, `accesskit 0.24`, Win32 layered colorkey transparency, 720×64 HUD expanding to 720×360 consultation panels.
8. Substrate crates (`hakille-substrate-*`): 6 OS targets (Linux, Windows, macOS, Redox OS orbital, Mobile, Web WASM) with clean separation of privilege tiers (Native, Sovereign, Guest), real Seccomp/Namespaces/Cgroups v2 on Linux, and Redox microkernel packet routing.
9. `crates/hakille-agents`: Sandboxed agent runtime featuring pure AEl zero-import sandbox (`wasm_runner.rs`) with fuel metering and 5ms epoch preemption, scoped WASI preview-1 runtime (`runtime.rs`), Biscuit attenuation verification, quota tracking, and declarative reference agents.
10. `crates/hakille-perception`: Telemetry capture, Silero VAD v5, SenseVoice-Small speech recognition, 21-wire kora audio metering, pure-Rust CV algorithms, and ambient world model.
11. `crates/hakille-audit`: ACID redb audit ledger with SHA-256 hash chaining, Ed25519 signatures, capability token binding, and non-destructive action reversal via signed flags.
12. `crates/hakille-daemon`: System service (`hakilled`), multi-turn cognition, ActionCard proposal/approval, durable crash-safe outbox queuing (`outbox.json`), and WASM reference agent execution.

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Decoupled Token Tooling | Make token checking/exporting runnable via `xtask` without linking ONNX/inference C++ runtimes | M1 | Survey Explorer 2 |
| 2 | Functional & Glass Design Tokens | Add error, success, caution, acrylic blur, glass reflection, and interactive state tokens to `tokens/*.toml` | M1 | Survey Explorer 2 |
| 3 | Complete Multi-Target Exporters | Fix Swift, Kotlin, and CSS exporters in `xtask` to export cubic-bezier easings, font stacks, and typed color constructs | M1 | Survey Explorer 2 |
| 4 | Token-Generated Rust Sync | Ensure `hakille-design/src/generated.rs` stays 100% in sync with enriched `tokens/*.toml` with full test validation | M1 | Survey Explorer 2 |
| 5 | Protocol Waveform & Haptic Schemas | Add voice waveform meter telemetry and haptic trigger variants to `hakille-ui-protocol` | M2 | Survey Explorer 1 & 3 |
| 6 | Rich Action Card Attachments | Support rich attachments (code diff previews, memory provenance links, cryptographic audit badges) in protocol and viewmodels | M2 | Survey Explorer 1 |
| 7 | Protocol Versioning & Golden Snapshot | Verify protocol v1 compatibility and update `schema_snapshot.json` with golden tests | M2 | Survey Explorer 1 |
| 8 | Viewmodel & Snapshot Parity | Maintain `StableSnapshot` 100% field-by-field parity guarantees with new telemetry/features | M2 | Survey Explorer 1 |
| 9 | WidgetState Interactive Transitions | Add `Hover`, `Active`, and `Disabled` states to `WidgetState` in `hakille-ui-desktop` | M3 | Survey Explorer 2 |
| 10 | Specular Glass Edge Highlights | Implement 1px top specular glass highlight and acrylic depth rendering for HUD and Action Cards | M3 | Survey Explorer 2 |
| 11 | 21-Wire Kora Presence Waveform Protocol | Implement 21 animated kora string waveforms contract in design tokens and E2E verification | M3 | Survey Explorer 2 & 3 |
| 12 | 600ms Hold-to-Confirm Progress Ring | Implement circular Braise progress ring on Action Card validation button during 600ms hold | M3 | Survey Explorer 2 & 3 |
| 13 | Elimination of Hardcoded Magic Colors | Replace hardcoded `0xFF5A5A`, `CORAL_HEX`, and blur constants with design token references | M3 | Survey Explorer 2 |
| 14 | Desktop Window Dragging & Transparency | Maintain and harden Win32 layered colorkey (`0x010101`) and non-Windows opaque windowing with strict hit-testing | M4 | Survey Explorer 3 |
| 15 | Multi-Platform Host Abstraction | Enhance Linux (Wayland layer-shell & X11 fallback), macOS (NSPanel & CGEventTap), Redox (orbital:), Web (Canvas), Mobile | M4 | Survey Explorer 3 |
| 16 | AccessKit Tree & Screen-Reader Audit | Maintain 100% labeled interactive controls and screen-reader accessibility tree parity | M4 | Survey Explorer 1 & 3 |
| 17 | Quiescence & Battery Energy Budget | Guarantee 0-frame idle/hidden quiescence and 30 fps battery throttling | M4 | Survey Explorer 1 & 3 |
| 18 | E2E Requirement-Driven Test Suite | Implement comprehensive opaque-box test runner and test cases across Tiers 1-4 | E2E-Track | Dual Track |
| 19 | 100% E2E Test Suite Pass | Verify implementation against 100% of E2E test suite published in `TEST_READY.md` | M5 | Dual Track |
| 20 | Adversarial Coverage Hardening (Tier 5) | Adversarial challenger-driven stress testing and coverage gap elimination | M5 | Dual Track |
| 21 | Daemon Multi-Substrate Dispatch | Dynamically instantiate native OS substrate via `cfg(target_os)` in `hakille-daemon` instead of hardcoding `WindowsSubstrate` | M6 | Survey Explorer 2 |
| 22 | Headless Audio Fallback & Systemd Lifecycle | Add dummy/mock audio fallback in `hakille-audio` for headless environments and systemd daemonization support in `hakille-substrate-linux` | M6 | Survey Explorer 2 |
| 23 | redb GraphQuery Push-Down & Index Encryption | Implement range scan query pushdown in `EdgeStore` and protect prefix keys in redb tables | M7 | Survey Explorer 1 |
| 24 | Persistent Capability Revocation Store | Add redb-backed persistent `RevocationStore` in `hakille-core` / `hakille-audit` for Biscuit token revocation | M7 | Survey Explorer 1 |
| 25 | GLIBC 2.36 C23 Linker Compatibility Shim | Provide C23 symbol compatibility shim (`__isoc23_strtol`) to enable test binaries linking on Debian Bookworm | M8 | Survey Explorer 3 |
| 26 | 21-Wire Animated Kora Presence UI Component | Implement animated 21-wire kora presence bars in `crates/hakille-ui-desktop/src/components.rs` (`Presence::draw`) for Speaking state | M8 | Survey Explorer 3 |
| 27 | Full Sovereign OS Regression & Victory Audit Readiness | Run 100% verification across all 12 subsystems and 230 E2E tests, verifying zero regressions | M9 | Survey Explorers 1-3 |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Design Tokens, Multi-Target Exporters & Check Tooling | Decouple `xtask` token commands, enrich `tokens/*.toml`, complete Swift/Kotlin/CSS exporters, sync `hakille-design` | none | COMPLETE |
| M2 | Protocol & View-Model Feature Extension | Enrich `hakille-ui-protocol` and `hakille-ui-model` (waveform streaming, rich card attachments, diffs), update schema snapshot | M1 | COMPLETE |
| M3 | Aesthetic Elevation, Futuristic Components & Desktop Shell | Upgrade `hakille-ui-desktop` (interactive `WidgetState`, specular glass reflections, 21-wire kora presence protocol, 600ms ring) | M1, M2 | COMPLETE |
| M4 | Multi-Platform Substrates & Platform Abstraction Hardening | Platform host adapters (`hakille-ui-platform`, Linux, Windows, macOS, Redox, Web, Mobile), AccessKit audit, quiescence | M2, M3 | COMPLETE |
| M5 | 100% E2E Test Suite Pass & Adversarial Hardening | Pass 100% E2E tests (Tiers 1-4) from `TEST_READY.md` and adversarial hardening | M1-M4, TEST_READY | COMPLETE |
| E2E | E2E Testing Track (Parallel) | Requirement-driven test suite (Tiers 1-4: Feature, Boundary, Combinatorial, Real-World Scenarios) | none | COMPLETE |
| M6 | Daemon Multi-Substrate Dispatch, IPC & Lifecycle | Dynamic substrate detection (`cfg(target_os)`), headless audio fallback in `hakille-audio`, systemd daemonization integration, consolidate tray/hotkey abstractions | M1-M5 | COMPLETE |
| M7 | Cognitive Kernel & Memory Hardening | redb range scan pushdown for `GraphQuery`, salt-hashed prefix keys, persistent Biscuit revocation store in `hakille-core`/`hakille-audit` | M6 | COMPLETE |
| M8 | WASM Agent Runtime, Linker Shim & 21-Wire Kora UI Rendering | GLIBC 2.36 C23 linker compatibility shim, 21-wire kora presence bars in `Presence::draw`, agent resource quota verification | M6, M7 | COMPLETE |
| M9 | Sovereign Cognitive OS E2E Regression & Victory Audit | Run 100% tests across all crates, verify 230 E2E tests pass, zero warnings/errors, and notify Sentinel for Victory Audit | M6, M7, M8 | COMPLETE |

## Interface Contracts
### `crates/hakille-daemon` ↔ `crates/hakille-substrate-*`
- `hakille-daemon` instantiates the host substrate via `hakille_substrate::detect_substrate(authority.root_public_key())` using `cfg(target_os)`.
- Fallback to mock host only in explicit unit testing mode.

### `crates/hakille-memory` ↔ `crates/hakille-core`
- `GraphQuery` translates filter predicates directly into redb prefix scans (`SPO_TABLE`, `POS_TABLE`, `OSP_TABLE`).
- Capability tokens presented for memory mutations must be verified against `RevocationStore`.

### `crates/hakille-ui-desktop` ↔ `crates/hakille-design`
- `Presence::draw` for `PresenceState::Speaking` (3) renders 21 vertical wires (2.0 px wire, 3.0 px gap, total span 102 px).
- Viewports under 24 px fallback to 7 compact wires (`wire_count(px)`).

## Code Layout
- `tokens/`: `color.toml`, `motion.toml`, `type.toml`
- `crates/hakille-core/src/`: `auth.rs`, `capability.rs`, `crypto.rs`, `trust.rs`, `types.rs`, `message.rs`, `error.rs`
- `crates/hakille-memory/src/`: `graph.rs`, `triplet.rs`, `query_lang.rs`, `decay.rs`, `contradiction.rs`, `storage/edge.rs`, `vector/mod.rs`, `runtime.rs`
- `crates/hakille-agents/src/`: `wasm_runner.rs`, `runtime.rs`, `sandbox.rs`, `capability.rs`, `registry.rs`, `protocol.rs`
- `crates/hakille-perception/src/`: `audio/capture.rs`, `audio/vad.rs`, `audio/pipeline.rs`, `vision/cv.rs`, `world_model.rs`
- `crates/hakille-audit/src/`: `journal.rs`, `entry.rs`, `consent_registry.rs`
- `crates/hakille-substrate-*`: Linux, Windows, macOS, Redox, Mobile, Web substrate implementations
- `crates/hakille-daemon/src/`: `main.rs`, `ipc.rs`, `agent_runtime.rs`
- `crates/hakille-ui-desktop/src/`: `components.rs`, `renderer.rs`, `app.rs`, `theme.rs`, `a11y.rs`, `bin/desktop.rs`
- `tests/e2e/`: 230 requirement-driven E2E tests
