# E2E Test Infra: Hakille Universal Capability Architecture

## Test Philosophy
- Opaque-box, requirement-driven. Derived strictly from `ORIGINAL_REQUEST.md` and public standards (MCP specification, JSON Schema, Biscuit Datalog), independent of internal implementation artifacts.
- Methodology: Category-Partition + Boundary Value Analysis (BVA) + Pairwise Combinatorial Testing + Real-World Workload Testing.

## Feature Inventory
| # | Feature | Source (requirement) | Tier 1 | Tier 2 | Tier 3 |
|---|---------|---------------------|:------:|:------:|:------:|
| 1 | Canonical Tool Contract & Data Model | ORIGINAL_REQUEST §4, §8 | 5 | 5 | ✓ |
| 2 | Lossless Provider Schema Translators | ORIGINAL_REQUEST §3.D, §4 | 5 | 5 | ✓ |
| 3 | Universal Capability Registry | ORIGINAL_REQUEST §5 | 5 | 5 | ✓ |
| 4 | Hybrid Capability Discovery Engine | ORIGINAL_REQUEST §5, §6 | 5 | 5 | ✓ |
| 5 | Universal Execution Engine | ORIGINAL_REQUEST §5 | 5 | 5 | ✓ |
| 6 | Sovereign Permission & Security Engine | ORIGINAL_REQUEST §5, §7 | 5 | 5 | ✓ |
| 7 | Zero-Knowledge Secret Egress Proxy | ORIGINAL_REQUEST §7 | 5 | 5 | ✓ |
| 8 | Cryptographic Audit Log Integration | ORIGINAL_REQUEST §7 | 5 | 5 | ✓ |
| 9 | Tokio MCP Client Adapter (Stdio & SSE) | ORIGINAL_REQUEST §3.B, §8 | 5 | 5 | ✓ |
| 10 | MCP Lifecycle & Reconnection Resilience | ORIGINAL_REQUEST §3.B | 5 | 5 | ✓ |
| 11 | Agent Skills Loader & Progressive Loading | ORIGINAL_REQUEST §3.C, §8 | 5 | 5 | ✓ |
| 12 | Native Rust Capability Adapter | ORIGINAL_REQUEST §4, §8 | 5 | 5 | ✓ |
| 13 | Wasmtime Sandboxed Capability Adapter | ORIGINAL_REQUEST §3.E | 5 | 5 | ✓ |
| 14 | Kernel System 2 Function Calling Loop | ORIGINAL_REQUEST §8 | 5 | 5 | ✓ |
| 15 | UI ActionCard 600ms Hold Confirmation | ORIGINAL_REQUEST §7, §8 | 5 | 5 | ✓ |

## Test Architecture
- **Location**: `tests/capability_e2e.rs` and `crates/hakille-capability/tests/`
- **Invocation**: `cargo test -p hakille-capability` and `cargo test --test capability_e2e`
- **Pass/Fail Semantics**: All test assertions must pass with exit code 0. Zero panics, zero unhandled errors.

## Real-World Application Scenarios (Tier 4)
| # | Scenario | Features Exercised | Complexity |
|---|----------|--------------------|------------|
| 1 | Cognitive Assistant Multi-Tool Investigation | Hybrid Discovery, Native FS, Audit Log, System 2 Loop | High |
| 2 | MCP External Tool Integration & Error Self-Correction | MCP Client, Stdio Server, Tool Call, `isError` feedback | High |
| 3 | Untrusted Skill Loading with Capability Attenuation | Skills Loader, Progressive Loading, Biscuit Token Deny | High |
| 4 | High-Risk Action Guarding & Hold-to-Confirm | Risk Tier 3, UI Action Card Confirmation, Reversible Action | High |
| 5 | Cross-Protocol Capability Federation (MCP + Skill + Native) | MCP Adapter, Skills Loader, Native Adapter, Unified Registry | Very High |

## Coverage Thresholds
- **Tier 1 (Feature Coverage)**: ≥5 test cases per feature (15 features × 5 = 75 test cases minimum)
- **Tier 2 (Boundary & Corner Cases)**: ≥5 test cases per feature (15 features × 5 = 75 test cases minimum)
- **Tier 3 (Cross-Feature Combinations)**: ≥15 pairwise interaction test cases
- **Tier 4 (Real-World Application Scenarios)**: ≥8 complex application scenarios
- **Total Minimum Threshold**: ~173+ test cases
