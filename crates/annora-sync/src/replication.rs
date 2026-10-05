use crate::crdt::{CrdtSync, VectorClock};
use annora_core::{decrypt_at_rest, encrypt_at_rest, EncryptedBlob, Result, TripletId};
use annora_memory::triplet::Triplet;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationDelta {
    pub new_triplets: Vec<Triplet>,
    pub new_tombstones: Vec<TripletId>,
    pub clock: VectorClock,
}

pub struct ReplicationEngine;

impl ReplicationEngine {
    pub fn build_encrypted_delta(
        sync_state: &CrdtSync,
        encryption_key: &[u8],
    ) -> Result<EncryptedBlob> {
        let delta = ReplicationDelta {
            new_triplets: sync_state.active_triplets().into_iter().cloned().collect(),
            new_tombstones: sync_state.tombstones.iter().cloned().collect(),
            clock: sync_state.vector_clock.clone(),
        };

        let serialized = serde_json::to_vec(&delta)
            .map_err(|e| annora_core::AnnoraError::SerializationError(e.to_string()))?;

        encrypt_at_rest(&serialized, encryption_key)
    }

    pub fn apply_encrypted_delta(
        sync_state: &mut CrdtSync,
        encrypted_blob: &EncryptedBlob,
        decryption_key: &[u8],
    ) -> Result<()> {
        let decrypted = decrypt_at_rest(encrypted_blob, decryption_key)?;
        let delta: ReplicationDelta = serde_json::from_slice(&decrypted)
            .map_err(|e| annora_core::AnnoraError::SerializationError(e.to_string()))?;

        for triplet in delta.new_triplets {
            sync_state.elements.insert(triplet.id.clone(), triplet);
        }
        for tombstone in delta.new_tombstones {
            sync_state.tombstones.insert(tombstone);
        }
        sync_state.vector_clock.merge(&delta.clock);

        Ok(())
    }
}
