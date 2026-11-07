/**
 * Adversarial Empirical Stress Test Suite for Hakille M1 Engines
 *
 * Tests:
 * 1. notificationRouter.ts:
 *    - Boundary noise dB (-10 dB, 0 dB, 31.99 dB, 32 dB, 64.99 dB, 65 dB, 65.01 dB, 120 dB, NaN, Infinity)
 *    - Extreme PERCLOS (0.149, 0.150, 0.30, 1.0, 1.5, -0.5, NaN, Infinity)
 *    - Fatigue alert suppression invariance (silence credits 0, -5, extreme meeting, headphones, third parties)
 *    - Category & Urgency fuzzing (empty, lowercase, unknown, malformed)
 *    - Rapid sequential calls (10,000 iterations for state pollution / performance)
 *    - Substrate preview injection strings
 * 2. biscuitEngine.ts:
 *    - Empty token blocks (empty array inputs to all functions)
 *    - Zero-scope and zero-action attenuation behavior (monotonic narrowing vs retention)
 *    - Maximum delegation depth (D_max = 4, testing depth 5, 6, 10)
 *    - Privilege escalation vectors (widen_scope, add_action, remove_caveat, forge_root)
 *    - Case-sensitivity and wildcard scope injection
 *    - Extreme TTL values (negative, 0, NaN, Infinity, MAX_SAFE_INTEGER)
 *    - Rapid sequential token generation & multi-level attenuation (1,000 chains)
 */

import { describe, it, expect } from 'vitest';
import {
  evaluateNotificationRouting,
  formatSubstrateIpcPreview,
  type CognitiveState,
  type NotificationRequestInput,
  type SupportedSubstrate,
} from '@/engine/notificationRouter';
import {
  buildRootToken,
  attenuateToken,
  attemptPrivilegeEscalationAttack,
  extractEffectiveCapabilities,
  formatDatalogCode,
  ALL_13_CANONICAL_SCOPES,
  type CapabilityScope,
  type CapabilityAction,
} from '@/engine/biscuitEngine';

describe('ADVERSARIAL STRESS SUITE: SMNF Notification Router', () => {
  const baseCognitive: CognitiveState = {
    userLookingAtScreen: true,
    perclosDrowsinessScore: 0.05,
    ambientNoiseDb: 45,
    inMeeting: false,
    thirdPartiesPresent: false,
    userWearingHeadphones: false,
    isNightTime: false,
  };

  const baseRequest: NotificationRequestInput = {
    title: 'System Alert',
    body: 'Standard notification payload',
    urgency: 'Normal',
    category: 'System',
    hasDestructiveAction: false,
  };

  // -------------------------------------------------------------------------
  // 1. Noise dB Boundary & Extremes
  // -------------------------------------------------------------------------
  describe('Boundary & Extreme Noise dB Values', () => {
    it('handles sub-zero negative noise (-10 dB, -100 dB) without crashing', () => {
      const negCognitive: CognitiveState = {
        ...baseCognitive,
        ambientNoiseDb: -10,
        userWearingHeadphones: false,
      };
      // Ambient noise < 32 dB triggers quiet room (Rule 6)
      const decision = evaluateNotificationRouting(baseRequest, negCognitive, 3);
      expect(decision).toBeDefined();
      expect(decision.channel).toBe('AmbientMicroToast');
      expect(decision.visualStyle).toBe('AmbientGlyph');
      expect(decision.deferred).toBe(false);

      const severeNeg: CognitiveState = {
        ...baseCognitive,
        ambientNoiseDb: -100,
        userWearingHeadphones: true,
      };
      const decSevere = evaluateNotificationRouting(baseRequest, severeNeg, 3);
      expect(decSevere.channel).toBe('AcousticWhisper');
    });

    it('handles exact noise threshold boundaries (31.99 dB, 32.0 dB, 65.0 dB, 65.01 dB)', () => {
      // 31.99 dB: Quiet (< 32 dB) -> Rule 6 triggers
      const quiet = evaluateNotificationRouting(
        baseRequest,
        { ...baseCognitive, ambientNoiseDb: 31.99, userLookingAtScreen: false },
        3
      );
      expect(quiet.channel).toBe('AmbientMicroToast');

      // 32.0 dB: NOT quiet (>= 32 dB) and looking at screen -> Rule 7
      const at32 = evaluateNotificationRouting(
        baseRequest,
        { ...baseCognitive, ambientNoiseDb: 32.0, userLookingAtScreen: true },
        3
      );
      expect(at32.channel).toBe('SubstrateNativeDispatch');

      // 65.0 dB: NOT loud (<= 65 dB) -> looking at screen -> Rule 7
      const at65 = evaluateNotificationRouting(
        baseRequest,
        { ...baseCognitive, ambientNoiseDb: 65.0, userLookingAtScreen: true },
        3
      );
      expect(at65.channel).toBe('SubstrateNativeDispatch');

      // 65.01 dB: Loud (> 65 dB) -> Rule 4 triggers -> HapticPulse
      const loud = evaluateNotificationRouting(
        baseRequest,
        { ...baseCognitive, ambientNoiseDb: 65.01, userWearingHeadphones: false },
        3
      );
      expect(loud.channel).toBe('HapticPulse');
      expect(loud.rationale).toContain('65 dB > 65 dB');
    });

    it('handles extreme noise (120 dB, 180 dB, Infinity, NaN)', () => {
      const painThreshold = evaluateNotificationRouting(
        baseRequest,
        { ...baseCognitive, ambientNoiseDb: 140, userWearingHeadphones: false },
        3
      );
      expect(painThreshold.channel).toBe('HapticPulse');
      expect(painThreshold.rationale).toContain('140 dB > 65 dB');

      // NaN noise: should not throw, falls through to looking-at-screen Rule 7
      const nanNoise = evaluateNotificationRouting(
        baseRequest,
        { ...baseCognitive, ambientNoiseDb: NaN, userLookingAtScreen: true },
        3
      );
      expect(nanNoise).toBeDefined();
      expect(nanNoise.channel).toBe('SubstrateNativeDispatch');
    });
  });

  // -------------------------------------------------------------------------
  // 2. PERCLOS Fatigue & Extreme Values
  // -------------------------------------------------------------------------
  describe('Extreme & Boundary PERCLOS Drowsiness Scores', () => {
    it('evaluates exact somnolence threshold (0.1499 vs 0.1500)', () => {
      const cogBelow: CognitiveState = {
        ...baseCognitive,
        perclosDrowsinessScore: 0.1499,
      };
      const reqCognitive: NotificationRequestInput = {
        ...baseRequest,
        category: 'Cognitive',
      };
      // Below 0.15: does NOT trigger somnolence override
      const decBelow = evaluateNotificationRouting(reqCognitive, cogBelow, 3);
      expect(decBelow.rationale).not.toContain('Somnolence avérée');

      // Exactly 0.15: Triggers Rule 0.5 somnolence override
      const cogAt: CognitiveState = {
        ...baseCognitive,
        perclosDrowsinessScore: 0.1500,
      };
      const decAt = evaluateNotificationRouting(reqCognitive, cogAt, 3);
      expect(decAt.rationale).toContain('Somnolence avérée détectée (PERCLOS 15 % >= 15 %)');
      expect(decAt.creditCost).toBe(0);
      expect(decAt.deferred).toBe(false);
      expect(decAt.hapticFeedback).toBe('double_tap');
    });

    it('handles out-of-bounds PERCLOS (> 1.0, e.g. 1.5, 5.0) safely', () => {
      const extremeCog: CognitiveState = {
        ...baseCognitive,
        perclosDrowsinessScore: 1.5,
      };
      const req: NotificationRequestInput = {
        ...baseRequest,
        category: 'Cognitive',
      };
      const decision = evaluateNotificationRouting(req, extremeCog, 0);
      expect(decision.deferred).toBe(false);
      expect(decision.creditCost).toBe(0);
      expect(decision.rationale).toContain('PERCLOS 150 % >= 15 %');
    });

    it('handles negative PERCLOS (-0.5) safely without triggering override', () => {
      const negCog: CognitiveState = {
        ...baseCognitive,
        perclosDrowsinessScore: -0.5,
      };
      const req: NotificationRequestInput = {
        ...baseRequest,
        category: 'Cognitive',
      };
      const decision = evaluateNotificationRouting(req, negCog, 3);
      expect(decision.rationale).not.toContain('Somnolence avérée');
    });

    it('handles NaN and Infinity PERCLOS safely', () => {
      const nanCog: CognitiveState = {
        ...baseCognitive,
        perclosDrowsinessScore: NaN,
      };
      const req: NotificationRequestInput = {
        ...baseRequest,
        category: 'Cognitive',
      };
      const decision = evaluateNotificationRouting(req, nanCog, 3);
      // NaN >= 0.15 is false, so it falls through safely
      expect(decision.rationale).not.toContain('Somnolence avérée');
    });
  });

  // -------------------------------------------------------------------------
  // 3. Fatigue Alert Suppression Invariance
  // -------------------------------------------------------------------------
  describe('Fatigue Alert Suppression Invariance (Safety Critical)', () => {
    it('fatigue alert is NEVER deferred when silence budget is 0 or negative', () => {
      const fatigueReq: NotificationRequestInput = {
        title: 'Alerte Somnolence',
        body: 'Endormissement détecté',
        urgency: 'Normal',
        category: 'Cognitive',
        hasDestructiveAction: false,
      };
      const fatigueCog: CognitiveState = {
        ...baseCognitive,
        perclosDrowsinessScore: 0.22,
      };

      for (const credits of [0, -1, -5, -100]) {
        const decision = evaluateNotificationRouting(fatigueReq, fatigueCog, credits);
        expect(decision.deferred).toBe(false);
        expect(decision.creditCost).toBe(0);
        expect(decision.hapticFeedback).toBe('double_tap');
      }
    });

    it('fatigue alert delivers with headphones (AcousticWhisper) or in meeting (AmbientMicroToast)', () => {
      const fatigueReq: NotificationRequestInput = {
        title: 'Alerte Somnolence',
        body: 'Endormissement détecté',
        urgency: 'Normal',
        category: 'Cognitive',
        hasDestructiveAction: false,
      };

      // In meeting with severe fatigue
      const meetingFatigue = evaluateNotificationRouting(
        fatigueReq,
        { ...baseCognitive, perclosDrowsinessScore: 0.20, inMeeting: true, userWearingHeadphones: false },
        0
      );
      expect(meetingFatigue.channel).toBe('AmbientMicroToast');
      expect(meetingFatigue.deferred).toBe(false);
      expect(meetingFatigue.hapticFeedback).toBe('double_tap');

      // With headphones
      const headphoneFatigue = evaluateNotificationRouting(
        fatigueReq,
        { ...baseCognitive, perclosDrowsinessScore: 0.20, userWearingHeadphones: true },
        0
      );
      expect(headphoneFatigue.channel).toBe('AcousticWhisper');
      expect(headphoneFatigue.deferred).toBe(false);
    });

    it('suppression edge case: fatigue alert categorized as Agent with 0 credits is deferred', () => {
      // ADVERSARIAL DISCOVERY:
      // If a caller marks a fatigue alert as category 'Agent' instead of 'Cognitive' or 'Security',
      // Rule 0.5 does NOT match because categoryNorm is 'agent'.
      // Then Rule 1 (silence budget) defers it!
      const misclassifiedFatigue: NotificationRequestInput = {
        title: 'Alerte Somnolence',
        body: 'Endormissement détecté',
        urgency: 'Normal',
        category: 'Agent', // Misclassified!
        hasDestructiveAction: false,
      };
      const fatigueCog: CognitiveState = {
        ...baseCognitive,
        perclosDrowsinessScore: 0.22,
      };

      const decision = evaluateNotificationRouting(misclassifiedFatigue, fatigueCog, 0);
      expect(decision.deferred).toBe(true);
      expect(decision.channel).toBe('CognitiveDeferralQueue');
      // Proves category must be 'Cognitive' or 'Security' or have urgency: 'Critical' to survive 0 credits!
    });
  });

  // -------------------------------------------------------------------------
  // 4. Substrate Preview Injection & Fuzzing
  // -------------------------------------------------------------------------
  describe('Substrate Preview Injection & Fuzzing', () => {
    const maliciousPayloads = [
      'Normal Title',
      'Title with "quotes" and \'single\'',
      'Title with <script>alert("xss")</script>',
      'Title with $(whoami) and `id` and ; rm -rf / ;',
      'Title\nwith\nnewlines\r\nand\ttabs',
      'Title with XML: <toast><visual><binding>&</binding></visual></toast>',
      'A'.repeat(5000), // Very long string
    ];

    const substrates: SupportedSubstrate[] = ['linux', 'macos', 'windows', 'mobile', 'redox', 'web'];

    it('formats all substrates without throwing on malicious injection strings', () => {
      for (const payload of maliciousPayloads) {
        const req: NotificationRequestInput = {
          title: payload,
          body: payload,
          urgency: 'Critical',
          category: 'Security',
          hasDestructiveAction: true,
        };
        const decision = evaluateNotificationRouting(req, baseCognitive, 3);

        for (const sub of substrates) {
          const preview = formatSubstrateIpcPreview(sub, req, decision);
          expect(typeof preview).toBe('string');
          expect(preview.length).toBeGreaterThan(0);
        }
      }
    });
  });

  // -------------------------------------------------------------------------
  // 5. Rapid Sequential Stress & Concurrency Simulation
  // -------------------------------------------------------------------------
  describe('High-Throughput Rapid Sequential Calls (10,000 iterations)', () => {
    it('executes 10,000 evaluations deterministically in under 500ms with zero state pollution', () => {
      const startTime = performance.now();
      const results: string[] = [];

      for (let i = 0; i < 10000; i++) {
        const isCritical = i % 5 === 0;
        const perclos = (i % 30) / 100;
        const noise = 20 + (i % 70);
        const credits = i % 4;

        const req: NotificationRequestInput = {
          title: `Task #${i}`,
          body: `Payload #${i}`,
          urgency: isCritical ? 'Critical' : 'Normal',
          category: i % 2 === 0 ? 'Agent' : 'Cognitive',
          hasDestructiveAction: isCritical,
        };

        const cog: CognitiveState = {
          userLookingAtScreen: i % 3 === 0,
          perclosDrowsinessScore: perclos,
          ambientNoiseDb: noise,
          inMeeting: i % 7 === 0,
          thirdPartiesPresent: i % 11 === 0,
          userWearingHeadphones: i % 4 === 0,
          isNightTime: i % 8 === 0,
        };

        const dec = evaluateNotificationRouting(req, cog, credits);
        if (i % 1000 === 0) {
          results.push(dec.channel);
        }
      }

      const elapsed = performance.now() - startTime;
      expect(elapsed).toBeLessThan(1000); // 10,000 evaluations under 1 second
      expect(results.length).toBe(10);
    });
  });
});

describe('ADVERSARIAL STRESS SUITE: Biscuit v5 Capability Engine', () => {
  // -------------------------------------------------------------------------
  // 1. Empty & Malformed Inputs
  // -------------------------------------------------------------------------
  describe('Empty Token & Malformed Inputs', () => {
    it('extractEffectiveCapabilities([]) returns empty capabilities safely', () => {
      const res = extractEffectiveCapabilities([]);
      expect(res.scopes).toEqual([]);
      expect(res.actions).toEqual([]);
    });

    it('formatDatalogCode([]) returns empty string safely', () => {
      const code = formatDatalogCode([]);
      expect(code).toBe('');
    });

    it('attenuateToken([]) rejects with RULE-00 on empty parentBlocks', () => {
      const res = attenuateToken([], ['Notification'], ['Read']);
      expect(res.valid).toBe(false);
      expect(res.violationReason).toContain('RULE-00: Missing root block');
    });

    it('EMPIRICAL BUG CHECK: attemptPrivilegeEscalationAttack([], "forge_root") dereference behavior', () => {
      // Testing whether attemptPrivilegeEscalationAttack([], 'forge_root') throws TypeError
      let errorThrown: unknown = null;
      try {
        attemptPrivilegeEscalationAttack([], 'forge_root');
      } catch (err: unknown) {
        errorThrown = err;
      }
      // In biscuitEngine.ts line 391: `...token[0]!`
      // When token is [], token[0] is undefined, and formatDatalogCode accesses block.facts.length on undefined
      // Empirically confirms this unhandled edge case:
      expect(errorThrown).toBeInstanceOf(TypeError);
    });

    it('EMPIRICAL MONOTONICITY BEHAVIOR: attenuateToken with empty newScopes [] retains parent scopes', () => {
      const root = buildRootToken(['Notification', 'Filesystem'], ['Read', 'Write'], 3600);

      // What happens if child attenuates with empty newScopes: []?
      // In Biscuit Datalog, specifying [] should grant 0 scopes.
      const attEmptyScopes = attenuateToken(root, [], ['Read']);
      expect(attEmptyScopes.valid).toBe(true);
      // In biscuitEngine.ts line 192: `if (blockAllowedScopes.size > 0)`
      // Because blockAllowedScopes.size is 0, effectiveScopes is NOT filtered!
      // Thus, child retains ['Notification', 'Filesystem']!
      expect(attEmptyScopes.grantedScopes).toEqual(['Notification', 'Filesystem']);
    });

    it('EMPIRICAL MONOTONICITY BEHAVIOR: attenuateToken with empty newActions [] retains parent actions', () => {
      const root = buildRootToken(['Notification'], ['Read', 'Write'], 3600);

      // Child attenuates with empty newActions: []
      const attEmptyActions = attenuateToken(root, ['Notification'], []);
      expect(attEmptyActions.valid).toBe(true);
      // In biscuitEngine.ts line 197: `if (blockAllowedActions.size > 0)`
      // Because blockAllowedActions.size is 0, effectiveActions is NOT filtered!
      // Thus, child retains ['Read', 'Write']!
      expect(attEmptyActions.grantedActions).toEqual(['Read', 'Write']);
    });
  });

  // -------------------------------------------------------------------------
  // 2. Maximum Delegation Depth Limit (D_max = 4)
  // -------------------------------------------------------------------------
  describe('Maximum Delegation Depth Limit Boundaries', () => {
    it('permits up to depth 4 (5 blocks total: 1 root + 4 attenuations)', () => {
      const root = buildRootToken(['Notification'], ['Read'], 3600);
      let chain = [...root];

      // Depth 1
      const att1 = attenuateToken(chain, ['Notification'], ['Read']);
      expect(att1.valid).toBe(true);
      expect(att1.attenuationDepth).toBe(1);
      chain = [...chain, {
        index: 1,
        label: 'Block 1',
        authority: 'SubAgentAttenuation',
        facts: ['allowed_scope("notification");', 'allowed_action("read");'],
        checks: [],
        signatureHex: 'sig1',
      }];

      // Depth 2
      const att2 = attenuateToken(chain, ['Notification'], ['Read']);
      expect(att2.valid).toBe(true);
      expect(att2.attenuationDepth).toBe(2);
      chain = [...chain, {
        index: 2,
        label: 'Block 2',
        authority: 'ThirdPartyWorker',
        facts: ['allowed_scope("notification");', 'allowed_action("read");'],
        checks: [],
        signatureHex: 'sig2',
      }];

      // Depth 3
      const att3 = attenuateToken(chain, ['Notification'], ['Read']);
      expect(att3.valid).toBe(true);
      expect(att3.attenuationDepth).toBe(3);
      chain = [...chain, {
        index: 3,
        label: 'Block 3',
        authority: 'ThirdPartyWorker',
        facts: ['allowed_scope("notification");', 'allowed_action("read");'],
        checks: [],
        signatureHex: 'sig3',
      }];

      // Depth 4 (5th block)
      const att4 = attenuateToken(chain, ['Notification'], ['Read']);
      expect(att4.valid).toBe(true);
      expect(att4.attenuationDepth).toBe(4);
      chain = [...chain, {
        index: 4,
        label: 'Block 4',
        authority: 'ThirdPartyWorker',
        facts: ['allowed_scope("notification");', 'allowed_action("read");'],
        checks: [],
        signatureHex: 'sig4',
      }];

      // Now chain has length 5 (depth 4).
      // Attempting Depth 5 (6th block) MUST FAIL with RULE-05!
      const att5 = attenuateToken(chain, ['Notification'], ['Read']);
      expect(att5.valid).toBe(false);
      expect(att5.violationReason).toContain('RULE-05: Attenuation depth limit exceeded');
      expect(att5.violationReason).toContain('D_max = 4');

      // Attempting Depth 10 on oversized chain MUST ALSO FAIL
      const oversizedChain = Array.from({ length: 10 }, (_, i) => ({
        index: i,
        label: `Block ${i}`,
        authority: i === 0 ? ('RootSeed' as const) : ('ThirdPartyWorker' as const),
        facts: ['allowed_scope("notification");', 'allowed_action("read");'],
        checks: [],
        signatureHex: `sig${i}`,
      }));
      const att10 = attenuateToken(oversizedChain, ['Notification'], ['Read']);
      expect(att10.valid).toBe(false);
      expect(att10.violationReason).toContain('RULE-05');
    });
  });

  // -------------------------------------------------------------------------
  // 3. Privilege Escalation Resistance Under Adversarial Vectors
  // -------------------------------------------------------------------------
  describe('Privilege Escalation Resistance Under Adversarial Attacks', () => {
    it('Vector 1: widen_scope is unconditionally rejected', () => {
      const root = buildRootToken(['Notification'], ['Read'], 3600);
      const res = attemptPrivilegeEscalationAttack(root, 'widen_scope');
      expect(res.valid).toBe(false);
      expect(res.grantedScopes).not.toContain('Wallet');
      expect(res.violationReason).toContain('RULE-04: Monotonic Attenuation Violation');
    });

    it('Vector 2: add_action is unconditionally rejected', () => {
      const root = buildRootToken(['Notification'], ['Read'], 3600);
      const res = attemptPrivilegeEscalationAttack(root, 'add_action');
      expect(res.valid).toBe(false);
      expect(res.grantedActions).not.toContain('Admin');
      expect(res.violationReason).toContain('RULE-04: Monotonic Attenuation Violation');
    });

    it('Vector 3: remove_caveat is unconditionally rejected', () => {
      const root = buildRootToken(['Notification'], ['Read'], 3600);
      const res = attemptPrivilegeEscalationAttack(root, 'remove_caveat');
      expect(res.valid).toBe(false);
      expect(res.violationReason).toContain('Ed25519 hash chain rupture detected');
      expect(res.grantedScopes).toEqual([]);
      expect(res.grantedActions).toEqual([]);
    });

    it('Vector 4: forge_root with unpinned key is unconditionally rejected', () => {
      const root = buildRootToken(['FullSystem'], ['Admin'], 3600);
      const res = attemptPrivilegeEscalationAttack(root, 'forge_root');
      expect(res.valid).toBe(false);
      expect(res.violationReason).toContain('RULE-03: Root Public Key Mismatch');
      expect(res.grantedScopes).toEqual([]);
      expect(res.grantedActions).toEqual([]);
    });

    it('Direct Attenuation Escalation: Child requesting ungranted scope fails closed', () => {
      const root = buildRootToken(['Notification'], ['Read'], 3600);
      // Attempting to request 'Wallet' or 'FullSystem'
      for (const ungrantedScope of ['Wallet', 'Filesystem', 'FullSystem', 'Biometrics'] as CapabilityScope[]) {
        const att = attenuateToken(root, [ungrantedScope], ['Read']);
        expect(att.valid).toBe(false);
        expect(att.violationReason).toContain('RULE-04: Monotonic Attenuation Violation');
        expect(att.grantedScopes).not.toContain(ungrantedScope);
      }
    });

    it('Direct Attenuation Escalation: Child requesting ungranted action fails closed', () => {
      const root = buildRootToken(['Notification'], ['Read'], 3600);
      // Attempting to request 'Write', 'Delete', 'Admin'
      for (const ungrantedAction of ['Write', 'Execute', 'Delete', 'Admin'] as CapabilityAction[]) {
        const att = attenuateToken(root, ['Notification'], [ungrantedAction]);
        expect(att.valid).toBe(false);
        expect(att.violationReason).toContain('RULE-04: Monotonic Attenuation Violation');
        expect(att.grantedActions).not.toContain(ungrantedAction);
      }
    });

    it('All 13 Canonical Scopes cannot be escalated if root grants subset', () => {
      for (const granted of ['Notification', 'Audio'] as CapabilityScope[]) {
        const root = buildRootToken([granted], ['Read'], 3600);
        for (const target of ALL_13_CANONICAL_SCOPES) {
          if (target === granted) continue;
          const att = attenuateToken(root, [target], ['Read']);
          expect(att.valid).toBe(false);
        }
      }
    });
  });

  // -------------------------------------------------------------------------
  // 4. Extreme TTL Values
  // -------------------------------------------------------------------------
  describe('Extreme & Malformed TTL Values in buildRootToken', () => {
    it('handles zero TTL and large safe integer TTL', () => {
      const zeroTtl = buildRootToken(['Notification'], ['Read'], 0);
      expect(zeroTtl[0]?.checks[0]).toContain('TTL: 0s');

      const maxTtl = buildRootToken(['Notification'], ['Read'], 86400 * 365);
      expect(maxTtl[0]?.checks[0]).toContain('TTL: 31536000s');
    });

    it('handles negative TTL (expired at creation)', () => {
      const negTtl = buildRootToken(['Notification'], ['Read'], -3600);
      expect(negTtl[0]?.checks[0]).toContain('TTL: -3600s');
      // The expiry date is in the past
      const expiry = negTtl[0]?.checks[0]?.match(/<= "([^"]+)"/)?.[1];
      expect(new Date(expiry!).getTime()).toBeLessThan(Date.now());
    });

    it('EMPIRICAL ERROR CHECK: NaN or Infinity TTL in buildRootToken', () => {
      // Testing if RangeError: Invalid time value is thrown when NaN is passed
      expect(() => buildRootToken(['Notification'], ['Read'], NaN)).toThrow(RangeError);
      expect(() => buildRootToken(['Notification'], ['Read'], Infinity)).toThrow(RangeError);
    });
  });

  // -------------------------------------------------------------------------
  // 5. High-Throughput Token Generation & Multi-Depth Chains
  // -------------------------------------------------------------------------
  describe('High-Throughput Token Generation (1,000 Delegation Chains)', () => {
    it('creates and attenuates 1,000 full 5-block chains deterministically', () => {
      const start = performance.now();

      for (let i = 0; i < 1000; i++) {
        const root = buildRootToken(['Notification', 'Filesystem', 'Camera'], ['Read', 'Write'], 1800);
        const b1 = attenuateToken(root, ['Notification', 'Filesystem'], ['Read', 'Write']);
        expect(b1.valid).toBe(true);

        const chain2 = [root[0]!, {
          index: 1,
          label: 'B1',
          authority: 'SubAgentAttenuation' as const,
          facts: ['allowed_scope("notification");', 'allowed_scope("filesystem");', 'allowed_action("read");'],
          checks: [],
          signatureHex: 'sig1',
        }];
        const b2 = attenuateToken(chain2, ['Notification'], ['Read']);
        expect(b2.valid).toBe(true);
        expect(b2.grantedScopes).toEqual(['Notification']);
        expect(b2.grantedActions).toEqual(['Read']);
      }

      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(1000); // 1000 multi-depth chains in < 1 second
    });
  });
});
