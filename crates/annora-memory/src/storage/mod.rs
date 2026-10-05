pub mod cluster;
pub mod edge;

use async_trait::async_trait;
use crate::triplet::{TriplePattern, Triplet};
use annora_core::TripletId;

#[async_trait]
pub trait GraphStorage: Send + Sync {
    async fn store(&self, triplet: &Triplet) -> annora_core::Result<()>;
    async fn load(&self, id: &TripletId) -> annora_core::Result<Option<Triplet>>;
    async fn query(&self, pattern: &TriplePattern) -> annora_core::Result<Vec<Triplet>>;
    async fn delete(&self, id: &TripletId) -> annora_core::Result<()>;
    async fn all_triplets(&self) -> annora_core::Result<Vec<Triplet>>;
}
