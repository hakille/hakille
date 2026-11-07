import { describe, expect, it } from 'vitest';
import {
  evaluateNotificationRouting,
  formatSubstrateIpcPreview,
  type CognitiveState,
  type NotificationRequestInput,
  type RoutingDecision,
} from '@/engine/notificationRouter';

describe('Feature 1: SMNF Cognitive Notification Router & Silence Budget', () => {
  const nominalCognitive: CognitiveState = {
    userLookingAtScreen: true,
    perclosDrowsinessScore: 0.05,
    ambientNoiseDb: 40,
    inMeeting: false,
    thirdPartiesPresent: false,
    userWearingHeadphones: false,
    isNightTime: false,
  };

  // =========================================================================
  // TIER 1: FEATURE COVERAGE (>= 5 tests)
  // =========================================================================
  describe('Tier 1: Feature Coverage', () => {
    it('T1.1 - nominal desktop dispatch routes to SubstrateNativeDispatch when user is looking at screen', () => {
      const request: NotificationRequestInput = {
        title: 'Compilation terminée',
        body: 'Le build x86_64-unknown-redox a réussi en 14.2s.',
        urgency: 'Normal',
        category: 'System',
        hasDestructiveAction: false,
      };

      const decision = evaluateNotificationRouting(request, nominalCognitive, 3);

      expect(decision.channel).toBe('SubstrateNativeDispatch');
      expect(decision.visualStyle).toBe('MinimalistToast');
      expect(decision.deferred).toBe(false);
      expect(decision.creditCost).toBe(0);
      expect(decision.hapticFeedback).toBe('pulse');
      expect(decision.targetSubstratePreview).toContain('org.freedesktop.Notifications');
    });

    it('T1.2 - meeting privacy: routes high-urgency notifications to AcousticWhisper via headphones', () => {
      const meetingCognitive: CognitiveState = {
        ...nominalCognitive,
        inMeeting: true,
        userWearingHeadphones: true,
      };
      const request: NotificationRequestInput = {
        title: 'Appel entrant prioritaire',
        body: 'Directeur technique demande une validation.',
        urgency: 'High',
        category: 'System',
        hasDestructiveAction: false,
      };

      const decision = evaluateNotificationRouting(request, meetingCognitive, 3);

      expect(decision.channel).toBe('AcousticWhisper');
      expect(decision.visualStyle).toBe('MinimalistToast');
      expect(decision.deferred).toBe(false);
      expect(decision.hapticFeedback).toBe('double_tap');
      expect(decision.rationale).toContain('Réunion en cours avec écouteurs');
    });

    it('T1.3 - third-party presence suppresses room audio and routes to AmbientMicroToast', () => {
      const privacyCognitive: CognitiveState = {
        ...nominalCognitive,
        thirdPartiesPresent: true,
        userWearingHeadphones: false,
      };
      const request: NotificationRequestInput = {
        title: 'Solde compte bancaire',
        body: 'Solde actuel : 4 250,00 €.',
        urgency: 'Normal',
        category: 'Wallet',
        hasDestructiveAction: false,
      };

      const decision = evaluateNotificationRouting(request, privacyCognitive, 3);

      expect(decision.channel).toBe('AmbientMicroToast');
      expect(decision.visualStyle).toBe('MinimalistToast');
      expect(decision.deferred).toBe(false);
      expect(decision.rationale).toContain('Tiers présents dans la pièce');
    });

    it('T1.4 - noisy environment (> 65 dB) shifts channel to HapticPulse and double_tap haptic', () => {
      const noisyCognitive: CognitiveState = {
        ...nominalCognitive,
        ambientNoiseDb: 74, // Crowded cafe / train
        userWearingHeadphones: false,
      };
      const request: NotificationRequestInput = {
        title: 'Station approche',
        body: 'Prochain arrêt : Gare Centrale dans 2 min.',
        urgency: 'Normal',
        category: 'System',
        hasDestructiveAction: false,
      };

      const decision = evaluateNotificationRouting(request, noisyCognitive, 3);

      expect(decision.channel).toBe('HapticPulse');
      expect(decision.visualStyle).toBe('MinimalistToast');
      expect(decision.hapticFeedback).toBe('double_tap');
      expect(decision.rationale).toContain('Bruit ambiant élevé');
    });

    it('T1.5 - night mode or quiet room (< 32 dB) uses subtle ambient glyph or whisper', () => {
      const quietCognitive: CognitiveState = {
        ...nominalCognitive,
        ambientNoiseDb: 25,
        isNightTime: true,
        userWearingHeadphones: false,
      };
      const request: NotificationRequestInput = {
        title: 'Mise à jour planifiée',
        body: 'Mise à jour système programmée à 03h00.',
        urgency: 'Normal',
        category: 'System',
        hasDestructiveAction: false,
      };

      const decision = evaluateNotificationRouting(request, quietCognitive, 3);

      expect(decision.channel).toBe('AmbientMicroToast');
      expect(decision.visualStyle).toBe('AmbientGlyph');
      expect(decision.deferred).toBe(false);
      expect(decision.rationale).toContain('Environnement nocturne');
    });

    it('T1.6 - autonomous suggestion consumes 1 silence credit when budget is available', () => {
      const request: NotificationRequestInput = {
        title: 'Suggestion d’archivage',
        body: '4 infolettres non lues depuis 7 jours. Les archiver ?',
        urgency: 'Normal',
        category: 'Agent',
        hasDestructiveAction: false,
      };

      const decision = evaluateNotificationRouting(request, nominalCognitive, 2);

      expect(decision.deferred).toBe(false);
      expect(decision.creditCost).toBe(1);
    });
  });

  // =========================================================================
  // TIER 2: BOUNDARY & CORNER CASES (>= 5 tests)
  // =========================================================================
  describe('Tier 2: Boundary & Corner Cases', () => {
    it('T2.1 - PERCLOS fatigue boundary: 0.149 is subject to silence budget, 0.150 somnolence bypasses it', () => {
      const subThresholdCognitive: CognitiveState = {
        ...nominalCognitive,
        perclosDrowsinessScore: 0.149,
      };
      const somnolenceCognitive: CognitiveState = {
        ...nominalCognitive,
        perclosDrowsinessScore: 0.150,
      };

      const request: NotificationRequestInput = {
        title: 'Alerte Vigilance',
        body: 'Baisse de vigilance détectée. Prenez une pause.',
        urgency: 'Normal',
        category: 'Cognitive',
        hasDestructiveAction: false,
      };

      // Below threshold (0.149) with 0 credits -> deferred by silence budget
      const subDecision = evaluateNotificationRouting(request, subThresholdCognitive, 0);
      expect(subDecision.deferred).toBe(true);
      expect(subDecision.channel).toBe('CognitiveDeferralQueue');

      // Exact threshold (0.150) with 0 credits -> SOMNOLENCE OVERRIDE bypasses budget!
      const overrideDecision = evaluateNotificationRouting(request, somnolenceCognitive, 0);
      expect(overrideDecision.deferred).toBe(false);
      expect(overrideDecision.creditCost).toBe(0);
      expect(overrideDecision.rationale).toContain('Somnolence avérée');
      expect(overrideDecision.hapticFeedback).toBe('double_tap');
    });

    it('T2.2 - silence budget exhaustion: credits = 1 delivers, credits = 0 defers to queue', () => {
      const request: NotificationRequestInput = {
        title: 'Synthèse quotidienne',
        body: 'Voici le résumé de vos tâches complétées.',
        urgency: 'Normal',
        category: 'Cognitive',
        hasDestructiveAction: false,
      };

      // 1 credit remaining -> deliver & cost 1
      const withCredit = evaluateNotificationRouting(request, nominalCognitive, 1);
      expect(withCredit.deferred).toBe(false);
      expect(withCredit.creditCost).toBe(1);

      // 0 credits remaining -> defer to queue & cost 0
      const noCredit = evaluateNotificationRouting(request, nominalCognitive, 0);
      expect(noCredit.deferred).toBe(true);
      expect(noCredit.channel).toBe('CognitiveDeferralQueue');
      expect(noCredit.visualStyle).toBe('SilentShadow');
      expect(noCredit.creditCost).toBe(0);
      expect(noCredit.rationale).toContain('Budget de silence quotidien épuisé');
    });

    it('T2.3 - noise boundary at 65 dB: 65.0 dB normal routing vs 65.1 dB haptic pulse', () => {
      const at65dB: CognitiveState = {
        ...nominalCognitive,
        ambientNoiseDb: 65.0,
      };
      const at651dB: CognitiveState = {
        ...nominalCognitive,
        ambientNoiseDb: 65.1,
      };

      const request: NotificationRequestInput = {
        title: 'Rappel',
        body: 'Rendez-vous dans 15 minutes.',
        urgency: 'Normal',
        category: 'System',
        hasDestructiveAction: false,
      };

      const decision65 = evaluateNotificationRouting(request, at65dB, 3);
      expect(decision65.channel).toBe('SubstrateNativeDispatch');

      const decision651 = evaluateNotificationRouting(request, at651dB, 3);
      expect(decision651.channel).toBe('HapticPulse');
      expect(decision651.hapticFeedback).toBe('double_tap');
    });

    it('T2.4 - destructive action forces BraiseActionRequired styling and urgent buzz regardless of urgency', () => {
      const request: NotificationRequestInput = {
        title: 'Révocation de clé API',
        body: 'Confirmer la suppression irréversible de la clé de production.',
        urgency: 'Low', // Even with Low urgency!
        category: 'Security',
        hasDestructiveAction: true,
      };

      const decision = evaluateNotificationRouting(request, nominalCognitive, 0);

      expect(decision.visualStyle).toBe('BraiseActionRequired');
      expect(decision.hapticFeedback).toBe('urgent_buzz');
      expect(decision.deferred).toBe(false);
      expect(decision.creditCost).toBe(0);
      expect(decision.rationale).toContain('Urgence critique ou mutation irréversible');
    });

    it('T2.5 - formats valid native IPC previews across all 6 supported substrates', () => {
      const request: NotificationRequestInput = {
        title: 'Alerte Système',
        body: 'Mise à jour disponible.',
        urgency: 'Normal',
        category: 'System',
        hasDestructiveAction: false,
      };
      const decision: RoutingDecision = {
        channel: 'SubstrateNativeDispatch',
        visualStyle: 'MinimalistToast',
        rationale: 'Nominal',
        hapticFeedback: 'pulse',
        deferred: false,
        creditCost: 0,
        targetSubstratePreview: '',
      };

      // 1. Linux
      const linuxIpc = formatSubstrateIpcPreview('linux', request, decision);
      expect(linuxIpc).toContain('org.freedesktop.Notifications');

      // 2. macOS
      const macosIpc = formatSubstrateIpcPreview('macos', request, decision);
      expect(macosIpc).toContain('display notification');

      // 3. Windows
      const windowsIpc = formatSubstrateIpcPreview('windows', request, decision);
      expect(windowsIpc).toContain('<toast');

      // 4. Mobile
      const mobileIpc = formatSubstrateIpcPreview('mobile', request, decision);
      expect(mobileIpc).toContain('MobileShadowSyncEnvelope');

      // 5. Redox OS
      const redoxIpc = formatSubstrateIpcPreview('redox', request, decision);
      expect(redoxIpc).toContain('orbital:notify');

      // 6. Web
      const webIpc = formatSubstrateIpcPreview('web', request, decision);
      expect(webIpc).toContain('new Notification');
    });
  });

  // =========================================================================
  // TIER 3: CROSS-FEATURE PAIRWISE COMBINATIONS (>= 2 tests)
  // =========================================================================
  describe('Tier 3: Cross-Feature Combinations', () => {
    it('T3.1 - Gaze looking away + in meeting + low urgency results in quiet deferral without distraction', () => {
      const complexContext: CognitiveState = {
        ...nominalCognitive,
        userLookingAtScreen: false,
        inMeeting: true,
        ambientNoiseDb: 45,
      };
      const request: NotificationRequestInput = {
        title: 'Météo hebdomadaire',
        body: 'Prévisions pour le week-end.',
        urgency: 'Low',
        category: 'Agent',
        hasDestructiveAction: false,
      };

      const decision = evaluateNotificationRouting(request, complexContext, 2);

      expect(decision.deferred).toBe(true);
      expect(decision.channel).toBe('CognitiveDeferralQueue');
      expect(decision.visualStyle).toBe('SilentShadow');
    });

    it('T3.2 - Fatigue override + meeting + third parties ensures safety alert reaches user without loudspeaker leak', () => {
      const sensitiveContext: CognitiveState = {
        ...nominalCognitive,
        perclosDrowsinessScore: 0.22, // Heavy somnolence
        inMeeting: true,
        thirdPartiesPresent: true,
        userWearingHeadphones: false,
      };
      const request: NotificationRequestInput = {
        title: 'Alerte vigilance critique',
        body: 'Microsommeils répétés détectés.',
        urgency: 'Normal',
        category: 'Cognitive',
        hasDestructiveAction: false,
      };

      const decision = evaluateNotificationRouting(request, sensitiveContext, 0);

      // Must NOT use loudspeaker (in meeting) -> routes to AmbientMicroToast
      expect(decision.channel).toBe('AmbientMicroToast');
      expect(decision.deferred).toBe(false); // Never suppressed!
      expect(decision.hapticFeedback).toBe('double_tap');
      expect(decision.rationale).toContain('Somnolence avérée');
    });
  });

  // =========================================================================
  // TIER 4: REAL-WORLD APPLICATION SCENARIOS
  // =========================================================================
  describe('Tier 4: Real-World Scenarios', () => {
    it('T4.1 - Scenario 1: Executive in High-Stakes Meeting (Discretion & Whisper Delivery)', () => {
      // Real-World Scenario: A CEO is presenting in a confidential board meeting.
      // Context: inMeeting = true, thirdPartiesPresent = true, userWearingHeadphones = true.
      // An urgent board security briefing arrives.
      const execContext: CognitiveState = {
        userLookingAtScreen: false,
        perclosDrowsinessScore: 0.04,
        ambientNoiseDb: 48,
        inMeeting: true,
        thirdPartiesPresent: true,
        userWearingHeadphones: true,
        isNightTime: false,
      };

      const securityAlert: NotificationRequestInput = {
        title: 'Briefing confidentiel',
        body: 'Audit d’infrastructure validé avec succès.',
        urgency: 'High',
        category: 'Security',
        hasDestructiveAction: false,
      };

      const decision = evaluateNotificationRouting(securityAlert, execContext, 1);

      // Verifications:
      expect(decision.channel).toBe('AcousticWhisper');
      expect(decision.hapticFeedback).toBe('double_tap');
      expect(decision.deferred).toBe(false);
      expect(decision.rationale).toContain('Réunion en cours avec écouteurs');
      // Substrate preview confirms no loud audio
      expect(decision.targetSubstratePreview).toBeTruthy();
    });

    it('T4.2 - Scenario 2: Driver Drowsiness Somnolence Override (Unconditional Alarm)', () => {
      // Real-World Scenario: A vehicle driver has been driving for 4 hours at night.
      // The daily silence quota is already exhausted (0 credits remaining).
      // Vision sensor registers PERCLOS P80 = 25% (> 15% threshold for severe somnolence).
      const driverContext: CognitiveState = {
        userLookingAtScreen: false,
        perclosDrowsinessScore: 0.25,
        ambientNoiseDb: 62,
        inMeeting: false,
        thirdPartiesPresent: false,
        userWearingHeadphones: false,
        isNightTime: true,
      };

      const fatigueAlert: NotificationRequestInput = {
        title: 'ARRÊT IMMÉDIAT CONSEILLÉ',
        body: 'Fermeture prolongée des paupières détectée. Danger imminent d’endormissement.',
        urgency: 'Normal',
        category: 'Cognitive',
        hasDestructiveAction: false,
      };

      const decision = evaluateNotificationRouting(fatigueAlert, driverContext, 0);

      // Verifications:
      expect(decision.deferred).toBe(false); // Safety-critical: NOT deferred even with 0 credits
      expect(decision.creditCost).toBe(0); // Safety alerts do not consume silence budget
      expect(decision.channel).toBe('SpatialChime'); // Chime alert to wake driver
      expect(decision.hapticFeedback).toBe('double_tap');
      expect(decision.rationale).toContain('Somnolence avérée');
      expect(decision.rationale).toContain('25 % >= 15 %');
    });
  });
});
