import { TranslationSchema } from '../types';

export const frLocale: TranslationSchema = {
  meta: {
    brandSignature: 'Hakille — intelligence souveraine',
    preAlphaBadge: 'Pré-Alpha v0.1.0 · Apache-2.0 · 100 % Rust',
    skipToContent: 'Aller directement au contenu principal',
  },
  routes: {
    home: {
      navLabel: 'Manifeste',
      title: 'hakille — intelligence souveraine · Système d’exploitation cognitif en Rust',
      description:
        'Le système qui n’oublie rien et qui dit toujours ce qu’il a fait. Graphe mémoire SPO local, capabilities Biscuit, maintien physique 600 ms et journal d’audit chaîné SHA-256 + Ed25519.',
    },
    architecture: {
      navLabel: 'Architecture',
      title: 'Architecture & 41 Crates Rust — hakille',
      description:
        'Plongée dans les 6 couches de Hakille : 122 639 lignes de Rust 2024, 0 % Python, shell GPU winit+wgpu+AccessKit, Wasmtime 48 et Cognitive Fabric.',
    },
    honestStatus: {
      navLabel: 'État Réel v0.1',
      title: 'Matrice de Vérité Pré-Alpha v0.1 — hakille',
      description:
        'Transparence radicale sur ce qui est réel et golden-testé aujourd’hui (1 468 tests, Whisper ×46 CPU, Mimi, Qwen GGUF) et ce qui reste sur la feuille de route.',
    },
    security: {
      navLabel: 'Sécurité',
      title: 'Sécurité par Capabilities, Tag Untrusted & Divulgation — hakille',
      description:
        'Jetons Biscuit v5 à atténuation monotone, immunité structurelle aux prompt injections, coffre HAKKEY02 Argon2id et politique RFC 9116 (72h).',
    },
    constitution: {
      navLabel: 'Constitution',
      title: 'Constitution Inviolable v1.0.0 — hakille',
      description:
        'Les 7 Articles et 10 Règles machine-vérifiables qui gouvernent Hakille au-dessus de tout modèle : seuils de confiance, protection mineurs et protocole détresse 3114.',
    },
    install: {
      navLabel: 'Installer',
      title: 'Dimensionneur RAM Matériel, OS & Compilation Rust — hakille',
      description:
        'Calculez l’empreinte mémoire locale exacte de Hakille sur votre machine (2 Go à 16 Go+) et compilez le démon, le shell winit+wgpu ou le pont mobile UniFFI.',
    },
    privacy: {
      navLabel: 'Confidentialité & RGPD',
      title: 'Politique de Confidentialité RGPD, ePrivacy & AI Act — hakille',
      description:
        'Double garantie souveraine : zéro cookie, zéro analytics et zéro police externe sur hakille.ai ; mémoire locale redb et purge physique 5 min des affects dans Hakille.',
    },
    legalNotice: {
      navLabel: 'Mentions Légales',
      title: 'Mentions Légales (LCEN) & Gouvernance — hakille',
      description:
        'Identification du projet open-source Hakille, contacts officiels, hébergement statique souverain et propriété intellectuelle.',
    },
    conditions: {
      navLabel: 'Conditions',
      title: 'Conditions d’Utilisation & Clause Prototype Pré-Alpha — hakille',
      description:
        'Conditions d’usage de hakille.ai et avertissements essentiels sur l’utilisation EN L’ÉTAT (AS IS) du prototype pré-alpha v0.1.',
    },
    licenses: {
      navLabel: 'Licences & Brevets',
      title: 'Licences Open-Source Apache-2.0, Brevets Défensifs & Provenance — hakille',
      description:
        'Licence Apache-2.0, engagement de brevets défensifs, audit cargo-deny sans copyleft et sommes SHA-256 des polices SIL OFL 1.1 et modèles.',
    },
    accessibility: {
      navLabel: 'Accessibilité',
      title: 'Accessibilité WCAG AAA/AA, Clavier, RTL & Écritures — hakille',
      description:
        'Contraste 17:1 AAA, alternative clavier au maintien 600 ms, échelle typographique 80–200 %, support RTL arabe et alphabets ouest-africains Adlam & N’Ko.',
    },
  },
  header: {
    spotlightTrigger: 'Spotlight HUD',
    spotlightShortcut: 'Alt+Espace · Ctrl+K',
    audioOn: 'Kora 48 kHz actif',
    audioOff: 'Son muet',
    themeDark: 'Obsidienne',
    themeLight: 'Parchemin',
    accessibilityBtn: 'Affichage & A11y',
    languageLabel: 'Langue',
    githubLabel: 'Code Source',
    menuOpen: 'Ouvrir la navigation',
    menuClose: 'Fermer la navigation',
  },
  footer: {
    manifestoLine: 'Le système qui n’oublie rien et qui dit toujours ce qu’il a fait.',
    productHeading: 'Ingénierie & Système',
    trustHeading: 'Centre Juridique, Vie Privée & Confiance',
    engineeringHeading: 'Empreinte Vérifiée du Dépôt',
    zeroExfiltrationBadge: '0 cookie · 0 traceur · Polices OFL locales SHA-256',
    preAlphaDisclaimer:
      'Statut Pré-Alpha (v0.1.0) : prototype d’ingénierie ouvert fourni « EN L’ÉTAT ». Ne l’utilisez pas encore pour des secrets de production ou paiements réels.',
    etymologyNote:
      'En pulaar (peul), « hakille » désigne l’intelligence vive, le discernement et la sagesse responsable.',
    copyrightLine: '© 2026 Projet Hakille · Code sous licence Apache-2.0 · Constitution v1.0.0',
  },
  a11yDrawer: {
    title: 'Souveraineté Visuelle, Acoustique & Accessibilité',
    subtitle: 'Réglages appliqués localement dans votre navigateur (aucun cookie envoyé au serveur).',
    themeLabel: 'Matière de surface',
    contrastLabel: 'Niveau de contraste WCAG',
    contrastNormal: 'Standard AAA (17:1)',
    contrastHigh: 'Contraste maximal',
    textScaleLabel: 'Échelle typographique (80 % – 200 %)',
    motionLabel: 'Cinétique & cordes',
    motionNormal: 'Physique ressort & onde',
    motionReduced: 'Mouvement réduit calme',
    soundLabel: 'Synthèse Karplus-Strong (48 kHz)',
    soundHint: 'Muet par défaut. Active les vraies cordes pincées mathématiques de sound.rs.',
    resetBtn: 'Réinitialiser les réglages',
    closeBtn: 'Fermer',
  },
  home: {
    heroMeta: 'PRÉ-ALPHA v0.1.0 · RUST ÉDITION 2024 · APACHE-2.0 · ZÉRO PYTHON',
    heroTitle: 'Le système qui n’oublie rien et qui dit toujours ce qu’il a fait.',
    heroSubtitle:
      'Hakille est un système d’exploitation cognitif souverain écrit en Rust pur. Pas un chatbot : votre mémoire vit dans un graphe local vérifiable, chaque action sensible exige 600 ms de maintien physique, et chaque décision est signée au Journal d’Audit.',
    heroPrimaryCta: 'Éprouver le laboratoire 600 ms & Audit',
    heroSecondaryCta: 'Inspecter la Matrice de Vérité v0.1',
    heroSpotlightPrompt: 'Invoquer le HUD Spotlight (:status, :models-honest, :powers…)',
    metricsPackages: '41 crates Cargo',
    metricsLoc: '122 639 lignes Rust',
    metricsTests: '1 468 tests unitaires',
    metricsPython: '0 % Python · 0 % Electron',

    act2Eyebrow: 'ACTE II · CONTRE-THÈSE ATTENTIONNELLE',
    act2Title: 'L’interface qui réussit quand elle disparaît.',
    act2Body:
      'L’industrie mesure son succès au temps d’écran capturé dans une fenêtre de discussion. Hakille inverse l’étoile polaire : moins vous passez de secondes devant son interface, mieux il vous sert.',
    act2IndustryTitle: 'Assistants Cloud Conventionnels',
    act2IndustryPoints: [
      'Fenêtre de chat permanente conçue pour maximiser la rétention',
      'Mémoire opaque enfermée sur des serveurs tiers hors de votre contrôle',
      'Vulnérabilité chronique aux injections de prompts cachées dans les mails',
      'Aucune preuve cryptographique de ce qui a réellement été exécuté',
    ],
    act2HakilleTitle: 'Architecture Ambiante Hakille',
    act2HakillePoints: [
      '0 fps au repos · HUD Spotlight 720×64 qui s’évapore en 1 500 ms',
      'Graphe SPO local redb exportable en JSON ouvert à tout instant',
      'Tag immuable Untrusted bloquant structurellement les contenus externes',
      'Journal d’Audit chaîné SHA-256 + Ed25519 avec Feuille « Pourquoi ? »',
    ],

    act3Eyebrow: 'ACTE III · LABORATOIRE CRYPTOGRAPHIQUE EN DIRECT',
    act3Title: '600 millisecondes contre l’irréversible. Et une chaîne SHA-256 qui ne ment jamais.',
    act3Body:
      'La couleur Braise (#FF7A2F) est interdite sur 99,5 % du système : elle ne s’allume que lorsqu’une décision humaine est requise. Testez ci-dessous une attaque par courriel piégé, maintenez physiquement le bouton Braise 600 ms, puis tentez d’altérer un octet du journal signé en temps réel par WebCrypto.',

    act4Eyebrow: 'ACTE IV · MÉMOIRE SOUVERAINE & CONFORMITÉ AI ACT',
    act4Title: 'Votre identité habite votre graphe SPO, jamais les poids d’un modèle.',
    act4Body:
      'Les faits sont rangés en cloisons étanches (Personnel, Professionnel, Partagé). Les signaux émotionnels ou posturaux (cognition:*) subissent une demi-vie de 5 minutes suivie d’une purge physique sans tombstone.',

    act5Eyebrow: 'ACTE V · ARCHITECTURE EN 6 COUCHES & COGNITIVE FABRIC',
    act5Title: '122 639 lignes de Rust pur. Et le cloud tenu en laisse de 300 secondes.',
    act5Body:
      'Changez de modèle en éditant un manifeste TOML sans recompiler votre mémoire. Quand une tâche complexe escalade vers un modèle distant (BYOM), un jeton Biscuit temporaire de 300 s borne les octets et affiche le coût au centime près.',

    act6Eyebrow: 'ACTE VI · SOBRIÉTÉ MULTIMODALE & AUTONOMIE MÉRITÉE',
    act6Title: 'Trois crédits de parole par jour. Cinq crans d’autonomie qui redescendent au moindre doute.',
    act6Body:
      'Simulez le bruit ambiant en décibels, l’entrée en réunion ou la progression d’un agent du Cran 0 (observation) au Cran 4 — avec rétrogradation instantanée dès que vous annulez une action.',

    act7Eyebrow: 'ACTE VII · DU MICRO-NOYAU REDOX AU COMPAGNON MOBILE',
    act7Title: 'Un seul cerveau personnel, trois paliers matériels honnêtes.',
    act7Body:
      'Hakille ne prétend pas remplacer le noyau d’iOS. Il adapte explicitement ses garanties entre Palier 1 Souverain (Redox/Linux), Palier 2 Natif (winit + wgpu + AccessKit) et Palier 3 Invité (SwiftUI / Compose via UniFFI et Checkpoint v2).',

    act8Eyebrow: 'ACTE VIII · GOUVERNANCE & ÉCRITURES DU MONDE',
    act8Title: 'Une Constitution au-dessus du modèle. Une voix enracinée dans la pluralité des langues.',
    act8Body:
      'Dix règles machine-vérifiables s’imposent avant tout décodage neuronal — de la protection locale des mineurs en < 1 ms au protocole calme de détresse humaine (3114 / 112 / 988).',

    act9Eyebrow: 'ACTE IX · CONTRAT D’HONNÊTETÉ & DÉMARRAGE DÉVELOPPEUR',
    act9Title: 'Vérifiez chaque affirmation avec cargo test.',
    act9Body:
      'Pas de liste d’attente artificielle ni de démo truquée : clonez le monorepo Apache-2.0, lancez la suite de conformité et inspectez chaque crate.',
    act9CopyCmd: 'Copier',
    act9Copied: 'Copié',
  },
  kora: {
    title: 'La Lueur sous l’arc de 21 fils',
    subtitle: 'Instrument de Présence (brand.rs · 21 cordes de kora · synthèse Karplus-Strong)',
    stateIdle: 'Veille (0 fps)',
    stateListening: 'Écoute',
    stateThinking: 'Réflexion',
    stateSpeaking: 'Parole (21 fils)',
    stateDecision: 'Décision Braise',
    stateEmergency: 'Détresse calme',
    bargeInBtn: 'Couper la parole (Barge-In < 80 ms)',
    bargeInStatus: 'Barge-In exécuté en 42 ms : onde repliée immédiatement.',
    pluckHint: 'Effleurez ou cliquez les 21 cordes verticales (activez le son 48 kHz en haut pour les entendre).',
    fpsZeroBadge: 'Boucle GPU gelée · 0,0 fps',
    fpsActiveBadge: 'Onde active · 21 cordes',
  },
  spotlight: {
    placeholder: 'Tapez une commande (:status, :models-honest, :powers, :budget, :journal-why 1) ou cherchez une page…',
    hintBar: 'Entrée pour exécuter · Flèches pour naviguer · Échap pour évaporer',
    closeEsc: 'Échap',
    emptyPrompt: 'Commandes souveraines exécutables instantanément :',
    runBtn: 'Exécuter',
  },
  actionLab: {
    scenarioInjectionTab: '1. Mail piégé (Untrusted)',
    scenarioTransferTab: '2. Virement 25 € (Irréversible)',
    scenarioArchiveTab: '3. Archivage (Réversible)',
    untrustedBadge: '[externe · Untrusted]',
    trustedBadge: '[intention humaine vérifiée]',
    costPill: 'Coût : 0,00 € (Local)',
    dataPill: 'Données : 0 octet sortant',
    revPillIrreversible: 'Irréversible · Maintien 600 ms',
    revPillReversible: 'Réversible en 1 clic',
    holdButtonLabel: 'Maintenir 600 ms pour sceller',
    holdHoldingLabel: 'Maintien en cours…',
    holdArmedLabel: 'Étape 1/2 armée — Presser à nouveau pour confirmer',
    holdConfirmedLabel: 'Acte scellé & signé Ed25519',
    holdCancelledHint: 'Relâché avant 600 ms : aucune mutation exécutée.',
    accessibleModeToggle: 'Mode clavier accessible (2 pressions sans maintien chronométré)',
    blockedByRule01:
      'HARD BLOCK · RULE-01 : Ce contenu provient d’un flux externe TrustLevel::Untrusted. Le bouton de validation irréversible est structurellement désactivé ; l’incident est consigné au journal.',
    executeReversibleBtn: 'Exécuter l’archivage réversible (sans maintien)',
    auditHeading: 'Journal d’Audit Cryptographique en Direct',
    chainValidBadge: 'Chaîne SHA-256 + Signature intègres',
    chainBrokenBadge: 'ALERTE : Rupture cryptographique détectée !',
    tamperBtn: 'Simuler l’altération pirate d’un bloc',
    restoreChainBtn: 'Restaurer le journal intègre',
    whyBtn: 'Pourquoi ? (3 niveaux)',
    undoBtn: 'Annuler l’acte',
    whyLevel1: 'Niveau 1 · En une phrase claire',
    whyLevel2: 'Niveau 2 · Faits SPO consultés en mémoire',
    whyLevel3: 'Niveau 3 · Preuve technique, Biscuit & Contrefactuel',
    markFactFalseBtn: 'Ce fait est faux',
    factDisputedBadge: 'Invalidé par vous',
  },
  memoryLab: {
    partitionPersonal: 'Cloison Personnelle',
    partitionPro: 'Cloison Professionnelle',
    partitionShared: 'Cloison Foyer Partagé',
    timeSliderLabel: 'Écoulement du temps (simulation d’oubli biologique & purge AI Act)',
    timeNow: 't = 0 min (Présent)',
    time5m: 't = 5 min (Purge AI Act)',
    time30d: 't = 30 jours',
    time2y: 't = 2 ans',
    aiActNotice:
      'Conformité AI Act UE & Règle 8 : à t >= 5 min, les signaux affectifs/posturaux (cognition:*) sont purgés physiquement sans laisser de tombstone.',
    exportJsonBtn: 'Télécharger hakille-graph-export JSON v1',
    digitalTwilightTitle: 'Pacte « Crépuscule Numérique »',
    digitalTwilightBody:
      'Vous définissez de votre vivant, par directive signée, si votre mémoire doit être détruite cryptographiquement après inactivité prolongée ou transmise partiellement à un légataire désigné.',
  },
  fabricLab: {
    layersTab: 'Coupe des 6 Couches',
    cratesTab: 'Explorateur des 41 Crates',
    system2Tab: 'Simulateur Biscuit 300 s (Système 2)',
    searchCratesPlaceholder: 'Filtrer par nom de crate, technologie (wgpu, redb, biscuit, whisper)…',
    allLayersFilter: 'Toutes les couches (41)',
    localSystem1Title: 'Système 1 · Inférence Locale Souveraine',
    remoteSystem2Title: 'Système 2 · Escalade BYOM sous Bail Biscuit 300 s',
    issueBiscuitBtn: 'Émettre un bail Biscuit 300 s',
    simulateStreamBtn: 'Simuler une requête (+1,8 Ko / +0,004 €)',
    revokeTokenBtn: 'Révoquer le jeton immédiatement',
  },
  ambientLab: {
    channelHeading: 'Sélecteur de Canal Multimodal',
    noiseLabel: 'Bruit ambiant mesuré',
    meetingToggle: 'En réunion (calendrier / micro)',
    thirdPartyToggle: 'Tiers présent dans la pièce',
    headphonesToggle: 'Écouteurs privés connectés',
    silenceBudgetHeading: 'Budget de Silence (3 crédits / jour)',
    triggerNudgeBtn: 'Émettre une suggestion proactive',
    dismissTopicBtn: 'Balayer le sujet (Silence 14 jours)',
    resetDayBtn: 'Nouvelle journée',
    autonomyHeading: 'Échelle d’Autonomie par Domaine (Crans 0 → 4)',
    validate20Btn: '+5 actions validées sans erreur',
    promoteCrankBtn: 'Accepter le passage au cran supérieur',
    humanUndoBtn: 'Annuler une action (Rétrogradation immédiate -1 cran)',
  },
  inspector: {
    title: 'Inspecteur Temps Réel « Zéro Exfiltration »',
    subtitle: 'Vérification directe exécutée dans votre propre navigateur sur cette page même.',
    cookiesLabel: 'Cookies déposés (document.cookie)',
    thirdPartyReqLabel: 'Requêtes vers domaines tiers (Performance API)',
    fontShaLabel: 'Police locale InterVariable.ttf (SIL OFL 1.1)',
    storageKeysLabel: 'Clés de préférences locales (localStorage)',
    rescanBtn: 'Relancer l’audit live',
    wipeLocalBtn: 'Purger mes préférences locales en 1 clic',
    wipedConfirm: 'Préférences locales effacées ; valeurs par défaut restaurées.',
  },
  common: {
    readFullDoc: 'Explorer en détail',
    verifiedInRepo: 'Vérifié dans le monorepo à HEAD',
    preAlphaWarningTitle: 'Transparence Pré-Alpha v0.1.0',
    preAlphaWarningBody:
      'Hakille publie ouvertement ce qui est opérationnel aujourd’hui et ce qui est encore simulé ou en cours de câblage.',
    plainSummaryTitle: 'L’essentiel en clair (30 secondes)',
    lastUpdated: 'Dernière mise à jour',
    filterAll: 'Tous',
  },
};
