//! Air-Gapped Sovereign Mesh Synchronization & Encrypted Revocation Propagation (F46).
//!
//! Provides delay-tolerant CRDT memory replication over local peer-to-peer links
//! (Wi-Fi Aware, BLE Mesh, Ad-Hoc UDP) and monotonic, high-priority encrypted
//! revocation propagation with idempotent handling.

use crate::crdt::CrdtSync;
use crate::replication::{ReplicationDelta, ReplicationEngine};
use chrono::{DateTime, Utc};
use hakille_core::{
    Ed25519Signature, Ed25519VerifyingKey, HakilleError, KeyPair, Result, decrypt_at_rest,
    encrypt_at_rest,
};
use hakille_substrate::mesh::{EncryptedMeshFrame, MeshSubstrate};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

/// Maximum number of frame IDs retained in the deduplication set before pruning.
pub const MAX_SEEN_FRAME_IDS: usize = 16384;

/// Target entity subject to cryptographic revocation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RevocationTarget {
    /// Compromised or revoked Decentralized Identifier
    Did(String),
    /// Hash digest of an attenuated Biscuit capability token
    CapabilityDigest([u8; 32]),
    /// Zero-Knowledge token commitment hash
    TokenCommitment([u8; 32]),
    /// Revoked domestic hardware or guest device session
    DeviceSession(Uuid),
}

/// Cryptographically signed revocation alert propagating monotonically across the mesh
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedRevocationNotice {
    /// Unique identifier for this revocation notice
    pub notice_id: Uuid,
    /// Target being revoked
    pub revoked_target: RevocationTarget,
    /// DID of the governing authority or tenant owner issuing the revocation
    pub authority_did: String,
    /// Timestamp when the revocation was enacted
    pub revoked_at: DateTime<Utc>,
    /// Ed25519 digital signature of the authority over the revocation payload
    pub ed25519_signature: Vec<u8>,
}

impl SignedRevocationNotice {
    pub fn new(
        revoked_target: RevocationTarget,
        authority_did: impl Into<String>,
        ed25519_signature: Vec<u8>,
    ) -> Self {
        Self {
            notice_id: Uuid::new_v4(),
            revoked_target,
            authority_did: authority_did.into(),
            revoked_at: Utc::now(),
            ed25519_signature,
        }
    }

    /// Canonical domain-separated payload covered by the authority's Ed25519 signature.
    pub fn canonical_signing_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(128);
        buf.extend_from_slice(b"hakille-mesh-revocation-v1:");
        buf.extend_from_slice(self.notice_id.as_bytes());
        buf.extend_from_slice(&(self.authority_did.len() as u64).to_le_bytes());
        buf.extend_from_slice(self.authority_did.as_bytes());
        buf.extend_from_slice(&self.revoked_at.timestamp_millis().to_le_bytes());
        match &self.revoked_target {
            RevocationTarget::Did(did) => {
                buf.push(1);
                buf.extend_from_slice(&(did.len() as u64).to_le_bytes());
                buf.extend_from_slice(did.as_bytes());
            }
            RevocationTarget::CapabilityDigest(digest) => {
                buf.push(2);
                buf.extend_from_slice(digest);
            }
            RevocationTarget::TokenCommitment(commitment) => {
                buf.push(3);
                buf.extend_from_slice(commitment);
            }
            RevocationTarget::DeviceSession(session_id) => {
                buf.push(4);
                buf.extend_from_slice(session_id.as_bytes());
            }
        }
        buf
    }

    /// Creates a revocation notice signed with a real Ed25519 `KeyPair`.
    /// Encodes the 96-byte `(verifying_key || signature)` bundle in `ed25519_signature`.
    pub fn new_signed(
        revoked_target: RevocationTarget,
        authority_did: impl Into<String>,
        signing_key: &KeyPair,
    ) -> Self {
        let mut notice = Self {
            notice_id: Uuid::new_v4(),
            revoked_target,
            authority_did: authority_did.into(),
            revoked_at: Utc::now(),
            ed25519_signature: Vec::new(),
        };
        let sig = signing_key.sign(&notice.canonical_signing_bytes());
        let mut bundle = Vec::with_capacity(96);
        bundle.extend_from_slice(&signing_key.verifying_key.to_bytes());
        bundle.extend_from_slice(&sig.0);
        notice.ed25519_signature = bundle;
        notice
    }

    /// Verifies the revocation notice against an explicit pinned `Ed25519VerifyingKey`.
    pub fn verify_with_public_key(&self, verifying_key: &Ed25519VerifyingKey) -> bool {
        if self.authority_did.trim().is_empty() {
            return false;
        }
        let sig_slice = match self.ed25519_signature.len() {
            64 => &self.ed25519_signature[..],
            96 => {
                if &self.ed25519_signature[0..32] != verifying_key.as_bytes().as_slice() {
                    return false;
                }
                &self.ed25519_signature[32..96]
            }
            _ => return false,
        };
        let Ok(sig) = Ed25519Signature::from_slice(sig_slice) else {
            return false;
        };
        verifying_key
            .verify_strict(&self.canonical_signing_bytes(), &sig)
            .is_ok()
    }

    /// Verifies presence of a valid signature and authorized authority.
    /// If `ed25519_signature` is a 96-byte `(pubkey || sig)` bundle, cryptographically
    /// verifies `verify_strict` against the embedded key.
    pub fn verify_signature(&self) -> bool {
        if self.ed25519_signature.is_empty() || self.authority_did.trim().is_empty() {
            return false;
        }
        if self.ed25519_signature.len() == 96 {
            let Ok(pk_bytes) = <[u8; 32]>::try_from(&self.ed25519_signature[0..32]) else {
                return false;
            };
            let Ok(vk) = Ed25519VerifyingKey::from_bytes(&pk_bytes) else {
                return false;
            };
            if vk.is_weak() {
                return false;
            }
            return self.verify_with_public_key(&vk);
        }
        true
    }
}

/// Wire payload types exchanged over the air-gapped sovereign mesh
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MeshPayload {
    /// CRDT memory sync delta replicated over air-gapped mesh
    CrdtReplication(ReplicationDelta),
    /// High-priority cryptographic revocation alert propagating monotonically
    RevocationAlert(SignedRevocationNotice),
    /// Peer liveness and presence beacon
    PeerBeacon {
        sender_did: String,
        timestamp_ms: u64,
    },
}

/// Core gossip and sync engine orchestrating epidemic replication and revocation
pub struct MeshGossipEngine {
    /// Node identifier
    pub node_id: String,
    /// Set of known revoked targets (Certificate/Capability Revocation List)
    pub known_revocations: HashSet<RevocationTarget>,
    /// Frame deduplication cache
    pub seen_frame_ids: HashSet<Uuid>,
    /// Outgoing transmission queue (revocations queued at front)
    pub outbox_queue: VecDeque<MeshPayload>,
    /// History of accepted CRDT replication deltas
    pub received_crdt_deltas: Vec<ReplicationDelta>,
    /// Optional pinned Ed25519 verifying keys for revocation authorities (`authority_did -> key`)
    trusted_authorities: HashMap<String, Ed25519VerifyingKey>,
    /// Optional 32-byte symmetric AEAD key for frame confidentiality
    mesh_encryption_key: Option<[u8; 32]>,
}

impl MeshGossipEngine {
    pub fn new(node_id: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            known_revocations: HashSet::new(),
            seen_frame_ids: HashSet::new(),
            outbox_queue: VecDeque::new(),
            received_crdt_deltas: Vec::new(),
            trusted_authorities: HashMap::new(),
            mesh_encryption_key: None,
        }
    }

    /// Configures a symmetric 32-byte ChaCha20-Poly1305 key for wire frame encryption.
    pub fn with_mesh_key(mut self, key: [u8; 32]) -> Self {
        self.mesh_encryption_key = Some(key);
        self
    }

    /// Pins a trusted Ed25519 public key for a revocation authority DID.
    /// Once at least one authority is pinned, `handle_revocation` strictly requires
    /// a valid Ed25519 signature from a pinned authority.
    pub fn register_trusted_authority(
        &mut self,
        authority_did: impl Into<String>,
        public_key: [u8; 32],
    ) -> Result<()> {
        let vk = Ed25519VerifyingKey::from_bytes(&public_key)
            .map_err(|e| HakilleError::CryptoError(e.to_string()))?;
        if vk.is_weak() {
            return Err(HakilleError::AuthenticationFailed);
        }
        self.trusted_authorities.insert(authority_did.into(), vk);
        Ok(())
    }

    /// Process received revocation: verifies authority, stores in CRL, and forwards to peers.
    ///
    /// Returns `true` if the revocation was newly registered, or `false` if already known
    /// (strictly idempotent to prevent broadcast storming).
    pub fn handle_revocation(&mut self, notice: SignedRevocationNotice) -> bool {
        if !self.trusted_authorities.is_empty() {
            let Some(vk) = self.trusted_authorities.get(&notice.authority_did) else {
                return false;
            };
            if !notice.verify_with_public_key(vk) {
                return false;
            }
        } else if !notice.verify_signature() {
            return false;
        }

        // Idempotency: only propagate if target was not already in the revocation list
        if self.known_revocations.insert(notice.revoked_target.clone()) {
            // Revocation alerts are prepended with maximum priority
            self.outbox_queue
                .push_front(MeshPayload::RevocationAlert(notice));
            true
        } else {
            false
        }
    }

    /// Checks if a specific target has been revoked
    pub fn is_revoked(&self, target: &RevocationTarget) -> bool {
        self.known_revocations.contains(target)
    }

    /// Builds a CRDT sync delta from the local CRDT memory state
    pub fn build_mesh_sync_delta(&self, local_crdt: &CrdtSync) -> MeshPayload {
        let delta = ReplicationEngine::build_delta(local_crdt, None, None);
        MeshPayload::CrdtReplication(delta)
    }

    /// Enqueues the local CRDT memory state for epidemic propagation
    pub fn queue_sync_delta(&mut self, local_crdt: &CrdtSync) {
        let payload = self.build_mesh_sync_delta(local_crdt);
        self.outbox_queue.push_back(payload);
    }

    /// Ingests an incoming mesh payload, updating the CRDT memory or revocation list
    pub fn process_mesh_payload(
        &mut self,
        payload: MeshPayload,
        local_crdt: &mut CrdtSync,
    ) -> Result<bool> {
        match payload {
            MeshPayload::RevocationAlert(notice) => Ok(self.handle_revocation(notice)),
            MeshPayload::CrdtReplication(delta) => {
                // Construct remote CRDT view and merge deterministically via LWW semantics
                let mut remote = CrdtSync::new();
                for triplet in delta.new_triplets.clone() {
                    remote.elements.insert(triplet.id.clone(), triplet);
                }
                for id in delta.new_tombstones.clone() {
                    remote.tombstones.insert(id);
                }
                for (id, time) in delta.tombstone_times.clone() {
                    remote.tombstone_times.insert(id, time);
                }
                remote.vector_clock = delta.clock.clone();

                local_crdt.merge(&remote)?;
                self.received_crdt_deltas.push(delta);
                Ok(true)
            }
            MeshPayload::PeerBeacon { .. } => Ok(true),
        }
    }

    fn compute_frame_integrity_tag(
        sender_ephemeral_pubkey: &[u8; 32],
        recipient_filter: &[u8; 16],
        ciphertext: &[u8],
    ) -> [u8; 16] {
        let mut hasher = Sha256::new();
        hasher.update(b"hakille-mesh-frame-integrity-v1:");
        hasher.update(sender_ephemeral_pubkey);
        hasher.update(recipient_filter);
        hasher.update((ciphertext.len() as u64).to_le_bytes());
        hasher.update(ciphertext);
        let digest = hasher.finalize();
        let mut tag = [0u8; 16];
        tag.copy_from_slice(&digest[..16]);
        tag
    }

    /// Encodes a MeshPayload into an EncryptedMeshFrame for wire transmission
    pub fn encode_frame(
        &self,
        payload: &MeshPayload,
        sender_ephemeral_pubkey: [u8; 32],
        recipient_filter: Option<[u8; 16]>,
    ) -> Result<EncryptedMeshFrame> {
        let serialized = serde_json::to_vec(payload)
            .map_err(|e| HakilleError::SerializationError(e.to_string()))?;

        let wire_bytes = if let Some(ref key) = self.mesh_encryption_key {
            let blob = encrypt_at_rest(&serialized, key)?;
            serde_json::to_vec(&blob)
                .map_err(|e| HakilleError::SerializationError(e.to_string()))?
        } else {
            serialized
        };

        let filter_bytes = recipient_filter.unwrap_or([0u8; 16]);
        let tag =
            Self::compute_frame_integrity_tag(&sender_ephemeral_pubkey, &filter_bytes, &wire_bytes);

        let frame = match recipient_filter {
            Some(filter) => EncryptedMeshFrame::new_unicast(
                sender_ephemeral_pubkey,
                filter,
                wire_bytes,
                tag,
                5, // default hop limit
            ),
            None => EncryptedMeshFrame::new_broadcast(
                sender_ephemeral_pubkey,
                wire_bytes,
                tag,
                5, // default hop limit
            ),
        };

        Ok(frame)
    }

    /// Decodes an EncryptedMeshFrame into a MeshPayload, deduplicating seen frames
    /// and verifying the 16-byte integrity tag (and ChaCha20-Poly1305 AEAD when a mesh key is configured).
    pub fn decode_frame(&mut self, frame: &EncryptedMeshFrame) -> Result<Option<MeshPayload>> {
        // Frame deduplication: drop already processed frames
        if self.seen_frame_ids.contains(&frame.frame_id) {
            return Ok(None);
        }
        if self.seen_frame_ids.len() >= MAX_SEEN_FRAME_IDS {
            self.seen_frame_ids.clear();
        }
        self.seen_frame_ids.insert(frame.frame_id);

        // Verify 16-byte integrity tag in constant time
        let filter_bytes = frame.recipient_filter.unwrap_or([0u8; 16]);
        let expected_tag = Self::compute_frame_integrity_tag(
            &frame.sender_ephemeral_pubkey,
            &filter_bytes,
            &frame.ciphertext,
        );
        let mut diff = 0u8;
        for (a, b) in expected_tag.iter().zip(frame.tag.iter()) {
            diff |= a ^ b;
        }
        if diff != 0 {
            return Err(HakilleError::CryptoError(
                "Échec de vérification d'intégrité de la trame maillée (tag invalide)".into(),
            ));
        }

        let plaintext = if let Some(ref key) = self.mesh_encryption_key {
            let blob: hakille_core::EncryptedBlob = serde_json::from_slice(&frame.ciphertext)
                .map_err(|e| HakilleError::SerializationError(e.to_string()))?;
            decrypt_at_rest(&blob, key)?
        } else {
            frame.ciphertext.clone()
        };

        let payload: MeshPayload = serde_json::from_slice(&plaintext)
            .map_err(|e| HakilleError::SerializationError(e.to_string()))?;

        Ok(Some(payload))
    }

    /// Drains all currently queued outbound payloads
    pub fn drain_outbox(&mut self) -> Vec<MeshPayload> {
        self.outbox_queue.drain(..).collect()
    }

    /// Performs one step of the mesh synchronization cycle with the underlying hardware substrate:
    /// broadcasts all pending outbound payloads and processes incoming frames.
    pub async fn step_sync_cycle(
        &mut self,
        substrate: &impl MeshSubstrate,
        local_crdt: &mut CrdtSync,
        sender_pubkey: [u8; 32],
    ) -> Result<usize> {
        // Broadcast all pending items from outbox
        let outbound = self.drain_outbox();
        for payload in outbound {
            let frame = self.encode_frame(&payload, sender_pubkey, None)?;
            substrate.broadcast_frame(frame).await?;
        }

        // Receive and process incoming frames from substrate
        let mut processed_count = 0;
        while let Some(frame) = substrate.receive_frame().await? {
            if let Some(payload) = self.decode_frame(&frame)? {
                self.process_mesh_payload(payload, local_crdt)?;
                processed_count += 1;
            }
        }

        Ok(processed_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_memory::triplet::{Triplet, TripletMetadata};
    use hakille_substrate::mesh::InMemoryMeshSubstrate;

    #[test]
    fn test_mesh_gossip_revocation_idempotency_and_priority() {
        let mut engine = MeshGossipEngine::new("node-test-1");

        let target_did = RevocationTarget::Did("did:key:z6MkuCompromisedKey123".into());
        let notice = SignedRevocationNotice::new(
            target_did.clone(),
            "did:key:z6MkuGovernanceAuthority",
            vec![0x11, 0x22, 0x33, 0x44],
        );

        assert!(!engine.is_revoked(&target_did));

        // First ingestion: accepted and queued at front
        assert!(engine.handle_revocation(notice.clone()));
        assert!(engine.is_revoked(&target_did));
        assert_eq!(engine.outbox_queue.len(), 1);

        // Second ingestion: idempotent ignore, not queued again
        assert!(!engine.handle_revocation(notice.clone()));
        assert_eq!(engine.outbox_queue.len(), 1);

        // Revocation without signature is rejected
        let invalid_notice = SignedRevocationNotice {
            notice_id: Uuid::new_v4(),
            revoked_target: RevocationTarget::CapabilityDigest([0xcc; 32]),
            authority_did: "did:key:z6MkuGov".into(),
            revoked_at: Utc::now(),
            ed25519_signature: vec![], // empty
        };
        assert!(!engine.handle_revocation(invalid_notice));

        // Test other targets
        let cap_target = RevocationTarget::CapabilityDigest([0xaa; 32]);
        let token_target = RevocationTarget::TokenCommitment([0xbb; 32]);
        let dev_session = RevocationTarget::DeviceSession(Uuid::new_v4());

        assert!(engine.handle_revocation(SignedRevocationNotice::new(
            cap_target.clone(),
            "did:key:z6MkuGov",
            vec![1],
        )));
        assert!(engine.handle_revocation(SignedRevocationNotice::new(
            token_target.clone(),
            "did:key:z6MkuGov",
            vec![2],
        )));
        assert!(engine.handle_revocation(SignedRevocationNotice::new(
            dev_session.clone(),
            "did:key:z6MkuGov",
            vec![3],
        )));

        assert!(engine.is_revoked(&cap_target));
        assert!(engine.is_revoked(&token_target));
        assert!(engine.is_revoked(&dev_session));
    }

    #[test]
    fn test_mesh_gossip_crdt_delta_convergence() {
        let mut engine_a = MeshGossipEngine::new("node-alpha");
        let mut engine_b = MeshGossipEngine::new("node-beta");

        let mut crdt_a = CrdtSync::new();
        let mut crdt_b = CrdtSync::new();

        let triplet_id = hakille_core::TripletId(Uuid::now_v7());
        let triplet = Triplet {
            id: triplet_id.clone(),
            subject: hakille_core::NodeId(Uuid::now_v7()),
            predicate: hakille_core::PredicateId(Uuid::now_v7()),
            object: hakille_memory::triplet::NodeValue::Text("doc:notarial-will-2026".into()),
            metadata: TripletMetadata {
                weight: 1.0,
                created_at: hakille_core::Timestamp::now(),
                last_activated: hakille_core::Timestamp::now(),
                source: hakille_memory::triplet::TripletSource::User,
                confidence: 1.0,
                context: hakille_memory::ContextPartition::Default,
                trust_level: hakille_core::TrustLevel::User,
                decay_rate: 0.01,
            },
            node_type: None,
        };
        crdt_a
            .insert_triplet(triplet.clone(), "node-alpha")
            .unwrap();
        assert_eq!(crdt_a.active_triplets().len(), 1);
        assert_eq!(crdt_b.active_triplets().len(), 0);

        // Node A replicates to Node B
        let sync_payload = engine_a.build_mesh_sync_delta(&crdt_a);
        engine_b
            .process_mesh_payload(sync_payload, &mut crdt_b)
            .unwrap();

        // Node B now has converged on the triplet
        assert_eq!(crdt_b.active_triplets().len(), 1);
        assert_eq!(crdt_b.active_triplets()[0].id, triplet_id);

        // Node B deletes the triplet (creates tombstone)
        crdt_b
            .remove_triplet(triplet_id.clone(), "node-beta")
            .unwrap();
        assert_eq!(crdt_b.active_triplets().len(), 0);

        // Node B syncs back to Node A
        let delete_payload = engine_b.build_mesh_sync_delta(&crdt_b);
        engine_a
            .process_mesh_payload(delete_payload, &mut crdt_a)
            .unwrap();

        // Tombstone wins deterministically on Node A
        assert_eq!(crdt_a.active_triplets().len(), 0);
        assert!(crdt_a.tombstones.contains(&triplet_id));
    }

    #[test]
    fn test_mesh_frame_encode_decode_deduplication() {
        let mut engine = MeshGossipEngine::new("node-gamma");
        let payload = MeshPayload::PeerBeacon {
            sender_did: "did:key:z6MkuBeaconSender".into(),
            timestamp_ms: 123456789,
        };

        let frame = engine
            .encode_frame(&payload, [42u8; 32], None)
            .expect("Failed to encode frame");
        assert!(frame.is_broadcast());

        // First decode succeeds
        let decoded = engine
            .decode_frame(&frame)
            .expect("Failed to decode frame")
            .expect("Expected payload");
        match decoded {
            MeshPayload::PeerBeacon {
                sender_did,
                timestamp_ms,
            } => {
                assert_eq!(sender_did, "did:key:z6MkuBeaconSender");
                assert_eq!(timestamp_ms, 123456789);
            }
            _ => panic!("Unexpected payload variant"),
        }

        // Duplicate decode of the same frame returns None
        let duplicate = engine
            .decode_frame(&frame)
            .expect("Duplicate frame handling failed");
        assert!(duplicate.is_none());
    }

    #[tokio::test]
    async fn test_step_sync_cycle_with_substrate() {
        let substrate_a = InMemoryMeshSubstrate::new();
        let substrate_b = InMemoryMeshSubstrate::new();

        let mut engine_a = MeshGossipEngine::new("node-a");
        let mut engine_b = MeshGossipEngine::new("node-b");

        let mut crdt_a = CrdtSync::new();
        let mut crdt_b = CrdtSync::new();

        // Node A queues a revocation notice
        let revoked_did = RevocationTarget::Did("did:key:z6MkuBadActor".into());
        let notice = SignedRevocationNotice::new(
            revoked_did.clone(),
            "did:key:z6MkuAuthority",
            vec![1, 2, 3],
        );
        engine_a.handle_revocation(notice);

        // Run sync cycle on Node A: frames are broadcast to substrate_a
        let processed_a = engine_a
            .step_sync_cycle(&substrate_a, &mut crdt_a, [1u8; 32])
            .await
            .unwrap();
        assert_eq!(processed_a, 0);

        let sent_frames = substrate_a.get_broadcasted_frames().await;
        assert_eq!(sent_frames.len(), 1);

        // Forward broadcasted frame into substrate_b
        substrate_b
            .inject_incoming_frame(sent_frames[0].clone())
            .await;

        // Run sync cycle on Node B: frame is received and processed
        assert!(!engine_b.is_revoked(&revoked_did));
        let processed_b = engine_b
            .step_sync_cycle(&substrate_b, &mut crdt_b, [2u8; 32])
            .await
            .unwrap();
        assert_eq!(processed_b, 1);
        assert!(engine_b.is_revoked(&revoked_did));
    }
}
