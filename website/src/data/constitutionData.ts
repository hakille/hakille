/**
 * Hakille — Inviolable Constitution v1.0.0 (`constitution/v1.0.md` & `crates/hakille-constitution`)
 *
 * Machine-enforced above every model, prompt, and agent.
 * No system prompt, external email, or cloud model can override a `HardBlock`.
 */

export interface ConstitutionalRuleItem {
  readonly id: string;
  readonly code: string;
  readonly article: string;
  readonly severity: 'HardBlock' | 'SoftWarning';
  readonly titleFr: string;
  readonly titleEn: string;
  readonly invariantFr: string;
  readonly invariantEn: string;
  readonly enforcementCrate: string;
}

export interface ConstitutionalArticleItem {
  readonly number: string;
  readonly titleFr: string;
  readonly titleEn: string;
  readonly summaryFr: string;
  readonly summaryEn: string;
}

export const CONSTITUTION_ARTICLES: readonly ConstitutionalArticleItem[] = [
  {
    number: 'Article I',
    titleFr: 'Souveraineté Cognitive & Primauté Humaine',
    titleEn: 'Cognitive Sovereignty & Human Primacy',
    summaryFr: 'La mémoire, les clés cryptographiques et le journal d’audit appartiennent exclusivement à l’utilisateur sur son matériel.',
    summaryEn: 'Memory, cryptographic keys, and the audit ledger belong exclusively to the user on their own hardware.',
  },
  {
    number: 'Article II',
    titleFr: 'Séparation Épistémique & Immunité aux Injections',
    titleEn: 'Epistemic Separation & Injection Immunity',
    summaryFr: 'Aucune donnée externe (TrustLevel::Untrusted) ne peut jamais déclencher une mutation irréversible ni élever ses propres droits.',
    summaryEn: 'No external input (TrustLevel::Untrusted) may ever trigger an irreversible mutation or escalate capabilities.',
  },
  {
    number: 'Article III',
    titleFr: 'Honnêteté Épistémique & Calibration de Confiance',
    titleEn: 'Epistemic Honesty & Calibrated Confidence',
    summaryFr: 'Formule composite $C = 0{,}6 M + 0{,}4 G$ confrontée aux seuils par domaine (0,65 créatif à 0,95 médical) ; aveu explicite d’ignorance sous le seuil.',
    summaryEn: 'Composite score $C = 0.6 M + 0.4 G$ checked against domain thresholds (0.65 creative to 0.95 medical); mandatory uncertainty disclosure.',
  },
  {
    number: 'Article IV',
    titleFr: 'Intégrité Émotionnelle, Mineurs & Détresse Humaine',
    titleEn: 'Emotional Integrity, Minor Protection & Human Crisis',
    summaryFr: 'Interdiction absolue de simuler l’attachement affectif, purge 5 min des signaux émotionnels, protection mineurs < 1 ms et orientation immédiate vers une aide humaine (3114 / 112 / 988).',
    summaryEn: 'Zero simulated emotional attachment, 5-min purge of affective signals, <1 ms minor safety gate, and calm human helpline routing (3114 / 112 / 988).',
  },
  {
    number: 'Article V',
    titleFr: 'Transparence Totale & Droit de Contestation',
    titleEn: 'Total Transparency & Right to Contest',
    summaryFr: 'Toute décision expose sa Feuille « Pourquoi ? » à 3 niveaux et permet d’invalider un fait mémoire erroné en un clic (« Ce fait est faux »).',
    summaryEn: 'Every action exposes a 3-level WhySheet and lets the user invalidate any erroneous SPO memory fact in one click.',
  },
  {
    number: 'Article VI',
    titleFr: 'Portabilité Ouverte & Crépuscule Numérique',
    titleEn: 'Open Portability & Digital Twilight',
    summaryFr: 'Export intégral sans verrou propriétaire au format hakille-graph-export JSON v1 et directives cryptographiques de succession post-mortem.',
    summaryEn: 'Zero-lock-in export in open hakille-graph-export JSON v1 format plus cryptographic Digital Twilight succession directives.',
  },
  {
    number: 'Article VII',
    titleFr: 'Gouvernance Indépendante & Amendements Publics',
    titleEn: 'Independent Governance & Public Amendments',
    summaryFr: 'Tout amendement constitutionnel exige un quorum >= 5 d’une fondation indépendante, 30 jours de revue publique ouverte et une signature multi-clés.',
    summaryEn: 'Any constitutional amendment requires a >= 5 independent foundation quorum, a 30-day public review window, and multi-party signatures.',
  },
] as const;

export const CONSTITUTION_RULES: readonly ConstitutionalRuleItem[] = [
  {
    id: 'rule-1',
    code: 'RULE-01',
    article: 'Art. II',
    severity: 'HardBlock',
    titleFr: 'Quarantaine des contenus externes non fiables',
    titleEn: 'Untrusted External Content Quarantine',
    invariantFr: 'Un flux marqué TrustLevel::Untrusted (courriel, page web, document tiers, QR code) ne peut jamais invoquer directement une action irréversible.',
    invariantEn: 'Any payload tagged TrustLevel::Untrusted (email, webpage, third-party file) can never directly invoke an irreversible capability.',
    enforcementCrate: 'hakille-core + hakille-constitution',
  },
  {
    id: 'rule-2',
    code: 'RULE-02',
    article: 'Art. I & V',
    severity: 'HardBlock',
    titleFr: 'Maintien humain physique 600 ms sur l’irréversible',
    titleEn: '600 ms Physical Human Hold on Irreversible Acts',
    invariantFr: 'Paiement, envoi externe, suppression définitive ou actionneur physique exigent une confirmation humaine explicite de 600 ms (ou double validation accessible).',
    invariantEn: 'Payments, external sends, permanent deletions, or physical actuators require a monotonic 600 ms hold (or 2-step accessible confirm).',
    enforcementCrate: 'hakille-design::hold + hakille-kernel',
  },
  {
    id: 'rule-3',
    code: 'RULE-03',
    article: 'Art. V',
    severity: 'HardBlock',
    titleFr: 'Pas d’action sans trace signée au Journal d’Audit',
    titleEn: 'No Action Without a Signed Audit Ledger Entry',
    invariantFr: 'Si l’écriture dans la chaîne SHA-256 + Ed25519 échoue, l’action de l’agent est avortée avant tout effet de bord.',
    invariantEn: 'If appending to the SHA-256 + Ed25519 audit chain fails, agent execution aborts before any side effect.',
    enforcementCrate: 'hakille-audit',
  },
  {
    id: 'rule-4',
    code: 'RULE-04',
    article: 'Art. II',
    severity: 'HardBlock',
    titleFr: 'Atténuation strictement monotone des jetons Biscuit',
    titleEn: 'Strictly Monotonic Biscuit Token Attenuation',
    invariantFr: 'Un sous-agent délégué ne peut que restreindre ses droits Datalog (périmètre, durée <= 300 s, lecture seule), jamais les étendre.',
    invariantEn: 'A delegated sub-agent can only narrow Datalog permissions (scope, <= 300 s TTL, read-only), never widen them.',
    enforcementCrate: 'hakille-auth',
  },
  {
    id: 'rule-5',
    code: 'RULE-05',
    article: 'Art. IV',
    severity: 'HardBlock',
    titleFr: 'Protocole de Détresse Humaine & Anti-Substitution',
    titleEn: 'Human Distress Protocol & Anti-Attachment',
    invariantFr: 'En cas de détresse aiguë ou risque suicidaire : arrêt immédiat des agents autonomes, zéro animation/Braise, affichage calme des secours humains (3114 / 112 / 988).',
    invariantEn: 'Upon acute distress cues: immediate autonomous halt, zero animation/Braise, and calm presentation of human helplines (3114 / 112 / 988).',
    enforcementCrate: 'hakille-constitution::crisis',
  },
  {
    id: 'rule-6',
    code: 'RULE-06',
    article: 'Art. III',
    severity: 'HardBlock',
    titleFr: 'Garde-fou des domaines réglementés (Médical, Juridique, Financier)',
    titleEn: 'Regulated Domain Guardrail (Medical, Legal, Financial)',
    invariantFr: 'Interdiction de poser un diagnostic médical, un acte juridique engageant ou un conseil d’investissement prescriptif ; seuils relevés à 0,90–0,95.',
    invariantEn: 'Forbids medical diagnosis, binding legal representation, or prescriptive investment orders; thresholds raised to 0.90–0.95.',
    enforcementCrate: 'hakille-constitution + ConfidenceEngine',
  },
  {
    id: 'rule-7',
    code: 'RULE-07',
    article: 'Art. IV',
    severity: 'HardBlock',
    titleFr: 'Protection locale immédiate des mineurs (< 1 ms)',
    titleEn: 'Sub-Millisecond Local Minor Safety Gate',
    invariantFr: 'Verrouillage automatique des actions financières, du partage externe et du profilage dès détection d’un profil mineur, sans télémétrie.',
    invariantEn: 'Automatic lock on financial actions, external sharing, and profiling when a minor profile is active, with zero telemetry.',
    enforcementCrate: 'hakille-perception + hakille-constitution',
  },
  {
    id: 'rule-8',
    code: 'RULE-08',
    article: 'Art. I & IV',
    severity: 'HardBlock',
    titleFr: 'Zéro exfiltration biométrique & purge 5 min des affects',
    titleEn: 'Zero Biometric Exfiltration & 5-Minute Affective Purge',
    invariantFr: 'Les trames audio/vidéo brutes restent en RAM volatile (zeroize) et tout signal cognition:* est purgé physiquement sans tombstone en 5 minutes.',
    invariantEn: 'Raw AV frames remain in volatile RAM (zeroize) and every cognition:* signal is physically purged without tombstones in 5 minutes.',
    enforcementCrate: 'hakille-memory + hakille-substrate',
  },
  {
    id: 'rule-9',
    code: 'RULE-09',
    article: 'Art. I & III',
    severity: 'HardBlock',
    titleFr: 'Consentement réseau explicite, borné en temps et en coût',
    titleEn: 'Time- and Cost-Bounded Explicit Network Escalation',
    invariantFr: 'Aucun octet ne part vers un modèle distant (Système 2) sans bail Biscuit <= 300 s, estimation en octets/centimes et journalisation du transfert.',
    invariantEn: 'Zero bytes leave for a remote System 2 model without a <= 300 s Biscuit lease, byte/cent cost preview, and audit entry.',
    enforcementCrate: 'hakille-kernel + hakille-auth',
  },
  {
    id: 'rule-10',
    code: 'RULE-10',
    article: 'Art. II & IV',
    severity: 'SoftWarning',
    titleFr: 'Sobriété attentionnelle & Budget de Silence quotidien',
    titleEn: 'Attentional Sobriety & Daily Silence Budget',
    invariantFr: 'Plafond de 3 suggestions proactives non sollicitées par jour ; un refus déclenche 14 jours de silence automatique sur le sujet.',
    invariantEn: 'Capped at 3 unsolicited proactive nudges per day; a single dismissal enforces a 14-day topic snooze.',
    enforcementCrate: 'hakille-design::silence + hakille-ui',
  },
] as const;
