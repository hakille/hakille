//! Continuous Authentication & Zero-Knowledge Storage Integration (F22).
//! Manages continuous session state, real-time presence verification,
//! searchable blinded user indices, revocable token commitments, and encrypted session persistence.

use hakille_core::crypto::{AadBinding, XChaChaBlob, XChaChaCipher, compute_blind_index};
use hakille_core::{HakilleError, Timestamp, UserId};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    pub user_id: UserId,
    pub started_at: Timestamp,
    pub last_verified: Timestamp,
    pub confidence_score: f64,
    pub is_fully_authenticated: bool,
}

pub struct ContinuousAuth;

impl ContinuousAuth {
    pub fn start_session(user_id: UserId, initial_confidence: f64) -> AuthSession {
        let now = Timestamp::now();
        AuthSession {
            user_id,
            started_at: now.clone(),
            last_verified: now,
            confidence_score: initial_confidence,
            is_fully_authenticated: initial_confidence >= 0.8,
        }
    }

    pub fn update_confidence(
        session: &mut AuthSession,
        new_embedding: &[f32],
        stored: &[f32],
    ) -> f64 {
        let similarity = Self::cosine_similarity(new_embedding, stored);
        session.confidence_score = (session.confidence_score + similarity) / 2.0;
        // Toute ré-évaluation réussie rafraîchit la preuve de présence.
        session.last_verified = Timestamp::now();
        session.is_fully_authenticated = session.confidence_score >= 0.8;
        session.confidence_score
    }

    /// Session valide si `score >= min` ET preuve récente (< 15 min).
    /// Le timeout borne la réutilisation d'une session volée/inactive
    /// (fail-closed au-delà de 15 min sans re-vérification).
    pub fn is_valid(session: &AuthSession, min_confidence: f64) -> bool {
        if session.confidence_score < min_confidence {
            return false;
        }
        let elapsed = chrono::Utc::now().signed_duration_since(session.last_verified.0);
        elapsed.num_seconds() >= 0 && elapsed.num_seconds() < 15 * 60
    }

    fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }
        let mut dot = 0.0;
        let mut norm_a = 0.0;
        let mut norm_b = 0.0;
        for (x, y) in a.iter().zip(b.iter()) {
            dot += x * y;
            norm_a += x * x;
            norm_b += y * y;
        }
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }
        (dot / (norm_a.sqrt() * norm_b.sqrt())) as f64
    }
}

// =============================================================================
// F22: ZK Storage Integration & Schemas
// =============================================================================

/// Cryptographic commitment C = SHA-256(Token || Salt) supporting revocation checks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RevocableTokenCommitment {
    pub commitment_hash: [u8; 32],
    pub salt: [u8; 16],
    pub created_at: Timestamp,
}

impl RevocableTokenCommitment {
    /// Generate a fresh random 32-byte token, 16-byte salt, and commitment C = SHA-256(token || salt)
    pub fn generate() -> (Self, [u8; 32]) {
        let mut token = [0u8; 32];
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut token);
        OsRng.fill_bytes(&mut salt);

        let mut hasher = Sha256::new();
        hasher.update(&token);
        hasher.update(&salt);
        let hash = hasher.finalize();

        let mut commitment_hash = [0u8; 32];
        commitment_hash.copy_from_slice(&hash);

        (
            Self {
                commitment_hash,
                salt,
                created_at: Timestamp::now(),
            },
            token,
        )
    }

    /// Constant-time verification of raw token against stored commitment
    pub fn verify_token(&self, candidate_token: &[u8; 32]) -> bool {
        let mut hasher = Sha256::new();
        hasher.update(candidate_token);
        hasher.update(&self.salt);
        let computed = hasher.finalize();

        // Constant-time comparison
        let mut diff = 0u8;
        for (a, b) in computed.iter().zip(self.commitment_hash.iter()) {
            diff |= a ^ b;
        }
        diff == 0
    }
}

/// Zero-Knowledge session persistence schema record stored on disk / redb.
/// Plaintext user ID is never exposed as an unencrypted primary key; lookup is mediated
/// by an HMAC-SHA256 blinded user index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionPersistenceRecord {
    pub session_id: Uuid,
    pub blinded_user_index: [u8; 32],
    pub token_commitment: RevocableTokenCommitment,
    pub encrypted_payload: XChaChaBlob,
    pub created_at: Timestamp,
    pub last_verified: Timestamp,
    pub expires_at: Timestamp,
    pub is_revoked: bool,
}

/// Zero-Knowledge Session Storage Engine with searchable blind indexing,
/// revocable commitments, and authenticated payload persistence.
#[derive(Default)]
pub struct ZkSessionStorage {
    sessions: HashMap<Uuid, SessionPersistenceRecord>,
    blinded_user_index_map: HashMap<[u8; 32], Vec<Uuid>>,
    revoked_commitments: HashSet<[u8; 32]>,
}

impl ZkSessionStorage {
    pub fn new() -> Self {
        Self::default()
    }

    /// Persist an active continuous AuthSession into the encrypted ZK storage.
    /// Derives the blinded user index, generates a revocable token commitment,
    /// encrypts the session state with XChaCha20-Poly1305 and AAD binding, and returns
    /// the persistence record alongside the raw capability bearer token.
    pub fn persist_session(
        &mut self,
        session: &AuthSession,
        blind_key: &[u8; 32],
        storage_key: &[u8; 32],
        duration_minutes: i64,
    ) -> Result<(SessionPersistenceRecord, [u8; 32]), HakilleError> {
        let session_id = Uuid::new_v4();
        let blinded_user_index = compute_blind_index(&session.user_id.0.to_string(), blind_key);
        let (token_commitment, raw_token) = RevocableTokenCommitment::generate();

        let plaintext_bytes = serde_json::to_vec(session)
            .map_err(|e| HakilleError::SerializationError(e.to_string()))?;

        // AAD binds tenant domain, table, and unique session_id to prevent record splicing
        let aad = AadBinding::new(
            "sovereign-session",
            "session_vault",
            "zk_session_table",
            &session_id.to_string(),
            1,
        )
        .to_bytes();

        let encrypted_payload = XChaChaCipher::encrypt(&plaintext_bytes, storage_key, &aad)
            .map_err(|e| HakilleError::CryptoError(e.to_string()))?;

        let now = chrono::Utc::now();
        let expires_at = now + chrono::Duration::minutes(duration_minutes);

        let record = SessionPersistenceRecord {
            session_id,
            blinded_user_index,
            token_commitment,
            encrypted_payload,
            created_at: session.started_at.clone(),
            last_verified: session.last_verified.clone(),
            expires_at: Timestamp(expires_at),
            is_revoked: false,
        };

        self.blinded_user_index_map
            .entry(blinded_user_index)
            .or_default()
            .push(session_id);

        self.sessions.insert(session_id, record.clone());

        Ok((record, raw_token))
    }

    /// Restore and decrypt an authenticated session from ZK storage.
    /// Strictly verifies:
    /// 1. Bearer token matches cryptographic commitment
    /// 2. Commitment is not revoked
    /// 3. Session has not expired
    /// 4. Authenticated ciphertext decrypts without tampering
    pub fn restore_session(
        &self,
        record: &SessionPersistenceRecord,
        storage_key: &[u8; 32],
        candidate_token: &[u8; 32],
    ) -> Result<AuthSession, HakilleError> {
        if record.is_revoked || self.is_commitment_revoked(&record.token_commitment.commitment_hash)
        {
            return Err(HakilleError::CapabilityDenied(
                "Session token commitment has been revoked".to_string(),
            ));
        }

        if chrono::Utc::now() > record.expires_at.0 {
            return Err(HakilleError::CapabilityExpired);
        }

        if !record.token_commitment.verify_token(candidate_token) {
            return Err(HakilleError::AuthenticationFailed);
        }

        let aad = AadBinding::new(
            "sovereign-session",
            "session_vault",
            "zk_session_table",
            &record.session_id.to_string(),
            1,
        )
        .to_bytes();

        let decrypted_bytes = XChaChaCipher::decrypt(&record.encrypted_payload, storage_key, &aad)
            .map_err(|e| HakilleError::CryptoError(e.to_string()))?;

        let session: AuthSession = serde_json::from_slice(&decrypted_bytes)
            .map_err(|e| HakilleError::SerializationError(e.to_string()))?;

        Ok(session)
    }

    /// Search sessions by blinded user index without knowing the plaintext user identity
    pub fn find_by_blind_user_index(
        &self,
        blind_index: &[u8; 32],
    ) -> Vec<&SessionPersistenceRecord> {
        if let Some(ids) = self.blinded_user_index_map.get(blind_index) {
            ids.iter().filter_map(|id| self.sessions.get(id)).collect()
        } else {
            Vec::new()
        }
    }

    /// Search sessions for a known user by computing their blind index with the domain key
    pub fn lookup_by_user(
        &self,
        user_id: &UserId,
        blind_key: &[u8; 32],
    ) -> Vec<&SessionPersistenceRecord> {
        let blind_index = compute_blind_index(&user_id.0.to_string(), blind_key);
        self.find_by_blind_user_index(&blind_index)
    }

    /// Revoke a session token commitment permanently
    pub fn revoke_commitment(&mut self, commitment_hash: [u8; 32]) -> Result<(), HakilleError> {
        self.revoked_commitments.insert(commitment_hash);

        for session in self.sessions.values_mut() {
            if session.token_commitment.commitment_hash == commitment_hash {
                session.is_revoked = true;
            }
        }

        Ok(())
    }

    /// Check if a commitment has been revoked
    pub fn is_commitment_revoked(&self, commitment_hash: &[u8; 32]) -> bool {
        self.revoked_commitments.contains(commitment_hash)
    }

    /// Purge expired sessions and cleanup index map
    pub fn purge_expired(&mut self) -> usize {
        let now = chrono::Utc::now();
        let initial_count = self.sessions.len();

        let expired_ids: Vec<Uuid> = self
            .sessions
            .iter()
            .filter(|(_, rec)| now > rec.expires_at.0 || rec.is_revoked)
            .map(|(id, _)| *id)
            .collect();

        for id in &expired_ids {
            if let Some(rec) = self.sessions.remove(id) {
                if let Some(list) = self.blinded_user_index_map.get_mut(&rec.blinded_user_index) {
                    list.retain(|x| x != id);
                }
            }
        }

        initial_count - self.sessions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_revocable_token_commitment_lifecycle() {
        let (commitment, token) = RevocableTokenCommitment::generate();

        // Valid token verifies
        assert!(commitment.verify_token(&token));

        // Tampered token fails
        let mut bad_token = token;
        bad_token[0] ^= 0xff;
        assert!(!commitment.verify_token(&bad_token));
    }

    #[test]
    fn test_blinded_user_index_and_session_persistence() {
        let mut storage = ZkSessionStorage::new();
        let user_id = UserId(Uuid::new_v4());
        let blind_key = [0x55u8; 32];
        let storage_key = [0x77u8; 32];

        let session = ContinuousAuth::start_session(user_id.clone(), 0.85);
        let (record, _token) = storage
            .persist_session(&session, &blind_key, &storage_key, 60)
            .expect("session persistence should succeed");

        // Blind index matches computed token
        let expected_blind_index = compute_blind_index(&user_id.0.to_string(), &blind_key);
        assert_eq!(record.blinded_user_index, expected_blind_index);

        // Lookup by user succeeds
        let found = storage.lookup_by_user(&user_id, &blind_key);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].session_id, record.session_id);

        // Revocation blocks session
        storage
            .revoke_commitment(record.token_commitment.commitment_hash)
            .unwrap();
        assert!(storage.is_commitment_revoked(&record.token_commitment.commitment_hash));

        // Purge cleans up revoked/expired
        let purged = storage.purge_expired();
        assert_eq!(purged, 1);
        assert!(storage.lookup_by_user(&user_id, &blind_key).is_empty());
    }

    #[test]
    fn test_session_restore_roundtrip() {
        let mut storage = ZkSessionStorage::new();
        let user_id = UserId(Uuid::new_v4());
        let blind_key = [0x42u8; 32];
        let storage_key = [0x99u8; 32];

        let original_session = ContinuousAuth::start_session(user_id.clone(), 0.92);
        let (record, token) = storage
            .persist_session(&original_session, &blind_key, &storage_key, 120)
            .expect("persist should succeed");

        // Correct token restores session cleanly
        let restored = storage
            .restore_session(&record, &storage_key, &token)
            .expect("restore should succeed");
        assert_eq!(restored.user_id, original_session.user_id);
        assert_eq!(restored.confidence_score, original_session.confidence_score);
        assert_eq!(
            restored.is_fully_authenticated,
            original_session.is_fully_authenticated
        );

        // Invalid token rejected
        let bad_token = [0x00u8; 32];
        let failed_auth = storage.restore_session(&record, &storage_key, &bad_token);
        assert!(failed_auth.is_err());

        // Tampered ciphertext rejected (Poly1305 MAC failure)
        let mut tampered_rec = record.clone();
        tampered_rec.encrypted_payload.ciphertext[0] ^= 0x01;
        let failed_decrypt = storage.restore_session(&tampered_rec, &storage_key, &token);
        assert!(failed_decrypt.is_err());
    }
}
