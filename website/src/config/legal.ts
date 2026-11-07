/**
 * Hakille — Centralized Legal, Brand & Governance Configuration
 *
 * All administrative, contact, domain, license, and regulatory constants referenced
 * across `/mentions-legales`, `/confidentialite`, `/conditions`, `/securite`,
 * `/licences`, and `/.well-known/security.txt` live in this single typed module.
 *
 * Before commercial incorporation or production hosting migration, update the
 * `REGISTRATION_STATUS` fields below — every legal page updates automatically.
 */

export const LEGAL_CONFIG = {
  brand: {
    wordmark: 'hakille',
    formalName: 'Hakille',
    signatureFr: 'Hakille — intelligence souveraine',
    signatureEn: 'Hakille — sovereign intelligence',
    etymologyLanguage: 'Pulaar / Fulfulde (Peul)',
    etymologyMeaningFr: 'intelligence, discernement, sagesse vive',
    etymologyMeaningEn: 'intelligence, discernment, living wisdom',
    historicalTechnicalCodename: 'Annora',
  },
  domains: {
    marketing: 'https://hakille.ai',
    docs: 'https://docs.hakille.ai',
    webCompanion: 'https://app.hakille.ai',
    api: 'https://api.hakille.ai',
    repository: 'https://github.com/hakille/hakille',
    securityAdvisories: 'https://github.com/hakille/hakille/security/advisories/new',
  },
  contacts: {
    generalEmail: 'hello@hakille.ai',
    securityEmail: 'security@hakille.ai',
    privacyEmail: 'hello@hakille.ai',
    accessibilityEmail: 'hello@hakille.ai',
    disclosureAckHours: 72,
  },
  publisher: {
    entityLabelFr: 'Collectif & Projet Open-Source Hakille (structure de gouvernance indépendante en cours de constitution)',
    entityLabelEn: 'Hakille Open-Source Project & Collective (independent governance entity in formation)',
    registrationStatusCode: 'PRE_INCORPORATION_OPEN_SOURCE',
    registrationNumberPlaceholder: 'En cours d’immatriculation (RCS / Fondation indépendante prévue par l’Article VII de la Constitution v1.0.0)',
    registeredSeatPlaceholder: 'Union Européenne — Domiciliation administrative publiée dès immatriculation formelle (contact direct : hello@hakille.ai)',
    publicationGovernance: 'Mainteneurs du dépôt officiel github.com/hakille/hakille',
  },
  hosting: {
    architectureFr: 'Distribution statique immuable auto-contenue (HTML/CSS/JS pré-rendus, zéro base de données utilisateur, zéro cookie tiers)',
    architectureEn: 'Self-contained immutable static distribution (pre-rendered HTML/CSS/JS, zero user database, zero third-party cookies)',
    codeHostName: 'GitHub, Inc.',
    codeHostAddress: '88 Colin P Kelly Jr St, San Francisco, CA 94107, USA',
    edgeNoteFr: 'Déployable à l’identique sur tout serveur HTTP statique souverain européen sans dépendance cloud propriétaire.',
    edgeNoteEn: 'Identically deployable on any European sovereign static HTTP server with zero proprietary cloud lock-in.',
  },
  software: {
    version: '0.1.0-pre-alpha',
    rustEdition: '2024',
    msrv: '1.95',
    licenseSpdx: 'Apache-2.0',
    constitutionVersion: '1.0.0',
    constitutionDate: '2026-04-12',
    lastLegalUpdate: '2026-11-06',
    cargoPackagesCount: 42,
    rustLinesOfCode: 146_229,
    automatedTestsCount: 1_948,
    pythonPercentage: 0,
  },
  crisisHotlines: [
    { region: 'France', number: '3114', label: 'Numéro national de prévention du suicide (24h/24, gratuit)' },
    { region: 'Union Européenne', number: '112', label: 'Numéro d’urgence européen unifié' },
    { region: 'United States & Canada', number: '988', label: 'Suicide & Crisis Lifeline (24/7)' },
    { region: 'International', number: 'findahelpline.com', label: 'Annuaire mondial vérifié de lignes d’écoute' },
  ],
} as const;
