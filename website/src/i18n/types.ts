import { RouteId } from '@/config/routes';

export type LocaleCode = 'fr' | 'en' | 'es' | 'de' | 'pt' | 'ar' | 'sw' | 'ja';

export const SUPPORTED_LOCALES: readonly LocaleCode[] = [
  'fr',
  'en',
  'es',
  'de',
  'pt',
  'ar',
  'sw',
  'ja',
] as const;

export interface LocaleMeta {
  readonly code: LocaleCode;
  readonly nativeName: string;
  readonly englishName: string;
  readonly dir: 'ltr' | 'rtl';
  readonly htmlLang: string;
}

export const LOCALE_METADATA: Readonly<Record<LocaleCode, LocaleMeta>> = {
  fr: { code: 'fr', nativeName: 'Français', englishName: 'French', dir: 'ltr', htmlLang: 'fr' },
  en: { code: 'en', nativeName: 'English', englishName: 'English', dir: 'ltr', htmlLang: 'en' },
  es: { code: 'es', nativeName: 'Español', englishName: 'Spanish', dir: 'ltr', htmlLang: 'es' },
  de: { code: 'de', nativeName: 'Deutsch', englishName: 'German', dir: 'ltr', htmlLang: 'de' },
  pt: { code: 'pt', nativeName: 'Português', englishName: 'Portuguese', dir: 'ltr', htmlLang: 'pt' },
  ar: { code: 'ar', nativeName: 'العربية', englishName: 'Arabic (RTL)', dir: 'rtl', htmlLang: 'ar' },
  sw: { code: 'sw', nativeName: 'Kiswahili', englishName: 'Swahili', dir: 'ltr', htmlLang: 'sw' },
  ja: { code: 'ja', nativeName: '日本語', englishName: 'Japanese', dir: 'ltr', htmlLang: 'ja' },
} as const;

export interface RouteSeoCopy {
  readonly navLabel: string;
  readonly title: string;
  readonly description: string;
}

export interface TranslationSchema {
  readonly meta: {
    readonly brandSignature: string;
    readonly preAlphaBadge: string;
    readonly skipToContent: string;
  };
  readonly routes: Readonly<Record<RouteId, RouteSeoCopy>>;
  readonly header: {
    readonly spotlightTrigger: string;
    readonly spotlightShortcut: string;
    readonly audioOn: string;
    readonly audioOff: string;
    readonly themeDark: string;
    readonly themeLight: string;
    readonly accessibilityBtn: string;
    readonly languageLabel: string;
    readonly githubLabel: string;
    readonly menuOpen: string;
    readonly menuClose: string;
  };
  readonly footer: {
    readonly manifestoLine: string;
    readonly productHeading: string;
    readonly trustHeading: string;
    readonly engineeringHeading: string;
    readonly zeroExfiltrationBadge: string;
    readonly preAlphaDisclaimer: string;
    readonly etymologyNote: string;
    readonly copyrightLine: string;
  };
  readonly a11yDrawer: {
    readonly title: string;
    readonly subtitle: string;
    readonly themeLabel: string;
    readonly contrastLabel: string;
    readonly contrastNormal: string;
    readonly contrastHigh: string;
    readonly textScaleLabel: string;
    readonly motionLabel: string;
    readonly motionNormal: string;
    readonly motionReduced: string;
    readonly soundLabel: string;
    readonly soundHint: string;
    readonly resetBtn: string;
    readonly closeBtn: string;
  };
  readonly home: {
    readonly heroMeta: string;
    readonly heroTitle: string;
    readonly heroSubtitle: string;
    readonly heroPrimaryCta: string;
    readonly heroSecondaryCta: string;
    readonly heroSpotlightPrompt: string;
    readonly metricsPackages: string;
    readonly metricsLoc: string;
    readonly metricsTests: string;
    readonly metricsPython: string;

    readonly act2Eyebrow: string;
    readonly act2Title: string;
    readonly act2Body: string;
    readonly act2IndustryTitle: string;
    readonly act2IndustryPoints: readonly [string, string, string, string];
    readonly act2HakilleTitle: string;
    readonly act2HakillePoints: readonly [string, string, string, string];

    readonly act3Eyebrow: string;
    readonly act3Title: string;
    readonly act3Body: string;

    readonly act4Eyebrow: string;
    readonly act4Title: string;
    readonly act4Body: string;

    readonly act5Eyebrow: string;
    readonly act5Title: string;
    readonly act5Body: string;

    readonly act6Eyebrow: string;
    readonly act6Title: string;
    readonly act6Body: string;

    readonly act7Eyebrow: string;
    readonly act7Title: string;
    readonly act7Body: string;

    readonly act8Eyebrow: string;
    readonly act8Title: string;
    readonly act8Body: string;

    readonly act9Eyebrow: string;
    readonly act9Title: string;
    readonly act9Body: string;
    readonly act9CopyCmd: string;
    readonly act9Copied: string;
  };
  readonly kora: {
    readonly title: string;
    readonly subtitle: string;
    readonly stateIdle: string;
    readonly stateListening: string;
    readonly stateThinking: string;
    readonly stateSpeaking: string;
    readonly stateDecision: string;
    readonly stateEmergency: string;
    readonly bargeInBtn: string;
    readonly bargeInStatus: string;
    readonly pluckHint: string;
    readonly fpsZeroBadge: string;
    readonly fpsActiveBadge: string;
  };
  readonly spotlight: {
    readonly placeholder: string;
    readonly hintBar: string;
    readonly closeEsc: string;
    readonly emptyPrompt: string;
    readonly runBtn: string;
  };
  readonly actionLab: {
    readonly scenarioInjectionTab: string;
    readonly scenarioTransferTab: string;
    readonly scenarioArchiveTab: string;
    readonly untrustedBadge: string;
    readonly trustedBadge: string;
    readonly costPill: string;
    readonly dataPill: string;
    readonly revPillIrreversible: string;
    readonly revPillReversible: string;
    readonly holdButtonLabel: string;
    readonly holdHoldingLabel: string;
    readonly holdArmedLabel: string;
    readonly holdConfirmedLabel: string;
    readonly holdCancelledHint: string;
    readonly accessibleModeToggle: string;
    readonly blockedByRule01: string;
    readonly executeReversibleBtn: string;
    readonly auditHeading: string;
    readonly chainValidBadge: string;
    readonly chainBrokenBadge: string;
    readonly tamperBtn: string;
    readonly restoreChainBtn: string;
    readonly whyBtn: string;
    readonly undoBtn: string;
    readonly whyLevel1: string;
    readonly whyLevel2: string;
    readonly whyLevel3: string;
    readonly markFactFalseBtn: string;
    readonly factDisputedBadge: string;
  };
  readonly memoryLab: {
    readonly partitionPersonal: string;
    readonly partitionPro: string;
    readonly partitionShared: string;
    readonly timeSliderLabel: string;
    readonly timeNow: string;
    readonly time5m: string;
    readonly time30d: string;
    readonly time2y: string;
    readonly aiActNotice: string;
    readonly exportJsonBtn: string;
    readonly digitalTwilightTitle: string;
    readonly digitalTwilightBody: string;
  };
  readonly fabricLab: {
    readonly layersTab: string;
    readonly cratesTab: string;
    readonly system2Tab: string;
    readonly searchCratesPlaceholder: string;
    readonly allLayersFilter: string;
    readonly localSystem1Title: string;
    readonly remoteSystem2Title: string;
    readonly issueBiscuitBtn: string;
    readonly simulateStreamBtn: string;
    readonly revokeTokenBtn: string;
  };
  readonly ambientLab: {
    readonly channelHeading: string;
    readonly noiseLabel: string;
    readonly meetingToggle: string;
    readonly thirdPartyToggle: string;
    readonly headphonesToggle: string;
    readonly silenceBudgetHeading: string;
    readonly triggerNudgeBtn: string;
    readonly dismissTopicBtn: string;
    readonly resetDayBtn: string;
    readonly autonomyHeading: string;
    readonly validate20Btn: string;
    readonly promoteCrankBtn: string;
    readonly humanUndoBtn: string;
  };
  readonly inspector: {
    readonly title: string;
    readonly subtitle: string;
    readonly cookiesLabel: string;
    readonly thirdPartyReqLabel: string;
    readonly fontShaLabel: string;
    readonly storageKeysLabel: string;
    readonly rescanBtn: string;
    readonly wipeLocalBtn: string;
    readonly wipedConfirm: string;
  };
  readonly common: {
    readonly readFullDoc: string;
    readonly verifiedInRepo: string;
    readonly preAlphaWarningTitle: string;
    readonly preAlphaWarningBody: string;
    readonly plainSummaryTitle: string;
    readonly lastUpdated: string;
    readonly filterAll: string;
  };
}

export function pickBi(locale: LocaleCode, frText: string, enText: string): string {
  return locale === 'fr' ? frText : enText;
}
