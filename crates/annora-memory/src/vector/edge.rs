use crate::vector::VectorIndex;
use annora_core::TripletId;
use std::collections::HashMap;

#[derive(Default)]
pub struct EdgeVectorStore {
    embeddings: HashMap<TripletId, Vec<f32>>,
}

impl EdgeVectorStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl VectorIndex for EdgeVectorStore {
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

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_edge_vector_store_search_and_ranking() {
        let mut store = EdgeVectorStore::new();

        let id1 = TripletId(Uuid::now_v7());
        let id2 = TripletId(Uuid::now_v7());
        let id3 = TripletId(Uuid::now_v7());

        // Exact match with query [1.0, 0.0, 0.0]
        store.insert(id1.clone(), vec![1.0, 0.0, 0.0]).unwrap();
        // Partially aligned [0.7, 0.7, 0.0]
        store.insert(id2.clone(), vec![0.7, 0.7, 0.0]).unwrap();
        // Orthogonal [0.0, 1.0, 0.0]
        store.insert(id3.clone(), vec![0.0, 1.0, 0.0]).unwrap();

        let query = vec![1.0, 0.0, 0.0];
        let results = store.search(&query, 2).unwrap();

        assert_eq!(results.len(), 2);
        // Top 1 must be id1 with cosine similarity ~ 1.0
        assert_eq!(results[0].0, id1);
        assert!((results[0].1 - 1.0).abs() < 1e-4);

        // Top 2 must be id2
        assert_eq!(results[1].0, id2);

        // Remove id1
        store.remove(&id1).unwrap();
        let results_after = store.search(&query, 2).unwrap();
        assert_eq!(results_after[0].0, id2);
    }
}

