//! Attestations de Conformité à Engagement Masqué (§35) :
//!
//! Contrat d'honnêteté cryptographique :
//! - Ce module produit des **attestations signées à engagement masqué**
//!   (`Commit(action_id, payload, nonce) = SHA256(domaine || len(id) || id || len(payload) || payload || nonce)`
//!   + signature Ed25519 de l'autorité d'audit).
//! - Il ne s'agit PAS d'une preuve SNARK/STARK non interactive prouvant sans
//!   confiance l'exécution d'un prédicat arbitraire : le vérificateur DOIT
//!   épingler la clé publique de l'autorité (`verify_proof_signature_pinned`).
//! - Les paramètres d'action restent masqués jusqu'à une ouverture explicite en
//!   enclave privée (`verify_commitment_opening`).

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;

const COMMITMENT_DOMAIN: &[u8] = b"hakille:compliance:commit:v2:";
const PROOF_SIGN_DOMAIN: &[u8] = b"hakille:compliance:sig:v2:";

/// Fenêtre maximale par défaut d'admissibilité temporelle (24 h + 60 s de dérive horloge).
pub const DEFAULT_MAX_PROOF_AGE_MS: u64 = 24 * 60 * 60 * 1000;
pub const MAX_FUTURE_SKEW_MS: u64 = 60 * 1000;

/// Les 10 Règles d'Or de la Constitution de Souveraineté Hakille.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConstitutionalRule {
    /// Règle 1 : Zéro fuite cloud, coupure réseau WAN fail-closed.
    ZeroCloudFailClosed = 1,
    /// Règle 2 : Souveraineté des clés privées, enclave locale étanche.
    LocalKeySovereignty = 2,
    /// Règle 3 : Priorité absolue aux révocations (Revocation-Wins).
    RevocationWins = 3,
    /// Règle 4 : Atténuation stricte et décroissante des capabilities Biscuit.
    MinimalCapabilityAttenuation = 4,
    /// Règle 5 : Anti-rejeu strict avec nonces uniques et horodatage monotone.
    AntiReplayMonotonicity = 5,
    /// Règle 6 : Droit à l'oubli cryptographique certifié avec zéro rémanence.
    RightToCryptoErasure = 6,
    /// Règle 7 : Suprématie du consentement utilisateur (maintien 600ms).
    UserConsentSupremacy = 7,
    /// Règle 8 : Journal d'audit append-only chaîné par empreintes cryptographiques.
    DeterministicAuditTrail = 8,
    /// Règle 9 : Fonctionnement local autonome en réseau maillé hors-ligne.
    OfflineMeshTolerance = 9,
    /// Règle 10 : Traitement vocal et perceptuel 100% sur puce locale.
    LivingVoiceAirGap = 10,
}

impl ConstitutionalRule {
    pub const fn rule_id(&self) -> u8 {
        *self as u8
    }

    pub const fn rule_name(&self) -> &'static str {
        match self {
            Self::ZeroCloudFailClosed => "Règle 1 : Zéro Fuite Cloud Fail-Closed",
            Self::LocalKeySovereignty => "Règle 2 : Souveraineté des Clés Privées",
            Self::RevocationWins => "Règle 3 : Suprématie de Révocation (Revocation-Wins)",
            Self::MinimalCapabilityAttenuation => "Règle 4 : Atténuation Stricte des Capabilities",
            Self::AntiReplayMonotonicity => "Règle 5 : Anti-Rejeu et Monotonie",
            Self::RightToCryptoErasure => "Règle 6 : Oubli Cryptographique Certifié",
            Self::UserConsentSupremacy => "Règle 7 : Suprématie du Consentement",
            Self::DeterministicAuditTrail => "Règle 8 : Journal d'Audit Déterministe Scellé",
            Self::OfflineMeshTolerance => "Règle 9 : Tolérance au Réseau Maillé Hors-Ligne",
            Self::LivingVoiceAirGap => "Règle 10 : Voix Vivante Air-Gap 100% Locale",
        }
    }
}

/// Erreurs de vérification d'attestation masquée.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ZkVerifyError {
    #[error("Empreinte d'engagement invalide ou corrompue")]
    InvalidCommitment,
    #[error("Signature cryptographique invalide")]
    InvalidSignature,
    #[error("Clé de vérification Ed25519 mal formée")]
    InvalidVerifyingKey,
    #[error("Clé signataire non épinglée / autorité non reconnue")]
    UntrustedAuthority,
    #[error("Règle constitutionnelle hors bornes : {0}")]
    UnknownRule(u8),
    #[error("Horodatage hors fenêtre d'admissibilité")]
    ExpiredOrFutureTimestamp,
}

/// Attestation de conformité à engagement masqué scellée par une autorité Ed25519.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZkComplianceProof {
    pub proof_id: String,
    pub rule: ConstitutionalRule,
    /// Engagement masqué `H(domaine || len(id) || id || len(payload) || payload || nonce)`
    pub blinded_commitment: [u8; 32],
    pub blinding_nonce_hash: [u8; 32],
    pub timestamp_ms: u64,
    pub verifying_key_bytes: [u8; 32],
    pub signature_bytes: Vec<u8>,
}

fn compute_commitment(
    action_id: &str,
    action_payload: &[u8],
    blinding_nonce: &[u8; 32],
) -> [u8; 32] {
    let mut commit_hasher = Sha256::new();
    commit_hasher.update(COMMITMENT_DOMAIN);
    commit_hasher.update((action_id.len() as u64).to_le_bytes());
    commit_hasher.update(action_id.as_bytes());
    commit_hasher.update((action_payload.len() as u64).to_le_bytes());
    commit_hasher.update(action_payload);
    commit_hasher.update(blinding_nonce);
    commit_hasher.finalize().into()
}

fn compute_signing_payload(
    proof_id: &str,
    rule: ConstitutionalRule,
    blinded_commitment: &[u8; 32],
    blinding_nonce_hash: &[u8; 32],
    timestamp_ms: u64,
) -> Vec<u8> {
    let mut msg =
        Vec::with_capacity(PROOF_SIGN_DOMAIN.len() + 8 + proof_id.len() + 1 + 32 + 32 + 8);
    msg.extend_from_slice(PROOF_SIGN_DOMAIN);
    msg.extend_from_slice(&(proof_id.len() as u64).to_le_bytes());
    msg.extend_from_slice(proof_id.as_bytes());
    msg.push(rule.rule_id());
    msg.extend_from_slice(blinded_commitment);
    msg.extend_from_slice(blinding_nonce_hash);
    msg.extend_from_slice(&timestamp_ms.to_le_bytes());
    msg
}

/// Générateur d'attestations de conformité à engagement masqué côté autorité/noyau.
pub struct ZkComplianceProver;

impl ZkComplianceProver {
    /// Produit une attestation signée à engagement masqué pour une action validée
    /// par l'autorité sous la règle constitutionnelle donnée.
    pub fn prove_compliance(
        action_id: &str,
        action_payload: &[u8],
        blinding_nonce: &[u8; 32],
        rule: ConstitutionalRule,
        signing_key: &SigningKey,
    ) -> ZkComplianceProof {
        let now_ms = hakille_core::Timestamp::now().0.timestamp_millis().max(0) as u64;
        let proof_id = format!("zkproof-{}", uuid::Uuid::new_v4());

        let blinded_commitment = compute_commitment(action_id, action_payload, blinding_nonce);
        let blinding_nonce_hash: [u8; 32] = Sha256::digest(blinding_nonce).into();

        let msg = compute_signing_payload(
            &proof_id,
            rule,
            &blinded_commitment,
            &blinding_nonce_hash,
            now_ms,
        );

        let signature = signing_key.sign(&msg);
        let verifying_key = signing_key.verifying_key();

        ZkComplianceProof {
            proof_id,
            rule,
            blinded_commitment,
            blinding_nonce_hash,
            timestamp_ms: now_ms,
            verifying_key_bytes: verifying_key.to_bytes(),
            signature_bytes: signature.to_bytes().to_vec(),
        }
    }
}

/// Vérificateur autonome d'attestations de conformité (exige une clé publique épinglée en prod).
pub struct ZkComplianceVerifier;

impl ZkComplianceVerifier {
    /// Vérifie strictement l'authenticité contre une clé publique d'autorité ÉPINGLÉE
    /// et valide la fraîcheur de l'horodatage (fail-closed).
    pub fn verify_proof_signature_pinned(
        proof: &ZkComplianceProof,
        expected_authority: &VerifyingKey,
        max_age_ms: u64,
    ) -> Result<bool, ZkVerifyError> {
        if !bool::from(
            proof
                .verifying_key_bytes
                .ct_eq(expected_authority.as_bytes()),
        ) {
            return Err(ZkVerifyError::UntrustedAuthority);
        }

        let now_ms = hakille_core::Timestamp::now().0.timestamp_millis().max(0) as u64;
        if proof.timestamp_ms > now_ms.saturating_add(MAX_FUTURE_SKEW_MS)
            || now_ms.saturating_sub(proof.timestamp_ms) > max_age_ms
        {
            return Err(ZkVerifyError::ExpiredOrFutureTimestamp);
        }

        let sig_array: [u8; 64] = proof
            .signature_bytes
            .as_slice()
            .try_into()
            .map_err(|_| ZkVerifyError::InvalidSignature)?;
        let signature = ed25519_dalek::Signature::from_bytes(&sig_array);

        let msg = compute_signing_payload(
            &proof.proof_id,
            proof.rule,
            &proof.blinded_commitment,
            &proof.blinding_nonce_hash,
            proof.timestamp_ms,
        );

        expected_authority
            .verify_strict(&msg, &signature)
            .map(|_| true)
            .map_err(|_| ZkVerifyError::InvalidSignature)
    }

    /// Vérifie la signature auto-déclarée et la fraîcheur (tests/compatibilité uniquement ;
    /// utiliser [`Self::verify_proof_signature_pinned`] en production).
    pub fn verify_proof_signature(proof: &ZkComplianceProof) -> Result<bool, ZkVerifyError> {
        let verifying_key = VerifyingKey::from_bytes(&proof.verifying_key_bytes)
            .map_err(|_| ZkVerifyError::InvalidVerifyingKey)?;
        Self::verify_proof_signature_pinned(proof, &verifying_key, DEFAULT_MAX_PROOF_AGE_MS)
    }

    /// Ouvre l'engagement masqué en présence du nonce et du payload révélés en enclave privée
    /// (vérifie à la fois `blinding_nonce_hash` et `blinded_commitment` en temps constant).
    pub fn verify_commitment_opening(
        proof: &ZkComplianceProof,
        action_id: &str,
        action_payload: &[u8],
        blinding_nonce: &[u8; 32],
    ) -> bool {
        let nonce_hash: [u8; 32] = Sha256::digest(blinding_nonce).into();
        let commitment = compute_commitment(action_id, action_payload, blinding_nonce);
        bool::from(
            nonce_hash.ct_eq(&proof.blinding_nonce_hash)
                & commitment.ct_eq(&proof.blinded_commitment),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zk_proof_generation_verification_and_tamper_detection() {
        let secret_bytes = [7u8; 32];
        let signing_key = SigningKey::from_bytes(&secret_bytes);

        let action_id = "purge-temp-data-42";
        let payload = b"{\"files_deleted\": 12, \"target\": \"/tmp/cache\"}";
        let nonce = [42u8; 32];

        // Règle 6 : Droit à l'oubli cryptographique
        let proof = ZkComplianceProver::prove_compliance(
            action_id,
            payload,
            &nonce,
            ConstitutionalRule::RightToCryptoErasure,
            &signing_key,
        );

        assert_eq!(proof.rule, ConstitutionalRule::RightToCryptoErasure);
        assert!(ZkComplianceVerifier::verify_proof_signature(&proof).unwrap());
        assert!(ZkComplianceVerifier::verify_commitment_opening(
            &proof, action_id, payload, &nonce,
        ));

        // Opening with false payload must fail
        assert!(!ZkComplianceVerifier::verify_commitment_opening(
            &proof,
            action_id,
            b"{\"files_deleted\": 999}",
            &nonce,
        ));

        // Tampered signature must fail
        let mut tampered = proof.clone();
        tampered.signature_bytes[0] ^= 0xFF;
        assert!(ZkComplianceVerifier::verify_proof_signature(&tampered).is_err());
    }

    #[test]
    fn test_all_10_constitutional_rules_covered() {
        let rules = [
            ConstitutionalRule::ZeroCloudFailClosed,
            ConstitutionalRule::LocalKeySovereignty,
            ConstitutionalRule::RevocationWins,
            ConstitutionalRule::MinimalCapabilityAttenuation,
            ConstitutionalRule::AntiReplayMonotonicity,
            ConstitutionalRule::RightToCryptoErasure,
            ConstitutionalRule::UserConsentSupremacy,
            ConstitutionalRule::DeterministicAuditTrail,
            ConstitutionalRule::OfflineMeshTolerance,
            ConstitutionalRule::LivingVoiceAirGap,
        ];
        assert_eq!(rules.len(), 10);
        for rule in &rules {
            assert!(rule.rule_name().contains("Règle"));
        }
    }
}
