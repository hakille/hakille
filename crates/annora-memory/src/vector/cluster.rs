use crate::vector::VectorIndex;
use annora_core::TripletId;
use std::collections::HashMap;

#[derive(Default)]
pub struct ClusterVectorStore {
    embeddings: HashMap<TripletId, Vec<f32>>,
}

impl ClusterVectorStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl VectorIndex for ClusterVectorStore {
    fn insert(&mut self, id: TripletId, embedding: Vec<f32>) -> annora_core::Result<()> {
        self.embeddings.insert(id, embedding);
        Ok(())
    }

    fn search(&self, query: &[f32], k: usize) -> annora_core::Result<Vec<(TripletId, f32)>> {
        let mut scored: Vec<(TripletId, f32)> = self.embeddings.iter().map(|(id, emb)| {
            let sim = cosine_sim(query, emb);
            (id.clone(), sim)
        }).collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        Ok(scored)
    }

    fn remove(&mut self, id: &TripletId) -> annora_core::Result<()> {
        self.embeddings.remove(id);
        Ok(())
    }
}

fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}
