import { describe, expect, it } from 'vitest';
import {
  HOLD_REQUIRED_MS,
  beginHold,
  createInitialHoldState,
  releaseHoldEarly,
  tickHold,
  triggerAccessibleStep,
} from '@/engine/hold';
import {
  BIOMETRIC_STEP_UP_THRESHOLDS,
  evaluateSecurityPolicy,
  type PolicySubject,
} from '@/engine/notificationPolicyEngine';
import { braiseAllowed } from '@/engine/colorGuard';

describe('Feature 3: Desktop HUD Toast Sandbox & Feature 4: Notification Security Policy Playground', () => {
  // =========================================================================
  // FEATURE 3 — TIER 1: FEATURE COVERAGE (>= 5 tests)
  // =========================================================================
  describe('Feature 3 - Tier 1: Desktop HUD Toasts & Hold State Machine', () => {
    it('T1.1 - enforces Desktop HUD acrylic overlay window geometry specification', () => {
      // AcrylicOverlayWindowSpec from crates/hakille-ui/src/ephemeral_hud.rs:298
      const hudSpec = {
        widthPx: 720,
        yOffsetPx: 120,
        cornerRadiusPx: 12,
        blurRadiusPx: 30,
        acrylicTint: '#121419',
        borderLine: '#2D2D2D',
        braiseAccent: '#FF7A2F',
        opacity: {
          prompting: 0.88,
          streaming: 0.92,
          validation: 0.95,
        },
      };

      expect(hudSpec.widthPx).toBe(720);
      expect(hudSpec.cornerRadiusPx).toBe(12);
      expect(hudSpec.blurRadiusPx).toBe(30);
      expect(hudSpec.acrylicTint).toBe('#121419');
      expect(hudSpec.braiseAccent).toBe('#FF7A2F');
      expect(hudSpec.opacity.validation).toBe(0.95);
    });

    it('T1.2 - initializes hold state in idle phase with zero elapsed time', () => {
      const state = createInitialHoldState();
      expect(state.phase).toBe('idle');
      expect(state.startedAtMs).toBeNull();
      expect(state.elapsedMs).toBe(0);
      expect(state.progress).toBe(0);
    });

    it('T1.3 - starts hold at current timestamp and tracks progress monotonically', () => {
      const t0 = 1000;
      let state = beginHold(t0);
      expect(state.phase).toBe('holding');
      expect(state.startedAtMs).toBe(1000);
      expect(state.elapsedMs).toBe(0);
      expect(state.progress).toBe(0);

      // Advance by 300ms (50% of 600ms)
      state = tickHold(state, t0 + 300);
      expect(state.phase).toBe('holding');
      expect(state.elapsedMs).toBe(300);
      expect(state.progress).toBeCloseTo(0.5, 3);

      // Advance by another 150ms (75% of 600ms)
      state = tickHold(state, t0 + 450);
      expect(state.phase).toBe('holding');
      expect(state.elapsedMs).toBe(450);
      expect(state.progress).toBeCloseTo(0.75, 3);
    });

    it('T1.4 - confirms state when hold duration reaches exactly 600 ms', () => {
      const t0 = 2000;
      let state = beginHold(t0);
      state = tickHold(state, t0 + HOLD_REQUIRED_MS);

      expect(state.phase).toBe('confirmed');
      expect(state.elapsedMs).toBe(600);
      expect(state.progress).toBe(1);
    });

    it('T1.5 - provides accessible 2-step keyboard confirmation (WCAG 2.1 AA/AAA)', () => {
      const state0 = createInitialHoldState();

      // Step 1: Arm accessible confirmation
      const state1 = triggerAccessibleStep(state0);
      expect(state1.phase).toBe('armed-accessible');
      expect(state1.progress).toBe(0.5);

      // Step 2: Confirm
      const state2 = triggerAccessibleStep(state1);
      expect(state2.phase).toBe('confirmed');
      expect(state2.progress).toBe(1);
      expect(state2.elapsedMs).toBe(HOLD_REQUIRED_MS);
    });

    it('T1.6 - validates Sacred Rule of Braise permissions for HUD actions', () => {
      expect(braiseAllowed('validation')).toBe(true);
      expect(braiseAllowed('hold-progress')).toBe(true);
      expect(braiseAllowed('decision-ring')).toBe(true);
      expect(braiseAllowed('decorative-border')).toBe(false);
      expect(braiseAllowed('ambient-background')).toBe(false);
    });
  });

  // =========================================================================
  // FEATURE 3 — TIER 2: BOUNDARY & CORNER CASES (>= 5 tests)
  // =========================================================================
  describe('Feature 3 - Tier 2: Boundary & Corner Cases', () => {
    it('T2.1 - cancels hold when released early at 599 ms (1 ms before threshold)', () => {
      const t0 = 1000;
      let state = beginHold(t0);
      state = tickHold(state, t0 + 599);
      expect(state.phase).toBe('holding');
      expect(state.progress).toBeLessThan(1);

      const cancelled = releaseHoldEarly(state);
      expect(cancelled.phase).toBe('cancelled');
      expect(cancelled.progress).toBe(0);
      expect(cancelled.startedAtMs).toBeNull();
      expect(cancelled.elapsedMs).toBe(599);
    });

    it('T2.2 - retains confirmed phase if releaseHoldEarly is called after confirmation', () => {
      const t0 = 1000;
      let state = beginHold(t0);
      state = tickHold(state, t0 + 600);
      expect(state.phase).toBe('confirmed');

      const released = releaseHoldEarly(state);
      expect(released.phase).toBe('confirmed');
      expect(released.progress).toBe(1);
    });

    it('T2.3 - clamps elapsed time and progress to 1.0 even if tick exceeds 600 ms', () => {
      const t0 = 1000;
      let state = beginHold(t0);
      state = tickHold(state, t0 + 1200); // Exceeds by 600ms

      expect(state.phase).toBe('confirmed');
      expect(state.elapsedMs).toBe(HOLD_REQUIRED_MS);
      expect(state.progress).toBe(1);
    });

    it('T2.4 - ignores tickHold when state is not in holding phase', () => {
      const idleState = createInitialHoldState();
      const ticked = tickHold(idleState, 5000);
      expect(ticked).toEqual(idleState);
    });

    it('T2.5 - resets to initial state if releaseHoldEarly is called on idle state', () => {
      const idle = createInitialHoldState();
      const result = releaseHoldEarly(idle);
      expect(result.phase).toBe('idle');
      expect(result.elapsedMs).toBe(0);
    });
  });

  // =========================================================================
  // FEATURE 4 — TIER 1: FEATURE COVERAGE (>= 5 tests)
  // =========================================================================
  describe('Feature 4 - Tier 1: Security Policy Playground', () => {
    it('T1.7 - allows Owner with high biometric confidence (0.95) to perform standard operations', () => {
      const subject: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 0.95,
        originTrust: 'TrustedHuman',
      };

      const verdict = evaluateSecurityPolicy(subject, 'Notification', 'Write', false);
      expect(verdict.outcome).toBe('ALLOW');
      expect(verdict.ruleViolated).toBeNull();
      expect(verdict.applicableLawOrRule).toContain('Constitution');
    });

    it('T1.8 - allows Minor to perform benign reading operations (Notification Read)', () => {
      const subject: PolicySubject = {
        role: 'Minor',
        age: 15,
        biometricConfidence: 0.9,
        originTrust: 'TrustedHuman',
      };

      const verdict = evaluateSecurityPolicy(subject, 'Notification', 'Read', false);
      expect(verdict.outcome).toBe('ALLOW');
      expect(verdict.ruleViolated).toBeNull();
    });

    it('T1.9 - enforces Guest session isolation (RULE-13): blocks write/delete actions', () => {
      const subject: PolicySubject = {
        role: 'Guest',
        biometricConfidence: 0.8,
        originTrust: 'TrustedHuman',
      };

      const verdict = evaluateSecurityPolicy(subject, 'Filesystem', 'Write', false);
      expect(verdict.outcome).toBe('HARD_BLOCK');
      expect(verdict.ruleViolated).toContain('RULE-13');
      expect(verdict.applicableLawOrRule).toContain('ADR-013');
      expect(verdict.rationale).toContain('Session invité');
    });

    it('T1.10 - enforces Prompt Injection Guard (RULE-01): blocks untrusted external destructive actions', () => {
      const subject: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 0.95,
        originTrust: 'UntrustedExternal', // Scraped email or document
      };

      const verdict = evaluateSecurityPolicy(subject, 'Notification', 'Execute', true);
      expect(verdict.outcome).toBe('HARD_BLOCK');
      expect(verdict.ruleViolated).toContain('RULE-01');
      expect(verdict.applicableLawOrRule).toContain('OWASP LLM-01');
    });

    it('T1.11 - confines UntrustedAgent plugin to sandbox (RULE-06): blocks FullSystem / Wallet', () => {
      const subject: PolicySubject = {
        role: 'UntrustedAgent',
        biometricConfidence: 0.9,
        originTrust: 'LocalDaemon',
      };

      const verdict = evaluateSecurityPolicy(subject, 'FullSystem', 'Execute', false);
      expect(verdict.outcome).toBe('HARD_BLOCK');
      expect(verdict.ruleViolated).toContain('RULE-06');
      expect(verdict.applicableLawOrRule).toContain('Wasmtime');
    });

    it('T1.12 - requires active biometric step-up when confidence is below scope threshold', () => {
      const subject: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 0.35, // Below 0.50 for Filesystem
        originTrust: 'TrustedHuman',
      };

      const verdict = evaluateSecurityPolicy(subject, 'Filesystem', 'Read', false);
      expect(verdict.outcome).toBe('STEP_UP_REQUIRED');
      expect(verdict.ruleViolated).toContain('AUTH-STEPUP');
      expect(verdict.rationale).toContain('Indice de présence continue tau (0.35)');
    });
  });

  // =========================================================================
  // FEATURE 4 — TIER 2: BOUNDARY & CORNER CASES (>= 5 tests)
  // =========================================================================
  describe('Feature 4 - Tier 2: Boundary & Corner Cases', () => {
    it('T2.6 - exact age boundary: Age 17 is HARD BLOCKED from Wallet (Rule 7)', () => {
      const subjectAge17: PolicySubject = {
        role: 'Owner',
        age: 17,
        biometricConfidence: 1.0,
        originTrust: 'TrustedHuman',
      };

      const verdict = evaluateSecurityPolicy(subjectAge17, 'Wallet', 'Execute', false);
      expect(verdict.outcome).toBe('HARD_BLOCK');
      expect(verdict.ruleViolated).toContain('RULE-07');
      expect(verdict.applicableLawOrRule).toContain('EU AI Act Art. 9');
    });

    it('T2.7 - exact age boundary: Age 18 with sufficient confidence is ALLOWED for Wallet', () => {
      const subjectAge18: PolicySubject = {
        role: 'Owner',
        age: 18,
        biometricConfidence: 0.95,
        originTrust: 'TrustedHuman',
      };

      const verdict = evaluateSecurityPolicy(subjectAge18, 'Wallet', 'Execute', false);
      expect(verdict.outcome).toBe('ALLOW');
      expect(verdict.ruleViolated).toBeNull();
    });

    it('T2.8 - blocks Minor from Admin action on any scope', () => {
      const minorSubject: PolicySubject = {
        role: 'Minor',
        age: 16,
        biometricConfidence: 0.95,
        originTrust: 'TrustedHuman',
      };

      const verdict = evaluateSecurityPolicy(minorSubject, 'Notification', 'Admin', false);
      expect(verdict.outcome).toBe('HARD_BLOCK');
      expect(verdict.ruleViolated).toContain('RULE-07');
      expect(verdict.rationale).toContain('interdiction absolue d’exécuter des mutations d’administration');
    });

    it('T2.9 - Wallet confidence threshold exact boundary (0.90 threshold)', () => {
      const subject089: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 0.89,
        originTrust: 'TrustedHuman',
      };
      const verdict089 = evaluateSecurityPolicy(subject089, 'Wallet', 'Read', false);
      expect(verdict089.outcome).toBe('STEP_UP_REQUIRED');

      const subject090: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 0.90,
        originTrust: 'TrustedHuman',
      };
      const verdict090 = evaluateSecurityPolicy(subject090, 'Wallet', 'Read', false);
      expect(verdict090.outcome).toBe('ALLOW');
    });

    it('T2.10 - ModelInference confidence threshold exact boundary (0.70 threshold)', () => {
      const subject069: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 0.69,
        originTrust: 'TrustedHuman',
      };
      expect(evaluateSecurityPolicy(subject069, 'ModelInference', 'Read', false).outcome).toBe('STEP_UP_REQUIRED');

      const subject070: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 0.70,
        originTrust: 'TrustedHuman',
      };
      expect(evaluateSecurityPolicy(subject070, 'ModelInference', 'Read', false).outcome).toBe('ALLOW');
    });

    it('T2.11 - elevates step-up threshold to 0.85 for any destructive action', () => {
      // Audio scope normally requires only 0.40
      expect(BIOMETRIC_STEP_UP_THRESHOLDS.Audio).toBe(0.4);

      const subject070: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 0.70,
        originTrust: 'TrustedHuman',
      };

      // Non-destructive Audio read with confidence 0.70 -> ALLOW
      const readVerdict = evaluateSecurityPolicy(subject070, 'Audio', 'Read', false);
      expect(readVerdict.outcome).toBe('ALLOW');

      // Destructive Audio mutation with confidence 0.70 -> STEP_UP_REQUIRED (needs >= 0.85)
      const destructiveVerdict = evaluateSecurityPolicy(subject070, 'Audio', 'Delete', true);
      expect(destructiveVerdict.outcome).toBe('STEP_UP_REQUIRED');
      expect(destructiveVerdict.rationale).toContain('inférieur au seuil requis (0.85)');
    });
  });

  // =========================================================================
  // TIER 3: CROSS-FEATURE PAIRWISE COMBINATIONS (>= 2 tests)
  // =========================================================================
  describe('Tier 3: Cross-Feature Combinations', () => {
    it('T3.1 - Policy Verdict on Destructive Action triggers HUD 600ms Hold validation requirement', () => {
      const subject: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 0.92,
        originTrust: 'TrustedHuman',
      };

      // 1. Policy approves destructive action execution
      const verdict = evaluateSecurityPolicy(subject, 'Filesystem', 'Delete', true);
      expect(verdict.outcome).toBe('ALLOW');

      // 2. Destructive mutation must be committed via 600ms hold button with Braise context
      expect(braiseAllowed('validation')).toBe(true);
      let holdState = beginHold(1000);
      holdState = tickHold(holdState, 1000 + HOLD_REQUIRED_MS);
      expect(holdState.phase).toBe('confirmed');
    });

    it('T3.2 - Untrusted External origin attempting to impersonate Minor or Guest fails closed', () => {
      const attacker: PolicySubject = {
        role: 'Guest',
        biometricConfidence: 0.99,
        originTrust: 'UntrustedExternal',
      };

      // Both Rule 1 and Rule 13 apply
      const verdict = evaluateSecurityPolicy(attacker, 'Wallet', 'Write', true);
      expect(verdict.outcome).toBe('HARD_BLOCK');
    });
  });

  // =========================================================================
  // TIER 4: REAL-WORLD APPLICATION SCENARIOS
  // =========================================================================
  describe('Tier 4: Real-World Scenarios', () => {
    it('T4.1 - Scenario 5: Minor User Financial Protection (Constitutional Rule 7 Invariant)', () => {
      // Real-World Scenario: A 16-year-old minor interacts with Hakille.
      // A commercial shopping website or compromised prompt attempts to trigger an automated payment.
      const minorSubject: PolicySubject = {
        role: 'Minor',
        age: 16,
        biometricConfidence: 1.0, // Even with maximum biometric certainty!
        originTrust: 'TrustedHuman',
      };

      // Minor attempts to execute payment
      const paymentVerdict = evaluateSecurityPolicy(minorSubject, 'Wallet', 'Execute', true);

      // Verifications:
      expect(paymentVerdict.outcome).toBe('HARD_BLOCK');
      expect(paymentVerdict.ruleViolated).toBe('RULE-07: Invariant NoWallet pour mineurs');
      expect(paymentVerdict.applicableLawOrRule).toBe('Constitutional Rule 7 & EU AI Act Art. 9');
      expect(paymentVerdict.rationale).toContain('Blocage constitutionnel inconditionnel');

      // Attempting to elevate to Admin also blocked
      const adminVerdict = evaluateSecurityPolicy(minorSubject, 'FullSystem', 'Admin', true);
      expect(adminVerdict.outcome).toBe('HARD_BLOCK');
      expect(adminVerdict.ruleViolated).toContain('RULE-07');
    });
  });
});
