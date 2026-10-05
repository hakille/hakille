pub mod cluster;
pub mod edge;

use annora_core::TripletId;

pub trait VectorIndex: Send + Sync {
    fn insert(&mut self, id: TripletId, embedding: Vec<f32>) -> annora_core::Result<()>;
    fn search(&self, query: &[f32], k: usize) -> annora_core::Result<Vec<(TripletId, f32)>>;
    fn remove(&mut self, id: &TripletId) -> annora_core::Result<()>;
}
