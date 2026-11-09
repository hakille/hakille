# Project: Hakille Sovereign Cognitive OS Architecture & Experience

## Architecture
Hakille follows a strict sovereign, layered, headless-first architecture across core subsystems:
1. `tokens/`: Single source of truth for design tokens (color, motion, type), checked via `cargo xtask tokens --check` and exported to Rust, Swift, Kotlin, and CSS.
2. `crates/hakille-design`: Pure Rust design system constants, palettes (Abyss dark `#0B0C0F`, Text `#ECEEF2`, Braise `#FF7A2F`), typography, WCAG AA contrast validation, Karplus-Strong sound synthesis, 21-wire kora geometry, and action card layouts.
3. `crates/hakille-core`: Foundational security, Biscuit Datalog capability tokens with monotonic attenuation, authorization contexts, VFS path normalization, Ed25519/ChaCha20-Poly1305 cryptography, confidence calculus ($0.6 \cdot \text{model} + 0.4 \cdot \text{graph}$), and domain thresholds.
4. `crates/hakille-memory`: Distributed Subject-Predicate-Object (SPO) semantic store, RAM knowledge graph (`KnowledgeGraph`), redb 2.0 persistent storage (`EdgeStore`) across 6 tables, hybrid HNSW vector indexing (`instant-distance`, 384-dim), declarative graph query DSL (`GraphQuery`), exponential decay engine, and `MemoryRuntime` facade. Extended with 3D force-directed layout (`palace_3d.rs`), SHA-256 historical state blocks (`timeline.rs`), visual contradiction scanning, and certified cryptographic forgetting (`crypto_forget.rs`).
5. `crates/hakille-ui` & `crates/hakille-ui-model`: Headless ambient presence (6 states), Spotlight HUD, environmental channel selector, silence budget, immutable snapshots (`StableSnapshot`), and 18 validated UI events.
6. `crates/hakille-ui-protocol` & `crates/hakille-ipc`: Versioned (v1) communication wire schema between kernel, daemon, and UI. Secure local-only transport (1 MiB framing, CSPRNG token file with 0600 permissions, constant-time verification, named pipe / domain socket separation, `TokenDelta` streaming, and `GhostActionDto` intent anticipations).
7. `crates/hakille-ui-platform` & `crates/hakille-ui-desktop`: Cross-platform host abstraction (`DesktopHost`) and native desktop shell running on `winit 0.30` + `wgpu 30` with `softbuffer 0.4` software fallback, `accesskit 0.24`, Win32 layered colorkey transparency, 720×64 HUD, 720×360 consultation panels, GhostActionOverlay with 600ms circular ring, and 3D Memory Palace wgpu renderer.
8. Substrate crates (`hakille-substrate-*`): 6 OS targets (Linux, Windows, macOS, Redox OS orbital, Mobile, Web WASM) with clean separation of privilege tiers (Native, Sovereign, Guest), real Seccomp/Namespaces/Cgroups v2 on Linux, and Redox microkernel packet routing.
9. `crates/hakille-agents`: Sandboxed agent runtime featuring pure AEl zero-import sandbox (`wasm_runner.rs`) with fuel metering and 5ms epoch preemption, scoped WASI preview-1 runtime (`runtime.rs`), Biscuit attenuation verification, quota tracking, and Bicameral Dialectic Engine (`bicameral.rs`: Devil's Advocate vs Catalyst parallel execution and weighted arbitration).
10. `crates/hakille-perception`: Telemetry capture, Silero VAD v5, SenseVoice-Small speech recognition, 21-wire kora audio metering, pure-Rust CV algorithms, spatial tracking (`GazeTracker`, `MicroGestureDetector` for pinch/swipe/silence), and Sovereign Voiceprint Gate (`CamPlusPlusIdentifier` 192-d embedding).
11. `crates/hakille-audio` & `crates/hakille-voice`: Zero-allocation YIN pitch detector, multi-channel Binaural Delay-and-Sum beamforming with directional ITD steering and harmonic comb filter, diagnostic health status, and Adaptive Bayesian Prosody Engine ($0.6 \cdot \text{model} + 0.4 \cdot \text{graph}$ and urgency rate/pitch/breathiness transfer functions).
12. `crates/hakille-mesh`: Dedicated pure-Rust offline P2P mesh fabric (BLE GATT, Wi-Fi Direct SoftAP/P2P-GO, `VirtualAirGapBus`, frame chunking, and fail-closed zero-Internet link-local guard).
13. `crates/hakille-sync` & `crates/hakille-audit`: CRDT vector clock replication with persistent redb tombstones, Revocation-Wins distributed consent, ACID redb audit ledger with SHA-256 hash chaining, Ed25519 signatures, and Decentralized Zero-Knowledge Verifiable Compliance proofs (`ZkComplianceProof`).
14. `hakille-sdk` (standalone repository `hakille/hakille-sdk`): Declarative WASM agent SDK for authoring, packaging, and distributing sandboxed sovereign applications with `agent.toml` manifest, fuel limits, Biscuit capability attenuation, and `.hap` App Store packaging.
15. `crates/hakille-daemon`: System service (`hakilled`), multi-turn cognition, ActionCard proposal/approval, durable crash-safe outbox queuing (`outbox.json`), and WASM reference agent execution.
16. `crates/hakille-capability`: Sovereign capability execution framework implementing canonical tool definitions, schema translators (OpenAI, Anthropic, Gemini, ChatML), dynamic capability registry, hybrid discovery engine, Tokio MCP client adapter (stdio/SSE), progressive `SKILL.md` loader, guarded execution engine, and Biscuit v5 authorization security.

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
| 28 | Audio Diagnostics Alignment | Expose `take_error` and `status` on `Capture` in `hakille-audio` forwarding to `CaptureHealth` | M10 | Survey Explorer 3 |
| 29 | Living Voice Bayesian Adaptive Prosody | Dynamic transfer functions mapping $C = 0.6 \cdot \text{model} + 0.4 \cdot \text{graph}$ and urgency $U$ to speech rate, pitch, breathiness, and terminal interrogative rise in `hakille-voice` | M10 | Survey Explorer 1 |
| 30 | Binaural Sovereign Speaker Isolation | Multi-mic Delay-and-Sum beamforming with directional ITD steering ($\ge 12$ dB off-axis attenuation), YIN harmonic comb filter, and CAM++ 192-d voiceprint gate in `hakille-audio`/`hakille-perception` | M10 | Survey Explorer 1 |
| 31 | Ghost Actions & Focal Point Suggestion | `GhostActionDto` wire schema in `hakille-ui-protocol`, `GhostActionOverlay` floating component in `hakille-ui-desktop` with Braise `#FF7A2F` halo, and 600ms circular progress ring | M11 | Survey Explorer 1 |
| 32 | Zero-Touch Spatial Micro-Gestures | MicroGestureDetector in `hakille-spatial`/`hakille-perception` for Index-Thumb Pinch validation, Lateral Swipe evaporation, and Silence gesture over mouth | M11 | Survey Explorer 1 |
| 33 | 3D SPO Knowledge Graph Memory Palace | 3D force-directed node layout in `hakille-memory`, wgpu 3D rendering pipeline in `hakille-ui-desktop`, SHA-256 historical time-travel, visual contradiction highlights, and certified 1-click cryptographic zeroization with Ed25519 erasure certificates | M12 | Survey Explorer 2 |
| 34 | Bicameral Dialectic Engine | Parallel WASM sandbox execution of Devil's Advocate vs Catalyst in `hakille-agents`, fuel metering, multi-round rebuttal, and weighted Bayesian arbitration via $0.6 \cdot \text{model} + 0.4 \cdot \text{graph}$ | M12 | Survey Explorer 2 |
| 35 | Offline Air-Gap P2P Mesh Fabric | Pure-Rust `crates/hakille-mesh` implementing BLE GATT, Wi-Fi Direct SoftAP/P2P-GO, `VirtualAirGapBus`, frame chunking, and fail-closed zero-Internet link-local guard | M13 | Survey Explorer 3 |
| 36 | CRDT Semantic Memory & Consent Replication | Reconcile `CrdtSync` with redb `EdgeStore` (persistent tombstones table) and implement `ConsentCrdtDelta` with Revocation-Wins policy | M13 | Survey Explorer 3 |
| 37 | Decentralized ZK Compliance Audit | Zero-knowledge verifiable compliance scheme (`ZkComplianceProof`) with blinded commitments and Ed25519 signatures against the 10 Constitutional rules | M13 | Survey Explorer 3 |
| 38 | Declarative WASM Agent SDK & App Store | Create `crates/hakille-sdk` with `agent.toml` manifest parser, `#[hakille_agent]` macro, AEl ABI runtime bridge, Biscuit capability attenuation helpers, and `.hap` App Store packaging | M14 | Survey Explorer 2 |
| 39 | E2E Testing Track Expansion (Tiers 1-4) | Expand E2E test suite from 230 to 345+ tests across Tiers 1-4 covering all 5 pillars | M15 | Survey Explorer 3 |
| 40 | Sovereign OS Full Regression & Victory Claim | Execute workspace-wide verification across all crates and 345+ E2E tests, full forensic audit, and handoff to Sentinel | M15 | Orchestrator R3 |

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
| M10 | Audio Diagnostics Repair & Pillar 4 Living Voice & Binaural Isolation | Fix `Capture` diagnostics alignment in `hakille-audio`; implement Adaptive Bayesian Prosody Engine in `hakille-voice`; implement Binaural Beamformer, YIN harmonic comb filter, and Sovereign Voiceprint Gate in `hakille-audio`/`hakille-perception` | M9 | COMPLETE |
| M11 | Pillar 1: Ambient Perception, Ghost Actions & Zero-Touch Gestures | `GhostActionDto` wire schema in `hakille-ui-protocol`, `GhostActionOverlay` component in `hakille-ui-desktop` with Braise `#FF7A2F` halo & 600ms ring, and `MicroGestureDetector` in `hakille-spatial`/`hakille-perception` (pinch/swipe/silence) | M9 | COMPLETE |
| M12 | Pillar 2: 3D Memory Palace & Bicameral Dialectic Engine | 3D force-directed layout, SHA-256 time-travel blocks, visual contradictions, and certified erasure in `hakille-memory`; wgpu 3D palace renderer in `hakille-ui-desktop`; Bicameral Dialectic Engine in `hakille-agents` | M9 | COMPLETE |
| M13 | Pillar 3: Offline Air-Gap P2P Mesh Fabric, CRDT Sync & ZK Audit | Pure Rust `crates/hakille-mesh` (BLE/Wi-Fi Direct P2P transport, link-local fail-closed guard); redb tombstone persistence & Revocation-Wins consent CRDT; decentralized ZK compliance audit in `hakille-audit` | M9 | COMPLETE |
| M14 | Pillar 5: Declarative WASM Agent SDK & Sovereign App Store | Create `crates/hakille-sdk` in workspace; `agent.toml` manifest validator, `#[hakille_agent]` macros, AEl ABI runtime bridge, Biscuit capability attenuation, and `.hap` App Store packaging | M9 | COMPLETE |
| M15 | E2E Testing Track Expansion, Full Regression & Victory Claim | Expand `tests/e2e` across Tiers 1-4 for all 5 pillars (F21-F30, +115 tests, total 345+ tests); run 100% workspace tests; complete Reviewer, Challenger, and Forensic Audit; report victory to Sentinel | M10-M14 | COMPLETE |

## Interface Contracts
### `crates/hakille-audio` ↔ `crates/hakille-voice`
- `Capture` in `crates/hakille-audio/src/capture.rs` forwards `take_error()` and `status()` to its internal `CaptureHealth` diagnostics structure.
- `AdaptiveProsodyEngine` in `crates/hakille-voice/src/prosody.rs` consumes `ConfidenceLevel` ($0.6 \cdot \text{model} + 0.4 \cdot \text{graph}$) and urgency $U \in [0.0, 1.0]$ to parameterize `FormantVoice` (speech rate, pitch, breathiness, and terminal interrogative rise).

### `crates/hakille-ui-protocol` ↔ `crates/hakille-ui-desktop`
- `GhostActionDto` defines `{ id: Uuid, label: String, confidence: f32, focal_u: f32, focal_v: f32, hold_duration_ms: u32, action: ActionCard }`.
- `GhostActionOverlay` renders at normalized screen coordinates $(u, v)$ with Braise `#FF7A2F` halo and 600ms circular hold progress ring.

### `crates/hakille-memory` ↔ `crates/hakille-ui-desktop`
- `PalaceNode3D` and `PalaceEdge3D` provide 3D spatial coordinates computed by `PalaceLayoutEngine` (force-directed 3D layout).
- `crates/hakille-ui-desktop/src/palace_renderer.rs` compiles WGSL vertex/fragment shaders for rendering 3D nodes, relations, and pulsating contradiction highlights.
- Certified cryptographic forgetting emits `CertifiedErasureCertificate` signed by the authority's Ed25519 key and deletes triplets from both RAM and redb `EdgeStore`.

### `crates/hakille-agents` ↔ `crates/hakille-core`
- `BicameralEngine` instantiates two isolated WASM runners (Devil's Advocate and Catalyst) with strict fuel metering.
- Arbitration combines arguments via `ConfidenceLevel::compute(advocate_score, catalyst_score)`.

### `crates/hakille-mesh` ↔ `crates/hakille-sync` ↔ `crates/hakille-audit`
- `MeshTransport` abstracts BLE GATT and Wi-Fi Direct local transmissions with `VirtualAirGapBus` fallback for headless testing.
- `ConsentCrdtDelta` applies strict Revocation-Wins policy for distributed consent & Biscuit token revocation across nodes.
- `ZkComplianceProver` generates `ZkComplianceProof` using SHA-256 blinded commitments and Ed25519 signature assertions without revealing user queries or secret memory.

### `crates/hakille-sdk` ↔ `crates/hakille-agents`
- `crates/hakille-sdk` defines `agent.toml` manifest specification, `HakilleAgent` trait, and `#[hakille_agent]` procedural macro targeting pure AEl WASM sandbox.
- `.hap` packages contain signed manifest, compiled bytecode, and Biscuit Datalog authorization policies.

### `crates/hakille-capability` ↔ `crates/hakille-core` & `crates/hakille-audit`
- `CanonicalToolDefinition`, `CanonicalToolCall`, `CanonicalToolResult` provide provider-neutral capability representations.
- `PermissionSecurityEngine` validates Biscuit tokens with monotonic temporal and scope attenuation.
- `CapabilityAuditAdapter` guarantees fail-closed Merkle-chained execution audit records in redb.

## Code Layout
- `tokens/`: `color.toml`, `motion.toml`, `type.toml`
- `crates/hakille-core/src/`: `auth.rs`, `capability.rs`, `crypto.rs`, `trust.rs`, `types.rs`, `message.rs`, `error.rs`
- `crates/hakille-capability/src/`: `types.rs`, `registry/`, `discovery/`, `execution/`, `security/`, `adapters/`, `translators/`, `audit/`
- `crates/hakille-memory/src/`: `graph.rs`, `triplet.rs`, `query_lang.rs`, `decay.rs`, `contradiction.rs`, `storage/edge.rs`, `vector/mod.rs`, `runtime.rs`, `palace_3d.rs`, `timeline.rs`, `crypto_forget.rs`
- `crates/hakille-agents/src/`: `wasm_runner.rs`, `runtime.rs`, `sandbox.rs`, `capability.rs`, `registry.rs`, `protocol.rs`, `bicameral.rs`
- `crates/hakille-perception/src/`: `audio/capture.rs`, `audio/vad.rs`, `audio/pipeline.rs`, `audio/speaker_id.rs`, `vision/cv.rs`, `world_model.rs`, `cognition/gaze_tracker.rs`, `cognition/gesture.rs`
- `crates/hakille-audio/src/`: `capture.rs`, `diagnostics.rs`, `yin.rs`, `beamforming.rs`
- `crates/hakille-voice/src/`: `tts.rs`, `formant.rs`, `input.rs`, `prosody.rs`
- `crates/hakille-spatial/src/`: `gaze.rs`, `skeleton.rs`, `gesture.rs`
- `crates/hakille-mesh/src/`: `transport.rs`, `ble.rs`, `wifi_direct.rs`, `virtual_bus.rs`, `frame.rs`, `lib.rs`
- `crates/hakille-sync/src/`: `crdt.rs`, `lib.rs`
- `crates/hakille-audit/src/`: `journal.rs`, `entry.rs`, `consent_registry.rs`, `zk_compliance.rs`
- `crates/hakille-sdk/src/`: `manifest.rs`, `agent.rs`, `biscuit.rs`, `package.rs`, `lib.rs`
- `crates/hakille-substrate-*`: Linux, Windows, macOS, Redox, Mobile, Web substrate implementations
- `crates/hakille-daemon/src/`: `main.rs`, `ipc.rs`, `agent_runtime.rs`
- `crates/hakille-ui-protocol/src/`: `lib.rs`, `ghost_action.rs`
- `crates/hakille-ui-desktop/src/`: `components.rs`, `renderer.rs`, `app.rs`, `theme.rs`, `a11y.rs`, `ghost_overlay.rs`, `palace_renderer.rs`, `bin/desktop.rs`
- `tests/e2e/`: Requirement-driven E2E tests (expanded to 345+ tests across Tiers 1-4)
