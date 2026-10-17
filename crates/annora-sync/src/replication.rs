use crate::crdt::{CrdtSync, VectorClock};
use annora_core::{EncryptedBlob, Result, Timestamp, TripletId, decrypt_at_rest, encrypt_at_rest};
use annora_memory::triplet::Triplet;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationDelta {
    pub new_triplets: Vec<Triplet>,
    pub new_tombstones: Vec<TripletId>,
    /// Horodatages des tombstones (propagés pour un GC équitable).
    /// `#[serde(default)]` pour compatibilité avec les deltas pré-LWW.
    #[serde(default)]
    pub tombstone_times: HashMap<TripletId, Timestamp>,
    pub clock: VectorClock,
}

pub struct ReplicationEngine;

impl ReplicationEngine {
    /// Construit un delta **différentiel** contre l'horloge du pair.
    ///
    /// - `from_clock = None` ⇒ **full-state** (l'état complet est envoyé) ;
    ///   c'est le repli honnête quand on ne sait pas ce que le pair a déjà
    ///   (première sync, horloge perdue).
    /// - `from_clock = Some` + `since = Some(t)` ⇒ différentiel temporel :
    ///   seuls les triplets avec `created_at > t` sont inclus. `t` est
    ///   l'heure du dernier sync réussi avec ce pair (les horloges
    ///   vectorielles seules n'ordonnent pas les créations murales, d'où ce
    ///   paramètre explicite et documenté).
    /// - `from_clock = Some` + `since = None` ⇒ repli conservateur
    ///   full-state **filtré par l'horloge** : si l'horloge distante domine
    ///   déjà la locale (rien de nouveau), delta vide ; sinon full-state
    ///   (on ne peut pas prouver le différentiel sans `since`).
    ///
    /// Les tombstones sont toujours envoyées en totalité (ensemble petit,
    /// sémantique de suppression prioritaire).
    pub fn build_delta(
        sync_state: &CrdtSync,
        from_clock: Option<&VectorClock>,
        since: Option<Timestamp>,
    ) -> ReplicationDelta {
        let all: Vec<Triplet> = sync_state.active_triplets().into_iter().cloned().collect();
        let new_triplets = match (from_clock, since) {
            // Différentiel temporel précis.
            (_, Some(since_ts)) => all
                .into_iter()
                .filter(|t| t.metadata.created_at.0 > since_ts.0)
                .collect(),
            // Sans `since` : si le pair est déjà à jour (horloges égales),
            // rien à envoyer ; sinon full-state conservateur.
            (Some(remote), None) => {
                if remote == &sync_state.vector_clock {
                    Vec::new()
                } else {
                    // Full-state : on ne peut pas calculer un diff fiable
                    // avec la seule horloge vectorielle (pas d'ordre mural).
                    all
                }
            }
            // Clock absent ⇒ full-state + commentaire (voir doc).
            (None, None) => all,
        };
        // Propage les horodatages des tombstones envoyées (GC équitable :
        // le pair garde le plus ancien via `CrdtSync::merge`).
        let tombstone_times: HashMap<TripletId, Timestamp> = sync_state
            .tombstones
            .iter()
            .map(|id| {
                let t = sync_state
                    .tombstone_time(id)
                    .cloned()
                    .unwrap_or_else(Timestamp::now);
                (id.clone(), t)
            })
            .collect();
        ReplicationDelta {
            new_triplets,
            new_tombstones: sync_state.tombstones.iter().cloned().collect(),
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

    /// Delta différentiel chiffré contre l'horloge du pair.
    ///
    /// `from_clock` vient typiquement de `SyncRequest.from_clock`
    /// (voir `protocol.rs`) ; `since` est l'heure du dernier sync réussi
    /// avec ce pair (`None` ⇒ repli full-state filtré par horloge).
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
        let serialized = serde_json::to_vec(&delta)
            .map_err(|e| annora_core::AnnoraError::SerializationError(e.to_string()))?;
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
        let decrypted = decrypt_at_rest(encrypted_blob, decryption_key)?;
        let delta: ReplicationDelta = serde_json::from_slice(&decrypted)
            .map_err(|e| annora_core::AnnoraError::SerializationError(e.to_string()))?;

        // Reconstruit un état distant minimal puis fusionne (LWW + tombstones).
        let mut remote = CrdtSync::new();
        for triplet in delta.new_triplets {
            remote.elements.insert(triplet.id.clone(), triplet);
        }
        for tombstone in delta.new_tombstones {
            remote.tombstones.insert(tombstone.clone());
            // Propage l'horodatage du delta ; `now` conservateur si absent
            // (delta pré-LWW) — `merge` garde ensuite le plus ancien.
            let t = delta
                .tombstone_times
                .get(&tombstone)
                .cloned()
                .unwrap_or_else(Timestamp::now);
            remote.tombstone_times.insert(tombstone, t);
        }
        remote.vector_clock = delta.clock;
        sync_state.merge(&remote);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use annora_core::{NodeId, PredicateId, TrustLevel};
    use annora_memory::ContextPartition;
    use annora_memory::triplet::{NodeValue, TripletMetadata, TripletSource};
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
        state.insert_triplet(
            triplet_at("a", chrono::Utc::now() - chrono::Duration::hours(2)),
            "n1",
        );
        state.insert_triplet(triplet_at("b", chrono::Utc::now()), "n1");
        // Clock absent ⇒ full-state : tout est envoyé.
        let delta = ReplicationEngine::build_delta(&state, None, None);
        assert_eq!(delta.new_triplets.len(), 2);
    }

    #[test]
    fn test_delta_differential_filters_by_created_at() {
        let mut state = CrdtSync::new();
        state.insert_triplet(
            triplet_at("vieux", chrono::Utc::now() - chrono::Duration::hours(2)),
            "n1",
        );
        state.insert_triplet(triplet_at("neuf", chrono::Utc::now()), "n1");
        let since = Timestamp(chrono::Utc::now() - chrono::Duration::hours(1));
        let remote_clock = state.vector_clock.clone();
        // Seul le triplet créé après `since` est inclus.
        let delta = ReplicationEngine::build_delta(&state, Some(&remote_clock), Some(since));
        assert_eq!(delta.new_triplets.len(), 1);
        match &delta.new_triplets[0].object {
            NodeValue::Text(s) => assert_eq!(s, "neuf"),
            _ => panic!("attendu du texte"),
        }
    }

    #[test]
    fn test_delta_empty_when_peer_up_to_date() {
        let mut state = CrdtSync::new();
        state.insert_triplet(triplet_at("a", chrono::Utc::now()), "n1");
        let remote_clock = state.vector_clock.clone();
        // Même horloge, pas de `since` ⇒ rien de nouveau (delta vide).
        let delta = ReplicationEngine::build_delta(&state, Some(&remote_clock), None);
        assert!(delta.new_triplets.is_empty());
    }

    #[test]
    fn test_encrypted_delta_roundtrip() {
        let mut state = CrdtSync::new();
        state.insert_triplet(triplet_at("hello", chrono::Utc::now()), "n1");
        let key = [42u8; 32];
        let blob = ReplicationEngine::build_encrypted_delta(&state, &key).unwrap();
        let mut peer = CrdtSync::new();
        ReplicationEngine::apply_encrypted_delta(&mut peer, &blob, &key).unwrap();
        assert_eq!(peer.active_triplets().len(), 1);
    }
}
