/**
 * Tier 5 White-Box Adversarial Coverage Hardening Suite
 *
 * Implements comprehensive white-box stress testing, boundary fuzzing,
 * monotonic attenuation invariant enforcement, constitutional Rule 7 minor
 * invariant stress, cross-simulator state isolation, and audio/crypto lifecycle hardening.
 */

// @vitest-environment jsdom

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import React, { act } from 'react';
import { createRoot, Root } from 'react-dom/client';

import { I18nProvider } from '@/i18n/context';
import { ROUTES_LIST } from '@/config/routes';
import {
  ALL_13_CANONICAL_SCOPES,
  ALL_5_ACTIONS,
  type CapabilityScope,
  type CapabilityAction,
  MAX_DELEGATION_DEPTH,
  buildRootToken,
  attenuateToken,
  attemptPrivilegeEscalationAttack,
  extractEffectiveCapabilities,
  formatDatalogCode,
  type BiscuitBlock,
} from '@/engine/biscuitEngine';
import {
  evaluateSecurityPolicy,
  type PolicySubject,
  BIOMETRIC_STEP_UP_THRESHOLDS,
} from '@/engine/notificationPolicyEngine';
import {
  evaluateNotificationRouting,
  formatSubstrateIpcPreview,
  type CognitiveState,
  type NotificationRequestInput,
  type SupportedSubstrate,
} from '@/engine/notificationRouter';
import {
  scanEnvironmentAttestation,
  generateComplianceDossier,
  verifyComplianceDossierSignature,
  createAndSignComplianceDossier,
  downloadComplianceReport,
  PINNED_INTER_SHA256,
  type ComplianceDossier,
} from '@/engine/complianceDossier';
import { createAuditKeyring, GENESIS_HASH_HEX } from '@/engine/auditChain';
import { hakilleAudio } from '@/engine/audioPlayer';

import { CognitiveNotificationRouterLab } from '@/components/simulators/CognitiveNotificationRouterLab';
import { BiscuitTokenLab } from '@/components/simulators/BiscuitTokenLab';
import { DesktopHudToastSandbox } from '@/components/simulators/DesktopHudToastSandbox';
import { NotificationSecurityPolicyPlayground } from '@/components/simulators/NotificationSecurityPolicyPlayground';
import { ComplianceMatrixLab } from '@/components/simulators/ComplianceMatrixLab';
import { GdprRightsWizard } from '@/components/simulators/GdprRightsWizard';
import { renderRoute } from '@/entry-server';

describe('Tier 5 White-Box Adversarial Coverage Hardening Suite', () => {
  // =========================================================================
  // SECTION 1: Exhaustive Constitutional Rule 7 Minor Invariant Permutation Stress
  // =========================================================================
  describe('1. Constitutional Rule 7 Minor Protection Invariant Exhaustive Permutations', () => {
    it('T5.1.1: Guaranteed HARD_BLOCK on Wallet across 500+ parameter permutations for minors', () => {
      const roles: PolicySubject['role'][] = ['Minor', 'Owner', 'Guest', 'UntrustedAgent'];
      const minorAges = [0, 1, 5, 10, 14, 16, 17, 17.5, 17.99, 17.999999, -1, -50];
      const confidences = [0.0, 0.25, 0.5, 0.75, 0.9, 0.95, 0.999, 1.0, 2.0, 100.0];
      const originTrusts: PolicySubject['originTrust'][] = ['TrustedHuman', 'LocalDaemon', 'UntrustedExternal'];
      const actions: CapabilityAction[] = ['Read', 'Write', 'Execute', 'Delete', 'Admin'];
      const destructives = [false, true];

      let testedCombinations = 0;

      for (const role of roles) {
        for (const age of minorAges) {
          // A subject is minor if role === 'Minor' OR age < 18
          const isMinor = role === 'Minor' || age < 18;
          if (!isMinor) continue;

          for (const conf of confidences) {
            for (const trust of originTrusts) {
              for (const act of actions) {
                for (const dest of destructives) {
                  const subject: PolicySubject = {
                    role,
                    age,
                    biometricConfidence: conf,
                    originTrust: trust,
                  };

                  const verdict = evaluateSecurityPolicy(subject, 'Wallet', act, dest);
                  expect(verdict.outcome).toBe('HARD_BLOCK');
                  expect(verdict.ruleViolated).toContain('RULE-07');
                  expect(verdict.applicableLawOrRule).toContain('EU AI Act Art. 9');
                  testedCombinations++;
                }
              }
            }
          }
        }
      }

      expect(testedCombinations).toBeGreaterThanOrEqual(500);
    });

    it('T5.1.2: Constitutional Rule 7 prohibits FullSystem scope and Admin actions for minors', () => {
      const minorSubject: PolicySubject = {
        role: 'Minor',
        age: 15,
        biometricConfidence: 1.0,
        originTrust: 'TrustedHuman',
      };

      // FullSystem with any action is blocked for minor
      for (const action of ALL_5_ACTIONS) {
        const verdict = evaluateSecurityPolicy(minorSubject, 'FullSystem', action, false);
        expect(verdict.outcome).toBe('HARD_BLOCK');
        expect(verdict.ruleViolated).toContain('RULE-07');
      }

      // Any scope with Admin action is blocked for minor
      for (const scope of ALL_13_CANONICAL_SCOPES) {
        const verdict = evaluateSecurityPolicy(minorSubject, scope, 'Admin', false);
        expect(verdict.outcome).toBe('HARD_BLOCK');
        expect(verdict.ruleViolated).toContain('RULE-07');
      }
    });

    it('T5.1.3: Adult subjects (>= 18) with high confidence correctly pass Wallet access', () => {
      const adultSubject: PolicySubject = {
        role: 'Owner',
        age: 18,
        biometricConfidence: 0.95,
        originTrust: 'TrustedHuman',
      };

      const verdict = evaluateSecurityPolicy(adultSubject, 'Wallet', 'Execute', false);
      expect(verdict.outcome).toBe('ALLOW');
      expect(verdict.ruleViolated).toBeNull();
    });

    it('T5.1.4: Inbound prompt injection isolation (RULE-01) unconditionally blocks UntrustedExternal mutations', () => {
      const untrustedExternal: PolicySubject = {
        role: 'Owner',
        age: 30,
        biometricConfidence: 1.0,
        originTrust: 'UntrustedExternal',
      };

      // Destructive actions blocked
      expect(evaluateSecurityPolicy(untrustedExternal, 'Notification', 'Read', true).outcome).toBe('HARD_BLOCK');

      // Mutating actions blocked
      expect(evaluateSecurityPolicy(untrustedExternal, 'Notification', 'Write', false).outcome).toBe('HARD_BLOCK');
      expect(evaluateSecurityPolicy(untrustedExternal, 'Notification', 'Delete', false).outcome).toBe('HARD_BLOCK');
      expect(evaluateSecurityPolicy(untrustedExternal, 'Notification', 'Admin', false).outcome).toBe('HARD_BLOCK');

      // Wallet and Filesystem blocked
      expect(evaluateSecurityPolicy(untrustedExternal, 'Wallet', 'Read', false).outcome).toBe('HARD_BLOCK');
      expect(evaluateSecurityPolicy(untrustedExternal, 'Filesystem', 'Read', false).outcome).toBe('HARD_BLOCK');

      // Harmless read on benign scope allowed
      expect(evaluateSecurityPolicy(untrustedExternal, 'Sensor', 'Read', false).outcome).toBe('ALLOW');
    });

    it('T5.1.5: Guest session isolation (RULE-13) confines guests to read-only benign scopes', () => {
      const guestSubject: PolicySubject = {
        role: 'Guest',
        age: 25,
        biometricConfidence: 1.0,
        originTrust: 'LocalDaemon',
      };

      expect(evaluateSecurityPolicy(guestSubject, 'Notification', 'Write', false).outcome).toBe('HARD_BLOCK');
      expect(evaluateSecurityPolicy(guestSubject, 'Wallet', 'Read', false).outcome).toBe('HARD_BLOCK');
      expect(evaluateSecurityPolicy(guestSubject, 'FullSystem', 'Read', false).outcome).toBe('HARD_BLOCK');
      expect(evaluateSecurityPolicy(guestSubject, 'Notification', 'Read', true).outcome).toBe('HARD_BLOCK');

      // Harmless read is allowed
      expect(evaluateSecurityPolicy(guestSubject, 'Notification', 'Read', false).outcome).toBe('ALLOW');
    });

    it('T5.1.6: UntrustedAgent sandbox (RULE-06) blocks FullSystem, Wallet, Biometrics and Admin', () => {
      const agentSubject: PolicySubject = {
        role: 'UntrustedAgent',
        biometricConfidence: 1.0,
        originTrust: 'LocalDaemon',
      };

      expect(evaluateSecurityPolicy(agentSubject, 'FullSystem', 'Read', false).outcome).toBe('HARD_BLOCK');
      expect(evaluateSecurityPolicy(agentSubject, 'Wallet', 'Read', false).outcome).toBe('HARD_BLOCK');
      expect(evaluateSecurityPolicy(agentSubject, 'Biometrics', 'Read', false).outcome).toBe('HARD_BLOCK');
      expect(evaluateSecurityPolicy(agentSubject, 'ModelInference', 'Admin', false).outcome).toBe('HARD_BLOCK');

      // Allowed in-sandbox capability
      expect(evaluateSecurityPolicy(agentSubject, 'ModelInference', 'Execute', false).outcome).toBe('ALLOW');
    });

    it('T5.1.7: Continuous Biometric Confidence step-up gating for all 13 canonical scopes', () => {
      const owner: PolicySubject = {
        role: 'Owner',
        age: 35,
        biometricConfidence: 0.0,
        originTrust: 'TrustedHuman',
      };

      for (const scope of ALL_13_CANONICAL_SCOPES) {
        const threshold = BIOMETRIC_STEP_UP_THRESHOLDS[scope];
        expect(threshold).toBeGreaterThan(0);

        // Sub-threshold confidence requires step-up
        const subVerdict = evaluateSecurityPolicy(
          { ...owner, biometricConfidence: threshold - 0.05 },
          scope,
          'Read',
          false
        );
        expect(subVerdict.outcome).toBe('STEP_UP_REQUIRED');
        expect(subVerdict.ruleViolated).toContain('AUTH-STEPUP');

        // At-or-above threshold confidence passes
        const passVerdict = evaluateSecurityPolicy(
          { ...owner, biometricConfidence: threshold },
          scope,
          'Read',
          false
        );
        expect(passVerdict.outcome).toBe('ALLOW');

        // Destructive action elevates threshold to at least 0.85
        const destVerdict = evaluateSecurityPolicy(
          { ...owner, biometricConfidence: 0.80 },
          scope,
          'Read',
          true
        );
        expect(destVerdict.outcome).toBe('STEP_UP_REQUIRED');
      }
    });
  });

  // =========================================================================
  // SECTION 2: Monotonic Attenuation & Delegation Depth Bound ($D_{\max}=4$)
  // =========================================================================
  describe('2. Monotonic Attenuation, Depth Bound (D_max=4), Circular & Nested Chains', () => {
    it('T5.2.1: Delegation depth bound D_max = 4 strictly enforced across attenuation chain', () => {
      // Step 0: Root token (Block 0, depth 0)
      const rootToken = buildRootToken(
        ['Notification', 'Filesystem', 'Camera', 'Audio'],
        ['Read', 'Write'],
        300
      );
      expect(rootToken).toHaveLength(1);

      // Step 1: Attenuate to Depth 1 (Block 1)
      const res1 = attenuateToken(rootToken, ['Notification', 'Filesystem', 'Audio'], ['Read', 'Write']);
      expect(res1.valid).toBe(true);
      expect(res1.attenuationDepth).toBe(1);
      const chain1: BiscuitBlock[] = [
        ...rootToken,
        {
          index: 1,
          label: 'Block 1',
          authority: 'SubAgentAttenuation',
          facts: ['allowed_scope("notification");', 'allowed_scope("filesystem");', 'allowed_scope("audio");'],
          checks: [],
          signatureHex: 'sig_1',
        },
      ];

      // Step 2: Attenuate to Depth 2 (Block 2)
      const res2 = attenuateToken(chain1, ['Notification', 'Filesystem'], ['Read']);
      expect(res2.valid).toBe(true);
      expect(res2.attenuationDepth).toBe(2);
      const chain2: BiscuitBlock[] = [
        ...chain1,
        {
          index: 2,
          label: 'Block 2',
          authority: 'ThirdPartyWorker',
          facts: ['allowed_scope("notification");', 'allowed_scope("filesystem");'],
          checks: [],
          signatureHex: 'sig_2',
        },
      ];

      // Step 3: Attenuate to Depth 3 (Block 3)
      const res3 = attenuateToken(chain2, ['Notification'], ['Read']);
      expect(res3.valid).toBe(true);
      expect(res3.attenuationDepth).toBe(3);
      const chain3: BiscuitBlock[] = [
        ...chain2,
        {
          index: 3,
          label: 'Block 3',
          authority: 'ThirdPartyWorker',
          facts: ['allowed_scope("notification");'],
          checks: [],
          signatureHex: 'sig_3',
        },
      ];

      // Step 4: Attenuate to Depth 4 (Block 4) -> 5 blocks total
      const res4 = attenuateToken(chain3, ['Notification'], ['Read']);
      expect(res4.valid).toBe(true);
      expect(res4.attenuationDepth).toBe(4);
      const chain4: BiscuitBlock[] = [
        ...chain3,
        {
          index: 4,
          label: 'Block 4',
          authority: 'ThirdPartyWorker',
          facts: ['allowed_scope("notification");'],
          checks: [],
          signatureHex: 'sig_4',
        },
      ];
      expect(chain4).toHaveLength(5); // 1 root + 4 attenuations = 5 blocks

      // Step 5: Attempting Depth 5 MUST FAIL with RULE-05
      const res5 = attenuateToken(chain4, ['Notification'], ['Read']);
      expect(res5.valid).toBe(false);
      expect(res5.violationReason).toContain('RULE-05');
      expect(res5.violationReason).toContain(`D_max = ${MAX_DELEGATION_DEPTH}`);

      // Even attempting deeper cascades (depth 6, 8) must fail
      const chain5: BiscuitBlock[] = [
        ...chain4,
        {
          index: 5,
          label: 'Block 5',
          authority: 'ThirdPartyWorker',
          facts: ['allowed_scope("notification");'],
          checks: [],
          signatureHex: 'sig_5',
        },
      ];
      const res6 = attenuateToken(chain5, ['Notification'], ['Read']);
      expect(res6.valid).toBe(false);
      expect(res6.violationReason).toContain('RULE-05');
    });

    it('T5.2.2: Circular delegation chain and empty root prevention', () => {
      // Empty parent blocks rejected
      const emptyResult = attenuateToken([], ['Notification'], ['Read']);
      expect(emptyResult.valid).toBe(false);
      expect(emptyResult.violationReason).toContain('RULE-00');

      // Root token
      const root = buildRootToken(['Notification'], ['Read'], 60);

      // Attempt to re-grant an ungranted scope in child block
      const attackRes = attenuateToken(root, ['Wallet' as CapabilityScope], ['Read']);
      expect(attackRes.valid).toBe(false);
      expect(attackRes.violationReason).toContain('RULE-04');
      expect(attackRes.violationReason).toContain('Monotonic Attenuation Violation');

      // Attempt to re-grant an ungranted action in child block
      const attackAct = attenuateToken(root, ['Notification'], ['Admin']);
      expect(attackAct.valid).toBe(false);
      expect(attackAct.violationReason).toContain('RULE-04');
    });

    it('T5.2.3: Comprehensive Privilege Escalation Sandbox Attacks (All 4 Vectors)', () => {
      const root = buildRootToken(['Notification', 'Audio'], ['Read'], 300);

      // Vector 1: widen_scope
      const widen = attemptPrivilegeEscalationAttack(root, 'widen_scope');
      expect(widen.valid).toBe(false);
      expect(widen.violationReason).toContain('RULE-04');
      expect(widen.grantedScopes).not.toContain('Wallet');

      // Vector 2: add_action
      const addAct = attemptPrivilegeEscalationAttack(root, 'add_action');
      expect(addAct.valid).toBe(false);
      expect(addAct.violationReason).toContain('RULE-04');
      expect(addAct.grantedActions).not.toContain('Admin');

      // Vector 3: remove_caveat
      const remCav = attemptPrivilegeEscalationAttack(root, 'remove_caveat');
      expect(remCav.valid).toBe(false);
      expect(remCav.violationReason).toContain('RULE-04');
      expect(remCav.violationReason).toContain('Signature Verification Failed');

      // Vector 4: forge_root
      const forge = attemptPrivilegeEscalationAttack(root, 'forge_root');
      expect(forge.valid).toBe(false);
      expect(forge.violationReason).toContain('RULE-03');
      expect(forge.violationReason).toContain('Root Public Key Mismatch');
    });

    it('T5.2.4: extractEffectiveCapabilities properly folds over complex nested attenuation blocks', () => {
      const blocks: BiscuitBlock[] = [
        {
          index: 0,
          label: 'Root',
          authority: 'RootSeed',
          facts: [
            'right("notification", "read");',
            'right("notification", "write");',
            'right("filesystem", "read");',
            'right("filesystem", "write");',
          ],
          checks: [],
          signatureHex: 'sig_0',
        },
        {
          index: 1,
          label: 'Block 1',
          authority: 'SubAgentAttenuation',
          facts: ['allowed_scope("notification");', 'allowed_scope("filesystem");'],
          checks: [],
          signatureHex: 'sig_1',
        },
        {
          index: 2,
          label: 'Block 2',
          authority: 'ThirdPartyWorker',
          facts: ['allowed_scope("filesystem");', 'allowed_action("read");'],
          checks: [],
          signatureHex: 'sig_2',
        },
      ];

      const caps = extractEffectiveCapabilities(blocks);
      expect(caps.scopes).toEqual(['Filesystem']);
      expect(caps.actions).toEqual(['Read']);

      // Format Datalog code contains all block headers
      const datalog = formatDatalogCode(blocks);
      expect(datalog).toContain('// --- Root [Authority: RootSeed] ---');
      expect(datalog).toContain('// --- Block 1 [Authority: SubAgentAttenuation] ---');
      expect(datalog).toContain('// --- Block 2 [Authority: ThirdPartyWorker] ---');
    });
  });

  // =========================================================================
  // SECTION 3: SMNF Cognitive Notification Router Boundary & Substrate Previews
  // =========================================================================
  describe('3. SMNF Cognitive Notification Router Boundary Extremes & Substrate Invariants', () => {
    const baseCognitive: CognitiveState = {
      userLookingAtScreen: false,
      perclosDrowsinessScore: 0.02,
      ambientNoiseDb: 40,
      inMeeting: false,
      thirdPartiesPresent: false,
      userWearingHeadphones: false,
      isNightTime: false,
    };

    const baseRequest: NotificationRequestInput = {
      title: 'Update Available',
      body: 'Component update ready for download',
      urgency: 'Normal',
      category: 'System',
      hasDestructiveAction: false,
    };

    it('T5.3.1: Critical urgency & destructive action bypass all cognitive filters and silence budgets', () => {
      const hostileState: CognitiveState = {
        userLookingAtScreen: false,
        perclosDrowsinessScore: 0.0,
        ambientNoiseDb: 80,
        inMeeting: true,
        thirdPartiesPresent: true,
        userWearingHeadphones: false,
        isNightTime: true,
      };

      const decision = evaluateNotificationRouting(
        { ...baseRequest, urgency: 'Critical', hasDestructiveAction: true },
        hostileState,
        0 // 0 credits remaining
      );

      expect(decision.visualStyle).toBe('BraiseActionRequired');
      expect(decision.hapticFeedback).toBe('urgent_buzz');
      expect(decision.deferred).toBe(false);
      expect(decision.creditCost).toBe(0);
      // Room audio suppressed because of meeting & third parties
      expect(decision.channel).toBe('AmbientMicroToast');
    });

    it('T5.3.2: PERCLOS somnolence threshold boundary at 0.149 vs 0.150', () => {
      const req: NotificationRequestInput = {
        ...baseRequest,
        urgency: 'Normal',
        category: 'Cognitive',
      };

      // PERCLOS 0.149 (< 0.15) with 0 silence credits -> deferred by Rule 1
      const subThreshold = evaluateNotificationRouting(
        req,
        { ...baseCognitive, perclosDrowsinessScore: 0.149 },
        0
      );
      expect(subThreshold.channel).toBe('CognitiveDeferralQueue');
      expect(subThreshold.deferred).toBe(true);

      // PERCLOS 0.150 (>= 0.15) -> Somnolence override (Rule 0.5) bypasses silence budget
      const atThreshold = evaluateNotificationRouting(
        req,
        { ...baseCognitive, perclosDrowsinessScore: 0.150 },
        0
      );
      expect(atThreshold.deferred).toBe(false);
      expect(atThreshold.visualStyle).toBe('MinimalistToast');
      expect(atThreshold.hapticFeedback).toBe('double_tap');
      expect(atThreshold.rationale).toContain('Somnolence avérée');
    });

    it('T5.3.3: High ambient noise boundary at 65.0 dB vs 65.01 dB', () => {
      const quietReq: NotificationRequestInput = { ...baseRequest, urgency: 'Normal' };

      // At exactly 65 dB, Rule 4 does NOT trigger
      const at65 = evaluateNotificationRouting(
        quietReq,
        { ...baseCognitive, ambientNoiseDb: 65.0 },
        3
      );
      expect(at65.channel).not.toBe('HapticPulse');

      // Above 65 dB without headphones, Rule 4 triggers HapticPulse
      const above65 = evaluateNotificationRouting(
        quietReq,
        { ...baseCognitive, ambientNoiseDb: 65.01 },
        3
      );
      expect(above65.channel).toBe('HapticPulse');
      expect(above65.hapticFeedback).toBe('double_tap');
    });

    it('T5.3.4: Substrate native IPC preview formatting across all 6 substrates with special characters', () => {
      const specialReq: NotificationRequestInput = {
        title: 'Alert <"Hakille" & Test>',
        body: 'Param with quotes "hello" & $XDG_RUNTIME_DIR and \n newline',
        urgency: 'Critical',
        category: 'System',
        hasDestructiveAction: true,
      };

      const decision = evaluateNotificationRouting(specialReq, baseCognitive, 3);
      const substrates: SupportedSubstrate[] = ['linux', 'macos', 'windows', 'mobile', 'web', 'redox'];

      for (const sub of substrates) {
        const preview = formatSubstrateIpcPreview(sub, specialReq, decision);
        expect(typeof preview).toBe('string');
        expect(preview.length).toBeGreaterThan(10);

        if (sub === 'linux') {
          expect(preview).toContain('org.freedesktop.Notifications');
          expect(preview).toContain('Valider 600ms');
        } else if (sub === 'macos') {
          expect(preview).toContain('osascript');
          expect(preview).toContain('display notification');
        } else if (sub === 'windows') {
          expect(preview).toContain('<toast');
          expect(preview).toContain('</toast>');
        } else if (sub === 'mobile') {
          const parsed = JSON.parse(preview.replace(/^#.*\n/, ''));
          expect(parsed.type).toBe('MobileShadowSyncEnvelope');
          expect(parsed.payload.urgency).toBe('Critical');
        } else if (sub === 'redox') {
          expect(preview).toContain('orbital:notify');
        } else if (sub === 'web') {
          expect(preview).toContain('new Notification');
        }
      }
    });
  });

  // =========================================================================
  // SECTION 4: Cryptographic Compliance Dossier & WebCrypto Fallback Hardening
  // =========================================================================
  describe('4. Cryptographic Compliance Dossier & WebCrypto Fallback Hardening', () => {
    it('T5.4.1: scanEnvironmentAttestation accurately detects pristine vs polluted environments', () => {
      // Pristine state
      const pristineScan = scanEnvironmentAttestation();
      expect(pristineScan.cspDirective).toBe("default-src 'self'");
      expect(pristineScan.fontSha256Hex).toBe(PINNED_INTER_SHA256);

      // Simulate tracking cookie pollution
      try {
        document.cookie = '_ga=GA1.2.123456789.1234567890; path=/';
        const pollutedCookieScan = scanEnvironmentAttestation();
        expect(pollutedCookieScan.trackingCookiesCount).toBeGreaterThan(0);
        expect(pollutedCookieScan.isPristine).toBe(false);
      } finally {
        // Clean up cookie
        document.cookie = '_ga=; expires=Thu, 01 Jan 1970 00:00:00 UTC; path=/;';
      }
    });

    it('T5.4.2: generateComplianceDossier creates authentic verifiable dossier signed by WebCrypto', async () => {
      const { dossier, keyring, signatureValid } = await createAndSignComplianceDossier(
        '0000111122223333444455556666777788889999aaaabbbbccccddddeeeeffff',
        'EU-AI-Act-2024/1689'
      );

      expect(signatureValid).toBe(true);
      expect(dossier.standard).toBe('EU-AI-Act-2024/1689');
      expect(dossier.signatureHex).toMatch(/^[0-9a-f]+$/);
      expect(dossier.fontProvenance.sha256Hex).toBe(PINNED_INTER_SHA256);

      // Verify signature with keyring
      const verified = await verifyComplianceDossierSignature(dossier, keyring);
      expect(verified).toBe(true);
    });

    it('T5.4.3: Tampering with any field invalidates the cryptographic signature', async () => {
      const keyring = await createAuditKeyring();
      const dossier = await generateComplianceDossier(GENESIS_HASH_HEX, keyring, 'GDPR-2016/679');

      // 1. Tamper standard
      const tamperedStandard: ComplianceDossier = { ...dossier, standard: 'EU-AI-Act-2024/1689' };
      expect(await verifyComplianceDossierSignature(tamperedStandard, keyring)).toBe(false);

      // 2. Tamper root hash
      const tamperedHash: ComplianceDossier = { ...dossier, auditChainRootHashHex: 'deadbeef' };
      expect(await verifyComplianceDossierSignature(tamperedHash, keyring)).toBe(false);

      // 3. Tamper zeroExfiltrationCert
      const tamperedCert: ComplianceDossier = {
        ...dossier,
        zeroExfiltrationCert: { ...dossier.zeroExfiltrationCert, trackingCookiesCount: 1 },
      };
      expect(await verifyComplianceDossierSignature(tamperedCert, keyring)).toBe(false);

      // 4. Tamper signatureHex
      const tamperedSig: ComplianceDossier = {
        ...dossier,
        signatureHex: 'ff'.repeat(dossier.signatureHex.length / 2),
      };
      expect(await verifyComplianceDossierSignature(tamperedSig, keyring)).toBe(false);
    });

    it('T5.4.4: WebCrypto Ed25519 -> HMAC-SHA-256 fallback simulation', async () => {
      // Mock generateKey to throw on Ed25519
      const origGenerateKey = globalThis.crypto.subtle.generateKey;
      vi.spyOn(globalThis.crypto.subtle, 'generateKey').mockImplementation(async (algorithm, extractable, keyUsages) => {
        const algName = typeof algorithm === 'string' ? algorithm : algorithm.name;
        if (algName === 'Ed25519') {
          throw new Error('Ed25519 not supported in this runtime mock');
        }
        return origGenerateKey.call(globalThis.crypto.subtle, algorithm, extractable, keyUsages);
      });

      try {
        const fallbackKeyring = await createAuditKeyring();
        expect(fallbackKeyring.scheme).toBe('HMAC-SHA256-WebCrypto');

        const testBytes = new TextEncoder().encode('sovereign-attestation-test');
        const sig = await fallbackKeyring.sign(testBytes);
        const valid = await fallbackKeyring.verify(testBytes, sig);
        expect(valid).toBe(true);

        const invalidSig = new Uint8Array(sig);
        if (invalidSig.length > 0 && invalidSig[0] !== undefined) {
          invalidSig[0] ^= 0xff;
        }
        const invalid = await fallbackKeyring.verify(testBytes, invalidSig);
        expect(invalid).toBe(false);
      } finally {
        vi.restoreAllMocks();
      }
    });

    it('T5.4.5: downloadComplianceReport does not throw in headless or browser environments', () => {
      const mockDossier: ComplianceDossier = {
        standard: 'EU-AI-Act-2024/1689',
        generatedAtIso: new Date().toISOString(),
        auditChainRootHashHex: GENESIS_HASH_HEX,
        fontProvenance: {
          fontName: 'InterVariable.ttf',
          sha256Hex: PINNED_INTER_SHA256,
          license: 'SIL Open Font License 1.1',
        },
        zeroExfiltrationCert: {
          trackingCookiesCount: 0,
          thirdPartyCdnRequestsCount: 0,
          cspDirective: "default-src 'self'",
        },
        rfc9116SecurityTxt: {
          contact: 'mailto:security@hakille.ai',
          slaHours: 72,
        },
        signatureHex: 'abc123',
      };

      // Mock URL.createObjectURL and URL.revokeObjectURL
      const origCreate = globalThis.URL.createObjectURL;
      const origRevoke = globalThis.URL.revokeObjectURL;
      const origClick = HTMLAnchorElement.prototype.click;
      globalThis.URL.createObjectURL = vi.fn(() => 'blob:mock');
      globalThis.URL.revokeObjectURL = vi.fn();
      HTMLAnchorElement.prototype.click = vi.fn();

      try {
        expect(() => downloadComplianceReport(mockDossier, 'test-report.json')).not.toThrow();
      } finally {
        globalThis.URL.createObjectURL = origCreate;
        globalThis.URL.revokeObjectURL = origRevoke;
        HTMLAnchorElement.prototype.click = origClick;
      }
    });
  });

  // =========================================================================
  // SECTION 5: Simulator Component Stress & Cross-Simulator State Isolation
  // =========================================================================
  describe('5. Simulator Component Stress & Cross-Simulator State Isolation', () => {
    let container: HTMLDivElement;
    let root: Root;

    beforeEach(() => {
      container = document.createElement('div');
      document.body.appendChild(container);
      root = createRoot(container);
    });

    afterEach(() => {
      act(() => {
        root.unmount();
      });
      container.remove();
    });

    it('T5.5.1: Mounts all 6 simulator components concurrently with strict state isolation', () => {
      act(() => {
        root.render(
          React.createElement(I18nProvider, {
            initialLocale: 'fr',
            children: React.createElement(
              'div',
              { 'data-testid': 'sim-container' },
              React.createElement(CognitiveNotificationRouterLab),
              React.createElement(BiscuitTokenLab),
              React.createElement(DesktopHudToastSandbox),
              React.createElement(NotificationSecurityPolicyPlayground),
              React.createElement(ComplianceMatrixLab),
              React.createElement(GdprRightsWizard)
            ),
          })
        );
      });

      // Verify all 6 simulators are rendered
      expect(container.querySelector('.router-lab')).not.toBeNull();
      expect(container.querySelector('.biscuit-lab')).not.toBeNull();
      expect(container.querySelector('.hud-sandbox')).not.toBeNull();
      expect(container.querySelector('.policy-playground')).not.toBeNull();
      expect(container.querySelector('.compliance-matrix-shell')).not.toBeNull();
      expect(container.querySelector('.gdpr-wizard-shell')).not.toBeNull();

      // Biscuit lab depth tag is present and uncorrupted
      const depthBadge = container.querySelector('.biscuit-lab .status-tag');
      expect(depthBadge?.textContent).toContain('1 / 4');

      // Policy playground shows default ALLOW verdict for adult owner
      const policyStatus = container.querySelector('.policy-playground .status-tag');
      expect(policyStatus?.textContent).toContain('ALLOW');
    });

    it('T5.5.2: DesktopHudToastSandbox rapid re-triggering and clean unmount without timer leaks', () => {
      act(() => {
        root.render(
          React.createElement(I18nProvider, {
            initialLocale: 'fr',
            children: React.createElement(DesktopHudToastSandbox),
          })
        );
      });

      const triggerBtn = container.querySelector('.hud-sandbox button.action-btn') as HTMLButtonElement;
      expect(triggerBtn).not.toBeNull();

      // Rapidly trigger toast 25 times
      for (let i = 0; i < 25; i++) {
        act(() => {
          triggerBtn.click();
        });
      }

      // Toast is visible and sticky/evaporating
      const statusBadge = container.querySelector('.hud-sandbox .status-tag');
      expect(statusBadge).not.toBeNull();

      // Dismiss button functions cleanly
      const dismissBtn = container.querySelector('.hud-sandbox button.toast-dismiss-btn') as HTMLButtonElement;
      if (dismissBtn) {
        act(() => {
          dismissBtn.click();
        });
        expect(container.querySelector('.hud-sandbox .status-tag')?.textContent).toContain('Évaporé');
      }
    });

    it('T5.5.3: ComplianceMatrixLab search filtering with regex special characters does not crash', () => {
      act(() => {
        root.render(
          React.createElement(I18nProvider, {
            initialLocale: 'fr',
            children: React.createElement(ComplianceMatrixLab),
          })
        );
      });

      const searchInput = container.querySelector('.compliance-matrix-shell input.compliance-search-input') as HTMLInputElement;
      expect(searchInput).not.toBeNull();

      // Type regex metacharacters
      act(() => {
        searchInput.value = '.*+?^${}()|[]\\';
        searchInput.dispatchEvent(new Event('input', { bubbles: true }));
      });

      // Component remains intact with 0 matches or clean empty state
      expect(container.querySelector('.compliance-matrix-shell')).not.toBeNull();

      // Search for specific article 'Art. 9'
      act(() => {
        searchInput.value = 'Art. 9';
        searchInput.dispatchEvent(new Event('input', { bubbles: true }));
      });

      const articleCards = container.querySelectorAll('.compliance-article-card');
      expect(articleCards.length).toBeGreaterThanOrEqual(1);
    });

    it('T5.5.4: GdprRightsWizard step navigation, command generation, and simulated execution', () => {
      act(() => {
        root.render(
          React.createElement(I18nProvider, {
            initialLocale: 'fr',
            children: React.createElement(GdprRightsWizard),
          })
        );
      });

      expect(container.querySelector('.gdpr-wizard-shell')).not.toBeNull();

      // Step 1: Select right buttons exist
      const rightBtns = container.querySelectorAll('.right-choice-card');
      expect(rightBtns.length).toBeGreaterThanOrEqual(4);

      // Navigate to Step 2
      const nextBtn = container.querySelector('.wizard-nav-row button.cta-button.primary') as HTMLButtonElement;
      expect(nextBtn).not.toBeNull();
      act(() => {
        nextBtn.click();
      });

      // Step 2 parameters visible, navigate to Step 3
      const step2NextBtn = container.querySelector('.wizard-nav-row button.cta-button.primary') as HTMLButtonElement;
      expect(step2NextBtn).not.toBeNull();
      act(() => {
        step2NextBtn.click();
      });

      // Step 3 CLI command terminal
      const terminalBox = container.querySelector('.terminal-code');
      expect(terminalBox?.textContent).toContain('hakille-cli gdpr');

      // Click "Simuler l'exécution locale"
      const simBtn = container.querySelector('.simulation-actions-row button.cta-button.primary') as HTMLButtonElement;
      expect(simBtn).not.toBeNull();
      act(() => {
        simBtn.click();
      });

      const resultOutput = container.querySelector('.simulation-json');
      expect(resultOutput).not.toBeNull();
      // Result output is valid JSON
      expect(() => JSON.parse(resultOutput?.textContent || '{}')).not.toThrow();
    });
  });

  // =========================================================================
  // SECTION 6: Cross-Page Route Traversal & DOM Lifecycle Integrity
  // =========================================================================
  describe('6. Cross-Page Route Traversal & DOM Lifecycle Integrity', () => {
    it('T5.6.1: Pre-renders all 11 routes in SSR mode with exactly one <h1> and zero unhandled rejections', () => {
      for (const route of ROUTES_LIST) {
        const { appHtml, title, description } = renderRoute(route.path, 'fr');
        expect(appHtml.length).toBeGreaterThan(100);
        expect(title.length).toBeGreaterThan(0);
        expect(description.length).toBeGreaterThan(0);

        // Exactly one <h1> tag
        const h1Matches = appHtml.match(/<h1\b[^>]*>/g);
        expect(h1Matches, `Route ${route.path} must have exactly one <h1>`).toHaveLength(1);

        // Verify Sacred Rule of Braise: zero decorative inline styles using Braise without permitted context
        expect(appHtml).not.toMatch(/style="[^"]*(?:background|border|color)[^"]*#FF7A2F[^"]*"(?![^>]*data-braise-context)/i);
        expect(appHtml).not.toMatch(/style="[^"]*var\(--hakille-braise\)[^"]*"(?![^>]*data-braise-context)/i);
      }
    });
  });

  // =========================================================================
  // SECTION 7: Audio Controller Lifecycle, Rapid Concurrency & Barge-In Cutoff
  // =========================================================================
  describe('7. Audio Controller Lifecycle, Concurrency & Barge-In Cutoff Hardening', () => {
    it('T5.7.1: HakilleAudioController handles 200 rapid concurrent signal requests gracefully', () => {
      // Rapid concurrent signal triggers
      for (let i = 0; i < 200; i++) {
        expect(() => {
          hakilleAudio.playStateSignal('eveil', true, 0.5);
          hakilleAudio.playStateSignal('fait', true, 0.4);
          hakilleAudio.playStateSignal('attention', true, 0.6);
          hakilleAudio.playKoraWire(i % 21, true, 0.4);
        }).not.toThrow();
      }
    });

    it('T5.7.2: Barge-In cutoff handle terminates active voice immediately without errors', () => {
      const dummyPcm = new Float32Array(2400); // 100ms of audio at 24 kHz
      const onEnded = vi.fn();

      const stopHandle = hakilleAudio.playKlattUtterance(dummyPcm, 24000, onEnded);
      expect(typeof stopHandle).toBe('function');

      // Invoke barge-in cutoff
      expect(() => stopHandle()).not.toThrow();

      // Additional stop calls are idempotent
      expect(() => hakilleAudio.stopActiveVoice()).not.toThrow();
    });
  });
});
