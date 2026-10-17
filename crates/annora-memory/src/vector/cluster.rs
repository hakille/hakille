/// Store vectoriel côté cluster : alias du store unifié
/// [`crate::vector::BruteForceVectorStore`] (factorisation — zéro duplication).
///
/// Compatibilité : mêmes APIs publiques que `EdgeVectorStore` via cet alias :
/// `new/insert/search/remove/dimension/len/is_empty` +
/// `search_with_scores`, `insert_triplet_embedding`, `link_triplet_embedding`,
/// `batch_insert`. Même sémantique exacte + validation de dimension (voir le
/// store unifié : brute-force O(N), `k == 0` ⇒ `[]`, `k > N` ⇒ N, `remove`
/// ne relâche `expected_dim` que si vide — comportement voulu).
pub type ClusterVectorStore = crate::vector::BruteForceVectorStore;

#[cfg(test)]
mod tests {
    use super::ClusterVectorStore;
    use crate::vector::VectorIndex;
    use annora_core::TripletId;
    use uuid::Uuid;

    #[test]
    fn test_cluster_alias_public_api_intact() {
        let mut store = ClusterVectorStore::new();
        assert!(store.is_empty());

        let id1 = TripletId(Uuid::now_v7());
        let id2 = TripletId(Uuid::now_v7());
        store.insert(id1.clone(), vec![1.0, 0.0]).unwrap();
        store.insert(id2.clone(), vec![0.0, 1.0]).unwrap();
        assert_eq!(store.dimension(), Some(2));
        assert_eq!(store.len(), 2);

        let results = store.search(&[1.0, 0.0], 2).unwrap();
        assert_eq!(results[0].0, id1);
        assert!((results[0].1 - 1.0).abs() < 1e-4);
        assert_eq!(results[1].0, id2);

        // k = 0 ⇒ [] ; k > N ⇒ N.
        assert!(store.search(&[1.0, 0.0], 0).unwrap().is_empty());
        assert_eq!(store.search(&[1.0, 0.0], 50).unwrap().len(), 2);

        // Dimension mismatch rejeté des deux côtés (insert + search).
        assert!(store.insert(TripletId(Uuid::now_v7()), vec![1.0]).is_err());
        assert!(store.search(&[1.0], 1).is_err());
    }
}
