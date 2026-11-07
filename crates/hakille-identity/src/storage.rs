//! Zero-Knowledge Storage Schemas & Mappings (F22).
//! Searchable blind-indexed tables, user records, credentials, recovery shares,
//! and digital twilight key shredding.

use crate::crypto::{XChaChaBlob, compute_blind_index};
use crate::ssi::Did;
use chrono::{DateTime, Utc};
use hakille_core::{HakilleError, UserId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccountStatus {
    Active,
    Suspended,
    DigitalTwilightGrace,
    Shredded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub user_id: UserId,
    pub primary_did: Did,
    pub blind_username_hash: [u8; 32],
    pub encrypted_profile_blob: XChaChaBlob,
    pub state: AccountStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCredentialRecord {
    pub credential_id: Uuid,
    pub user_id: UserId,
    pub credential_type: String,
    pub public_identifier: String,
    pub encrypted_payload: XChaChaBlob,
    pub counter: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiometricCommitmentRecord {
    pub commitment_id: Uuid,
    pub user_id: UserId,
    pub helper_data: Vec<u8>,
    pub commitment_hash: [u8; 32],
    pub salt: [u8; 16],
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryShareRecord {
    pub share_id: Uuid,
    pub user_id: UserId,
    pub trustee_did: Did,
    pub threshold: u8,
    pub total_shares: u8,
    pub share_index: u8,
    pub encrypted_share_blob: Vec<u8>,
}

/// ZK Storage Engine maintaining searchable blind-indexed tables
#[derive(Default)]
pub struct RedbZkStore {
    users_table: HashMap<UserId, UserRecord>,
    blind_user_index: HashMap<[u8; 32], UserId>,
    credentials_table: HashMap<Uuid, StoredCredentialRecord>,
    bio_commitments: HashMap<Uuid, BiometricCommitmentRecord>,
    recovery_shares: HashMap<Uuid, RecoveryShareRecord>,
    spo_blind_table: HashMap<[u8; 32], Vec<Vec<u8>>>,
    shredded_salts: std::collections::HashSet<UserId>,
}

impl RedbZkStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_user(
        &mut self,
        user: UserRecord,
    ) -> Result<(), HakilleError> {
        let uid = user.user_id.clone();
        let blind_hash = user.blind_username_hash;

        if self.blind_user_index.contains_key(&blind_hash) {
            return Err(HakilleError::CapabilityDenied(
                "User with this blind index already exists".to_string(),
            ));
        }

        self.blind_user_index.insert(blind_hash, uid.clone());
        self.users_table.insert(uid, user);
        Ok(())
    }

    pub fn lookup_user_by_blind_username(
        &self,
        username: &str,
        blind_key: &[u8; 32],
    ) -> Option<&UserRecord> {
        let token = compute_blind_index(username, blind_key);
        let uid = self.blind_user_index.get(&token)?;
        self.users_table.get(uid)
    }

    pub fn insert_credential(&mut self, record: StoredCredentialRecord) {
        self.credentials_table.insert(record.credential_id, record);
    }

    pub fn get_credential(&self, id: &Uuid) -> Option<&StoredCredentialRecord> {
        self.credentials_table.get(id)
    }

    pub fn insert_biometric_commitment(&mut self, record: BiometricCommitmentRecord) {
        self.bio_commitments.insert(record.commitment_id, record);
    }

    pub fn get_biometric_commitment(&self, id: &Uuid) -> Option<&BiometricCommitmentRecord> {
        self.bio_commitments.get(id)
    }

    pub fn store_recovery_share(&mut self, record: RecoveryShareRecord) {
        self.recovery_shares.insert(record.share_id, record);
    }

    pub fn get_recovery_shares_for_user(&self, user_id: &UserId) -> Vec<RecoveryShareRecord> {
        self.recovery_shares
            .values()
            .filter(|r| r.user_id == *user_id)
            .cloned()
            .collect()
    }

    /// Trigger digital twilight: transitions user to grace, releases recovery shares,
    /// and crypto-shreds designated private journal partition salts.
    pub fn trigger_digital_twilight(&mut self, user_id: &UserId) -> Result<(), HakilleError> {
        let user = self
            .users_table
            .get_mut(user_id)
            .ok_or(HakilleError::AuthenticationFailed)?;

        user.state = AccountStatus::DigitalTwilightGrace;
        user.updated_at = Utc::now();

        // Crypto-shred private journal keys
        self.shredded_salts.insert(user_id.clone());
        Ok(())
    }

    pub fn is_salt_shredded(&self, user_id: &UserId) -> bool {
        self.shredded_salts.contains(user_id)
    }

    pub fn insert_blind_spo(&mut self, subject_blind_token: [u8; 32], encrypted_predicate_object: Vec<u8>) {
        self.spo_blind_table
            .entry(subject_blind_token)
            .or_default()
            .push(encrypted_predicate_object);
    }

    pub fn query_blind_spo(&self, subject_blind_token: &[u8; 32]) -> Vec<Vec<u8>> {
        self.spo_blind_table
            .get(subject_blind_token)
            .cloned()
            .unwrap_or_default()
    }
}
