use crate::crdt::{CrdtSync, VectorClock};
use hakille_core::{EncryptedBlob, Result, Timestamp, TripletId, decrypt_at_rest, encrypt_at_rest};
use hakille_memory::triplet::Triplet;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationDelta {
    pub new_triplets: Vec<Triplet>,
    pub new_tombstones: Vec<TripletId>,
    /// Horodatages connus des tombstones (aucune purge sans preuve de convergence).
    /// `#[serde(default)]` pour compatibilité avec les deltas pré-LWW.
    #[serde(default)]
    pub tombstone_times: HashMap<TripletId, Timestamp>,
    pub clock: VectorClock,
}

pub struct ReplicationEngine;

impl ReplicationEngine {
    /// État complet conservateur. Les anciens paramètres de différentiel sont
    /// conservés pour compatibilité mais ignorés : ni created_at, ni une horloge
    /// globale égale ne prouvent que le pair possède chaque contenu courant.
    /// Un différentiel sûr nécessiterait des versions/ACK par opération.
    pub fn build_delta(
        sync_state: &CrdtSync,
        _from_clock: Option<&VectorClock>,
        _since: Option<Timestamp>,
    ) -> ReplicationDelta {
        let mut new_triplets: Vec<_> = sync_state.active_triplets().into_iter().cloned().collect();
        new_triplets.sort_by(|left, right| left.id.cmp(&right.id));
        let mut new_tombstones: Vec<_> = sync_state.tombstones.iter().cloned().collect();
        new_tombstones.sort();
        let tombstone_times = sync_state
            .tombstone_times
            .iter()
            .filter(|(id, _)| sync_state.tombstones.contains(id))
            .map(|(id, time)| (id.clone(), time.clone()))
            .collect();
        ReplicationDelta {
            new_triplets,
            new_tombstones,
            tombstone_times,
            clock: sync_state.vector_clock.clone(),
        }
    }

    /// Full-state chiffré (compatibilité) : équivaut à
    /// `build_delta(state, None, None)` puis chiffrement.
    pub fn build_encrypted_delta(
        sync_state: &CrdtSync,
        encryption_key: &[u8],
    ) -> Result<EncryptedBlob> {
        // Full-state : clock absent ⇒ on envoie tout (première sync ou
        // reprise sans horloge ; voir `build_delta`).
        let delta = Self::build_delta(sync_state, None, None);
        Self::encrypt_delta(&delta, encryption_key)
    }

    /// État complet chiffré ; paramètres historiques conservés mais aucun
    /// différentiel fiable n'est déduit de la date ou de l'horloge globale.
    pub fn build_encrypted_delta_from(
        sync_state: &CrdtSync,
        encryption_key: &[u8],
        from_clock: Option<&VectorClock>,
        since: Option<Timestamp>,
    ) -> Result<EncryptedBlob> {
        let delta = Self::build_delta(sync_state, from_clock, since);
        Self::encrypt_delta(&delta, encryption_key)
    }

    fn encrypt_delta(delta: &ReplicationDelta, encryption_key: &[u8]) -> Result<EncryptedBlob> {
        for triplet in &delta.new_triplets {
            crate::crdt::canonical_triplet(triplet)?;
        }
        let serialized = serde_json::to_vec(&delta)
            .map_err(|e| hakille_core::HakilleError::SerializationError(e.to_string()))?;
        if serialized.len() > 32 * 1024 * 1024 {
            return Err(hakille_core::HakilleError::SerializationError(
                "État de réplication limité à 32 Mio".into(),
            ));
        }
        encrypt_at_rest(&serialized, encryption_key)
    }

    /// Applique un delta chiffré via la fusion LWW [`CrdtSync::merge`]
    /// (jamais d'insert direct : la suppression gagne sur la création
    /// concurrente, le plus récent gagne à ID égal, les horodatages de
    /// tombstones sont propagés avec conservation du plus ancien).
    pub fn apply_encrypted_delta(
        sync_state: &mut CrdtSync,
        encrypted_blob: &EncryptedBlob,
        decryption_key: &[u8],
    ) -> Result<()> {
        if encrypted_blob.ciphertext.len() > 32 * 1024 * 1024 + 16 {
            return Err(hakille_core::HakilleError::SerializationError(
                "État de réplication limité à 32 Mio".into(),
            ));
        }
        let decrypted = decrypt_at_rest(encrypted_blob, decryption_key)?;
        let delta: ReplicationDelta = serde_json::from_slice(&decrypted)
            .map_err(|e| hakille_core::HakilleError::SerializationError(e.to_string()))?;

        // Reconstruit un état distant minimal puis fusionne (LWW + tombstones).
        let mut remote = CrdtSync::new();
        for triplet in delta.new_triplets {
            if remote
                .elements
                .insert(triplet.id.clone(), triplet)
                .is_some()
            {
                return Err(hakille_core::HakilleError::SerializationError(
                    "ID de triplet dupliqué dans le delta".into(),
                ));
            }
        }
        for tombstone in delta.new_tombstones {
            if !remote.tombstones.insert(tombstone.clone()) {
                return Err(hakille_core::HakilleError::SerializationError(
                    "Tombstone dupliquée dans le delta".into(),
                ));
            }
            // Une date absente reste inconnue, sans dépendance à l'horloge locale.
            if let Some(time) = delta.tombstone_times.get(&tombstone) {
                remote.tombstone_times.insert(tombstone, time.clone());
            }
        }
        remote.vector_clock = delta.clock;
        sync_state.merge(&remote)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_core::{NodeId, PredicateId, TrustLevel};
    use hakille_memory::ContextPartition;
    use hakille_memory::triplet::{NodeValue, TripletMetadata, TripletSource};
    use uuid::Uuid;

    fn triplet_at(text: &str, when: chrono::DateTime<chrono::Utc>) -> Triplet {
        Triplet {
            id: TripletId(Uuid::now_v7()),
            subject: NodeId(Uuid::now_v7()),
            predicate: PredicateId(Uuid::now_v7()),
            object: NodeValue::Text(text.to_string()),
            metadata: TripletMetadata {
                weight: 1.0,
                created_at: Timestamp(when),
                last_activated: Timestamp(when),
                source: TripletSource::User,
                confidence: 1.0,
                context: ContextPartition::Default,
                trust_level: TrustLevel::User,
                decay_rate: 0.01,
            },
            node_type: None,
        }
    }

    #[test]
    fn test_delta_full_state_when_clock_absent() {
        let mut state = CrdtSync::new();
        state
            .insert_triplet(
                triplet_at("a", chrono::Utc::now() - chrono::Duration::hours(2)),
                "n1",
            )
            .unwrap();
        state
            .insert_triplet(triplet_at("b", chrono::Utc::now()), "n1")
            .unwrap();
        // Clock absent ⇒ full-state : tout est envoyé.
        let delta = ReplicationEngine::build_delta(&state, None, None);
        assert_eq!(delta.new_triplets.len(), 2);
    }

    #[test]
    fn test_delta_since_is_not_a_proof_of_complete_replication() {
        let mut state = CrdtSync::new();
        state
            .insert_triplet(
                triplet_at("vieux", chrono::Utc::now() - chrono::Duration::hours(2)),
                "n1",
            )
            .unwrap();
        state
            .insert_triplet(triplet_at("neuf", chrono::Utc::now()), "n1")
            .unwrap();
        let since = Timestamp(chrono::Utc::now() - chrono::Duration::hours(1));
        let remote_clock = state.vector_clock.clone();
        // Une date de création ne suffit pas : envoyer aussi les anciens contenus.
        let delta = ReplicationEngine::build_delta(&state, Some(&remote_clock), Some(since));
        assert_eq!(delta.new_triplets.len(), 2);
    }

    #[test]
    fn test_delta_equal_global_clock_is_not_a_content_ack() {
        let mut state = CrdtSync::new();
        state
            .insert_triplet(triplet_at("a", chrono::Utc::now()), "n1")
            .unwrap();
        let remote_clock = state.vector_clock.clone();
        // Même horloge globale : aucun accusé de réception de contenu prouvé.
        let delta = ReplicationEngine::build_delta(&state, Some(&remote_clock), None);
        assert_eq!(delta.new_triplets.len(), 1);
    }

    #[test]
    fn test_encrypted_delta_roundtrip() {
        let mut state = CrdtSync::new();
        state
            .insert_triplet(triplet_at("hello", chrono::Utc::now()), "n1")
            .unwrap();
        let key = [42u8; 32];
        let blob = ReplicationEngine::build_encrypted_delta(&state, &key).unwrap();
        let mut peer = CrdtSync::new();
        ReplicationEngine::apply_encrypted_delta(&mut peer, &blob, &key).unwrap();
        assert_eq!(peer.active_triplets().len(), 1);
    }
    #[test]
    fn old_creation_date_does_not_hide_a_changed_content() {
        let old = chrono::Utc::now() - chrono::Duration::days(30);
        let mut triplet = triplet_at("old", old);
        let mut state = CrdtSync::new();
        state.insert_triplet(triplet.clone(), "node").unwrap();
        let clock = state.vector_clock.clone();
        triplet.object = NodeValue::Text("modified".into());
        triplet.metadata.last_activated = Timestamp::now();
        state.insert_triplet(triplet.clone(), "node").unwrap();
        let delta = ReplicationEngine::build_delta(&state, Some(&clock), Some(Timestamp::now()));
        assert_eq!(delta.new_triplets, vec![triplet]);
    }

    #[test]
    fn duplicate_wire_ids_fail_without_mutating_receiver() {
        let triplet = triplet_at("one", chrono::Utc::now());
        let mut duplicate = triplet.clone();
        duplicate.object = NodeValue::Text("two".into());
        let delta = ReplicationDelta {
            new_triplets: vec![triplet, duplicate],
            new_tombstones: vec![],
            tombstone_times: HashMap::new(),
            clock: VectorClock::new(),
        };
        let key = [7; 32];
        let encrypted = ReplicationEngine::encrypt_delta(&delta, &key).unwrap();
        let mut state = CrdtSync::new();
        assert!(ReplicationEngine::apply_encrypted_delta(&mut state, &encrypted, &key).is_err());
        assert!(state.elements.is_empty() && state.tombstones.is_empty());
        assert!(state.vector_clock.clock.is_empty());
    }
}
