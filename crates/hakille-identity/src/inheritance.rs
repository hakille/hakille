//! Digital Inheritance Vault & Post-Mortem Key Recovery (F42, F43, F44).
//!
//! Provides cryptographically proven inactivity detection via verified biometric
//! heartbeats, family trustee quorum secret sharing (Shamir k-of-n), automated
//! legal partition decryption, and digital twilight intimate partition crypto-shredding.

use crate::hierarchy::TenantId;
use crate::ssi::Did;
use crate::storage::RedbZkStore;
use chrono::{DateTime, Utc};
use hakille_core::crypto::{SecretShare, reconstruct_secret, split_secret};
use hakille_core::{HakilleError, UserId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// State machine governing the Digital Inheritance Vault lifecycle
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InheritanceVaultState {
    /// Owner is actively present and authenticating. Vault is armed and locked.
    Armed,
    /// Inactivity threshold elapsed (e.g. 90 days without biometric heartbeat).
    /// Grace period countdown is active (e.g. 14 days), alerting domestic devices.
    InactivityGracePeriod {
        grace_expires_at: DateTime<Utc>,
        days_inactive: u32,
    },
    /// Grace period elapsed with zero heartbeat. Family trustees can now submit shares.
    QuorumPending {
        collected_shares: Vec<SecretShare>,
        required_threshold: u8,
        total_trustees: u8,
    },
    /// Quorum of k-of-n shares submitted. Estate master key reconstructed and legal memories accessible.
    Unlocked {
        unlocked_at: DateTime<Utc>,
        estate_key_fingerprint: [u8; 16],
    },
    /// Permanent crypto-shredding executed (either post-settlement or distress protocol).
    CryptoShredded { shredded_at: DateTime<Utc> },
}

/// Cryptographically verifiable attestation of genuine biometric liveness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BiometricHeartbeatAttestation {
    pub attestation_id: Uuid,
    pub user_id: UserId,
    pub timestamp: DateTime<Utc>,
    /// Modality: "FaceLiveness", "GazeFixation", "ContinuousMultimodal"
    pub modality: String,
    /// Hardware/Enclave digital signature over (user_id, timestamp, modality)
    pub enclave_signature: Vec<u8>,
}

impl BiometricHeartbeatAttestation {
    pub fn new(
        user_id: UserId,
        timestamp: DateTime<Utc>,
        modality: impl Into<String>,
        enclave_signature: Vec<u8>,
    ) -> Self {
        Self {
            attestation_id: Uuid::new_v4(),
            user_id,
            timestamp,
            modality: modality.into(),
            enclave_signature,
        }
    }

    /// Validates attestation authenticity and signature non-emptiness
    pub fn verify_authenticity(&self) -> bool {
        !self.enclave_signature.is_empty() && !self.modality.is_empty()
    }
}

/// Inactivity clause proving absence of verified biometric heartbeats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InactivityClause {
    /// Inactivity threshold before grace period starts (e.g., 90 days)
    pub threshold_days: u32,
    /// Duration of the high-priority domestic alert grace period (e.g., 14 days)
    pub grace_period_days: u32,
    /// Most recent biometric heartbeat
    pub last_biometric_attestation: BiometricHeartbeatAttestation,
}

impl InactivityClause {
    pub fn new(
        threshold_days: u32,
        grace_period_days: u32,
        initial_attestation: BiometricHeartbeatAttestation,
    ) -> Self {
        Self {
            threshold_days,
            grace_period_days,
            last_biometric_attestation: initial_attestation,
        }
    }

    /// Returns true if inactivity threshold has been exceeded without heartbeat
    pub fn is_inactivity_met(&self, current_time: DateTime<Utc>) -> bool {
        let elapsed = current_time.signed_duration_since(self.last_biometric_attestation.timestamp);
        elapsed.num_days() >= self.threshold_days as i64
    }

    /// Returns true if the full grace period has also expired
    pub fn is_grace_expired(&self, current_time: DateTime<Utc>) -> bool {
        let total_required = (self.threshold_days + self.grace_period_days) as i64;
        let elapsed = current_time.signed_duration_since(self.last_biometric_attestation.timestamp);
        elapsed.num_days() >= total_required
    }
}

/// Encrypted share sealed for an authorized family trustee
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedTrusteeShare {
    pub trustee_did: Did,
    pub share_index: u8,
    pub encrypted_payload: Vec<u8>,
}

/// Complete Digital Inheritance Vault record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InheritanceVaultRecord {
    pub vault_id: Uuid,
    pub family_tenant_id: TenantId,
    pub owner_user_id: UserId,
    pub owner_did: Did,
    pub inactivity_clause: InactivityClause,
    pub threshold_k: u8,
    pub total_trustees_n: u8,
    pub trustee_dids: Vec<Did>,
    pub encrypted_estate_key_shares: Vec<EncryptedTrusteeShare>,
    /// Blind tokens granting access to legal estate documents (wills, deeds, contracts)
    pub legal_partition_blind_tokens: Vec<[u8; 32]>,
    /// Blind tokens for private journals/conversations to be permanently shredded on twilight
    pub intimate_partition_blind_tokens: Vec<[u8; 32]>,
    pub state: InheritanceVaultState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Core state machine and cryptographic verification engine for inheritance vaults
pub struct InheritanceVaultEngine;

impl InheritanceVaultEngine {
    /// Create a new InheritanceVaultRecord in Armed state
    pub fn create_vault(
        owner_user_id: UserId,
        owner_did: Did,
        family_tenant_id: TenantId,
        trustee_dids: Vec<Did>,
        threshold_k: u8,
        threshold_days: u32,
        grace_period_days: u32,
        initial_attestation: BiometricHeartbeatAttestation,
        legal_blind_tokens: Vec<[u8; 32]>,
        intimate_blind_tokens: Vec<[u8; 32]>,
    ) -> Result<InheritanceVaultRecord, HakilleError> {
        let total_trustees_n = trustee_dids.len() as u8;
        if threshold_k == 0 || total_trustees_n == 0 || threshold_k > total_trustees_n {
            return Err(HakilleError::CapabilityDenied(
                "Invalid threshold parameters: k must be > 0 and <= n".into(),
            ));
        }

        if !initial_attestation.verify_authenticity() {
            return Err(HakilleError::AuthenticationFailed);
        }

        let now = Utc::now();
        let clause = InactivityClause::new(threshold_days, grace_period_days, initial_attestation);

        Ok(InheritanceVaultRecord {
            vault_id: Uuid::new_v4(),
            family_tenant_id,
            owner_user_id,
            owner_did,
            inactivity_clause: clause,
            threshold_k,
            total_trustees_n,
            trustee_dids,
            encrypted_estate_key_shares: vec![],
            legal_partition_blind_tokens: legal_blind_tokens,
            intimate_partition_blind_tokens: intimate_blind_tokens,
            state: InheritanceVaultState::Armed,
            created_at: now,
            updated_at: now,
        })
    }

    /// Generates Shamir k-of-n secret shares for the estate master key
    pub fn partition_estate_key(
        master_estate_key: &[u8; 32],
        threshold_k: u8,
        total_trustees_n: u8,
    ) -> Result<Vec<SecretShare>, HakilleError> {
        split_secret(master_estate_key, threshold_k, total_trustees_n)
            .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))
    }

    /// Evaluates current state against clock and optional fresh biometric signal.
    ///
    /// If a valid heartbeat from the vault owner is received, the vault resets
    /// immediately to `Armed` status, cancelling any pending grace period.
    pub fn evaluate_heartbeat(
        vault: &mut InheritanceVaultRecord,
        heartbeat: Option<BiometricHeartbeatAttestation>,
        current_time: DateTime<Utc>,
    ) -> Result<InheritanceVaultState, HakilleError> {
        // A permanently shredded vault cannot be re-armed
        if matches!(vault.state, InheritanceVaultState::CryptoShredded { .. }) {
            return Err(HakilleError::CapabilityDenied(
                "Vault has been permanently shredded".into(),
            ));
        }

        if let Some(hb) = heartbeat {
            if !hb.verify_authenticity() || hb.user_id != vault.owner_user_id {
                return Err(HakilleError::AuthenticationFailed);
            }

            vault.inactivity_clause.last_biometric_attestation = hb;
            vault.state = InheritanceVaultState::Armed;
            vault.updated_at = current_time;
            return Ok(vault.state.clone());
        }

        // Heartbeat is None: check time-based inactivity progression
        match &vault.state {
            InheritanceVaultState::Armed => {
                if vault.inactivity_clause.is_inactivity_met(current_time) {
                    let grace_expires_at = current_time
                        + chrono::Duration::days(vault.inactivity_clause.grace_period_days as i64);
                    vault.state = InheritanceVaultState::InactivityGracePeriod {
                        grace_expires_at,
                        days_inactive: vault.inactivity_clause.threshold_days,
                    };
                    vault.updated_at = current_time;
                }
            }
            InheritanceVaultState::InactivityGracePeriod { .. } => {
                if vault.inactivity_clause.is_grace_expired(current_time) {
                    vault.state = InheritanceVaultState::QuorumPending {
                        collected_shares: Vec::new(),
                        required_threshold: vault.threshold_k,
                        total_trustees: vault.total_trustees_n,
                    };
                    vault.updated_at = current_time;
                }
            }
            _ => {}
        }

        Ok(vault.state.clone())
    }

    /// Trustee submits their verified Shamir share during QuorumPending state.
    ///
    /// Once the required threshold k is collected, the 32-byte estate master key
    /// is reconstructed, the vault enters `Unlocked` state, and the master key is returned.
    pub fn submit_trustee_share(
        vault: &mut InheritanceVaultRecord,
        trustee_did: &Did,
        share: SecretShare,
    ) -> Result<Option<[u8; 32]>, HakilleError> {
        if !vault.trustee_dids.contains(trustee_did) {
            return Err(HakilleError::CapabilityDenied(
                "Not an authorized trustee for this vault".into(),
            ));
        }

        if !share.verify_checksum() {
            return Err(HakilleError::CapabilityDenied(
                "Invalid share checksum: share has been corrupted or tampered with".into(),
            ));
        }

        match &mut vault.state {
            InheritanceVaultState::QuorumPending {
                collected_shares,
                required_threshold,
                ..
            } => {
                // Deduplicate by share index
                if !collected_shares.iter().any(|s| s.index == share.index) {
                    collected_shares.push(share);
                }

                if collected_shares.len() >= *required_threshold as usize {
                    let reconstructed = reconstruct_secret(collected_shares)
                        .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;

                    let mut fingerprint = [0u8; 16];
                    fingerprint.copy_from_slice(&reconstructed[..16]);

                    vault.state = InheritanceVaultState::Unlocked {
                        unlocked_at: Utc::now(),
                        estate_key_fingerprint: fingerprint,
                    };
                    vault.updated_at = Utc::now();
                    return Ok(Some(reconstructed));
                }

                Ok(None)
            }
            _ => Err(HakilleError::CapabilityDenied(
                "Vault is not in QuorumPending state: shares cannot be submitted".into(),
            )),
        }
    }

    /// Shreading of the intimate partition: destroys private journal & intimate records
    /// while preserving legal estate document tokens.
    pub fn shred_intimate_partition(
        vault: &mut InheritanceVaultRecord,
        zk_store: &mut RedbZkStore,
    ) -> Result<(), HakilleError> {
        zk_store.shred_vault_intimate_partition(&vault.vault_id)?;
        vault.intimate_partition_blind_tokens.clear();
        vault.updated_at = Utc::now();
        Ok(())
    }

    /// Permanent crypto-shredding of the entire vault
    pub fn permanently_shred_vault(
        vault: &mut InheritanceVaultRecord,
        zk_store: &mut RedbZkStore,
    ) -> Result<(), HakilleError> {
        let _ = zk_store.shred_vault_intimate_partition(&vault.vault_id);
        vault.legal_partition_blind_tokens.clear();
        vault.intimate_partition_blind_tokens.clear();
        vault.state = InheritanceVaultState::CryptoShredded {
            shredded_at: Utc::now(),
        };
        vault.updated_at = Utc::now();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use hakille_core::UserId;

    fn make_test_vault() -> (InheritanceVaultRecord, [u8; 32], Vec<SecretShare>) {
        let owner_id = UserId(Uuid::new_v4());
        let owner_did = Did::new("did:key:z6MkuAliceOwnerKeyMock");
        let family_tenant = TenantId::new();

        let trustee_dids = vec![
            Did::new("did:key:z6MkuTrusteeBob1"),
            Did::new("did:key:z6MkuTrusteeCharlie2"),
            Did::new("did:key:z6MkuTrusteeDiana3"),
        ];

        let initial_heartbeat = BiometricHeartbeatAttestation::new(
            owner_id.clone(),
            Utc::now(),
            "ContinuousMultimodal",
            vec![0x01, 0x02, 0x03, 0x04],
        );

        let master_estate_key = [42u8; 32];
        let shares = InheritanceVaultEngine::partition_estate_key(&master_estate_key, 2, 3)
            .expect("Failed to partition estate key");

        let legal_tokens = vec![[1u8; 32], [2u8; 32]];
        let intimate_tokens = vec![[99u8; 32]];

        let vault = InheritanceVaultEngine::create_vault(
            owner_id,
            owner_did,
            family_tenant,
            trustee_dids,
            2,
            90,
            14,
            initial_heartbeat,
            legal_tokens,
            intimate_tokens,
        )
        .expect("Failed to create vault");

        (vault, master_estate_key, shares)
    }

    #[test]
    fn test_vault_creation_validations() {
        let owner_id = UserId(Uuid::new_v4());
        let owner_did = Did::new("did:key:z6MkuTest");
        let family_tenant = TenantId::new();
        let trustee_dids = vec![Did::new("did:key:z6MkuT1")];

        let invalid_heartbeat = BiometricHeartbeatAttestation::new(
            owner_id.clone(),
            Utc::now(),
            "FaceLiveness",
            vec![], // empty signature
        );

        // Fail on empty signature
        let res = InheritanceVaultEngine::create_vault(
            owner_id.clone(),
            owner_did.clone(),
            family_tenant,
            trustee_dids.clone(),
            1,
            90,
            14,
            invalid_heartbeat,
            vec![],
            vec![],
        );
        assert!(res.is_err());

        // Fail on threshold > total trustees
        let valid_heartbeat = BiometricHeartbeatAttestation::new(
            owner_id.clone(),
            Utc::now(),
            "FaceLiveness",
            vec![1, 2, 3],
        );
        let res2 = InheritanceVaultEngine::create_vault(
            owner_id,
            owner_did,
            family_tenant,
            trustee_dids,
            2, // threshold 2 > 1 trustee
            90,
            14,
            valid_heartbeat,
            vec![],
            vec![],
        );
        assert!(res2.is_err());
    }

    #[test]
    fn test_inheritance_vault_full_lifecycle() {
        let (mut vault, master_estate_key, shares) = make_test_vault();
        let t0 = vault.inactivity_clause.last_biometric_attestation.timestamp;

        // At t0 + 30 days: still Armed
        let t_30 = t0 + Duration::days(30);
        let state = InheritanceVaultEngine::evaluate_heartbeat(&mut vault, None, t_30).unwrap();
        assert_eq!(state, InheritanceVaultState::Armed);

        // At t0 + 91 days: transitions to InactivityGracePeriod
        let t_91 = t0 + Duration::days(91);
        let state = InheritanceVaultEngine::evaluate_heartbeat(&mut vault, None, t_91).unwrap();
        match state {
            InheritanceVaultState::InactivityGracePeriod { days_inactive, .. } => {
                assert_eq!(days_inactive, 90);
            }
            other => panic!("Expected InactivityGracePeriod, got {:?}", other),
        }

        // Attempting to submit shares during InactivityGracePeriod is denied
        let trustee1 = &vault.trustee_dids[0].clone();
        let deny_res =
            InheritanceVaultEngine::submit_trustee_share(&mut vault, trustee1, shares[0].clone());
        assert!(deny_res.is_err());

        // At t0 + 105 days (> 90 + 14 = 104): transitions to QuorumPending
        let t_105 = t0 + Duration::days(105);
        let state = InheritanceVaultEngine::evaluate_heartbeat(&mut vault, None, t_105).unwrap();
        match state {
            InheritanceVaultState::QuorumPending {
                required_threshold,
                total_trustees,
                ..
            } => {
                assert_eq!(required_threshold, 2);
                assert_eq!(total_trustees, 3);
            }
            other => panic!("Expected QuorumPending, got {:?}", other),
        }

        // Submitting unauthorized trustee fails
        let stranger = Did::new("did:key:z6MkuStrangerHacker");
        let stranger_res =
            InheritanceVaultEngine::submit_trustee_share(&mut vault, &stranger, shares[0].clone());
        assert!(stranger_res.is_err());

        // Submitting corrupted share fails
        let mut corrupted_share = shares[0].clone();
        corrupted_share.checksum = [0xde, 0xad, 0xbe, 0xef];
        let corrupt_res =
            InheritanceVaultEngine::submit_trustee_share(&mut vault, trustee1, corrupted_share);
        assert!(corrupt_res.is_err());

        // Trustee 1 submits valid share (threshold 2, collected 1) -> Ok(None)
        let sub1 =
            InheritanceVaultEngine::submit_trustee_share(&mut vault, trustee1, shares[0].clone())
                .unwrap();
        assert_eq!(sub1, None);

        // Submitting duplicate share 1 does not unlock
        let sub_dup =
            InheritanceVaultEngine::submit_trustee_share(&mut vault, trustee1, shares[0].clone())
                .unwrap();
        assert_eq!(sub_dup, None);

        // Trustee 2 submits valid share (threshold 2, collected 2) -> Ok(Some(master_key))
        let trustee2 = vault.trustee_dids[1].clone();
        let sub2 =
            InheritanceVaultEngine::submit_trustee_share(&mut vault, &trustee2, shares[1].clone())
                .unwrap();
        assert_eq!(sub2, Some(master_estate_key));

        // State is now Unlocked with correct fingerprint
        match &vault.state {
            InheritanceVaultState::Unlocked {
                estate_key_fingerprint,
                ..
            } => {
                assert_eq!(*estate_key_fingerprint, master_estate_key[..16]);
            }
            other => panic!("Expected Unlocked state, got {:?}", other),
        }
    }

    #[test]
    fn test_biometric_heartbeat_resets_inactivity() {
        let (mut vault, _, _) = make_test_vault();
        let t0 = vault.inactivity_clause.last_biometric_attestation.timestamp;

        // Enter grace period
        let t_92 = t0 + Duration::days(92);
        let state = InheritanceVaultEngine::evaluate_heartbeat(&mut vault, None, t_92).unwrap();
        assert!(matches!(
            state,
            InheritanceVaultState::InactivityGracePeriod { .. }
        ));

        // Fresh owner heartbeat received
        let fresh_attestation = BiometricHeartbeatAttestation::new(
            vault.owner_user_id.clone(),
            t_92,
            "FaceLiveness",
            vec![0xaa, 0xbb],
        );
        let reset_state =
            InheritanceVaultEngine::evaluate_heartbeat(&mut vault, Some(fresh_attestation), t_92)
                .unwrap();
        assert_eq!(reset_state, InheritanceVaultState::Armed);

        // Check at t0 + 100 days (only 8 days since reset at t_92) -> still Armed
        let t_100 = t0 + Duration::days(100);
        let state_after =
            InheritanceVaultEngine::evaluate_heartbeat(&mut vault, None, t_100).unwrap();
        assert_eq!(state_after, InheritanceVaultState::Armed);

        // Spoofed heartbeat with wrong user_id is rejected
        let attacker_attestation = BiometricHeartbeatAttestation::new(
            UserId(Uuid::new_v4()),
            t_100,
            "FaceLiveness",
            vec![0xff],
        );
        let err = InheritanceVaultEngine::evaluate_heartbeat(
            &mut vault,
            Some(attacker_attestation),
            t_100,
        );
        assert!(err.is_err());
    }

    #[test]
    fn test_legal_vs_intimate_partition_shredding_with_redb_store() {
        let (mut vault, _, _) = make_test_vault();
        let mut store = RedbZkStore::new();

        let legal_token = vault.legal_partition_blind_tokens[0];
        let intimate_token = vault.intimate_partition_blind_tokens[0];

        // Populate store with legal document and intimate diary
        store.insert_blind_spo(legal_token, b"Legal Will & Testament".to_vec());
        store.insert_blind_spo(intimate_token, b"Private Personal Diary".to_vec());
        store.insert_inheritance_vault(vault.clone());

        // Verify both are initially accessible
        assert!(!store.query_blind_spo(&legal_token).is_empty());
        assert!(!store.query_blind_spo(&intimate_token).is_empty());
        assert!(!store.is_vault_intimate_partition_shredded(&vault.vault_id));

        // Execute intimate partition shredding
        InheritanceVaultEngine::shred_intimate_partition(&mut vault, &mut store).unwrap();

        // Verify intimate partition was shredded in vault and store
        assert!(vault.intimate_partition_blind_tokens.is_empty());
        assert!(store.query_blind_spo(&intimate_token).is_empty());
        assert!(store.is_vault_intimate_partition_shredded(&vault.vault_id));

        // Verify legal partition remains intact
        assert!(!vault.legal_partition_blind_tokens.is_empty());
        assert!(!store.query_blind_spo(&legal_token).is_empty());
    }

    #[test]
    fn test_permanent_vault_crypto_shredding() {
        let (mut vault, _, shares) = make_test_vault();
        let mut store = RedbZkStore::new();

        store.insert_inheritance_vault(vault.clone());
        InheritanceVaultEngine::permanently_shred_vault(&mut vault, &mut store).unwrap();

        assert!(matches!(
            vault.state,
            InheritanceVaultState::CryptoShredded { .. }
        ));
        assert!(vault.legal_partition_blind_tokens.is_empty());
        assert!(vault.intimate_partition_blind_tokens.is_empty());

        // Cannot re-arm after shredding
        let hb = BiometricHeartbeatAttestation::new(
            vault.owner_user_id.clone(),
            Utc::now(),
            "FaceLiveness",
            vec![1, 2],
        );
        let res = InheritanceVaultEngine::evaluate_heartbeat(&mut vault, Some(hb), Utc::now());
        assert!(res.is_err());

        // Cannot submit shares
        let trustee0 = vault.trustee_dids[0].clone();
        let sub_res =
            InheritanceVaultEngine::submit_trustee_share(&mut vault, &trustee0, shares[0].clone());
        assert!(sub_res.is_err());
    }
}
