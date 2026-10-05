use crate::storage::GraphStorage;
use crate::triplet::{TriplePattern, Triplet};
use annora_core::{AnnoraError, TripletId};
use async_trait::async_trait;
use redb::{Database, ReadableTable, TableDefinition};
use std::sync::Arc;

pub const SPO_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("spo");
pub const POS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("pos");
pub const OSP_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("osp");
pub const ID_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("id_to_triplet");

pub struct EdgeStore {
    pub db: Arc<Database>,
}

impl EdgeStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl GraphStorage for EdgeStore {
    async fn store(&self, triplet: &Triplet) -> annora_core::Result<()> {
        let id_bytes = bincode::serialize(&triplet.id).map_err(|e| AnnoraError::Internal(e.to_string()))?;
        let triplet_bytes = bincode::serialize(triplet).map_err(|e| AnnoraError::Internal(e.to_string()))?;

        let write_txn = self.db.begin_write().map_err(|e| AnnoraError::Internal(e.to_string()))?;
        {
            let mut id_table = write_txn.open_table(ID_TABLE).map_err(|e| AnnoraError::Internal(e.to_string()))?;
            id_table.insert(id_bytes.as_slice(), triplet_bytes.as_slice()).map_err(|e| AnnoraError::Internal(e.to_string()))?;
        }
        write_txn.commit().map_err(|e| AnnoraError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn load(&self, id: &TripletId) -> annora_core::Result<Option<Triplet>> {
        let id_bytes = bincode::serialize(id).map_err(|e| AnnoraError::Internal(e.to_string()))?;
        let read_txn = self.db.begin_read().map_err(|e| AnnoraError::Internal(e.to_string()))?;
        let id_table = read_txn.open_table(ID_TABLE).map_err(|e| AnnoraError::Internal(e.to_string()))?;
        if let Some(entry) = id_table.get(id_bytes.as_slice()).map_err(|e| AnnoraError::Internal(e.to_string()))? {
            let triplet: Triplet = bincode::deserialize(entry.value()).map_err(|e| AnnoraError::Internal(e.to_string()))?;
            Ok(Some(triplet))
        } else {
            Ok(None)
        }
    }

    async fn query(&self, pattern: &TriplePattern) -> annora_core::Result<Vec<Triplet>> {
        let triplets = self.all_triplets().await?;
        let matches = triplets.into_iter().filter(|t| {
            let s_match = pattern.subject.as_ref().map_or(true, |s| s == &t.subject);
            let p_match = pattern.predicate.as_ref().map_or(true, |p| p == &t.predicate);
            let o_match = pattern.object.as_ref().map_or(true, |o| o == &t.object);
            s_match && p_match && o_match
        }).collect();
        Ok(matches)
    }

    async fn delete(&self, id: &TripletId) -> annora_core::Result<()> {
        let id_bytes = bincode::serialize(id).map_err(|e| AnnoraError::Internal(e.to_string()))?;
        let write_txn = self.db.begin_write().map_err(|e| AnnoraError::Internal(e.to_string()))?;
        {
            let mut id_table = write_txn.open_table(ID_TABLE).map_err(|e| AnnoraError::Internal(e.to_string()))?;
            id_table.remove(id_bytes.as_slice()).map_err(|e| AnnoraError::Internal(e.to_string()))?;
        }
        write_txn.commit().map_err(|e| AnnoraError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn all_triplets(&self) -> annora_core::Result<Vec<Triplet>> {
        let read_txn = self.db.begin_read().map_err(|e| AnnoraError::Internal(e.to_string()))?;
        let id_table = read_txn.open_table(ID_TABLE).map_err(|e| AnnoraError::Internal(e.to_string()))?;
        let mut results = Vec::new();
        for item in id_table.iter().map_err(|e| AnnoraError::Internal(e.to_string()))? {
            let (_k, v) = item.map_err(|e| AnnoraError::Internal(e.to_string()))?;
            let triplet: Triplet = bincode::deserialize(v.value()).map_err(|e| AnnoraError::Internal(e.to_string()))?;
            results.push(triplet);
        }
        Ok(results)
    }
}
