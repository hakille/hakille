/**
 * Hakille — Deterministic Cognitive, Ambient, Memory & Constitutional Simulators
 *
 * Faithful TypeScript implementations of:
 * 1. `ChannelSelector` (`crates/hakille-ui-shell` & Vision §9): noise dB, meeting, third-party, a11y
 * 2. `SilenceBudget` (`crates/hakille-design/src/silence.rs`): 3 proactive credits/day, 14-day snooze
 * 3. `DomainAutonomy` (`crates/hakille-agents`): Crank 0..4, 20-action promotion, instant -1 on undo
 * 4. `ConfidenceEngine` (`constitution/v1.0.md` Art. III): C = 0.6*M + 0.4*G vs domain thresholds
 * 5. `SpoMemoryGraph` (`crates/hakille-memory`): W(t) = W0 * exp(-lambda * dt) + 5-minute physical
 *    tombstone-free purge of `cognition:*` affective/postural signals (EU AI Act compliance).
 */

export type AccessibilityProfile = 'Standard' | 'VisualOnly' | 'AudioFirst' | 'HapticAssisted';

export interface AmbientContext {
  readonly ambientDb: number; // 20 .. 85 dB
  readonly inMeeting: boolean;
  readonly thirdPartyPresent: boolean;
  readonly headphonesConnected: boolean;
  readonly accessibilityProfile: AccessibilityProfile;
}

export type OutputChannel =
  | 'VoiceWhisperAndPresence'
  | 'VoiceNormalAndPresence'
  | 'HeadphonesPrivateVoice'
  | 'VisualHudSilent'
  | 'VisualPlusHapticTick';

export interface ChannelDecision {
  readonly channel: OutputChannel;
  readonly voicePermitted: boolean;
  readonly recommendedVolume: number; // 0.0 .. 1.0
  readonly rationaleCode:
    | 'a11y-visual-only'
    | 'a11y-haptic'
    | 'meeting-silence'
    | 'third-party-privacy'
    | 'noisy-environment-above-65db'
    | 'quiet-room-whisper'
    | 'balanced-voice'
    | 'headphones-private';
}

export function selectMultimodalChannel(ctx: AmbientContext): ChannelDecision {
  if (ctx.accessibilityProfile === 'VisualOnly') {
    return {
      channel: 'VisualHudSilent',
      voicePermitted: false,
      recommendedVolume: 0,
      rationaleCode: 'a11y-visual-only',
    };
  }
  if (ctx.accessibilityProfile === 'HapticAssisted' && (ctx.inMeeting || ctx.ambientDb > 65)) {
    return {
      channel: 'VisualPlusHapticTick',
      voicePermitted: false,
      recommendedVolume: 0,
      rationaleCode: 'a11y-haptic',
    };
  }
  if (ctx.inMeeting) {
    return {
      channel: 'VisualHudSilent',
      voicePermitted: false,
      recommendedVolume: 0,
      rationaleCode: 'meeting-silence',
    };
  }
  if (ctx.thirdPartyPresent && !ctx.headphonesConnected) {
    return {
      channel: 'VisualHudSilent',
      voicePermitted: false,
      recommendedVolume: 0,
      rationaleCode: 'third-party-privacy',
    };
  }
  if (ctx.ambientDb > 65 && !ctx.headphonesConnected) {
    return {
      channel: 'VisualPlusHapticTick',
      voicePermitted: false,
      recommendedVolume: 0,
      rationaleCode: 'noisy-environment-above-65db',
    };
  }
  if (ctx.headphonesConnected) {
    return {
      channel: 'HeadphonesPrivateVoice',
      voicePermitted: true,
      recommendedVolume: 0.62,
      rationaleCode: 'headphones-private',
    };
  }
  if (ctx.ambientDb < 32) {
    return {
      channel: 'VoiceWhisperAndPresence',
      voicePermitted: true,
      recommendedVolume: 0.28,
      rationaleCode: 'quiet-room-whisper',
    };
  }
  return {
    channel: 'VoiceNormalAndPresence',
    voicePermitted: true,
    recommendedVolume: Math.min(0.85, 0.4 + (ctx.ambientDb - 32) * 0.012),
    rationaleCode: 'balanced-voice',
  };
}

// --- Silence Budget (3/day, 14-day snooze) ---

export const DAILY_SILENCE_BUDGET_MAX = 3;
export const DISMISS_SNOOZE_DAYS = 14;

export interface SilenceBudgetState {
  readonly remainingCredits: number;
  readonly snoozedTopics: Readonly<Record<string, number>>; // topic -> days remaining
}

export function consumeProactiveCredit(
  state: SilenceBudgetState,
  topicId: string
): { allowed: boolean; next: SilenceBudgetState } {
  if ((state.snoozedTopics[topicId] ?? 0) > 0 || state.remainingCredits <= 0) {
    return { allowed: false, next: state };
  }
  return {
    allowed: true,
    next: {
      ...state,
      remainingCredits: state.remainingCredits - 1,
    },
  };
}

export function dismissProactiveSuggestion(
  state: SilenceBudgetState,
  topicId: string
): SilenceBudgetState {
  return {
    ...state,
    snoozedTopics: {
      ...state.snoozedTopics,
      [topicId]: DISMISS_SNOOZE_DAYS,
    },
  };
}

// --- Autonomy Crank Ladder (0 .. 4) ---

export type AutonomyCrank = 0 | 1 | 2 | 3 | 4;

export const PROMOTION_STREAK_REQUIRED = 20;

export interface DomainAutonomyRecord {
  readonly domainId: 'mail' | 'calendar' | 'code' | 'home' | 'wallet';
  readonly crank: AutonomyCrank;
  readonly streakWithoutUndo: number;
  readonly promotionEligible: boolean;
}

export function recordValidatedAction(rec: DomainAutonomyRecord): DomainAutonomyRecord {
  const nextStreak = rec.streakWithoutUndo + 1;
  return {
    ...rec,
    streakWithoutUndo: nextStreak,
    promotionEligible: rec.crank < 4 && nextStreak >= PROMOTION_STREAK_REQUIRED,
  };
}

export function acceptCrankPromotion(rec: DomainAutonomyRecord): DomainAutonomyRecord {
  if (!rec.promotionEligible || rec.crank >= 4) return rec;
  return {
    ...rec,
    crank: (rec.crank + 1) as AutonomyCrank,
    streakWithoutUndo: 0,
    promotionEligible: false,
  };
}

export function recordHumanUndo(rec: DomainAutonomyRecord): DomainAutonomyRecord {
  return {
    ...rec,
    crank: Math.max(0, rec.crank - 1) as AutonomyCrank,
    streakWithoutUndo: 0,
    promotionEligible: false,
  };
}

// --- Constitutional Confidence Engine (Article III & Annex B) ---

export type ConstitutionalDomain =
  | 'creative'
  | 'general'
  | 'professional'
  | 'financial'
  | 'legal'
  | 'medical';

export const DOMAIN_THRESHOLDS: Readonly<Record<ConstitutionalDomain, number>> = {
  creative: 0.65,
  general: 0.75,
  professional: 0.85,
  financial: 0.90,
  legal: 0.92,
  medical: 0.95,
} as const;

export interface ConfidenceEvaluation {
  readonly compositeScore: number;
  readonly threshold: number;
  readonly internalDisagreement: boolean;
  readonly decision: 'ExecuteOrDeliver' | 'ProposeWithDisclaimerAndHold' | 'RefuseInsufficientProof';
}

export function evaluateConstitutionalConfidence(
  modelConfidence: number,
  groundingConfidence: number,
  domain: ConstitutionalDomain
): ConfidenceEvaluation {
  const m = Math.max(0, Math.min(1, modelConfidence));
  const g = Math.max(0, Math.min(1, groundingConfidence));
  const compositeScore = Number((0.6 * m + 0.4 * g).toFixed(3));
  const threshold = DOMAIN_THRESHOLDS[domain];
  const internalDisagreement = Math.abs(m - g) >= 0.35;

  let decision: ConfidenceEvaluation['decision'];
  if (compositeScore >= threshold && !internalDisagreement) {
    decision = 'ExecuteOrDeliver';
  } else if (compositeScore >= threshold - 0.18) {
    decision = 'ProposeWithDisclaimerAndHold';
  } else {
    decision = 'RefuseInsufficientProof';
  }

  return {
    compositeScore,
    threshold,
    internalDisagreement,
    decision,
  };
}

// --- SPO Memory Decay & 5-Minute EU AI Act Affective Purge ---

export interface SpoMemoryNode {
  readonly id: string;
  readonly subject: string;
  readonly predicate: string;
  readonly object: string;
  readonly partition: 'Personal' | 'Professional' | 'Shared';
  readonly category: 'identity-core' | 'preference' | 'ephemeral-context' | 'cognition-affective';
  readonly initialWeight: number;
  readonly halfLifeMinutes: number;
  readonly contradictsId?: string;
}

export interface EvaluatedMemoryNode extends SpoMemoryNode {
  readonly currentWeight: number;
  readonly physicallyPurgedNoTombstone: boolean;
  readonly epistemicTag: 'frais' | 'récent' | 'fané' | 'purgé';
}

export function evaluateSpoMemoryAtTime(
  nodes: readonly SpoMemoryNode[],
  elapsedMinutes: number
): EvaluatedMemoryNode[] {
  const safeMinutes = Math.max(0, elapsedMinutes);

  return nodes.map((node) => {
    // EU AI Act & RULE-08 / cognition.rs: affective/postural signals have a 5-min half-life
    // and are physically purged without tombstone once >= 5 minutes have elapsed.
    if (node.category === 'cognition-affective' && safeMinutes >= 5) {
      return {
        ...node,
        currentWeight: 0,
        physicallyPurgedNoTombstone: true,
        epistemicTag: 'purgé',
      };
    }

    const lambda = Math.LN2 / Math.max(1, node.halfLifeMinutes);
    const currentWeight = Number((node.initialWeight * Math.exp(-lambda * safeMinutes)).toFixed(3));

    let epistemicTag: EvaluatedMemoryNode['epistemicTag'];
    if (currentWeight >= 0.72) {
      epistemicTag = 'frais';
    } else if (currentWeight >= 0.35) {
      epistemicTag = 'récent';
    } else {
      epistemicTag = 'fané';
    }

    return {
      ...node,
      currentWeight,
      physicallyPurgedNoTombstone: false,
      epistemicTag,
    };
  });
}
