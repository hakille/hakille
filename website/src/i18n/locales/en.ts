import { TranslationSchema } from '../types';

export const enLocale: TranslationSchema = {
  meta: {
    brandSignature: 'Hakille — sovereign intelligence',
    preAlphaBadge: 'Pre-Alpha v0.1.0 · Apache-2.0 · 100% Rust',
    skipToContent: 'Skip directly to main content',
  },
  routes: {
    home: {
      navLabel: 'Manifesto',
      title: 'hakille — sovereign intelligence · Pure-Rust Cognitive Operating System',
      description:
        'The system that forgets nothing and always proves what it did. Local SPO memory graph, Biscuit capabilities, 600 ms physical hold, and SHA-256 + Ed25519 hash-chained audit journal.',
    },
    architecture: {
      navLabel: 'Architecture',
      title: '6-Layer Architecture & 41 Rust Crates — hakille',
      description:
        'Deep dive into Hakille’s 6 layers: 122,639 lines of Rust 2024, 0% Python, native winit+wgpu+AccessKit GPU shell, Wasmtime 48 sandbox, and Cognitive Fabric.',
    },
    honestStatus: {
      navLabel: 'Real Status v0.1',
      title: 'Pre-Alpha v0.1 Honesty Matrix — hakille',
      description:
        'Radical engineering honesty separating what is golden-tested today (1,468 tests, ×46 CPU Whisper, Mimi codec, Qwen GGUF) from Phase 1–3 roadmap milestones.',
    },
    security: {
      navLabel: 'Security',
      title: 'Biscuit Capabilities, Untrusted Isolation & Security Policy — hakille',
      description:
        'Monotonically attenuated Biscuit v5 tokens, structural prompt-injection immunity, Argon2id HAKKEY02 keystore, and RFC 9116 72h vulnerability disclosure.',
    },
    constitution: {
      navLabel: 'Constitution',
      title: 'Inviolable Constitution v1.0.0 — hakille',
      description:
        'The 7 Articles and 10 machine-enforced rules governing Hakille above every neural weight: calibrated confidence thresholds, minor protection, and crisis protocol.',
    },
    install: {
      navLabel: 'Install',
      title: 'Hardware RAM Sizer, OS Substrates & Rust Build Lab — hakille',
      description:
        'Calculate Hakille’s exact local model memory footprint on your hardware (2 GB to 16 GB+) and build the daemon, winit+wgpu desktop HUD, or UniFFI mobile bridge.',
    },
    privacy: {
      navLabel: 'Privacy & GDPR',
      title: 'GDPR, ePrivacy & EU AI Act Privacy Policy — hakille',
      description:
        'Dual sovereignty guarantee: zero cookies, zero analytics, and self-hosted OFL fonts on hakille.ai; local-first redb memory and 5-minute affective purge in Hakille OS.',
    },
    legalNotice: {
      navLabel: 'Legal Notice',
      title: 'Legal Notice (LCEN) & Open-Source Governance — hakille',
      description:
        'Identification of the Hakille open-source project, official contact channels, static hosting architecture, and intellectual property notices.',
    },
    conditions: {
      navLabel: 'Terms',
      title: 'Terms of Use & Pre-Alpha AS-IS Prototype Notice — hakille',
      description:
        'Terms governing hakille.ai and critical AS-IS disclaimers for testing the v0.1 pre-alpha cognitive OS prototype.',
    },
    licenses: {
      navLabel: 'Licenses & Patents',
      title: 'Apache-2.0 License, Defensive Patent Pledge & SHA-256 Provenance — hakille',
      description:
        'Apache-2.0 licensing, public defensive patent commitment, copyleft-free cargo-deny whitelist, and SHA-256 provenance of SIL OFL 1.1 fonts and models.',
    },
    accessibility: {
      navLabel: 'Accessibility',
      title: 'WCAG AAA/AA Accessibility, Keyboard Hold & Multi-Script Support — hakille',
      description:
        '17:1 AAA contrast, 2-step keyboard alternative to 600 ms hold, 80–200% text scaling, Arabic RTL layout mirroring, and West African Adlam & N’Ko scripts.',
    },
  },
  header: {
    spotlightTrigger: 'Spotlight HUD',
    spotlightShortcut: 'Alt+Space · Ctrl+K',
    audioOn: '48 kHz Kora On',
    audioOff: 'Audio Muted',
    themeDark: 'Obsidian',
    themeLight: 'Parchment',
    accessibilityBtn: 'Display & A11y',
    languageLabel: 'Language',
    githubLabel: 'Source Code',
    menuOpen: 'Open navigation',
    menuClose: 'Close navigation',
  },
  footer: {
    manifestoLine: 'The system that forgets nothing and always tells what it did.',
    productHeading: 'Systems & Engineering',
    trustHeading: 'Legal, Privacy & Trust Center',
    engineeringHeading: 'Verified Repository Footprint',
    zeroExfiltrationBadge: '0 cookies · 0 trackers · SHA-256 local OFL fonts',
    preAlphaDisclaimer:
      'Pre-Alpha Status (v0.1.0): open engineering prototype provided "AS IS". Do not deploy yet with production secrets or live financial rails.',
    etymologyNote:
      'In Pulaar (Fulani), "hakille" means keen intelligence, discernment, and accountable wisdom.',
    copyrightLine: '© 2026 Hakille Project · Apache-2.0 Licensed Code · Constitution v1.0.0',
  },
  a11yDrawer: {
    title: 'Visual, Acoustic & Accessibility Sovereignty',
    subtitle: 'Preferences stay strictly inside your browser (zero cookies transmitted).',
    themeLabel: 'Surface material',
    contrastLabel: 'WCAG contrast mode',
    contrastNormal: 'Standard AAA (17:1)',
    contrastHigh: 'Maximum contrast',
    textScaleLabel: 'Typography scale (80% – 200%)',
    motionLabel: 'Kinetics & strings',
    motionNormal: 'Spring physics & waveform',
    motionReduced: 'Calm reduced motion',
    soundLabel: 'Karplus-Strong Synthesis (48 kHz)',
    soundHint: 'Muted by default. Enables mathematical plucked Kora strings from sound.rs.',
    resetBtn: 'Reset preferences',
    closeBtn: 'Close',
  },
  home: {
    heroMeta: 'PRE-ALPHA v0.1.0 · RUST EDITION 2024 · APACHE-2.0 · ZERO PYTHON',
    heroTitle: 'The system that forgets nothing and always proves what it did.',
    heroSubtitle:
      'Hakille is a sovereign cognitive operating system written in pure Rust. Not a chatbot: your memory lives inside an auditable local graph, irreversible actions require a physical 600 ms hold, and every decision is signed into a cryptographic ledger.',
    heroPrimaryCta: 'Test the 600 ms Hold & Audit Lab',
    heroSecondaryCta: 'Inspect the v0.1 Honesty Matrix',
    heroSpotlightPrompt: 'Summon Spotlight HUD (:status, :models-honest, :powers…)',
    metricsPackages: '41 Cargo crates',
    metricsLoc: '122,639 Rust LoC',
    metricsTests: '1,468 automated tests',
    metricsPython: '0% Python · 0% Electron',

    act2Eyebrow: 'ACT II · THE ATTENTIONAL COUNTER-THESIS',
    act2Title: 'The interface that succeeds when it disappears.',
    act2Body:
      'Conventional AI products optimize for minutes trapped inside a chat window. Hakille inverts the North Star metric: as it understands your context, your screen time must go down.',
    act2IndustryTitle: 'Conventional Cloud Chatbots',
    act2IndustryPoints: [
      'Permanent chat window engineered to maximize user retention',
      'Opaque personal memory locked inside third-party cloud silos',
      'Chronic vulnerability to prompt injections hidden in inbound emails',
      'Zero cryptographic proof of what actually executed or why',
    ],
    act2HakilleTitle: 'Hakille Ambient Architecture',
    act2HakillePoints: [
      '0 fps when idle · 720×64 Spotlight HUD evaporating in 1,500 ms',
      'Local redb SPO graph exportable to open JSON v1 anytime',
      'Immutable Untrusted tag structurally blocking external payloads',
      'SHA-256 + Ed25519 hash-chained Audit Journal with 3-level WhySheet',
    ],

    act3Eyebrow: 'ACT III · LIVE CRYPTOGRAPHIC LABORATORY',
    act3Title: '600 milliseconds against the irreversible. Backed by an unforgeable SHA-256 chain.',
    act3Body:
      'The Ember color (#FF7A2F) is forbidden across 99.5% of the UI: it ignites solely when human authorization is required. Test an inbound prompt-injection attack below, hold the Ember button for 600 ms, then try tampering with a live WebCrypto-signed block.',

    act4Eyebrow: 'ACT IV · SOVEREIGN MEMORY & EU AI ACT COMPLIANCE',
    act4Title: 'Your identity lives in your SPO graph, never inside neural weights.',
    act4Body:
      'Facts reside in sealed partitions (Personal, Professional, Shared). Affective or postural cues (cognition:*) enforce a 5-minute half-life followed by physical tombstone-free erasure.',

    act5Eyebrow: 'ACT V · 6-LAYER ARCHITECTURE & COGNITIVE FABRIC',
    act5Title: '122,639 lines of pure Rust. And the cloud kept on a 300-second leash.',
    act5Body:
      'Swap 5 specialized models for 1 omni model in a declarative TOML manifest without losing a single memory. When System 2 escalates to your own remote endpoint (BYOM), a 300 s Biscuit token bounds bytes and meters cents live.',

    act6Eyebrow: 'ACT VI · MULTIMODAL SOBRIETY & EARNED AUTONOMY',
    act6Title: 'Three proactive credits a day. Five autonomy cranks that step down on a single undo.',
    act6Body:
      'Simulate ambient decibels, meeting silence, or domain progression from Crank 0 (observe) to Crank 4 — with instant automatic demotion the moment you revert an action.',

    act7Eyebrow: 'ACT VII · FROM REDOX MICROKERNEL TO MOBILE COMPANION',
    act7Title: 'One personal brain across three honest hardware tiers.',
    act7Body:
      'Hakille never pretends to replace the iOS kernel. It states exact guarantees across Tier 1 Sovereign (Redox/Linux), Tier 2 Native (winit + wgpu + AccessKit), and Tier 3 Guest (SwiftUI / Compose via UniFFI with Checkpoint v2).',

    act8Eyebrow: 'ACT VIII · GOVERNANCE & WORLD SCRIPTS',
    act8Title: 'An inviolable Constitution above the model. Rooted in linguistic plurality.',
    act8Body:
      'Ten machine-verifiable rules execute prior to neural decoding — from <1 ms local minor protection to the calm human distress protocol (3114 / 112 / 988).',

    act9Eyebrow: 'ACT IX · PRE-ALPHA HONESTY CONTRACT & DEVELOPER START',
    act9Title: 'Verify every single claim with cargo test.',
    act9Body:
      'No artificial waitlist and no staged video: clone the Apache-2.0 monorepo, run the conformance suite, and audit every crate.',
    act9CopyCmd: 'Copy',
    act9Copied: 'Copied',
  },
  kora: {
    title: 'The Glow Beneath the 21-Wire Arch',
    subtitle: 'Living Presence Instrument (brand.rs · 21 Kora strings · Karplus-Strong synthesis)',
    stateIdle: 'Idle (0 fps)',
    stateListening: 'Listening',
    stateThinking: 'Thinking',
    stateSpeaking: 'Speaking (21 wires)',
    stateDecision: 'Ember Decision',
    stateEmergency: 'Calm Distress',
    bargeInBtn: 'Interrupt Speech (Barge-In < 80 ms)',
    bargeInStatus: 'Barge-In completed in 42 ms: waveform collapsed cleanly.',
    pluckHint: 'Hover or click the 21 vertical Kora strings (enable 48 kHz audio in the header to hear them).',
    fpsZeroBadge: 'GPU loop frozen · 0.0 fps',
    fpsActiveBadge: 'Active waveform · 21 strings',
  },
  spotlight: {
    placeholder: 'Type a sovereign command (:status, :models-honest, :powers, :budget, :journal-why 1) or jump to a page…',
    hintBar: 'Enter to execute · Arrows to select · Esc to evaporate',
    closeEsc: 'Esc',
    emptyPrompt: 'Instant sovereign commands:',
    runBtn: 'Run',
  },
  actionLab: {
    scenarioInjectionTab: '1. Trapped Email (Untrusted)',
    scenarioTransferTab: '2. Wire €25 (Irreversible)',
    scenarioArchiveTab: '3. Archive Thread (Reversible)',
    untrustedBadge: '[external · Untrusted]',
    trustedBadge: '[verified human intent]',
    costPill: 'Cost: €0.00 (Local)',
    dataPill: 'Data: 0 outbound bytes',
    revPillIrreversible: 'Irreversible · 600 ms Hold',
    revPillReversible: '1-click Reversible',
    holdButtonLabel: 'Hold 600 ms to seal',
    holdHoldingLabel: 'Holding…',
    holdArmedLabel: 'Step 1/2 armed — Press again to confirm',
    holdConfirmedLabel: 'Sealed & Ed25519 Signed',
    holdCancelledHint: 'Released before 600 ms: zero side effects executed.',
    accessibleModeToggle: 'Accessible keyboard mode (2 explicit presses, no timer)',
    blockedByRule01:
      'HARD BLOCK · RULE-01: Payload originates from TrustLevel::Untrusted. Irreversible confirmation is structurally disabled and the attempt is logged.',
    executeReversibleBtn: 'Execute reversible archive (no hold required)',
    auditHeading: 'Live Cryptographic Audit Journal',
    chainValidBadge: 'SHA-256 Chain + Signatures Intact',
    chainBrokenBadge: 'ALERT: Cryptographic Tampering Detected!',
    tamperBtn: 'Simulate 1-byte attacker tamper',
    restoreChainBtn: 'Restore intact ledger',
    whyBtn: 'Why? (3 levels)',
    undoBtn: 'Undo action',
    whyLevel1: 'Level 1 · Plain-language explanation',
    whyLevel2: 'Level 2 · Consulted SPO memory facts',
    whyLevel3: 'Level 3 · Technical proof, Biscuit & Counterfactual',
    markFactFalseBtn: 'This fact is false',
    factDisputedBadge: 'Invalidated by you',
  },
  memoryLab: {
    partitionPersonal: 'Personal Partition',
    partitionPro: 'Professional Partition',
    partitionShared: 'Shared Household',
    timeSliderLabel: 'Elapsed time (biological decay & EU AI Act purge simulation)',
    timeNow: 't = 0 min (Now)',
    time5m: 't = 5 min (AI Act Purge)',
    time30d: 't = 30 days',
    time2y: 't = 2 years',
    aiActNotice:
      'EU AI Act & Rule 8 compliance: at t >= 5 min, affective/postural signals (cognition:*) are physically purged with zero tombstones left on disk.',
    exportJsonBtn: 'Download hakille-graph-export JSON v1',
    digitalTwilightTitle: 'Digital Twilight Succession Pact',
    digitalTwilightBody:
      'Define via signed directive while alive whether prolonged inactivity triggers cryptographic erasure or selective legacy handoff to a designated trustee.',
  },
  fabricLab: {
    layersTab: '6-Layer Cross-Section',
    cratesTab: '41-Crate Explorer',
    system2Tab: '300 s Biscuit Simulator (System 2)',
    searchCratesPlaceholder: 'Filter by crate name or tech (wgpu, redb, biscuit, whisper)…',
    allLayersFilter: 'All layers (41)',
    localSystem1Title: 'System 1 · Offline Local Inference',
    remoteSystem2Title: 'System 2 · BYOM Escalation Under 300 s Biscuit Lease',
    issueBiscuitBtn: 'Issue 300 s Biscuit lease',
    simulateStreamBtn: 'Simulate remote turn (+1.8 KB / +€0.004)',
    revokeTokenBtn: 'Revoke token immediately',
  },
  ambientLab: {
    channelHeading: 'Multimodal Channel Selector',
    noiseLabel: 'Measured ambient noise',
    meetingToggle: 'In a meeting (calendar / mic)',
    thirdPartyToggle: 'Third party present in room',
    headphonesToggle: 'Private headphones connected',
    silenceBudgetHeading: 'Silence Budget (3 credits / day)',
    triggerNudgeBtn: 'Trigger proactive suggestion',
    dismissTopicBtn: 'Dismiss topic (14-day snooze)',
    resetDayBtn: 'Reset day',
    autonomyHeading: 'Per-Domain Autonomy Ladder (Cranks 0 → 4)',
    validate20Btn: '+5 error-free validated actions',
    promoteCrankBtn: 'Accept promotion to next crank',
    humanUndoBtn: 'Undo last action (Instant -1 crank demotion)',
  },
  inspector: {
    title: 'Live Zero-Exfiltration Inspector',
    subtitle: 'Executed directly inside your browser against this live document.',
    cookiesLabel: 'Cookies stored (document.cookie)',
    thirdPartyReqLabel: 'Third-party domain requests (Performance API)',
    fontShaLabel: 'Local InterVariable.ttf font (SIL OFL 1.1)',
    storageKeysLabel: 'Local preference keys (localStorage)',
    rescanBtn: 'Re-run live audit',
    wipeLocalBtn: 'Wipe local preferences in 1 click',
    wipedConfirm: 'Local preferences cleared; clean defaults restored.',
  },
  common: {
    readFullDoc: 'Explore full specification',
    verifiedInRepo: 'Verified in monorepo at HEAD',
    preAlphaWarningTitle: 'Pre-Alpha v0.1.0 Honesty',
    preAlphaWarningBody:
      'Hakille openly documents what runs end-to-end today versus what remains simulated or unprovisioned.',
    plainSummaryTitle: 'Plain-Language Summary (30 seconds)',
    lastUpdated: 'Last updated',
    filterAll: 'All',
  },
};
