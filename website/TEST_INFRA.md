# E2E Test Infra: Hakille Showcase Website Enrichment

## Test Philosophy
- **Requirement-Driven & Opaque-Box**: Tests are derived strictly from `ORIGINAL_REQUEST.md` and user-facing requirements, exercising features as an end user or auditor would.
- **Methodology**: Systematic 4-tier testing strategy combining Category-Partition, Boundary Value Analysis (BVA), Pairwise Combinatorial Interaction, and Real-World Workload Testing.
- **Progressive Testability**: Verification mechanisms do not depend on features more complex than what they verify. Early tiers provide clean pass/fail signals independently of later UI polish.

## Feature Inventory & Test Matrix
| # | Feature | Requirement Source | Tier 1 (Min 5) | Tier 2 (Min 5) | Tier 3 (Pairwise) | Tier 4 (Scenario) |
|---|---------|-------------------|:--------------:|:--------------:|:-----------------:|:-----------------:|
| 1 | SMNF Cognitive Notification Router | ORIGINAL_REQUEST & `notification_router.rs` | 5 | 5 | ✓ | ✓ |
| 2 | Biscuit v5 Capability Token Inspector | ORIGINAL_REQUEST & `capability.rs` | 5 | 5 | ✓ | ✓ |
| 3 | Desktop HUD Toast Sandbox | ORIGINAL_REQUEST & `components.rs` | 5 | 5 | ✓ | ✓ |
| 4 | Notification Security Policy Playground | ORIGINAL_REQUEST & `minor.rs` / `guest.rs` | 5 | 5 | ✓ | ✓ |
| 5 | Repository Metric Synchronization | ORIGINAL_REQUEST & `cratesCatalog.ts` | 5 | 5 | ✓ | ✓ |
| 6 | RTL Directionality & Arrow Mirroring | ORIGINAL_REQUEST & `context.tsx` | 5 | 5 | ✓ | ✓ |
| 7 | Cybernetic Styling & Sovereignty Invariants | ORIGINAL_REQUEST & `audit-sovereignty.mjs` | 5 | 5 | ✓ | ✓ |
| 8 | 8-Locale Translation Schema & Parity | ORIGINAL_REQUEST & `types.ts` | 5 | 5 | ✓ | ✓ |
| 9 | Interactive EU AI Act & GDPR Matrix | ORIGINAL_REQUEST & `PrivacyPage.tsx` | 5 | 5 | ✓ | ✓ |
| 10 | Exportable Signed Compliance Dossier | ORIGINAL_REQUEST & RFC 9116 / GDPR | 5 | 5 | ✓ | ✓ |

## Test Architecture
- **Test Runner**: Vitest 5.1.0 (`environment: 'jsdom'`).
- **Execution Command**: `npm test` (or `npx vitest run`).
- **Pass/Fail Semantics**: Exit code 0, 0 failed tests, 0 unhandled promise rejections.
- **Directory Layout**:
  - `src/__tests__/`: Unit, engine, and integration test suites.
  - `src/__tests__/smnfRouter.test.ts`: Tests for SMNF Cognitive Router & silence budget.
  - `src/__tests__/biscuitTokens.test.ts`: Tests for Biscuit v5 token construction, monotonic attenuation, and escalation attacks.
  - `src/__tests__/hudAndPolicy.test.ts`: Tests for Desktop HUD toast behavior and constitutional security policies.
  - `src/__tests__/i18nAndMetrics.test.ts`: Tests for 8-locale key parity, metric consistency (42 crates / 146k LoC), and RTL logic.
  - `src/__tests__/complianceAndAudit.test.ts`: Tests for compliance dossier generation, zero exfiltration invariants, and SHA-256 font provenance.

## Real-World Application Scenarios (Tier 4)
| # | Scenario | Features Exercised | Complexity |
|---|----------|--------------------|------------|
| 1 | Executive in High-Stakes Meeting | SMNF Router + Gaze + Noise + Silence Budget + HUD Whisper | High |
| 2 | Driver Drowsiness somnolence override | SMNF Router + PERCLOS fatigue detection + Unconditional alarm | High |
| 3 | Sandboxed Sub-Agent Attenuation & Delegation | Biscuit v5 Block chain + Monotonic attenuation + Scope bounds | High |
| 4 | Rogue Agent Privilege Escalation Attack | Biscuit v5 Verifier + Rule 4 detection + Datalog violation reject | High |
| 5 | Minor User Financial Protection | Policy Engine + Rule 7 NoWallet + Continuous biometric step-up | High |
| 6 | Sovereign Audit & Compliance Export | Zero Exfiltration + RFC 9116 + Signed JSON dossier export | Medium |
| 7 | RTL Arabic Complete Navigation Walkthrough | Router + RTL context + Mirror arrows + Isolated waveforms | Medium |

## Coverage Thresholds
- **Tier 1 (Feature Coverage)**: >= 5 tests per feature (50 tests minimum).
- **Tier 2 (Boundary & Corner Cases)**: >= 5 boundary tests per feature (50 tests minimum).
- **Tier 3 (Pairwise Interactions)**: >= 10 cross-feature combination tests.
- **Tier 4 (Real-World Workloads)**: >= 7 holistic application scenario tests.
- **Total Minimum Target**: >= 117 automated test cases passing with 100% success rate.
