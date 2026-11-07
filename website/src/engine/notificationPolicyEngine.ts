/**
 * Notification Security Policy Engine
 *
 * Faithful deterministic TypeScript port of:
 * - `crates/hakille-auth/src/minor.rs` (Constitutional RULE-07 NoWallet)
 * - `crates/hakille-auth/src/guest.rs` (Guest session isolation, <= 24h lifespan)
 * - `crates/hakille-auth/src/multimodal.rs` (Continuous biometric confidence tau, step-up)
 * - `docs/SECURITY_AND_GOVERNANCE.md` (Constitutional Rules 1, 4, 7, 8)
 *
 * Evaluates emission requests across subject roles, age restrictions,
 * prompt injection tags, and continuous multimodal confidence levels.
 */

import { CapabilityAction, CapabilityScope } from './biscuitEngine';

export interface PolicySubject {
  role: 'Owner' | 'Minor' | 'Guest' | 'UntrustedAgent';
  age?: number;
  biometricConfidence: number; // tau in [0.0, 1.0]
  originTrust: 'TrustedHuman' | 'LocalDaemon' | 'UntrustedExternal';
}

export interface PolicyVerdict {
  outcome: 'ALLOW' | 'HARD_BLOCK' | 'STEP_UP_REQUIRED';
  ruleViolated: string | null;
  rationale: string;
  applicableLawOrRule: string;
}

/**
 * Biometric confidence tau thresholds per scope matching
 * `crates/hakille-auth/src/multimodal.rs:287-350`.
 */
export const BIOMETRIC_STEP_UP_THRESHOLDS: Record<CapabilityScope, number> = {
  Wallet: 0.9,
  FullSystem: 0.9,
  Biometrics: 0.7,
  ModelInference: 0.7,
  CognitiveInference: 0.7,
  Filesystem: 0.5,
  Notification: 0.5,
  Camera: 0.4,
  Audio: 0.4,
  Network: 0.4,
  Spatial: 0.4,
  GuiInput: 0.4,
  Sensor: 0.3,
};

/**
 * Evaluates the full Hakille constitutional policy matrix.
 */
export function evaluateSecurityPolicy(
  subject: PolicySubject,
  targetScope: CapabilityScope,
  action: CapabilityAction,
  isDestructive: boolean
): PolicyVerdict {
  // 1. Constitutional RULE-07: Minor Protection Invariant (`minor.rs:1-160`)
  const isMinor =
    subject.role === 'Minor' ||
    (typeof subject.age === 'number' && subject.age < 18);

  if (isMinor) {
    if (targetScope === 'Wallet') {
      return {
        outcome: 'HARD_BLOCK',
        ruleViolated: 'RULE-07: Invariant NoWallet pour mineurs',
        applicableLawOrRule: 'Constitutional Rule 7 & EU AI Act Art. 9',
        rationale:
          'Blocage constitutionnel inconditionnel : les utilisateurs mineurs (< 18 ans) ne peuvent jamais accéder au Wallet ni valider des flux financiers.',
      };
    }

    if (targetScope === 'FullSystem' || action === 'Admin') {
      return {
        outcome: 'HARD_BLOCK',
        ruleViolated: 'RULE-07: Invariant FullSystem / Admin pour mineurs',
        applicableLawOrRule: 'Constitutional Rule 7 & Protection des mineurs',
        rationale:
          'Les comptes mineurs ont interdiction absolue d’exécuter des mutations d’administration globale du système.',
      };
    }
  }

  // 2. Constitutional RULE-01: Inbound Prompt Injection Isolation (`crates/hakille-auth`)
  // Payloads originating from UntrustedExternal (emails, web scrapers) cannot trigger destructive actions or write capabilities.
  if (subject.originTrust === 'UntrustedExternal') {
    if (
      isDestructive ||
      action === 'Write' ||
      action === 'Delete' ||
      action === 'Admin' ||
      targetScope === 'Wallet' ||
      targetScope === 'Filesystem'
    ) {
      return {
        outcome: 'HARD_BLOCK',
        ruleViolated: 'RULE-01: Blocage dur d’injection de prompt',
        applicableLawOrRule: 'Constitutional Rule 1 (Prompt Injection Guard) & OWASP LLM-01',
        rationale:
          'Entrée externe non fiable (courriel, document, scraper) marquée TrustLevel::Untrusted. Interdiction stricte de muter l’état ou de modifier des fichiers.',
      };
    }
  }

  // 3. Guest Session Isolation (`guest.rs:1-230`, `ADR-013`)
  // Guest sessions are bounded (< 24h) and strictly read-only.
  if (subject.role === 'Guest') {
    if (
      isDestructive ||
      action === 'Write' ||
      action === 'Delete' ||
      action === 'Admin' ||
      targetScope === 'Wallet' ||
      targetScope === 'FullSystem'
    ) {
      return {
        outcome: 'HARD_BLOCK',
        ruleViolated: 'RULE-13: Isolation stricte des sessions invités',
        applicableLawOrRule: 'ADR-013 (Multi-Tenancy 5 Niveaux) & guest.rs',
        rationale:
          'Session invité bornée (<= 24 h) : restreinte aux scopes en lecture seule. Aucune persistance disque ni opération financière autorisée sans le titulaire racine.',
      };
    }
  }

  // 4. Untrusted Agent Plugin Sandbox (`ADR-002`, Wasmtime 48 Zero-Import)
  if (subject.role === 'UntrustedAgent') {
    if (
      targetScope === 'FullSystem' ||
      targetScope === 'Wallet' ||
      targetScope === 'Biometrics' ||
      action === 'Admin'
    ) {
      return {
        outcome: 'HARD_BLOCK',
        ruleViolated: 'RULE-06: Violation du bac à sable Wasmtime',
        applicableLawOrRule: 'ADR-002 & Wasmtime Zero-Import Specification',
        rationale:
          'Agent tiers non audité : confiné au bac à sable Wasmtime zéro-import. Accès refusé aux scopes de biométrie, portefeuille et système racine.',
      };
    }
  }

  // 5. Continuous Biometric Confidence tau Step-Up Gate (`multimodal.rs:287-350`)
  let baseThreshold = BIOMETRIC_STEP_UP_THRESHOLDS[targetScope] ?? 0.5;
  if (isDestructive && baseThreshold < 0.85) {
    baseThreshold = 0.85;
  }

  const normalizedConfidence = Math.max(0, Math.min(1, subject.biometricConfidence));
  if (normalizedConfidence < baseThreshold) {
    return {
      outcome: 'STEP_UP_REQUIRED',
      ruleViolated: 'AUTH-STEPUP: Confiance biométrique continue insuffisante',
      applicableLawOrRule: 'crates/hakille-auth/src/multimodal.rs (Paliers d’escalade continue)',
      rationale: `Indice de présence continue tau (${normalizedConfidence.toFixed(2)}) inférieur au seuil requis (${baseThreshold.toFixed(2)}) pour le scope "${targetScope}". Défi biométrique actif (face liveness ISO/IEC 30107-3 ou code PIN) obligatoire.`,
    };
  }

  // 6. All invariants and gates pass
  return {
    outcome: 'ALLOW',
    ruleViolated: null,
    applicableLawOrRule: 'Conforme à la Constitution Hakille & ISO/IEC 30107-3',
    rationale:
      'Toutes les règles constitutionnelles, l’isolation du rôle et les seuils de confiance biométrique continue sont respectés.',
  };
}
