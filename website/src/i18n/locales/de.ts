import { TranslationSchema } from '../types';

export const deLocale: TranslationSchema = {
  meta: {
    brandSignature: 'Hakille — souveräne Intelligenz',
    preAlphaBadge: 'Pre-Alpha v0.1.0 · Apache-2.0 · 100 % Rust',
    skipToContent: 'Direkt zum Hauptinhalt springen',
  },
  routes: {
    home: {
      navLabel: 'Manifest',
      title: 'hakille — souveräne Intelligenz · Kognitives Betriebssystem in reinem Rust',
      description:
        'Das System, das nichts vergisst und stets beweist, was es getan hat. Lokaler SPO-Speichergraph, Biscuit-Capabilities, 600 ms Haltebestätigung und SHA-256 + Ed25519 Audit-Journal.',
    },
    architecture: {
      navLabel: 'Architektur',
      title: '6-Schichten-Architektur & 41 Rust-Crates — hakille',
      description:
        'Einblick in Hakilles 6 Schichten: 122.639 Zeilen Rust 2024, 0 % Python, native winit+wgpu+AccessKit GPU-Shell, Wasmtime 48 und Cognitive Fabric.',
    },
    honestStatus: {
      navLabel: 'Realer Status v0.1',
      title: 'Pre-Alpha v0.1 Ehrlichkeitsmatrix — hakille',
      description:
        'Radikale Transparenz darüber, was heute real getestet läuft (1.468 Tests, ×46 CPU-Whisper, Mimi, Qwen GGUF) und was Roadmap für Phase 1–3 bleibt.',
    },
    security: {
      navLabel: 'Sicherheit',
      title: 'Biscuit-Capabilities, Untrusted-Isolation & Sicherheit — hakille',
      description:
        'Monoton abgeschwächte Biscuit-v5-Tokens, strukturelle Prompt-Injection-Abwehr, Argon2id HAKKEY02-Keystore und RFC 9116 (72h Offenlegung).',
    },
    constitution: {
      navLabel: 'Verfassung',
      title: 'Unverletzliche Verfassung v1.0.0 — hakille',
      description:
        'Die 7 Artikel und 10 maschinenprüfbaren Regeln über jedem Modell: kalibrierte Vertrauensschwellen, Jugendschutz und Krisenprotokoll.',
    },
    install: {
      navLabel: 'Installieren',
      title: 'RAM-Konfigurator, OS-Substrate & Rust-Kompilierung — hakille',
      description:
        'Berechnen Sie den exakten lokalen Speicherbedarf von Hakille (2 GB bis 16 GB+) und kompilieren Sie Daemon, winit+wgpu-Desktop oder UniFFI-Mobile-Bridge.',
    },
    privacy: {
      navLabel: 'Datenschutz & DSGVO',
      title: 'DSGVO-, ePrivacy- & EU-AI-Act-Datenschutzerklärung — hakille',
      description:
        'Doppelte Souveränität: null Cookies, null Tracker und lokale OFL-Schriften auf hakille.ai; lokaler redb-Speicher und physische 5-Minuten-Löschung affektiver Signale.',
    },
    legalNotice: {
      navLabel: 'Impressum',
      title: 'Rechtliche Hinweise & Open-Source-Governance — hakille',
      description:
        'Angaben zum Open-Source-Projekt Hakille, offizielle Kontaktwege, souveräne statische Bereitstellung und Markenhinweise.',
    },
    conditions: {
      navLabel: 'Nutzungsbedingungen',
      title: 'Nutzungsbedingungen & Pre-Alpha-Haftungsausschluss (AS IS) — hakille',
      description:
        'Nutzungsregeln für hakille.ai sowie wichtige Hinweise zur Erprobung des Pre-Alpha-Prototyps v0.1 ohne Gewährleistung.',
    },
    licenses: {
      navLabel: 'Lizenzen & Patente',
      title: 'Apache-2.0-Lizenz, Defensives Patentversprechen & SHA-256 — hakille',
      description:
        'Apache-2.0-Lizenzierung, defensives Patentversprechen, copyleft-freie cargo-deny-Prüfung und SHA-256-Nachweise für SIL-OFL-1.1-Schriften.',
    },
    accessibility: {
      navLabel: 'Barrierefreiheit',
      title: 'WCAG AAA/AA Barrierefreiheit, Tastatur, RTL & Schriften — hakille',
      description:
        '17:1 AAA-Kontrast, 2-Schritt-Tastaturalternative für 600 ms, 80–200 % Schriftgröße, arabisches RTL-Layout sowie Adlam- & N’Ko-Schriften.',
    },
  },
  header: {
    spotlightTrigger: 'Spotlight HUD',
    spotlightShortcut: 'Alt+Leertaste · Strg+K',
    audioOn: '48 kHz Kora aktiv',
    audioOff: 'Ton stumm',
    themeDark: 'Obsidian',
    themeLight: 'Pergament',
    accessibilityBtn: 'Anzeige & A11y',
    languageLabel: 'Sprache',
    githubLabel: 'Quellcode',
    menuOpen: 'Navigation öffnen',
    menuClose: 'Navigation schließen',
  },
  footer: {
    manifestoLine: 'Das System, das nichts vergisst und stets offenlegt, was es getan hat.',
    productHeading: 'System & Engineering',
    trustHeading: 'Rechts-, Datenschutz- & Vertrauenszentrum',
    engineeringHeading: 'Verifizierter Repository-Umfang',
    zeroExfiltrationBadge: '0 Cookies · 0 Tracker · Lokale SHA-256 OFL-Schriften',
    preAlphaDisclaimer:
      'Pre-Alpha-Status (v0.1.0): offener Engineering-Prototyp „WIE BESEHEN“ (AS IS). Noch nicht für Produktivgeheimnisse oder echte Zahlungen einsetzen.',
    etymologyNote:
      'Auf Pulaar (Fulfulde) bedeutet „hakille“ wache Intelligenz, Urteilskraft und verantwortungsvolle Weisheit.',
    copyrightLine: '© 2026 Hakille-Projekt · Code unter Apache-2.0 · Verfassung v1.0.0',
  },
  a11yDrawer: {
    title: 'Visuelle, Akustische & Barrierefreie Souveränität',
    subtitle: 'Einstellungen verbleiben ausschließlich lokal im Browser (keine Cookies).',
    themeLabel: 'Oberflächenmaterial',
    contrastLabel: 'WCAG-Kontrastmodus',
    contrastNormal: 'Standard AAA (17:1)',
    contrastHigh: 'Maximalkontrast',
    textScaleLabel: 'Schriftskalierung (80 % – 200 %)',
    motionLabel: 'Bewegung & Saiten',
    motionNormal: 'Federphysik & Wellenform',
    motionReduced: 'Ruhige reduzierte Bewegung',
    soundLabel: 'Karplus-Strong-Synthese (48 kHz)',
    soundHint: 'Standardmäßig stumm. Aktiviert mathematisch gezupfte Kora-Saiten aus sound.rs.',
    resetBtn: 'Zurücksetzen',
    closeBtn: 'Schließen',
  },
  home: {
    heroMeta: 'PRE-ALPHA v0.1.0 · RUST EDITION 2024 · APACHE-2.0 · NULL PYTHON',
    heroTitle: 'Das System, das nichts vergisst und immer beweist, was es getan hat.',
    heroSubtitle:
      'Hakille ist ein souveränes kognitives Betriebssystem in reinem Rust. Kein Chatbot: Ihr Gedächtnis liegt in einem prüfbaren lokalen Graphen, unumkehrbare Aktionen verlangen 600 ms physisches Halten, und jede Entscheidung wird kryptografisch signiert.',
    heroPrimaryCta: '600-ms- & Audit-Labor testen',
    heroSecondaryCta: 'v0.1 Ehrlichkeitsmatrix prüfen',
    heroSpotlightPrompt: 'Spotlight HUD öffnen (:status, :models-honest, :powers…)',
    metricsPackages: '41 Cargo-Crates',
    metricsLoc: '122.639 Zeilen Rust',
    metricsTests: '1.468 Unit-Tests',
    metricsPython: '0 % Python · 0 % Electron',

    act2Eyebrow: 'AKT II · GEGENTHESE ZUR AUFMERKSAMKEITSÖKONOMIE',
    act2Title: 'Die Oberfläche, die erfolgreich ist, wenn sie verschwindet.',
    act2Body:
      'Herkömmliche KI-Produkte maximieren Bildschirmzeit im Chatfenster. Hakille kehrt die Leitmetrik um: Je besser das System Sie versteht, desto kürzer sehen Sie es.',
    act2IndustryTitle: 'Klassische Cloud-Chatbots',
    act2IndustryPoints: [
      'Dauerhaftes Chatfenster zur Maximierung der Verweildauer',
      'Intransparenter persönlicher Speicher auf fremden Cloud-Servern',
      'Chronische Anfälligkeit für versteckte Prompt-Injections in E-Mails',
      'Kein kryptografischer Nachweis über tatsächlich ausgeführte Aktionen',
    ],
    act2HakilleTitle: 'Hakille Ambient-Architektur',
    act2HakillePoints: [
      '0 fps im Ruhezustand · 720×64 Spotlight-HUD verdampft nach 1.500 ms',
      'Lokaler redb-SPO-Graph jederzeit als offenes JSON v1 exportierbar',
      'Unveränderliches Untrusted-Tag blockiert externe Inhalte strukturell',
      'SHA-256 + Ed25519 verkettetes Audit-Journal mit 3-stufigem „Warum?“-Blatt',
    ],

    act3Eyebrow: 'AKT III · KRYPTOGRAFISCHES LIVE-LABOR',
    act3Title: '600 Millisekunden gegen das Unumkehrbare. Gesichert durch eine SHA-256-Kette.',
    act3Body:
      'Die Glut-Farbe (#FF7A2F) ist auf 99,5 % der Oberfläche verboten: Sie leuchtet nur bei menschlichen Entscheidungen. Testen Sie unten eine E-Mail-Injektion, halten Sie 600 ms gedrückt und versuchen Sie danach, einen per WebCrypto signierten Block zu manipulieren.',

    act4Eyebrow: 'AKT IV · SOUVERÄNER SPEICHER & EU-AI-ACT',
    act4Title: 'Ihre Identität lebt in Ihrem SPO-Graphen, niemals in Modellgewichten.',
    act4Body:
      'Fakten sind strikt getrennt (Privat, Beruflich, Geteilt). Affektive Signale (cognition:*) haben 5 Minuten Halbwertszeit und werden rückstandsfrei physisch gelöscht.',

    act5Eyebrow: 'AKT V · 6-SCHICHTEN-ARCHITEKTUR & COGNITIVE FABRIC',
    act5Title: '122.639 Zeilen reines Rust. Und die Cloud an einer 300-Sekunden-Leine.',
    act5Body:
      'Tauschen Sie Modelle deklarativ per TOML-Manifest ohne Gedächtnisverlust. Eskaliert System 2 zu Ihrem eigenen Endpunkt (BYOM), begrenzt ein 300-s-Biscuit-Token Bytes und Cent-Kosten live.',

    act6Eyebrow: 'AKT VI · MULTIMODALE RUHE & VERDIENTE AUTONOMIE',
    act6Title: 'Drei proaktive Impulse pro Tag. Fünf Autonomiestufen mit sofortigem Rückschritt bei Widerruf.',
    act6Body:
      'Simulieren Sie Umgebungsgeräusche in dB, Besprechungsruhe oder den Aufstieg von Stufe 0 bis 4 — inklusive sofortiger Rückstufung beim ersten Rückgängigmachen.',

    act7Eyebrow: 'AKT VII · VOM REDOX-MIKROKERNEL ZUM MOBILEN BEGLEITER',
    act7Title: 'Ein persönliches Gehirn über drei ehrliche Hardware-Stufen.',
    act7Body:
      'Hakille benennt präzise Garantien für Stufe 1 Souverän (Redox/Linux), Stufe 2 Nativ (winit + wgpu + AccessKit) und Stufe 3 Gast (SwiftUI / Compose via UniFFI mit Checkpoint v2).',

    act8Eyebrow: 'AKT VIII · VERFASSUNG & SCHRIFTEN DER WELT',
    act8Title: 'Eine unverletzliche Verfassung über dem Modell. Verwurzelt in sprachlicher Vielfalt.',
    act8Body:
      'Zehn maschinenprüfbare Regeln greifen vor jeder neuronalen Ausgabe — vom lokalen Jugendschutz in < 1 ms bis zum ruhigen Krisenprotokoll (3114 / 112 / 988).',

    act9Eyebrow: 'AKT IX · EHRLICHKEITSVERTRAG & SCHNELLSTART',
    act9Title: 'Überprüfen Sie jede Aussage mit cargo test.',
    act9Body:
      'Keine künstliche Warteliste: Klonen Sie das Apache-2.0-Monorepo, starten Sie die Konformitätssuite und prüfen Sie jedes Crate.',
    act9CopyCmd: 'Kopieren',
    act9Copied: 'Kopiert',
  },
  kora: {
    title: 'Das Leuchten unter dem Bogen aus 21 Fäden',
    subtitle: 'Präsenz-Instrument (brand.rs · 21 Kora-Saiten · Karplus-Strong-Synthese)',
    stateIdle: 'Ruhe (0 fps)',
    stateListening: 'Hören',
    stateThinking: 'Denken',
    stateSpeaking: 'Sprechen (21 Fäden)',
    stateDecision: 'Glut-Entscheidung',
    stateEmergency: 'Stille Notfallhilfe',
    bargeInBtn: 'Sprache unterbrechen (Barge-In < 80 ms)',
    bargeInStatus: 'Barge-In in 42 ms ausgeführt: Welle sofort eingeklappt.',
    pluckHint: 'Berühren oder klicken Sie die 21 Kora-Saiten (48-kHz-Ton oben aktivieren, um sie zu hören).',
    fpsZeroBadge: 'GPU-Schleife pausiert · 0,0 fps',
    fpsActiveBadge: 'Aktive Welle · 21 Saiten',
  },
  spotlight: {
    placeholder: 'Befehl eingeben (:status, :models-honest, :powers, :budget, :journal-why 1) oder Seite suchen…',
    hintBar: 'Eingabe zum Ausführen · Pfeile zur Auswahl · Esc zum Ausblenden',
    closeEsc: 'Esc',
    emptyPrompt: 'Sofort ausführbare souveräne Befehle:',
    runBtn: 'Ausführen',
  },
  actionLab: {
    scenarioInjectionTab: '1. Präparierte Mail (Untrusted)',
    scenarioTransferTab: '2. 25 € Überweisung (Unumkehrbar)',
    scenarioArchiveTab: '3. Archivierung (Umkehrbar)',
    untrustedBadge: '[extern · Untrusted]',
    trustedBadge: '[geprüfte menschliche Absicht]',
    costPill: 'Kosten: 0,00 € (Lokal)',
    dataPill: 'Daten: 0 Bytes ausgehend',
    revPillIrreversible: 'Unumkehrbar · 600 ms Halten',
    revPillReversible: '1-Klick umkehrbar',
    holdButtonLabel: '600 ms halten zum Besiegeln',
    holdHoldingLabel: 'Halten…',
    holdArmedLabel: 'Schritt 1/2 bereit — Erneut drücken zum Bestätigen',
    holdConfirmedLabel: 'Besiegelt & Ed25519-signiert',
    holdCancelledHint: 'Vor 600 ms losgelassen: keine Änderung ausgeführt.',
    accessibleModeToggle: 'Barrierefreier Tastaturmodus (2 explizite Tastendrücke)',
    blockedByRule01:
      'HARD BLOCK · RULE-01: Inhalt stammt aus TrustLevel::Untrusted. Unumkehrbare Bestätigung ist strukturell gesperrt und protokolliert.',
    executeReversibleBtn: 'Umkehrbare Archivierung sofort ausführen',
    auditHeading: 'Kryptografisches Live-Audit-Journal',
    chainValidBadge: 'SHA-256-Kette + Signaturen intakt',
    chainBrokenBadge: 'WARNUNG: Kryptografische Manipulation erkannt!',
    tamperBtn: '1-Byte-Manipulation simulieren',
    restoreChainBtn: 'Intaktes Journal wiederherstellen',
    whyBtn: 'Warum? (3 Ebenen)',
    undoBtn: 'Rückgängig',
    whyLevel1: 'Ebene 1 · Klare Ein-Satz-Erklärung',
    whyLevel2: 'Ebene 2 · Verwendete SPO-Fakten',
    whyLevel3: 'Ebene 3 · Technischer Nachweis, Biscuit & Kontrafaktisch',
    markFactFalseBtn: 'Dieser Fakt ist falsch',
    factDisputedBadge: 'Von Ihnen widerrufen',
  },
  memoryLab: {
    partitionPersonal: 'Persönlicher Bereich',
    partitionPro: 'Beruflicher Bereich',
    partitionShared: 'Geteilter Haushalt',
    timeSliderLabel: 'Zeitverlauf (biologisches Vergessen & EU-AI-Act-Löschung)',
    timeNow: 't = 0 Min. (Jetzt)',
    time5m: 't = 5 Min. (AI-Act-Purge)',
    time30d: 't = 30 Tage',
    time2y: 't = 2 Jahre',
    aiActNotice:
      'EU-AI-Act & Regel 8: Ab t >= 5 Min. werden affektive Signale (cognition:*) physisch ohne Tombstone gelöscht.',
    exportJsonBtn: 'hakille-graph-export JSON v1 herunterladen',
    digitalTwilightTitle: 'Pakt „Digitales Abendrot“',
    digitalTwilightBody:
      'Legen Sie per signierter Verfügung fest, ob Ihr Graph nach langer Inaktivität kryptografisch gelöscht oder gezielt übergeben wird.',
  },
  fabricLab: {
    layersTab: '6-Schichten-Schnitt',
    cratesTab: '41-Crates-Explorer',
    system2Tab: '300-s-Biscuit-Simulator (System 2)',
    searchCratesPlaceholder: 'Nach Crate oder Technik filtern (wgpu, redb, biscuit, whisper)…',
    allLayersFilter: 'Alle Schichten (41)',
    localSystem1Title: 'System 1 · Lokale Offline-Inferenz',
    remoteSystem2Title: 'System 2 · BYOM-Eskalation mit 300-s-Biscuit-Lease',
    issueBiscuitBtn: '300-s-Biscuit-Token ausstellen',
    simulateStreamBtn: 'Anfrage simulieren (+1,8 KB / +0,004 €)',
    revokeTokenBtn: 'Token sofort widerrufen',
  },
  ambientLab: {
    channelHeading: 'Multimodale Kanalauswahl',
    noiseLabel: 'Gemessener Geräuschpegel',
    meetingToggle: 'In Besprechung (Kalender / Mikro)',
    thirdPartyToggle: 'Dritte Person im Raum',
    headphonesToggle: 'Kopfhörer verbunden',
    silenceBudgetHeading: 'Stille-Budget (3 Impulse / Tag)',
    triggerNudgeBtn: 'Proaktiven Hinweis senden',
    dismissTopicBtn: 'Thema ablehnen (14 Tage Pause)',
    resetDayBtn: 'Neuer Tag',
    autonomyHeading: 'Autonomieleiter pro Domäne (Stufen 0 → 4)',
    validate20Btn: '+5 fehlerfreie Aktionen',
    promoteCrankBtn: 'Aufstieg bestätigen',
    humanUndoBtn: 'Aktion widerrufen (Sofort -1 Stufe)',
  },
  inspector: {
    title: 'Live-Inspektor „Null Exfiltration“',
    subtitle: 'Direkt in Ihrem Browser gegen dieses Dokument geprüft.',
    cookiesLabel: 'Gespeicherte Cookies (document.cookie)',
    thirdPartyReqLabel: 'Drittanbieter-Anfragen (Performance API)',
    fontShaLabel: 'Lokale Schrift InterVariable.ttf (SIL OFL 1.1)',
    storageKeysLabel: 'Lokale Schlüssel (localStorage)',
    rescanBtn: 'Live-Prüfung wiederholen',
    wipeLocalBtn: 'Lokale Einstellungen mit 1 Klick löschen',
    wipedConfirm: 'Lokale Einstellungen gelöscht; Standard wiederhergestellt.',
  },
  common: {
    readFullDoc: 'Vollständige Spezifikation öffnen',
    verifiedInRepo: 'Verifiziert im Monorepo bei HEAD',
    preAlphaWarningTitle: 'Pre-Alpha v0.1.0 Transparenz',
    preAlphaWarningBody:
      'Hakille dokumentiert offen, was heute real läuft und was noch simuliert oder ungeprovisioniert ist.',
    plainSummaryTitle: 'Kurzfassung im Klartext (30 Sekunden)',
    lastUpdated: 'Zuletzt aktualisiert',
    filterAll: 'Alle',
  },
};
