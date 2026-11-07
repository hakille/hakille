# Project: Hakille Showcase Website Enrichment & Advancement

## Architecture
- **Web Stack**: Vite 8.3.6 (Rolldown engine) + React 19 + TypeScript 5.7.3 + Vitest 5.1.0.
- **Styling**: Native CSS Cascade Layers (`@layer tokens, base, layout, simulators, pages`) in `src/styles/*.css`. Zero external CSS-in-JS, zero Tailwind, zero PostCSS.
- **Tokens & Assets**: TOML design tokens (`color.toml`, `motion.toml`, `type.toml`) synchronized via `scripts/sync-from-repo.mjs`. Cryptographically verified SIL OFL font `InterVariable.ttf` (SHA-256: `29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031`) and official brand assets.
- **Sovereignty & Invariants**:
  - Exactly one `<h1>` per pre-rendered static route.
  - Strict Content Security Policy (`default-src 'self'`). Zero external CDN requests or analytics trackers.
  - Sacred Rule of Braise: `--hakille-braise` (`#FF7A2F`) strictly forbidden for decorative use; permitted exclusively under `[data-braise-context="validation" | "hold-progress" | "decision-ring"]`.
- **Dual Track Organization**:
  - **Implementation Track**: Delivers the 4 new simulator labs, design system enhancements, 8-locale i18n expansion, architecture documentation, and legal compliance center.
  - **E2E Testing Track**: Derives independent, requirement-driven opaque-box test suites (Tiers 1–4) covering all features, boundaries, combinations, and application workflows, culminating in `TEST_READY.md`.

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | SMNF Cognitive Notification Router Lab | Interactive simulator modeling multimodal attention routing, gaze/somnolence tracking, silence budget (3/day), and cross-substrate dispatch | M1 | explorer_survey_1 & 2 |
| 2 | Biscuit v5 Capability Token Lab | Interactive Datalog token attenuator, monotonic restriction verifier, depth bound enforcement ($D_{\max}=4$), and privilege escalation attack sandbox | M1 | explorer_survey_1 & 2 |
| 3 | Desktop HUD Toast Sandbox | Pixel-accurate acrylic overlay window (720px, 30px blur), auto-evaporation bar, Karplus-Strong chime trigger, and 600ms Braise hold button | M1 | explorer_survey_1 & 2 |
| 4 | Notification Security Policy Playground | Interactive evaluation of constitutional rules (Rule 7 minors NoWallet, guest session isolation, prompt injection Rule 1, continuous biometric step-up) | M1 | explorer_survey_1 & 2 |
| 5 | Repository Metric Synchronization | Align metrics across `cratesCatalog.ts`, `legal.ts`, and all 8 locale dictionaries to 42 packages, 146,229 LoC, and 1,948 tests | M2 | explorer_survey_3 |
| 6 | RTL Bidirectional Navigation & Arrow Polish | Mirror directional CTA arrows (`←` in RTL vs `→` in LTR), isolate LTR tabular metrics and audio waveforms | M2 | explorer_survey_3 |
| 7 | Cybernetic Design & Performance Polish | Mineral glow styling (`--hakille-lueur`), acrylic glass refinement, dynamic code-splitting (`React.lazy`) for heavy simulator labs | M2 | explorer_survey_1 |
| 8 | 8-Locale Translation Schema Expansion | Expand `TranslationSchema` in `src/i18n/types.ts` for all new simulator labs, tooltips, HUD states, policy explanations, and error messages | M3 | explorer_survey_1 & 3 |
| 9 | Full 8-Locale Parity across Dictionaries | Provide complete, authentic translations in `fr`, `en`, `es`, `de`, `pt`, `ar`, `sw`, `ja` with 100% recursive key parity | M3 | explorer_survey_3 |
| 10 | Deep Technical Content i18n Migration | Replace binary `pickBi` / `isFr` fallbacks with structured i18n keys for ADRs, terms, install, and simulator controls | M3 | explorer_survey_3 |
| 11 | SMNF & Multiplatform Architecture Docs | Comprehensive technical documentation covering SMNF substrates, Biscuit v5 capabilities, multi-client IPC, and cross-OS runtime | M4 | explorer_survey_1 & 2 |
| 12 | Interactive EU AI Act & GDPR Compliance Matrix | Interactive conformity checklist for EU AI Act (Regulation 2024/1689) and GDPR Articles 15–22 with live verification status | M4 | explorer_survey_3 |
| 13 | Exportable Signed Compliance Dossier | Client-side cryptographic receipt generator (`hakille-compliance-report.json`) attesting zero cookies, zero external trackers, and SHA-256 font provenance | M4 | explorer_survey_3 |
| 14 | Interactive GDPR Rights Exercise Wizard | Structured wizard for exercising GDPR rights (Art. 15 access, Art. 16 dispute, Art. 17 erasure/succession) generating signed audit commands | M4 | explorer_survey_3 |
| 15 | E2E Test Suite Creation (Tiers 1–4) | Independent opaque-box test suites covering feature requirements, boundaries, pairwise interactions, and user workflows | E2E Track | E2E Track Orchestrator |
| 16 | Final Integration & Adversarial Hardening (Tier 5) | Pass 100% of E2E suites + adversarial challenge tests + static verification (typecheck, lint, SSG build, sovereignty audit) | M5 | orchestrator |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Interactive Simulator Labs | 4 new simulators (SMNF Router, Biscuit v5, HUD Toast, Security Policy), engines, styling | none | DONE |
| M2 | Design Polish & Metrics Sync | Metrics sync (42 crates, 146k LoC), RTL arrow fix, cybernetic styling, bundle code-splitting | M1 interface contracts | DONE |
| M3 | Multilingual 8-Locale i18n | Expand `TranslationSchema` and all 8 locale files (`fr`, `en`, `es`, `de`, `pt`, `ar`, `sw`, `ja`), 100% key parity | M1, M2 | DONE |
| M4 | Architecture Docs & Compliance | SMNF architecture docs, EU AI Act & GDPR interactive compliance tools, signed JSON dossier | M1, M2, M3 | DONE |
| M5 | Final E2E Pass & Hardening | 100% E2E tests pass (Tiers 1–4), Tier 5 adversarial tests, full build & sovereignty audit verification | M1, M2, M3, M4, TEST_READY | DONE |

## Interface Contracts

### 1. SMNF Notification Router Engine (`src/engine/notificationRouter.ts`)
```typescript
export interface CognitiveState {
  userLookingAtScreen: boolean;
  perclosDrowsinessScore: number;
  ambientNoiseDb: number;
  inMeeting: boolean;
  thirdPartiesPresent: boolean;
  userWearingHeadphones: boolean;
  isNightTime: boolean;
}

export type VisualPresentationStyle = 'BraiseActionRequired' | 'MinimalistToast' | 'AmbientGlyph' | 'SilentShadow';
export type NotificationChannel = 'HeadphonesPrivateVoice' | 'AmbientMicroToast' | 'AcousticWhisper' | 'SpatialChime' | 'HapticPulse' | 'CognitiveDeferralQueue' | 'SubstrateNativeDispatch';

export interface RoutingDecision {
  channel: NotificationChannel;
  visualStyle: VisualPresentationStyle;
  rationale: string;
  hapticFeedback: 'pulse' | 'double_tap' | 'urgent_buzz' | null;
  deferred: boolean;
  creditCost: 0 | 1;
  targetSubstratePreview: string;
}

export function evaluateNotificationRouting(
  request: { title: string; body: string; urgency: string; category: string; hasDestructiveAction: boolean },
  cognitive: CognitiveState,
  silenceCreditsRemaining: number
): RoutingDecision;
```

### 2. Biscuit v5 Capability Engine (`src/engine/biscuitEngine.ts`)
```typescript
export type CapabilityScope = 'Notification' | 'Wallet' | 'Filesystem' | 'Camera' | 'Audio' | 'ModelInference' | 'Biometrics' | 'Spatial' | 'CognitiveInference';
export type CapabilityAction = 'Read' | 'Write' | 'Execute' | 'Delete' | 'Admin';

export interface BiscuitBlock {
  index: number;
  label: string;
  authority: 'RootSeed' | 'SubAgentAttenuation' | 'ThirdPartyWorker';
  facts: string[];
  checks: string[];
  signatureHex: string;
}

export interface AttenuationResult {
  valid: boolean;
  grantedScopes: CapabilityScope[];
  grantedActions: CapabilityAction[];
  attenuationDepth: number;
  violationReason: string | null;
  datalogCode: string;
}

export function buildRootToken(scopes: CapabilityScope[], actions: CapabilityAction[], ttlSeconds: number): BiscuitBlock[];
export function attenuateToken(parentBlocks: BiscuitBlock[], newScopes: CapabilityScope[], newActions: CapabilityAction[], newChecks: string[]): AttenuationResult;
export function attemptPrivilegeEscalationAttack(token: BiscuitBlock[], attackType: 'widen_scope' | 'add_action' | 'remove_caveat' | 'forge_root'): AttenuationResult;
```

### 3. Notification Policy Engine (`src/engine/notificationPolicyEngine.ts`)
```typescript
export interface PolicySubject {
  role: 'Owner' | 'Minor' | 'Guest' | 'UntrustedAgent';
  age?: number;
  biometricConfidence: number;
  originTrust: 'TrustedHuman' | 'LocalDaemon' | 'UntrustedExternal';
}

export interface PolicyVerdict {
  outcome: 'ALLOW' | 'HARD_BLOCK' | 'STEP_UP_REQUIRED';
  ruleViolated: string | null;
  rationale: string;
  applicableLawOrRule: string;
}

export function evaluateSecurityPolicy(
  subject: PolicySubject,
  targetScope: CapabilityScope,
  action: CapabilityAction,
  isDestructive: boolean
): PolicyVerdict;
```

## Code Layout
- `src/engine/`: Core simulation logic (`notificationRouter.ts`, `biscuitEngine.ts`, `notificationPolicyEngine.ts`). Owned by M1 Worker.
- `src/components/simulators/`: New simulator components (`CognitiveNotificationRouterLab.tsx`, `BiscuitTokenLab.tsx`, `DesktopHudToastSandbox.tsx`, `NotificationSecurityPolicyPlayground.tsx`). Owned by M1 Worker.
- `src/styles/simulators.css`: Simulator styles scoped to `@layer simulators`. Owned by M1 Worker.
- `src/data/cratesCatalog.ts`, `src/config/legal.ts`: Metrics & legal configuration. Owned by M2 Worker.
- `src/i18n/types.ts`, `src/i18n/locales/*.ts`: Translation schema and locale files. Owned by M3 Worker.
- `src/pages/`: Page additions/updates (`ArchitecturePage.tsx`, `SecurityPage.tsx`, `HomePage.tsx`). Owned by M1, M2, M4 Workers according to page ownership contracts.
- `src/__tests__/`: Automated unit and integration tests. Owned by E2E Testing Track / Test Writer.
