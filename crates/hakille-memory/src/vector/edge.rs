/// Store vectoriel côté edge : alias du store unifié
/// [`crate::vector::BruteForceVectorStore`] (factorisation — zéro duplication).
///
/// Compatibilité : toutes les APIs publiques historiques restent accessibles
/// via cet alias sans import supplémentaire :
/// `new/insert/search/remove/dimension/len/is_empty` (trait
/// [`crate::vector::VectorIndex`] pour insert/search/remove/batch_insert,
/// méthodes inhérentes pour new/dimension/len/is_empty/search_with_scores),
/// plus `insert_triplet_embedding`, `link_triplet_embedding`, `batch_insert`.
///
/// Sémantique (voir le store unifié pour le détail documenté) : brute-force
/// O(N) exact, `k == 0` ⇒ `Ok(vec![])`, `k > N` ⇒ N résultats, `remove` ne
/// relâche le verrou de dimension que si le store redevient vide.
pub type EdgeVectorStore = crate::vector::BruteForceVectorStore;

#[cfg(test)]
mod tests {
    use super::EdgeVectorStore;
    use crate::vector::VectorIndex;
    use hakille_core::TripletId;
    use uuid::Uuid;

    #[test]
    fn test_edge_alias_public_api_intact() {
        let mut store = EdgeVectorStore::new();
        assert!(store.is_empty());
        assert_eq!(store.dimension(), None);
        assert_eq!(store.len(), 0);

        let id1 = TripletId(Uuid::now_v7());
        let id2 = TripletId(Uuid::now_v7());
        store.insert(id1.clone(), vec![1.0, 0.0, 0.0]).unwrap();
        store.insert(id2.clone(), vec![0.0, 1.0, 0.0]).unwrap();
        assert_eq!(store.dimension(), Some(3));
        assert_eq!(store.len(), 2);

        // k = 0 ⇒ [] sans panique ; k > N ⇒ N.
        assert!(store.search(&[1.0, 0.0, 0.0], 0).unwrap().is_empty());
        assert_eq!(store.search(&[1.0, 0.0, 0.0], 99).unwrap().len(), 2);

        let top = store.search(&[1.0, 0.0, 0.0], 1).unwrap();
        assert_eq!(top[0].0, id1);

        // search_with_scores + batch_insert via l'alias.
        let scored = store.search_with_scores(&[1.0, 0.0, 0.0], 2).unwrap();
        assert_eq!(scored.len(), 2);
        store
            .batch_insert(vec![(TripletId(Uuid::now_v7()), vec![0.0, 0.0, 1.0])])
            .unwrap();
        assert_eq!(store.len(), 3);

        store.remove(&id1).unwrap();
        store.remove(&id2).unwrap();
        // Verrou relâché seulement une fois vide (documenté) : ici il reste
        // 1 embedding, donc la dimension est toujours verrouillée.
        assert_eq!(store.dimension(), Some(3));
    }
}
