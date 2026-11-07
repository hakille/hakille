/**
 * Sovereign Multiplatform Notification Fabric (SMNF) — Cognitive Notification Router
 *
 * Faithful deterministic TypeScript port of:
 * - `crates/hakille-ui/src/notification_router.rs`
 * - `crates/hakille-ui/src/channel_selector.rs`
 * - `crates/hakille-ui/src/silence_budget.rs`
 * - `crates/hakille-substrate/src/notification.rs`
 *
 * Evaluates real-time multimodal perception context (gaze, PERCLOS fatigue,
 * ambient noise, meeting state, third parties, headphones, night mode) against
 * the daily silence budget (3 suggestions/day) and selects the least intrusive
 * delivery channel.
 */

export interface CognitiveState {
  userLookingAtScreen: boolean;
  perclosDrowsinessScore: number; // 0.00..0.30 (somnolence threshold >= 0.15)
  ambientNoiseDb: number;         // 20..85 dB (high noise > 65 dB, quiet < 32 dB)
  inMeeting: boolean;
  thirdPartiesPresent: boolean;
  userWearingHeadphones: boolean;
  isNightTime: boolean;
}

export type VisualPresentationStyle =
  | 'BraiseActionRequired'
  | 'MinimalistToast'
  | 'AmbientGlyph'
  | 'SilentShadow';

export type NotificationChannel =
  | 'HeadphonesPrivateVoice'
  | 'AmbientMicroToast'
  | 'AcousticWhisper'
  | 'SpatialChime'
  | 'HapticPulse'
  | 'CognitiveDeferralQueue'
  | 'SubstrateNativeDispatch';

export interface RoutingDecision {
  channel: NotificationChannel;
  visualStyle: VisualPresentationStyle;
  rationale: string;
  hapticFeedback: 'pulse' | 'double_tap' | 'urgent_buzz' | null;
  deferred: boolean;
  creditCost: 0 | 1;
  targetSubstratePreview: string;
}

export interface NotificationRequestInput {
  title: string;
  body: string;
  urgency: string; // 'Low' | 'Normal' | 'High' | 'Critical'
  category: string; // 'System' | 'Cognitive' | 'Agent' | 'Security' | 'Wallet'
  hasDestructiveAction: boolean;
}

export type SupportedSubstrate = 'linux' | 'macos' | 'windows' | 'mobile' | 'web' | 'redox';

/**
 * Generates exact IPC / Substrate native dispatch command string
 * matching `crates/hakille-substrate-[target]/src/substrate_impl.rs`.
 */
export function formatSubstrateIpcPreview(
  substrate: SupportedSubstrate,
  request: NotificationRequestInput,
  decision: RoutingDecision
): string {
  const t = request.title;
  const b = request.body;
  const urg = (request.urgency || 'Normal').toLowerCase();
  const lineCont = ' ' + String.fromCharCode(92);

  if (substrate === 'linux') {
    const act = request.hasDestructiveAction
      ? '["confirm","Valider 600ms","dismiss","Ignorer"]'
      : '["dismiss","OK"]';
    const uByte = urg === 'critical' ? '2' : urg === 'high' ? '1' : '0';
    const timeout = decision.deferred ? '0' : (urg === 'critical' ? '0' : '4000');
    return [
      '# Linux FreeDesktop D-Bus org.freedesktop.Notifications::Notify',
      'dbus-send --session --dest=org.freedesktop.Notifications' + lineCont,
      '  /org/freedesktop/Notifications org.freedesktop.Notifications.Notify' + lineCont,
      '  string:"Hakille" uint32:0 string:"dialog-information"' + lineCont,
      '  string:"' + t + '" string:"' + b + '"' + lineCont,
      '  array:string:' + act + lineCont,
      '  dict:string:variant:"urgency",byte:' + uByte + lineCont,
      '  int32:' + timeout,
    ].join('\n');
  }

  if (substrate === 'macos') {
    return [
      '# macOS Darwin UserNotifications (AppleScript bridge)',
      `osascript -e 'display notification "${b}" with title "${t}" subtitle "Hakille SMNF"'`,
      decision.hapticFeedback
        ? '# Haptic feedback dispatched via IOKit MultitouchEngine: ' + decision.hapticFeedback
        : '# Audio suppressed per cognitive silence policy',
    ].join('\n');
  }

  if (substrate === 'windows') {
    return [
      '# Windows Shell_NotifyIconW & WinRT Toast XML',
      '<toast activationType="background" duration="short">',
      '  <visual>',
      '    <binding template="ToastGeneric">',
      '      <text>' + t + '</text>',
      '      <text>' + b + '</text>',
      '      <attributionText>Hakille SMNF · ' + decision.visualStyle + '</attributionText>',
      '    </binding>',
      '  </visual>',
      '</toast>',
    ].join('\n');
  }

  if (substrate === 'mobile') {
    return [
      '# Mobile ShadowSync Queue (Doze-preserving batching)',
      JSON.stringify(
        {
          type: 'MobileShadowSyncEnvelope',
          channel: decision.channel,
          deferred: decision.deferred,
          powerSavingEligible: !decision.deferred && urg !== 'critical',
          payload: {
            title: request.title,
            body: request.body,
            urgency: request.urgency,
            haptic: decision.hapticFeedback,
          },
        },
        null,
        2
      ),
    ].join('\n');
  }

  if (substrate === 'redox') {
    return [
      '# Redox OS orbital:notify Scheme Dispatch',
      'scheme_open("orbital:notify", O_WRONLY) -> fd',
      'write(fd, {"title": "' + t + '", "body": "' + b + '", "channel": "' + decision.channel + '"})',
    ].join('\n');
  }

  return [
    '// W3C HTML5 Notification API + WebAudio Bridge',
    'new Notification("' + t + '", {',
    '  body: "' + b + '",',
    '  tag: "hakille-smnf",',
    '  silent: ' + (decision.channel === 'AmbientMicroToast' || decision.channel === 'HapticPulse'),
    '});',
  ].join('\n');
}

/**
 * Core 8-Rule Multimodal Channel Selection & Silence Budget Arbitrator.
 * Port of `notification_router.rs:78-208` and `channel_selector.rs:80-244`.
 */
export function evaluateNotificationRouting(
  request: NotificationRequestInput,
  cognitive: CognitiveState,
  silenceCreditsRemaining: number
): RoutingDecision {
  const normUrgency = (request.urgency || 'Normal').toLowerCase();
  const isCritical = normUrgency === 'critical' || request.hasDestructiveAction;
  const isHigh = normUrgency === 'high';
  const isLow = normUrgency === 'low';
  const categoryNorm = (request.category || 'System').toLowerCase();

  // Rule 0: Critical Urgency or Destructive Action
  // Must always deliver immediately with BraiseActionRequired styling,
  // bypassing the daily silence budget.
  if (isCritical) {
    let chosenChannel: NotificationChannel = 'SpatialChime';
    if (cognitive.userWearingHeadphones) {
      chosenChannel = 'HeadphonesPrivateVoice';
    } else if (cognitive.inMeeting || cognitive.thirdPartiesPresent) {
      // In meeting or third parties: strictly suppress open room audio
      chosenChannel = 'AmbientMicroToast';
    }

    const decision: RoutingDecision = {
      channel: chosenChannel,
      visualStyle: 'BraiseActionRequired',
      rationale:
        'Urgence critique ou mutation irréversible : validation physique requise. Contourne le budget de silence.',
      hapticFeedback: 'urgent_buzz',
      deferred: false,
      creditCost: 0,
      targetSubstratePreview: '',
    };
    decision.targetSubstratePreview = formatSubstrateIpcPreview('linux', request, decision);
    return decision;
  }

  // Rule 0.5: Fatigue & Somnolence Override (`notification_router.rs:119`)
  // When PERCLOS drowsiness score >= 0.15 (15% eye closure over 60s window),
  // fatigue wellness alert is promoted to High urgency and bypasses silence budget.
  if (cognitive.perclosDrowsinessScore >= 0.15 && (categoryNorm === 'cognitive' || categoryNorm === 'security')) {
    const channel: NotificationChannel = cognitive.userWearingHeadphones
      ? 'AcousticWhisper'
      : cognitive.inMeeting
        ? 'AmbientMicroToast'
        : 'SpatialChime';

    const decision: RoutingDecision = {
      channel,
      visualStyle: 'MinimalistToast',
      rationale: 'Somnolence avérée détectée (PERCLOS ' + (cognitive.perclosDrowsinessScore * 100).toFixed(0) + ' % >= 15 %). Alerte vigilance prioritaire dispensée du quota de silence.',
      hapticFeedback: 'double_tap',
      deferred: false,
      creditCost: 0,
      targetSubstratePreview: '',
    };
    decision.targetSubstratePreview = formatSubstrateIpcPreview('linux', request, decision);
    return decision;
  }

  // Rule 1: Silence Budget Enforcement for Autonomous Suggestions
  // Autonomous/inferred suggestions (category 'Agent', 'Cognitive', 'System')
  // consume 1 credit. If budget is exhausted (<= 0), defer to queue.
  const isAutonomousSuggestion = categoryNorm === 'agent' || categoryNorm === 'cognitive';
  if (isAutonomousSuggestion && silenceCreditsRemaining <= 0) {
    const decision: RoutingDecision = {
      channel: 'CognitiveDeferralQueue',
      visualStyle: 'SilentShadow',
      rationale:
        'Budget de silence quotidien épuisé (0 / 3 crédits restants). Suggestion autonome différée dans la file d’attention.',
      hapticFeedback: null,
      deferred: true,
      creditCost: 0,
      targetSubstratePreview: '',
    };
    decision.targetSubstratePreview = formatSubstrateIpcPreview('linux', request, decision);
    return decision;
  }

  const creditCost: 0 | 1 = isAutonomousSuggestion ? 1 : 0;

  // Rule 2: In Meeting (`channel_selector.rs:95`)
  // Loud voice is strictly forbidden. If headphones and high urgency, whisper; otherwise silent visual toast or defer.
  if (cognitive.inMeeting) {
    if (cognitive.userWearingHeadphones && isHigh) {
      const decision: RoutingDecision = {
        channel: 'AcousticWhisper',
        visualStyle: 'MinimalistToast',
        rationale: 'Réunion en cours avec écouteurs : chuchotement discret en audio privé.',
        hapticFeedback: 'double_tap',
        deferred: false,
        creditCost,
        targetSubstratePreview: '',
      };
      decision.targetSubstratePreview = formatSubstrateIpcPreview('linux', request, decision);
      return decision;
    }

    if (isLow) {
      const decision: RoutingDecision = {
        channel: 'CognitiveDeferralQueue',
        visualStyle: 'SilentShadow',
        rationale: 'Réunion en cours et faible urgence : mise en file d’attente sans interruption.',
        hapticFeedback: null,
        deferred: true,
        creditCost: 0,
        targetSubstratePreview: '',
      };
      decision.targetSubstratePreview = formatSubstrateIpcPreview('linux', request, decision);
      return decision;
    }

    const decision: RoutingDecision = {
      channel: 'AmbientMicroToast',
      visualStyle: 'AmbientGlyph',
      rationale: 'Réunion en cours : notification visuelle discrète sans son pour préserver la discrétion.',
      hapticFeedback: 'pulse',
      deferred: false,
      creditCost,
      targetSubstratePreview: '',
    };
    decision.targetSubstratePreview = formatSubstrateIpcPreview('linux', request, decision);
    return decision;
  }

  // Rule 3: Third Parties Present (`channel_selector.rs:118`)
  // Privacy protection: prevent room audio leak of sensitive content.
  if (cognitive.thirdPartiesPresent && !cognitive.userWearingHeadphones) {
    const decision: RoutingDecision = {
      channel: 'AmbientMicroToast',
      visualStyle: 'MinimalistToast',
      rationale: 'Tiers présents dans la pièce : sortie vocale coupée pour éviter les fuites acoustiques privées.',
      hapticFeedback: 'pulse',
      deferred: false,
      creditCost,
      targetSubstratePreview: '',
    };
    decision.targetSubstratePreview = formatSubstrateIpcPreview('linux', request, decision);
    return decision;
  }

  // Rule 4: High Ambient Noise (> 65 dB) (`channel_selector.rs:135`)
  if (cognitive.ambientNoiseDb > 65 && !cognitive.userWearingHeadphones) {
    const decision: RoutingDecision = {
      channel: 'HapticPulse',
      visualStyle: 'MinimalistToast',
      rationale: 'Bruit ambiant élevé (' + Math.round(cognitive.ambientNoiseDb) + ' dB > 65 dB) : bascule sur impulsion haptique et HUD visuel.',
      hapticFeedback: 'double_tap',
      deferred: false,
      creditCost,
      targetSubstratePreview: '',
    };
    decision.targetSubstratePreview = formatSubstrateIpcPreview('linux', request, decision);
    return decision;
  }

  // Rule 5: Low Urgency / Ambient Filtering (`channel_selector.rs:152`)
  if (isLow) {
    if (!cognitive.userLookingAtScreen) {
      const decision: RoutingDecision = {
        channel: 'CognitiveDeferralQueue',
        visualStyle: 'SilentShadow',
        rationale: 'Faible urgence et regard détourné : différé sans distraction.',
        hapticFeedback: null,
        deferred: true,
        creditCost: 0,
        targetSubstratePreview: '',
      };
      decision.targetSubstratePreview = formatSubstrateIpcPreview('linux', request, decision);
      return decision;
    }

    const decision: RoutingDecision = {
      channel: 'AmbientMicroToast',
      visualStyle: 'AmbientGlyph',
      rationale: 'Faible urgence : simple glyphe d’état ambiant sur le bord d’écran.',
      hapticFeedback: null,
      deferred: false,
      creditCost,
      targetSubstratePreview: '',
    };
    decision.targetSubstratePreview = formatSubstrateIpcPreview('linux', request, decision);
    return decision;
  }

  // Rule 6: Night Time or Quiet Environment (< 32 dB) (`channel_selector.rs:175`)
  if (cognitive.isNightTime || cognitive.ambientNoiseDb < 32) {
    if (cognitive.userWearingHeadphones) {
      const decision: RoutingDecision = {
        channel: 'AcousticWhisper',
        visualStyle: 'MinimalistToast',
        rationale: 'Environnement nocturne calme : synthèse vocale chuchotée dans le casque.',
        hapticFeedback: 'pulse',
        deferred: false,
        creditCost,
        targetSubstratePreview: '',
      };
      decision.targetSubstratePreview = formatSubstrateIpcPreview('linux', request, decision);
      return decision;
    }

    const decision: RoutingDecision = {
      channel: 'AmbientMicroToast',
      visualStyle: 'AmbientGlyph',
      rationale: 'Environnement nocturne : notification visuelle adoucie pour préserver le repos.',
      hapticFeedback: null,
      deferred: false,
      creditCost,
      targetSubstratePreview: '',
    };
    decision.targetSubstratePreview = formatSubstrateIpcPreview('linux', request, decision);
    return decision;
  }

  // Rule 7: User Looking At Screen (`channel_selector.rs:195`)
  if (cognitive.userLookingAtScreen) {
    const decision: RoutingDecision = {
      channel: 'SubstrateNativeDispatch',
      visualStyle: 'MinimalistToast',
      rationale: 'Regard actif sur l’écran : toast HUD natif avec carillon discret.',
      hapticFeedback: 'pulse',
      deferred: false,
      creditCost,
      targetSubstratePreview: '',
    };
    decision.targetSubstratePreview = formatSubstrateIpcPreview('linux', request, decision);
    return decision;
  }

  // Rule 8: Default Fallback (`channel_selector.rs:215`)
  const defaultChannel: NotificationChannel = cognitive.userWearingHeadphones
    ? 'HeadphonesPrivateVoice'
    : 'SpatialChime';

  const defaultDecision: RoutingDecision = {
    channel: defaultChannel,
    visualStyle: 'MinimalistToast',
    rationale: 'Contexte nominal : acheminement standard avec carillon harmonique.',
    hapticFeedback: 'pulse',
    deferred: false,
    creditCost,
    targetSubstratePreview: '',
  };
  defaultDecision.targetSubstratePreview = formatSubstrateIpcPreview('linux', request, defaultDecision);
  return defaultDecision;
}
