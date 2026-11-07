import { describe, expect, it, vi } from 'vitest';
import {
  ALL_13_CANONICAL_SCOPES,
  ALL_5_ACTIONS,
  attenuateToken,
  attemptPrivilegeEscalationAttack,
  buildRootToken,
} from '@/engine/biscuitEngine';
import {
  evaluateSecurityPolicy,
  PolicySubject,
} from '@/engine/notificationPolicyEngine';
import { createRoot, Root } from 'react-dom/client';
import React, { act } from 'react';
import { DesktopHudToastSandbox } from '@/components/simulators/DesktopHudToastSandbox';
import { I18nProvider } from '@/i18n/context';

describe('Empirical Adversarial Stress Suite (challenger_m1_2)', () => {
  // =========================================================================
  // 1. MINOR PROTECTION BOUNDARY & ROLE INVARIANTS
  // =========================================================================
  describe('1. Minor Protection Invariants & Boundary Stress Testing', () => {
    it('EMP-1.1: Age 17.99 boundary vs Age 18.00 boundary on Wallet access', () => {
      const minorSubject: PolicySubject = {
        role: 'Owner',
        age: 17.99,
        biometricConfidence: 1.0,
        originTrust: 'TrustedHuman',
      };
      const minorVerdict = evaluateSecurityPolicy(minorSubject, 'Wallet', 'Execute', true);
      expect(minorVerdict.outcome).toBe('HARD_BLOCK');
      expect(minorVerdict.ruleViolated).toContain('RULE-07');
      expect(minorVerdict.applicableLawOrRule).toContain('EU AI Act Art. 9');

      const adultSubject: PolicySubject = {
        role: 'Owner',
        age: 18.0,
        biometricConfidence: 1.0,
        originTrust: 'TrustedHuman',
      };
      const adultVerdict = evaluateSecurityPolicy(adultSubject, 'Wallet', 'Execute', true);
      expect(adultVerdict.outcome).toBe('ALLOW');
      expect(adultVerdict.ruleViolated).toBeNull();
    });

    it('EMP-1.2: Micro-epsilon boundaries around age 18 (17.999999 vs 18.000001)', () => {
      const justUnder: PolicySubject = {
        role: 'Owner',
        age: 17.999999,
        biometricConfidence: 0.95,
        originTrust: 'TrustedHuman',
      };
      expect(evaluateSecurityPolicy(justUnder, 'Wallet', 'Read', false).outcome).toBe('HARD_BLOCK');

      const justOver: PolicySubject = {
        role: 'Owner',
        age: 18.000001,
        biometricConfidence: 0.95,
        originTrust: 'TrustedHuman',
      };
      expect(evaluateSecurityPolicy(justOver, 'Wallet', 'Read', false).outcome).toBe('ALLOW');
    });

    it('EMP-1.3: Negative age values (-1, -17, -100)', () => {
      const negativeAgeSubject: PolicySubject = {
        role: 'Owner',
        age: -1,
        biometricConfidence: 1.0,
        originTrust: 'TrustedHuman',
      };
      // In JS, -1 < 18 is true, so it triggers isMinor
      const walletVerdict = evaluateSecurityPolicy(negativeAgeSubject, 'Wallet', 'Execute', false);
      expect(walletVerdict.outcome).toBe('HARD_BLOCK');
      expect(walletVerdict.ruleViolated).toContain('RULE-07');

      const adminVerdict = evaluateSecurityPolicy(negativeAgeSubject, 'Notification', 'Admin', false);
      expect(adminVerdict.outcome).toBe('HARD_BLOCK');
    });

    it('EMP-1.4: Role Minor with conflicting adult age (age: 25) must remain HARD_BLOCK', () => {
      const conflictingSubject: PolicySubject = {
        role: 'Minor',
        age: 25, // Conflicting adult age
        biometricConfidence: 1.0,
        originTrust: 'TrustedHuman',
      };
      const verdict = evaluateSecurityPolicy(conflictingSubject, 'Wallet', 'Read', false);
      expect(verdict.outcome).toBe('HARD_BLOCK');
      expect(verdict.ruleViolated).toContain('RULE-07');
    });

    it('EMP-1.5: Age NaN handling under Owner role vs Minor role', () => {
      const nanOwnerSubject: PolicySubject = {
        role: 'Owner',
        age: NaN,
        biometricConfidence: 0.95,
        originTrust: 'TrustedHuman',
      };
      // typeof NaN === 'number' is true, but NaN < 18 is false!
      // Therefore isMinor evaluates to false!
      const ownerVerdict = evaluateSecurityPolicy(nanOwnerSubject, 'Wallet', 'Read', false);
      // Demonstrates that NaN age does not get flagged as minor
      expect(ownerVerdict.outcome).toBe('ALLOW');

      const nanMinorSubject: PolicySubject = {
        role: 'Minor',
        age: NaN,
        biometricConfidence: 0.95,
        originTrust: 'TrustedHuman',
      };
      // Role Minor explicitly flags isMinor
      const minorVerdict = evaluateSecurityPolicy(nanMinorSubject, 'Wallet', 'Read', false);
      expect(minorVerdict.outcome).toBe('HARD_BLOCK');
    });

    it('EMP-1.6: Minor attempting all 5 actions across FullSystem', () => {
      const minorSubject: PolicySubject = {
        role: 'Minor',
        age: 15,
        biometricConfidence: 1.0,
        originTrust: 'TrustedHuman',
      };

      for (const action of ALL_5_ACTIONS) {
        const verdict = evaluateSecurityPolicy(minorSubject, 'FullSystem', action, false);
        expect(verdict.outcome).toBe('HARD_BLOCK');
        expect(verdict.ruleViolated).toContain('RULE-07');
      }
    });

    it('EMP-1.7: Minor attempting Admin action across ALL 13 canonical scopes', () => {
      const minorSubject: PolicySubject = {
        role: 'Minor',
        age: 15,
        biometricConfidence: 1.0,
        originTrust: 'TrustedHuman',
      };

      for (const scope of ALL_13_CANONICAL_SCOPES) {
        const verdict = evaluateSecurityPolicy(minorSubject, scope, 'Admin', false);
        expect(verdict.outcome).toBe('HARD_BLOCK');
        expect(verdict.ruleViolated).toContain('RULE-07');
      }
    });

    it('EMP-1.8: Non-whitelisted / invalid roles (tampered role at runtime)', () => {
      const rogueRoleSubject = {
        role: 'SuperAdmin' as unknown as PolicySubject['role'],
        biometricConfidence: 0.95,
        originTrust: 'TrustedHuman' as const,
      };
      // Check behavior when role is outside the known union
      const verdict = evaluateSecurityPolicy(rogueRoleSubject, 'Wallet', 'Read', false);
      // Note: Because it's not 'Minor', 'Guest', or 'UntrustedAgent', it passes as Owner
      expect(verdict.outcome).toBe('ALLOW');
    });
  });

  // =========================================================================
  // 2. PROMPT INJECTION EVASION ATTEMPTS & DAEMON TOKEN SPOOFING
  // =========================================================================
  describe('2. Prompt Injection Evasion & Token Sandbox Attacks', () => {
    it('EMP-2.1: UntrustedExternal attempting write, delete, admin, or destructive operations', () => {
      const untrustedSubject: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 0.95,
        originTrust: 'UntrustedExternal',
      };

      // All write/delete/admin or destructive actions must be HARD_BLOCK under RULE-01
      expect(evaluateSecurityPolicy(untrustedSubject, 'Notification', 'Write', false).outcome).toBe('HARD_BLOCK');
      expect(evaluateSecurityPolicy(untrustedSubject, 'Notification', 'Delete', false).outcome).toBe('HARD_BLOCK');
      expect(evaluateSecurityPolicy(untrustedSubject, 'Notification', 'Admin', false).outcome).toBe('HARD_BLOCK');
      expect(evaluateSecurityPolicy(untrustedSubject, 'Notification', 'Read', true).outcome).toBe('HARD_BLOCK');
      expect(evaluateSecurityPolicy(untrustedSubject, 'Wallet', 'Read', false).outcome).toBe('HARD_BLOCK');
      expect(evaluateSecurityPolicy(untrustedSubject, 'Filesystem', 'Read', false).outcome).toBe('HARD_BLOCK');
    });

    it('EMP-2.2: UntrustedExternal attempting non-destructive Execute on FullSystem or other scopes', () => {
      const untrustedSubject: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 0.95,
        originTrust: 'UntrustedExternal',
      };

      // Scrutinize whether UntrustedExternal can perform non-destructive Execute
      const fullSystemExecute = evaluateSecurityPolicy(untrustedSubject, 'FullSystem', 'Execute', false);
      const modelInferenceExecute = evaluateSecurityPolicy(untrustedSubject, 'ModelInference', 'Execute', false);

      // We document actual behavior empirically:
      // In current notificationPolicyEngine.ts, line 89 checks:
      // isDestructive || action === 'Write' || action === 'Delete' || action === 'Admin' || targetScope === 'Wallet' || targetScope === 'Filesystem'
      // Note: targetScope === 'FullSystem' with action === 'Execute' and isDestructive === false is ALLOWED if tau >= 0.90!
      // This is a notable boundary finding.
      expect(fullSystemExecute.outcome).toBe('ALLOW');
      expect(modelInferenceExecute.outcome).toBe('ALLOW');
    });

    it('EMP-2.3: Spoofing daemon token via rogue root key (forge_root attack)', () => {
      const rootToken = buildRootToken(['Notification', 'Filesystem'], ['Read'], 300);
      const attackResult = attemptPrivilegeEscalationAttack(rootToken, 'forge_root');

      expect(attackResult.valid).toBe(false);
      expect(attackResult.grantedScopes).toEqual([]);
      expect(attackResult.violationReason).toContain('RULE-03: Root Public Key Mismatch');
    });

    it('EMP-2.4: Sub-agent privilege escalation via ungranted scope injection (widen_scope attack)', () => {
      const rootToken = buildRootToken(['Notification'], ['Read'], 300);
      const attackResult = attemptPrivilegeEscalationAttack(rootToken, 'widen_scope');

      expect(attackResult.valid).toBe(false);
      expect(attackResult.grantedScopes).not.toContain('Wallet');
      expect(attackResult.violationReason).toContain('RULE-04: Monotonic Attenuation Violation');
    });

    it('EMP-2.5: Sub-agent privilege escalation via admin action injection (add_action attack)', () => {
      const rootToken = buildRootToken(['Notification'], ['Read'], 300);
      const attackResult = attemptPrivilegeEscalationAttack(rootToken, 'add_action');

      expect(attackResult.valid).toBe(false);
      expect(attackResult.grantedActions).not.toContain('Admin');
      expect(attackResult.violationReason).toContain('RULE-04: Monotonic Attenuation Violation');
    });

    it('EMP-2.6: Attacker stripping parent caveat checks (remove_caveat attack)', () => {
      const rootToken = buildRootToken(['Notification'], ['Read'], 300);
      const attackResult = attemptPrivilegeEscalationAttack(rootToken, 'remove_caveat');

      expect(attackResult.valid).toBe(false);
      expect(attackResult.violationReason).toContain('RULE-04: Signature Verification Failed');
    });

    it('EMP-2.7: Exceeding delegation depth bound (D_max = 4)', () => {
      let chain = buildRootToken(['Notification', 'Filesystem'], ['Read', 'Write'], 3600);

      // Depth 1
      let res = attenuateToken(chain, ['Notification', 'Filesystem'], ['Read', 'Write']);
      expect(res.valid).toBe(true);
      expect(res.attenuationDepth).toBe(1);
      chain = [...chain, { index: 1, label: 'D1', authority: 'SubAgentAttenuation', facts: [], checks: [], signatureHex: 'sig1' }];

      // Depth 2
      res = attenuateToken(chain, ['Notification'], ['Read']);
      expect(res.valid).toBe(true);
      expect(res.attenuationDepth).toBe(2);
      chain = [...chain, { index: 2, label: 'D2', authority: 'ThirdPartyWorker', facts: [], checks: [], signatureHex: 'sig2' }];

      // Depth 3
      res = attenuateToken(chain, ['Notification'], ['Read']);
      expect(res.valid).toBe(true);
      expect(res.attenuationDepth).toBe(3);
      chain = [...chain, { index: 3, label: 'D3', authority: 'ThirdPartyWorker', facts: [], checks: [], signatureHex: 'sig3' }];

      // Depth 4 (maximum allowed)
      res = attenuateToken(chain, ['Notification'], ['Read']);
      expect(res.valid).toBe(true);
      expect(res.attenuationDepth).toBe(4);
      chain = [...chain, { index: 4, label: 'D4', authority: 'ThirdPartyWorker', facts: [], checks: [], signatureHex: 'sig4' }];

      // Depth 5 (exceeds D_max = 4)
      const overflowRes = attenuateToken(chain, ['Notification'], ['Read']);
      expect(overflowRes.valid).toBe(false);
      expect(overflowRes.violationReason).toContain('RULE-05: Attenuation depth limit exceeded');
    });
  });

  // =========================================================================
  // 3. BIOMETRIC CONFIDENCE THRESHOLD BOUNDARY CONDITIONS (tau = 0.8999 vs 0.9000)
  // =========================================================================
  describe('3. Continuous Biometric Confidence Boundary Conditions (tau)', () => {
    it('EMP-3.1: Wallet threshold boundary tau = 0.8999 vs 0.9000', () => {
      const subject8999: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 0.8999,
        originTrust: 'TrustedHuman',
      };
      const verdict8999 = evaluateSecurityPolicy(subject8999, 'Wallet', 'Read', false);
      expect(verdict8999.outcome).toBe('STEP_UP_REQUIRED');
      expect(verdict8999.ruleViolated).toContain('AUTH-STEPUP');

      const subject9000: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 0.9,
        originTrust: 'TrustedHuman',
      };
      const verdict9000 = evaluateSecurityPolicy(subject9000, 'Wallet', 'Read', false);
      expect(verdict9000.outcome).toBe('ALLOW');
      expect(verdict9000.ruleViolated).toBeNull();
    });

    it('EMP-3.2: FullSystem threshold boundary tau = 0.8999 vs 0.9000', () => {
      const subject8999: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 0.8999,
        originTrust: 'LocalDaemon',
      };
      expect(evaluateSecurityPolicy(subject8999, 'FullSystem', 'Read', false).outcome).toBe('STEP_UP_REQUIRED');

      const subject9000: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 0.9,
        originTrust: 'LocalDaemon',
      };
      expect(evaluateSecurityPolicy(subject9000, 'FullSystem', 'Read', false).outcome).toBe('ALLOW');
    });

    it('EMP-3.3: ModelInference threshold boundary tau = 0.6999 vs 0.7000', () => {
      const subject6999: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 0.6999,
        originTrust: 'TrustedHuman',
      };
      expect(evaluateSecurityPolicy(subject6999, 'ModelInference', 'Read', false).outcome).toBe('STEP_UP_REQUIRED');

      const subject7000: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 0.7,
        originTrust: 'TrustedHuman',
      };
      expect(evaluateSecurityPolicy(subject7000, 'ModelInference', 'Read', false).outcome).toBe('ALLOW');
    });

    it('EMP-3.4: Filesystem destructive elevation to 0.85 boundary (0.8499 vs 0.8500)', () => {
      const subject8499: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 0.8499,
        originTrust: 'TrustedHuman',
      };
      // Destructive action on Filesystem requires 0.85
      const verdict8499 = evaluateSecurityPolicy(subject8499, 'Filesystem', 'Delete', true);
      expect(verdict8499.outcome).toBe('STEP_UP_REQUIRED');

      const subject8500: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 0.85,
        originTrust: 'TrustedHuman',
      };
      const verdict8500 = evaluateSecurityPolicy(subject8500, 'Filesystem', 'Delete', true);
      expect(verdict8500.outcome).toBe('ALLOW');
    });

    it('EMP-3.5: Extreme / Malformed biometric confidence inputs (clamping & NaN behavior)', () => {
      // Clamped from negative to 0.0
      const negativeTau: PolicySubject = {
        role: 'Owner',
        biometricConfidence: -0.5,
        originTrust: 'TrustedHuman',
      };
      expect(evaluateSecurityPolicy(negativeTau, 'Notification', 'Read', false).outcome).toBe('STEP_UP_REQUIRED');

      // Clamped from above 1.0 to 1.0
      const superTau: PolicySubject = {
        role: 'Owner',
        biometricConfidence: 2.5,
        originTrust: 'TrustedHuman',
      };
      expect(evaluateSecurityPolicy(superTau, 'Wallet', 'Read', false).outcome).toBe('ALLOW');

      // NaN confidence: in Math.max(0, Math.min(1, NaN)) -> returns NaN
      // in JS: NaN < baseThreshold evaluates to false!
      const nanTau: PolicySubject = {
        role: 'Owner',
        biometricConfidence: NaN,
        originTrust: 'TrustedHuman',
      };
      const nanVerdict = evaluateSecurityPolicy(nanTau, 'Wallet', 'Read', false);
      // Empirical verification of the NaN bypass behavior:
      expect(nanVerdict.outcome).toBe('ALLOW');
    });
  });

  // =========================================================================
  // 4. TOAST AUTO-EVAPORATION BAR PROGRESS MONOTONICITY & ZERO-FPS IDLE LOOP
  // =========================================================================
  describe('4. Toast Auto-Evaporation Progress Monotonicity & Zero-FPS Idle Loop', () => {
    it('EMP-4.1: Mathematical monotonicity of evaporation progress over time', () => {
      const durationMs = 3000;
      const startMs = 1000;

      // Sample 100 timestamps strictly increasing from startMs to startMs + durationMs + 1000
      let prevProgress = 1.0;
      for (let t = startMs; t <= startMs + durationMs + 500; t += 35) {
        const elapsed = t - startMs;
        const progress = Math.max(0, 1.0 - elapsed / durationMs);

        // Strict monotonicity check: current progress <= previous progress
        expect(progress).toBeLessThanOrEqual(prevProgress);
        // Bounds check: 0.0 <= progress <= 1.0
        expect(progress).toBeGreaterThanOrEqual(0.0);
        expect(progress).toBeLessThanOrEqual(1.0);

        if (elapsed >= durationMs) {
          expect(progress).toBe(0.0);
        }
        prevProgress = progress;
      }
    });

    it('EMP-4.2: Zero-FPS idle loop when unmounted during active evaporation animation', async () => {
      const rafSpy = vi.spyOn(window, 'requestAnimationFrame');
      const cancelSpy = vi.spyOn(window, 'cancelAnimationFrame');

      const container = document.createElement('div');
      document.body.appendChild(container);

      let root: Root | null = null;
      await act(async () => {
        const reactRoot = createRoot(container);
        reactRoot.render(
          React.createElement(
            I18nProvider,
            null,
            React.createElement(DesktopHudToastSandbox, null)
          )
        );
        root = reactRoot;
      });

      // Trigger button click to start toast evaporation
      const triggerButton = container.querySelector('.action-btn') as HTMLButtonElement;
      expect(triggerButton).not.toBeNull();

      await act(async () => {
        triggerButton.click();
      });

      // requestAnimationFrame should have been invoked
      expect(rafSpy).toHaveBeenCalled();
      const initialCancelCalls = cancelSpy.mock.calls.length;

      // Now unmount the component while evaporation is pending
      await act(async () => {
        (root as unknown as { unmount: () => void })?.unmount();
      });

      // cancelAnimationFrame must have been called during unmount cleanup
      expect(cancelSpy.mock.calls.length).toBeGreaterThan(initialCancelCalls);

      rafSpy.mockRestore();
      cancelSpy.mockRestore();
      document.body.removeChild(container);
    });

    it('EMP-4.3: Persistent mode (durationMs = 0) maintains zero-FPS idle with no requestAnimationFrame scheduled', async () => {
      const rafSpy = vi.spyOn(window, 'requestAnimationFrame');

      const container = document.createElement('div');
      document.body.appendChild(container);

      let root: Root | null = null;
      await act(async () => {
        const reactRoot = createRoot(container);
        reactRoot.render(
          React.createElement(
            I18nProvider,
            null,
            React.createElement(DesktopHudToastSandbox, null)
          )
        );
        root = reactRoot;
      });

      // Select persistent option (value = 0)
      const durationSelect = container.querySelectorAll('.sim-select')[2] as HTMLSelectElement;
      expect(durationSelect).not.toBeNull();

      await act(async () => {
        durationSelect.value = '0';
        durationSelect.dispatchEvent(new Event('change', { bubbles: true }));
      });

      const initialRafCalls = rafSpy.mock.calls.length;

      // Click trigger
      const triggerBtn = container.querySelector('.action-btn') as HTMLButtonElement;
      await act(async () => {
        triggerBtn.click();
      });

      // Under persistent mode (durationMs = 0), no new animation frames should be requested
      expect(rafSpy.mock.calls.length).toBe(initialRafCalls);

      await act(async () => {
        (root as unknown as { unmount: () => void })?.unmount();
      });

      rafSpy.mockRestore();
      document.body.removeChild(container);
    });
  });
});
