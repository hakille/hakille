//! Mock in-memory du tier "cluster" (`ClusterStore`).
//!
//! Honnêteté documentée : ce N'EST PAS un backend TiKV ni un stockage
//! distribué — c'est un simple `HashMap` protégé par `RwLock`, destiné aux
//! tests et au développement local sans redb. Aucune persistance disque,
//! aucune réplication, aucune tolérance aux pannes réseau/partitions.
//! - `batch_store()` prend un seul verrou d'écriture (atomicité limitée au
//!   processus, pas de transaction distribuée).
//! - `flush()` est un no-op assumé (rien à flusher en RAM).
//! - Les `PoisonError` de verrou sont convertis en `HakilleError::Internal`
//!   avec le nom de l'opération en contexte (jamais de message nu).

use crate::storage::GraphStorage;
use crate::triplet::{TriplePattern, Triplet};
use async_trait::async_trait;
use hakille_core::TripletId;
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Default)]
pub struct ClusterStore {
    data: RwLock<HashMap<TripletId, Triplet>>,
}

impl ClusterStore {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Convertit un empoisonnement de verrou en erreur interne contextualisée
/// (opération + détail), au lieu d'un `to_string()` nu inexploitable.
#[inline]
fn poisoned_lock(op: &'static str, detail: String) -> hakille_core::HakilleError {
    hakille_core::HakilleError::Internal(format!(
        "ClusterStore::{op}: verrou empoisonné (un holder précédent a paniqué) : {detail}"
    ))
}

#[async_trait]
impl GraphStorage for ClusterStore {
    async fn store(&self, triplet: &Triplet) -> hakille_core::Result<()> {
        let mut data = self
            .data
            .write()
            .map_err(|e| poisoned_lock("store", e.to_string()))?;
        data.insert(triplet.id.clone(), triplet.clone());
        Ok(())
    }

    /// Insertion du lot sous UN seul verrou d'écriture : pas d'état
    /// partiel observable par les lecteurs concurrents (atomicité
    /// processus-local, sans transaction distribuée — voir doc module).
    async fn batch_store(&self, triplets: Vec<Triplet>) -> hakille_core::Result<()> {
        if triplets.is_empty() {
            return Ok(());
        }
        let mut data = self
            .data
            .write()
            .map_err(|e| poisoned_lock("batch_store", e.to_string()))?;
        for t in &triplets {
            data.insert(t.id.clone(), t.clone());
        }
        Ok(())
    }

    /// Compte O(1) via `HashMap::len()`.
    async fn count(&self) -> hakille_core::Result<usize> {
        let data = self
            .data
            .read()
            .map_err(|e| poisoned_lock("count", e.to_string()))?;
        Ok(data.len())
    }

    /// No-op assumé : mock volatil, rien à rendre durable (voir doc module).
    async fn flush(&self) -> hakille_core::Result<()> {
        Ok(())
    }

    async fn load(&self, id: &TripletId) -> hakille_core::Result<Option<Triplet>> {
        let data = self
            .data
            .read()
            .map_err(|e| poisoned_lock("load", e.to_string()))?;
        Ok(data.get(id).cloned())
    }

    async fn query(&self, pattern: &TriplePattern) -> hakille_core::Result<Vec<Triplet>> {
        let data = self
            .data
            .read()
            .map_err(|e| poisoned_lock("query", e.to_string()))?;
        let matches = data
            .values()
            .filter(|t| {
                let s_match = pattern.subject.as_ref().is_none_or(|s| s == &t.subject);
                let p_match = pattern.predicate.as_ref().is_none_or(|p| p == &t.predicate);
                let o_match = pattern.object.as_ref().is_none_or(|o| o == &t.object);
                s_match && p_match && o_match
            })
            .cloned()
            .collect();
        Ok(matches)
    }

    async fn delete(&self, id: &TripletId) -> hakille_core::Result<()> {
        let mut data = self
            .data
            .write()
            .map_err(|e| poisoned_lock("delete", e.to_string()))?;
        data.remove(id);
        Ok(())
    }

    async fn all_triplets(&self) -> hakille_core::Result<Vec<Triplet>> {
        let data = self
            .data
            .read()
            .map_err(|e| poisoned_lock("all_triplets", e.to_string()))?;
        Ok(data.values().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mini-exécuteur synchrone (même rationale que dans `edge.rs` : pas de
    /// runtime async en dépendance de test).
    fn block_on<F: std::future::Future>(mut f: F) -> F::Output {
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
        fn clone_raw(_: *const ()) -> RawWaker {
            RawWaker::new(std::ptr::null(), raw_vtable())
        }
        fn noop_raw(_: *const ()) {}
        fn raw_vtable() -> &'static RawWakerVTable {
            &RawWakerVTable::new(clone_raw, noop_raw, noop_raw, noop_raw)
        }
        let waker = unsafe { Waker::from_raw(clone_raw(std::ptr::null())) };
        let mut cx = Context::from_waker(&waker);
        // SAFETY : future épinglé sur la pile, jamais déplacé après pin.
        let mut pinned = unsafe { std::pin::Pin::new_unchecked(&mut f) };
        loop {
            match pinned.as_mut().poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    fn sample(id: u128, subject: u128) -> Triplet {
        use crate::triplet::{NodeType, NodeValue, TripletMetadata, TripletSource};
        Triplet {
            id: hakille_core::TripletId(uuid::Uuid::from_u128(id)),
            subject: hakille_core::NodeId(uuid::Uuid::from_u128(subject)),
            predicate: hakille_core::PredicateId(uuid::Uuid::from_u128(0xB0B)),
            object: NodeValue::Text(format!("objet-{id:x}")),
            node_type: Some(NodeType::Fact),
            metadata: TripletMetadata {
                weight: 1.0,
                created_at: hakille_core::Timestamp::now(),
                last_activated: hakille_core::Timestamp::now(),
                source: TripletSource::User,
                confidence: 1.0,
                context: crate::ContextPartition::Personal,
                trust_level: hakille_core::TrustLevel::User,
                decay_rate: 0.01,
            },
        }
    }

    #[test]
    fn test_cluster_batch_count_flush_delete() {
        let store = ClusterStore::new();
        assert_eq!(block_on(store.count()).expect("count initial"), 0);

        // Lot vide : no-op.
        block_on(store.batch_store(Vec::new())).expect("batch vide");
        assert_eq!(block_on(store.count()).expect("count"), 0);

        // Batch de 2 sous un seul verrou.
        let lot = vec![sample(0xC1D, 0xA11CE), sample(0xC1E, 0xA11CE)];
        block_on(store.batch_store(lot.clone())).expect("batch_store");
        assert_eq!(block_on(store.count()).expect("count"), 2);
        for t in &lot {
            assert_eq!(
                block_on(store.load(&t.id)).expect("load").expect("présent"),
                *t
            );
        }

        // Requête par sujet sur le mock.
        let pattern = TriplePattern {
            subject: Some(lot[0].subject.clone()),
            predicate: None,
            object: None,
        };
        assert_eq!(block_on(store.query(&pattern)).expect("query").len(), 2);

        // flush no-op Ok, delete, count final.
        block_on(store.flush()).expect("flush");
        block_on(store.delete(&lot[0].id)).expect("delete");
        assert_eq!(block_on(store.count()).expect("count"), 1);
        assert_eq!(block_on(store.all_triplets()).expect("all").len(), 1);
    }

    #[test]
    fn test_poisoned_lock_carries_operation_context() {
        let msg = poisoned_lock("store", "verrou test".to_string()).to_string();
        assert!(
            msg.contains("ClusterStore::store"),
            "le contexte d'opération doit figurer dans le message, obtenu : {msg}"
        );
    }
}
