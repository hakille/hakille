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
  readonly smnfLab: {
    readonly title: string;
    readonly subtitle: string;
    readonly requestSection: {
      readonly title: string;
      readonly fieldTitle: string;
      readonly fieldBody: string;
      readonly fieldUrgency: string;
      readonly fieldCategory: string;
      readonly sampleTitle: string;
      readonly sampleBody: string;
      readonly urgencyLow: string;
      readonly urgencyNormal: string;
      readonly urgencyHigh: string;
      readonly urgencyCritical: string;
      readonly categoryAgent: string;
      readonly categoryCognitive: string;
      readonly categorySystem: string;
      readonly categorySecurity: string;
      readonly categoryWallet: string;
      readonly destructiveLabel: string;
      readonly destructiveHint: string;
    };
    readonly cognitiveSensors: {
      readonly title: string;
      readonly gazeScreen: string;
      readonly perclosLabel: string;
      readonly perclosAlert: string;
      readonly perclosModerate: string;
      readonly perclosNominal: string;
      readonly noiseLabel: string;
      readonly noiseHigh: string;
      readonly noiseQuiet: string;
      readonly noiseOffice: string;
      readonly inMeeting: string;
      readonly thirdParties: string;
      readonly headphones: string;
      readonly nightMode: string;
    };
    readonly silenceBudget: {
      readonly label: string;
      readonly spendCredit: string;
      readonly resetDaily: string;
      readonly remainingUnit: string;
    };
    readonly channels: {
      readonly label: string;
    };
    readonly visualStyles: {
      readonly label: string;
      readonly deferredBadge: string;
      readonly immediateBadge: string;
    };
    readonly decisionCard: {
      readonly title: string;
      readonly hapticLabel: string;
      readonly hapticNone: string;
      readonly costLabel: string;
      readonly rationaleLabel: string;
    };
    readonly auditionBtn: {
      readonly label: string;
    };
    readonly ipcPreview: {
      readonly title: string;
    };
    readonly substrates: {
      readonly linux: string;
      readonly macos: string;
      readonly windows: string;
      readonly mobile: string;
      readonly redox: string;
      readonly web: string;
    };
  };
  readonly biscuitLab: {
    readonly title: string;
    readonly subtitle: string;
    readonly blockChain: {
      readonly title: string;
      readonly rootBadge: string;
      readonly rootKeyLabel: string;
      readonly attenuationConnector: string;
      readonly childBadge: string;
      readonly childDetail: string;
      readonly depthLabel: string;
    };
    readonly authorityLabels: {
      readonly rootAuthority: string;
      readonly subAgentAttenuation: string;
    };
    readonly scopes: {
      readonly title: string;
      readonly rootPill: string;
      readonly subPill: string;
    };
    readonly actions: {
      readonly rootLabel: string;
      readonly subLabel: string;
    };
    readonly leaseSlider: {
      readonly label: string;
      readonly unit: string;
    };
    readonly attacks: {
      readonly title: string;
      readonly description: string;
      readonly widenScopeBtn: string;
      readonly addActionBtn: string;
      readonly removeCaveatBtn: string;
      readonly forgeRootBtn: string;
      readonly resetBtn: string;
    };
    readonly verifyConsole: {
      readonly title: string;
      readonly rfcBadge: string;
      readonly monotonicPassBadge: string;
      readonly escalationBlockedBadge: string;
      readonly validBanner: string;
      readonly rejectedBanner: string;
      readonly datalogSourceEyebrow: string;
      readonly theoremTitle: string;
      readonly theoremBody: string;
    };
  };
  readonly hudToastSandbox: {
    readonly title: string;
    readonly subtitle: string;
    readonly previewStage: {
      readonly statusSticky: string;
      readonly statusEvaporating: string;
      readonly statusEvaporated: string;
      readonly osBrand: string;
      readonly sovereignBadge: string;
      readonly idleZeroFps: string;
      readonly sealedBadge: string;
    };
    readonly configurator: {
      readonly title: string;
      readonly fieldTitle: string;
      readonly fieldBody: string;
      readonly fieldUrgency: string;
      readonly fieldCategory: string;
      readonly fieldDuration: string;
      readonly sampleTitle: string;
      readonly sampleBody: string;
      readonly duration1500: string;
      readonly duration3000: string;
      readonly duration6000: string;
      readonly durationPersistent: string;
      readonly destructiveLabel: string;
      readonly destructiveHint: string;
      readonly specsTitle: string;
      readonly specWidth: string;
      readonly specBlur: string;
      readonly specRadius: string;
      readonly specHold: string;
      readonly specIdle: string;
    };
    readonly actions: {
      readonly dismissBtn: string;
      readonly triggerToastBtn: string;
      readonly braiseIdle: string;
      readonly braiseHolding: string;
      readonly braiseArmed: string;
      readonly braiseConfirmed: string;
      readonly braiseCancelled: string;
    };
    readonly accessKitInspector: {
      readonly title: string;
    };
  };
  readonly policyPlayground: {
    readonly title: string;
    readonly subtitle: string;
    readonly subjectProfiles: {
      readonly title: string;
      readonly roleLabel: string;
      readonly roleOwner: string;
      readonly roleMinor: string;
      readonly roleGuest: string;
      readonly roleUntrustedAgent: string;
      readonly ageLabel: string;
      readonly trustLabel: string;
      readonly trustHuman: string;
      readonly trustDaemon: string;
      readonly trustExternal: string;
      readonly biometricLabel: string;
      readonly bioAbsent: string;
      readonly bioPresence: string;
      readonly bioFaceVoice: string;
      readonly bioCertified: string;
    };
    readonly emissionRequest: {
      readonly title: string;
      readonly targetScopeLabel: string;
      readonly thresholdLabel: string;
      readonly actionLabel: string;
      readonly actionRead: string;
      readonly actionWrite: string;
      readonly actionExecute: string;
      readonly actionDelete: string;
      readonly actionAdmin: string;
      readonly destructiveLabel: string;
      readonly destructiveHint: string;
    };
    readonly verdictBanner: {
      readonly title: string;
      readonly outcomeAllow: string;
      readonly outcomeHardBlock: string;
      readonly outcomeStepUp: string;
    };
    readonly rulesCited: {
      readonly rule01Title: string;
      readonly rule01Desc: string;
      readonly rule07Title: string;
      readonly rule07Desc: string;
      readonly adr013Title: string;
      readonly adr013Desc: string;
      readonly isoStepUpTitle: string;
      readonly isoStepUpDesc: string;
    };
  };
}

export function pickBi(locale: LocaleCode, frText: string, enText: string): string {
  return locale === 'fr' ? frText : enText;
}
