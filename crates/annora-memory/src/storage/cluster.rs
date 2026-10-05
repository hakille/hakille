use crate::storage::GraphStorage;
use crate::triplet::{TriplePattern, Triplet};
use annora_core::TripletId;
use async_trait::async_trait;
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

#[async_trait]
impl GraphStorage for ClusterStore {
    async fn store(&self, triplet: &Triplet) -> annora_core::Result<()> {
        let mut data = self.data.write().map_err(|e| annora_core::AnnoraError::Internal(e.to_string()))?;
        data.insert(triplet.id.clone(), triplet.clone());
        Ok(())
    }

    async fn load(&self, id: &TripletId) -> annora_core::Result<Option<Triplet>> {
        let data = self.data.read().map_err(|e| annora_core::AnnoraError::Internal(e.to_string()))?;
        Ok(data.get(id).cloned())
    }

    async fn query(&self, pattern: &TriplePattern) -> annora_core::Result<Vec<Triplet>> {
        let data = self.data.read().map_err(|e| annora_core::AnnoraError::Internal(e.to_string()))?;
        let matches = data.values().filter(|t| {
            let s_match = pattern.subject.as_ref().map_or(true, |s| s == &t.subject);
            let p_match = pattern.predicate.as_ref().map_or(true, |p| p == &t.predicate);
            let o_match = pattern.object.as_ref().map_or(true, |o| o == &t.object);
            s_match && p_match && o_match
        }).cloned().collect();
        Ok(matches)
    }

    async fn delete(&self, id: &TripletId) -> annora_core::Result<()> {
        let mut data = self.data.write().map_err(|e| annora_core::AnnoraError::Internal(e.to_string()))?;
        data.remove(id);
        Ok(())
    }

    async fn all_triplets(&self) -> annora_core::Result<Vec<Triplet>> {
        let data = self.data.read().map_err(|e| annora_core::AnnoraError::Internal(e.to_string()))?;
        Ok(data.values().cloned().collect())
    }
}
