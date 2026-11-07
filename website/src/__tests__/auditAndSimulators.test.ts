import { describe, expect, it } from 'vitest';
import {
  appendSignedAuditEntry,
  createAuditKeyring,
  tamperAuditEntryTitle,
  verifyAuditChain,
} from '@/engine/auditChain';
import {
  DAILY_SILENCE_BUDGET_MAX,
  DomainAutonomyRecord,
  acceptCrankPromotion,
  consumeProactiveCredit,
  dismissProactiveSuggestion,
  evaluateConstitutionalConfidence,
  evaluateSpoMemoryAtTime,
  recordHumanUndo,
  recordValidatedAction,
  selectMultimodalChannel,
} from '@/engine/cognitiveSimulators';

describe('WebCrypto SHA-256 + Ed25519 Audit Chain & Cognitive Simulators', () => {
  it('signs chained audit blocks via WebCrypto and detects 1-byte tampering immediately', async () => {
    const keyring = await createAuditKeyring();
    let chain = await appendSignedAuditEntry([], keyring, {
      id: 'b1',
      timestampIso: '2026-11-06T10:00:00Z',
      actor: 'ConstitutionalGuard',
      trustOrigin: 'UntrustedExternal',
      title: 'Blocked Untrusted email injection',
      outcome: 'BlockedRule01',
      reversible: false,
      why: {
        level1PlainSentence: 'Blocked by RULE-01.',
        level2Facts: [],
        level3Technical: {
          agentId: 'agent-mail',
          biscuitCapabilities: [],
          confidenceScore: 0.99,
          domainThreshold: 0.9,
          modelUsed: 'rule_01',
          networkBytesSent: 0,
          counterfactual: 'None',
        },
      },
    });

    chain = await appendSignedAuditEntry(chain, keyring, {
      id: 'b2',
      timestampIso: '2026-11-06T10:01:00Z',
      actor: 'HumanConfirmed600ms',
      trustOrigin: 'TrustedHuman',
      title: 'Confirmed €25 transfer',
      outcome: 'Sealed',
      reversible: false,
      why: {
        level1PlainSentence: 'Confirmed via 600ms hold.',
        level2Facts: [],
        level3Technical: {
          agentId: 'outbox',
          biscuitCapabilities: ['allow'],
          confidenceScore: 0.95,
          domainThreshold: 0.9,
          modelUsed: 'qwen2.5',
          networkBytesSent: 0,
          counterfactual: 'Release < 600ms',
        },
      },
    });

    const intact = await verifyAuditChain(chain, keyring);
    expect(intact.valid).toBe(true);
    expect(intact.checkedCount).toBe(2);

    const forged = tamperAuditEntryTitle(chain, 1, 'FORGED TITLE');
    const broken = await verifyAuditChain(forged, keyring);
    expect(broken.valid).toBe(false);
    expect(broken.brokenSequence).toBe(1);
    expect(broken.errorKind).toBe('hash-mismatch');
  });

  it('physically purges cognition:* affective cues at t >= 5 minutes (EU AI Act & RULE-08)', () => {
    const nodes = [
      {
        id: 'core',
        subject: 'user:self',
        predicate: 'lang',
        object: 'fr',
        partition: 'Personal' as const,
        category: 'identity-core' as const,
        initialWeight: 0.99,
        halfLifeMinutes: 500000,
      },
      {
        id: 'affect',
        subject: 'cognition:emotion',
        predicate: 'valence',
        object: '0.4',
        partition: 'Personal' as const,
        category: 'cognition-affective' as const,
        initialWeight: 0.85,
        halfLifeMinutes: 5,
      },
    ];

    const at3Min = evaluateSpoMemoryAtTime(nodes, 3);
    expect(at3Min[1]?.physicallyPurgedNoTombstone).toBe(false);
    expect(at3Min[1]?.currentWeight).toBeGreaterThan(0.5);

    const at5Min = evaluateSpoMemoryAtTime(nodes, 5);
    expect(at5Min[1]?.physicallyPurgedNoTombstone).toBe(true);
    expect(at5Min[1]?.currentWeight).toBe(0);
    expect(at5Min[1]?.epistemicTag).toBe('purgé');
    expect(at5Min[0]?.epistemicTag).toBe('frais');
  });

  it('enforces Silence Budget (3/day + 14d snooze), ChannelSelector, Autonomy Undo, and Confidence', () => {
    // ChannelSelector silence in meetings & >65dB
    expect(
      selectMultimodalChannel({
        ambientDb: 40,
        inMeeting: true,
        thirdPartyPresent: false,
        headphonesConnected: false,
        accessibilityProfile: 'Standard',
      }).voicePermitted
    ).toBe(false);

    expect(
      selectMultimodalChannel({
        ambientDb: 74,
        inMeeting: false,
        thirdPartyPresent: false,
        headphonesConnected: false,
        accessibilityProfile: 'Standard',
      }).channel
    ).toBe('VisualPlusHapticTick');

    // Silence budget
    let budget = { remainingCredits: DAILY_SILENCE_BUDGET_MAX, snoozedTopics: {} };
    budget = consumeProactiveCredit(budget, 'news').next;
    expect(budget.remainingCredits).toBe(2);
    budget = dismissProactiveSuggestion(budget, 'news');
    expect(consumeProactiveCredit(budget, 'news').allowed).toBe(false);

    // Autonomy crank promotion at 20 & instant -1 on undo
    let rec: DomainAutonomyRecord = { domainId: 'mail', crank: 1, streakWithoutUndo: 19, promotionEligible: false };
    rec = recordValidatedAction(rec);
    expect(rec.promotionEligible).toBe(true);
    rec = acceptCrankPromotion(rec);
    expect(rec.crank).toBe(2);
    rec = recordHumanUndo(rec);
    expect(rec.crank).toBe(1);

    // Confidence Engine C = 0.6M + 0.4G
    const medHigh = evaluateConstitutionalConfidence(0.98, 0.95, 'medical');
    expect(medHigh.decision).toBe('ExecuteOrDeliver');
    const medLow = evaluateConstitutionalConfidence(0.7, 0.6, 'medical');
    expect(medLow.decision).toBe('RefuseInsufficientProof');
  });
});
