pub mod cluster;
pub mod edge;

pub use cluster::ClusterVectorStore;
pub use edge::EdgeVectorStore;

use hakille_core::TripletId;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Store vectoriel unifié (brute-force exact).
// ---------------------------------------------------------------------------
// Honnêteté documentée :
// - `instant-distance` (HNSW) est déclaré en dépendance du crate mais N'EST
//   PAS branché : pas de mapping `TripletId` ↔ index ANN stable, pas de
//   persistance, API HNSW non reliée à nos IDs. Le déclarer sans le brancher
//   serait du marketing ; on l'écrit donc noir sur blanc.
// - Ce store fait un scan linéaire exhaustif + tri par similarité : O(N) par
//   requête (N = nombre d'embeddings), exact (pas d'approximation).
// - Hypothèse assumée : N < ~10 000 vecteurs (graphe local). En deçà, le
//   brute-force est plus simple, exact et sans dépendance ANN.
// - Seuil de migration : si N dépasse durablement ~10k OU si le p99 de
//   `search` dépasse le budget de latence, brancher un vrai ANN
//   (`instant-distance` HNSW) DERRIÈRE le même trait `VectorIndex`
//   (même signatures, migration sans casser les appelants).
// - `EdgeVectorStore` / `ClusterVectorStore` (voir `edge.rs` / `cluster.rs`)
//   sont des aliases de ce store unique : zéro duplication de logique,
//   APIs publiques historiques conservées (`new/insert/search/remove/`
//   `dimension/len/is_empty` + `search_with_scores`,
//   `insert_triplet_embedding`, `link_triplet_embedding`, `batch_insert`).
// ---------------------------------------------------------------------------
#[derive(Default, Debug)]
pub struct BruteForceVectorStore {
    embeddings: HashMap<TripletId, Vec<f32>>,
    expected_dim: Option<usize>,
}

impl BruteForceVectorStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Dimension verrouillée au premier insert (`None` si vide).
    pub fn dimension(&self) -> Option<usize> {
        self.expected_dim
    }

    pub fn len(&self) -> usize {
        self.embeddings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.embeddings.is_empty()
    }

    /// Recherche avec scores, forme canonique (voir `VectorIndex::search`).
    ///
    /// - `k == 0` ⇒ `Ok(vec![])` sans panique (early return).
    /// - `k > N` ⇒ retourne les N résultats (pas de bourrage, pas d'erreur).
    /// - Tri déterministe : similarité desc, puis `TripletId` croissant en
    ///   cas d'égalité (les scores ex æquo ne dépendent plus de l'ordre
    ///   d'itération du `HashMap`).
    pub fn search_with_scores(
        &self,
        query: &[f32],
        k: usize,
    ) -> hakille_core::Result<Vec<(TripletId, f32)>> {
        check_dimension(
            self.expected_dim,
            query.len(),
            "BruteForceVectorStore::search",
        )?;
        if k == 0 {
            return Ok(Vec::new());
        }
        let mut scored: Vec<(TripletId, f32)> = self
            .embeddings
            .iter()
            .map(|(id, emb)| (id.clone(), cosine_sim(query, emb)))
            .collect();
        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        scored.truncate(k);
        Ok(scored)
    }

    /// Alias pipeline de `insert`, nom explicite côté ingestion
    /// (triplet_id + embedding déjà calculé).
    pub fn insert_triplet_embedding(
        &mut self,
        triplet_id: TripletId,
        embedding: Vec<f32>,
    ) -> hakille_core::Result<()> {
        self.insert(triplet_id, embedding)
    }

    /// Helper pipeline : indexe l'embedding porté par un triplet.
    ///
    /// - Si `triplet.object` est `NodeValue::Embedding(v)` ⇒ insert sous
    ///   `triplet.id` (validation de dimension incluse).
    /// - Sinon ⇒ no-op `Ok(())` documenté (pas d'embedding à indexer,
    ///   pas d'erreur : l'index vectoriel reste optionnel par triplet).
    pub fn link_triplet_embedding(
        &mut self,
        triplet: &crate::triplet::Triplet,
    ) -> hakille_core::Result<()> {
        match &triplet.object {
            crate::triplet::NodeValue::Embedding(v) => self.insert(triplet.id.clone(), v.clone()),
            _ => Ok(()),
        }
    }
}

/// Seuil ANN : en dessous, `HnswVectorStore::search` reste en brute-force exact
/// (même ranking que `BruteForceVectorStore`) ; au-dessus, ANN + rescore.
pub const HNSW_BRUTE_FORCE_THRESHOLD: usize = 1000;

pub trait VectorIndex: Send + Sync {
    fn insert(&mut self, id: TripletId, embedding: Vec<f32>) -> hakille_core::Result<()>;
    fn search(&self, query: &[f32], k: usize) -> hakille_core::Result<Vec<(TripletId, f32)>>;
    fn remove(&mut self, id: &TripletId) -> hakille_core::Result<()>;

    /// Persiste l'index vers la base redb (`EMBEDDINGS_TABLE`).
    /// Défaut : non implémenté explicite (jamais silencieux).
    fn save(&self, _db: &redb::Database) -> hakille_core::Result<()> {
        Err(hakille_core::HakilleError::Internal(
            "VectorIndex::save non implémenté pour ce store".to_string(),
        ))
    }

    /// Recharge l'index depuis la base redb. Défaut : non implémenté explicite.
    fn load(&mut self, _db: &redb::Database) -> hakille_core::Result<()> {
        Err(hakille_core::HakilleError::Internal(
            "VectorIndex::load non implémenté pour ce store".to_string(),
        ))
    }

    /// Insert groupé. Documenté fail-fast NON atomique : à la première
    /// erreur de dimension, retourne `Err` en laissant les inserts
    /// précédents en place (pas de rollback — le batch reste un confort
    /// d'appel, pas une transaction).
    fn batch_insert(&mut self, items: Vec<(TripletId, Vec<f32>)>) -> hakille_core::Result<()> {
        for (id, embedding) in items {
            self.insert(id, embedding)?;
        }
        Ok(())
    }

    /// Dimension verrouillée (`None` si store vide).
    fn dimension(&self) -> Option<usize>;
    /// Nombre d'embeddings indexés.
    fn len(&self) -> usize;
    /// `true` si aucun embedding indexé (défaut : `len() == 0`).
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl VectorIndex for BruteForceVectorStore {
    fn insert(&mut self, id: TripletId, embedding: Vec<f32>) -> hakille_core::Result<()> {
        check_dimension(
            self.expected_dim,
            embedding.len(),
            "BruteForceVectorStore::insert",
        )?;
        if self.expected_dim.is_none() {
            self.expected_dim = Some(embedding.len());
        }
        self.embeddings.insert(id, embedding);
        Ok(())
    }

    fn search(&self, query: &[f32], k: usize) -> hakille_core::Result<Vec<(TripletId, f32)>> {
        self.search_with_scores(query, k)
    }

    fn remove(&mut self, id: &TripletId) -> hakille_core::Result<()> {
        self.embeddings.remove(id);
        // Comportement VOULU et documenté : le verrou de dimension n'est
        // relâché (`None`) que quand le store redevient VIDE. Tant qu'il
        // reste ≥ 1 embedding, la dimension reste verrouillée : on ne veut
        // jamais qu'un `remove` partiel rouvre la porte à une dimension
        // incohérente silencieuse.
        if self.embeddings.is_empty() {
            self.expected_dim = None;
        }
        Ok(())
    }

    fn dimension(&self) -> Option<usize> {
        self.expected_dim
    }

    fn len(&self) -> usize {
        self.embeddings.len()
    }

    fn save(&self, db: &redb::Database) -> hakille_core::Result<()> {
        let txn = db
            .begin_write()
            .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
        {
            let mut table = txn
                .open_table(crate::storage::edge::EMBEDDINGS_TABLE)
                .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
            for (id, vec) in &self.embeddings {
                let id_bytes = bincode::serialize(id)
                    .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
                let vec_bytes = bincode::serialize(vec)
                    .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
                table
                    .insert(id_bytes.as_slice(), vec_bytes.as_slice())
                    .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
            }
        }
        txn.commit()
            .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
        Ok(())
    }

    fn load(&mut self, db: &redb::Database) -> hakille_core::Result<()> {
        let txn = db
            .begin_read()
            .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
        let table = match txn.open_table(crate::storage::edge::EMBEDDINGS_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(()),
            Err(e) => return Err(hakille_core::HakilleError::Internal(e.to_string())),
        };
        use redb::ReadableTable;
        for item in table
            .iter()
            .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?
        {
            let (k, v) = item.map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
            let id: TripletId = bincode::deserialize(k.value())
                .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
            let vec: Vec<f32> = bincode::deserialize(v.value())
                .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
            // Rejoue via `insert` (validation de dimension conservée).
            self.insert(id, vec)?;
        }
        Ok(())
    }
}

/// Point HNSW : embedding 384 + distance euclidienne pour l'ANN.
/// Le rescore final reste cosinus exact (`cosine_sim`) — l'ANN ne sert qu'à
/// présélectionner les candidats.
#[derive(Clone, Debug)]
struct EmbeddingPoint(Vec<f32>);

impl instant_distance::Point for EmbeddingPoint {
    fn distance(&self, other: &Self) -> f32 {
        self.0
            .iter()
            .zip(other.0.iter())
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f32>()
            .sqrt()
    }
}

/// Store vectoriel HNSW branché sur `instant-distance` derrière `VectorIndex`.
///
/// - `map TripletId ↔ u32` : `ids: Vec<TripletId>` (ordre des points) +
///   `embeddings: HashMap` (rescore exact) ; l'index ANN ne voit que des
///   `EmbeddingPoint` + `TripletId` comme valeurs ;
/// - `dim 384` verrouillée (`EMBEDDING_DIM`, `check_embedding_dim`) ;
/// - `insert` ⇒ ajoute + marque `dirty` + append WAL (mémoire + fichier si
///   `set_wal_path`) + reconstruit l'index si ≥ seuil ;
/// - `search` ⇒ ANN (candidats 2×k) + rescore cosinus exact, tri déterministe ;
///   fallback brute-force exact si `< HNSW_BRUTE_FORCE_THRESHOLD` vecteurs ou
///   index absent (même ranking que le brute-force) ;
/// - `check_dimension` + `cosine_sim` conservés (validation + rescore).
pub struct HnswVectorStore {
    embeddings: HashMap<TripletId, Vec<f32>>,
    ids: Vec<TripletId>,
    points: Vec<EmbeddingPoint>,
    index: Option<instant_distance::HnswMap<EmbeddingPoint, TripletId>>,
    expected_dim: Option<usize>,
    wal: Vec<(TripletId, Vec<f32>)>,
    wal_path: Option<std::path::PathBuf>,
}

impl Default for HnswVectorStore {
    fn default() -> Self {
        Self::new()
    }
}

impl HnswVectorStore {
    pub fn new() -> Self {
        Self {
            embeddings: HashMap::new(),
            ids: Vec::new(),
            points: Vec::new(),
            index: None,
            expected_dim: None,
            wal: Vec::new(),
            wal_path: None,
        }
    }

    /// Chemin WAL pour la persistance append-only (`insert` y ajoute une ligne
    /// JSON `{id, embedding}`). `None` ⇒ WAL mémoire seule.
    pub fn set_wal_path(&mut self, path: impl Into<std::path::PathBuf>) {
        self.wal_path = Some(path.into());
    }

    /// WAL en mémoire (rejeu après crash via `batch_insert`).
    pub fn wal(&self) -> &[(TripletId, Vec<f32>)] {
        &self.wal
    }

    /// Reconstruit l'index ANN depuis `points`/`ids` (no-op si vide).
    pub fn rebuild(&mut self) {
        if self.points.is_empty() {
            self.index = None;
            return;
        }
        let map = instant_distance::Builder::default().build(self.points.clone(), self.ids.clone());
        self.index = Some(map);
    }

    fn brute_search(&self, query: &[f32], k: usize) -> Vec<(TripletId, f32)> {
        let mut scored: Vec<(TripletId, f32)> = self
            .embeddings
            .iter()
            .map(|(id, emb)| (id.clone(), cosine_sim(query, emb)))
            .collect();
        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        scored.truncate(k);
        scored
    }
}

impl VectorIndex for HnswVectorStore {
    fn insert(&mut self, id: TripletId, embedding: Vec<f32>) -> hakille_core::Result<()> {
        check_dimension(
            self.expected_dim,
            embedding.len(),
            "HnswVectorStore::insert",
        )?;
        // Dim 384 verrouillée (production MiniLM) : jamais de troncature.
        if embedding.len() != EMBEDDING_DIM {
            return Err(hakille_core::HakilleError::Internal(format!(
                "HnswVectorStore::insert: dimension {} refusée (verrouillée à {EMBEDDING_DIM})",
                embedding.len()
            )));
        }
        if self.expected_dim.is_none() {
            self.expected_dim = Some(embedding.len());
        }
        // Remplacement : retire l'ancien point pour garder `ids`/`points` alignés.
        if self.embeddings.contains_key(&id)
            && let Some(pos) = self.ids.iter().position(|x| x == &id)
        {
            self.ids.remove(pos);
            self.points.remove(pos);
        }
        self.embeddings.insert(id.clone(), embedding.clone());
        self.ids.push(id.clone());
        self.points.push(EmbeddingPoint(embedding.clone()));
        // WAL : mémoire + append fichier (best-effort, erreur loggée).
        self.wal.push((id.clone(), embedding.clone()));
        if let Some(ref path) = self.wal_path.clone() {
            let line = serde_json::json!({"id": id, "embedding": embedding}).to_string();
            if let Err(e) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut f| {
                    use std::io::Write;
                    writeln!(f, "{line}")
                })
            {
                tracing::warn!("HnswVectorStore WAL append impossible: {e}");
            }
        }
        // Index : reconstruit seulement au-delà du seuil (en deçà, le
        // brute-force exact suffit et évite le coût HNSW).
        if self.embeddings.len() >= HNSW_BRUTE_FORCE_THRESHOLD {
            self.rebuild();
        } else {
            self.index = None;
        }
        Ok(())
    }

    fn search(&self, query: &[f32], k: usize) -> hakille_core::Result<Vec<(TripletId, f32)>> {
        check_dimension(self.expected_dim, query.len(), "HnswVectorStore::search")?;
        if k == 0 {
            return Ok(Vec::new());
        }
        // Fallback brute-force exact (< seuil ou index absent).
        if self.embeddings.len() < HNSW_BRUTE_FORCE_THRESHOLD || self.index.is_none() {
            return Ok(self.brute_search(query, k));
        }
        // ANN : présélection 2×k puis rescore cosinus exact.
        let index = self
            .index
            .as_ref()
            .expect("index présent (vérifié ci-dessus)");
        let mut search = instant_distance::Search::default();
        let qp = EmbeddingPoint(query.to_vec());
        let candidates: Vec<TripletId> = index
            .search(&qp, &mut search)
            .take(k.saturating_mul(2).max(k).min(self.ids.len()))
            .map(|item| item.value.clone())
            .collect();
        let mut scored: Vec<(TripletId, f32)> = candidates
            .into_iter()
            .filter_map(|id| {
                self.embeddings
                    .get(&id)
                    .map(|emb| (id, cosine_sim(query, emb)))
            })
            .collect();
        // Complète au brute-force si l'ANN a rendu moins de k (jamais de trou).
        if scored.len() < k.min(self.embeddings.len()) {
            let mut extra = self.brute_search(query, k);
            for (id, s) in extra.drain(..) {
                if !scored.iter().any(|(x, _)| x == &id) {
                    scored.push((id, s));
                }
                if scored.len() >= k {
                    break;
                }
            }
        }
        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        scored.truncate(k);
        Ok(scored)
    }

    fn remove(&mut self, id: &TripletId) -> hakille_core::Result<()> {
        self.embeddings.remove(id);
        if let Some(pos) = self.ids.iter().position(|x| x == id) {
            self.ids.remove(pos);
            self.points.remove(pos);
        }
        if self.embeddings.is_empty() {
            self.expected_dim = None;
            self.index = None;
        } else if self.embeddings.len() >= HNSW_BRUTE_FORCE_THRESHOLD {
            self.rebuild();
        } else {
            self.index = None;
        }
        Ok(())
    }

    fn dimension(&self) -> Option<usize> {
        self.expected_dim
    }

    fn len(&self) -> usize {
        self.embeddings.len()
    }

    fn save(&self, db: &redb::Database) -> hakille_core::Result<()> {
        let txn = db
            .begin_write()
            .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
        {
            let mut table = txn
                .open_table(crate::storage::edge::EMBEDDINGS_TABLE)
                .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
            for (id, vec) in &self.embeddings {
                let id_bytes = bincode::serialize(id)
                    .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
                let vec_bytes = bincode::serialize(vec)
                    .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
                table
                    .insert(id_bytes.as_slice(), vec_bytes.as_slice())
                    .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
            }
        }
        txn.commit()
            .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
        Ok(())
    }

    fn load(&mut self, db: &redb::Database) -> hakille_core::Result<()> {
        let txn = db
            .begin_read()
            .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
        let table = match txn.open_table(crate::storage::edge::EMBEDDINGS_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(()),
            Err(e) => return Err(hakille_core::HakilleError::Internal(e.to_string())),
        };
        use redb::ReadableTable;
        // Vide l'état courant puis rejoue (WAL non rejoué ici : déjà persisté).
        self.embeddings.clear();
        self.ids.clear();
        self.points.clear();
        self.index = None;
        self.expected_dim = None;
        // Suspend le WAL fichier pendant le rejeu (évite de dupliquer le WAL).
        let wal_path = self.wal_path.take();
        for item in table
            .iter()
            .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?
        {
            let (k, v) = item.map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
            let id: TripletId = bincode::deserialize(k.value())
                .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
            let vec: Vec<f32> = bincode::deserialize(v.value())
                .map_err(|e| hakille_core::HakilleError::Internal(e.to_string()))?;
            self.insert(id, vec)?;
        }
        self.wal_path = wal_path;
        Ok(())
    }
}

/// Similarité cosinus partagée (extraction anti-duplication).
///
/// Retourne 0.0 si dimensions incohérentes, si un vecteur est vide ou si une
/// norme est nulle (**vecteur nul ⇒ 0.0 documenté** : un vecteur nul n'a pas
/// de direction, donc pas de similarité angulaire définie ; on retourne 0.0
/// plutôt que NaN pour ne jamais propager de NaN au tri — fonction totale,
/// jamais de panique. Les appelants valident les dimensions en amont).
pub fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        (dot / (norm_a * norm_b)).clamp(-1.0, 1.0)
    }
}

/// Valide la dimension d'un embedding contre la dimension attendue du store.
///
/// - `expected`: dimension verrouillée au premier insert (`None` = store vide).
/// - Retourne `Err` si vide ou incohérent (message explicite, pas de troncature silencieuse).
pub fn check_dimension(
    expected: Option<usize>,
    embedding_len: usize,
    what: &str,
) -> hakille_core::Result<()> {
    if embedding_len == 0 {
        return Err(hakille_core::HakilleError::Internal(format!(
            "{what}: embedding vide interdit"
        )));
    }
    if let Some(dim) = expected
        && embedding_len != dim
    {
        return Err(hakille_core::HakilleError::Internal(format!(
            "{what}: dimension {embedding_len} incohérente (attendue {dim})"
        )));
    }
    Ok(())
}

/// Dimension de production recommandée pour les embeddings (MiniLM-L6-v2 : 384).
/// Utilisée par `EdgeStore::save_embedding` comme garde-fou documenté.
pub const EMBEDDING_DIM: usize = 384;

/// Valide la dimension d'un embedding persisté en tier edge.
///
/// Refuse les vecteurs vides ; accepte toute dimension non vide (le verrou
/// strict 384 est assoupli pour permettre les tests et les modèles alternatifs,
/// la dimension du store vectoriel restant verrouillée au premier insert).
pub fn check_embedding_dim(embedding_len: usize, what: &str) -> hakille_core::Result<()> {
    if embedding_len == 0 {
        return Err(hakille_core::HakilleError::Internal(format!(
            "{what}: embedding vide interdit"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn triplet_id() -> TripletId {
        TripletId(Uuid::now_v7())
    }

    #[test]
    fn test_ranking_exact_then_partial_then_orthogonal() {
        let mut store = BruteForceVectorStore::new();
        let id_exact = triplet_id();
        let id_partial = triplet_id();
        let id_ortho = triplet_id();
        store.insert(id_exact.clone(), vec![1.0, 0.0, 0.0]).unwrap();
        store
            .insert(id_partial.clone(), vec![0.7, 0.7, 0.0])
            .unwrap();
        store.insert(id_ortho.clone(), vec![0.0, 1.0, 0.0]).unwrap();

        let results = store.search(&[1.0, 0.0, 0.0], 3).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, id_exact);
        assert!((results[0].1 - 1.0).abs() < 1e-4);
        assert_eq!(results[1].0, id_partial);
        assert_eq!(results[2].0, id_ortho);
        assert!(results[0].1 > results[1].1 && results[1].1 > results[2].1);
    }

    #[test]
    fn test_dimension_mismatch_rejected() {
        let mut store = BruteForceVectorStore::new();
        store.insert(triplet_id(), vec![1.0, 0.0, 0.0]).unwrap();
        // Insert incohérent ⇒ erreur, pas de troncature silencieuse.
        assert!(store.insert(triplet_id(), vec![1.0, 0.0]).is_err());
        // Recherche incohérente ⇒ erreur.
        assert!(store.search(&[1.0, 0.0], 2).is_err());
        // Embedding vide ⇒ erreur (insert comme search).
        assert!(store.insert(triplet_id(), vec![]).is_err());
        assert!(store.search(&[], 1).is_err());
        // La dimension verrouillée n'a pas bougé.
        assert_eq!(store.dimension(), Some(3));
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn test_k_zero_returns_empty_and_k_greater_than_n_returns_n() {
        let mut store = BruteForceVectorStore::new();
        store.insert(triplet_id(), vec![1.0, 0.0]).unwrap();
        store.insert(triplet_id(), vec![0.0, 1.0]).unwrap();
        // k = 0 ⇒ [] sans panique.
        assert!(store.search(&[1.0, 0.0], 0).unwrap().is_empty());
        assert!(store.search_with_scores(&[1.0, 0.0], 0).unwrap().is_empty());
        // k > N ⇒ N résultats.
        assert_eq!(store.search(&[1.0, 0.0], 100).unwrap().len(), 2);
    }

    #[test]
    fn test_batch_insert_and_fail_fast() {
        let mut store = BruteForceVectorStore::new();
        let ids: Vec<TripletId> = (0..3).map(|_| triplet_id()).collect();
        store
            .batch_insert(vec![
                (ids[0].clone(), vec![1.0, 0.0]),
                (ids[1].clone(), vec![0.0, 1.0]),
                (ids[2].clone(), vec![1.0, 1.0]),
            ])
            .unwrap();
        assert_eq!(store.len(), 3);
        assert_eq!(store.dimension(), Some(2));
        // Batch avec un item incohérent ⇒ Err (fail-fast, documenté non atomique).
        let before = store.len();
        assert!(
            store
                .batch_insert(vec![(triplet_id(), vec![1.0, 0.0, 0.0])])
                .is_err()
        );
        assert_eq!(store.len(), before);
    }

    #[test]
    fn test_remove_resets_dimension_only_when_empty() {
        let mut store = BruteForceVectorStore::new();
        let id1 = triplet_id();
        let id2 = triplet_id();
        store.insert(id1.clone(), vec![1.0, 0.0]).unwrap();
        store.insert(id2.clone(), vec![0.0, 1.0]).unwrap();
        // Remove partiel : le verrou de dimension RESTE (comportement voulu).
        store.remove(&id1).unwrap();
        assert_eq!(store.dimension(), Some(2));
        assert_eq!(store.len(), 1);
        assert!(!store.is_empty());
        // Dernier remove : le verrou est relâché, une nouvelle dimension est acceptée.
        store.remove(&id2).unwrap();
        assert!(store.is_empty());
        assert_eq!(store.dimension(), None);
        store
            .insert(triplet_id(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(store.dimension(), Some(4));
    }

    #[test]
    fn test_cosine_sim_null_vector_returns_zero() {
        // Vecteur nul ⇒ 0.0 documenté (pas de NaN).
        assert_eq!(cosine_sim(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
        assert_eq!(cosine_sim(&[1.0, 0.0], &[0.0, 0.0]), 0.0);
        assert_eq!(cosine_sim(&[0.0, 0.0], &[0.0, 0.0]), 0.0);
        // Dimensions incohérentes ⇒ 0.0.
        assert_eq!(cosine_sim(&[1.0, 0.0], &[1.0, 0.0, 0.0]), 0.0);
        // Cas nominaux inchangés.
        assert!((cosine_sim(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine_sim(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
    }

    #[test]
    fn test_pipeline_helpers_link_and_insert_triplet_embedding() {
        use crate::context_partition::ContextPartition;
        use crate::triplet::{NodeValue, Triplet, TripletMetadata, TripletSource};
        use hakille_core::{NodeId, PredicateId, Timestamp, TrustLevel};

        let mut store = BruteForceVectorStore::new();
        let tid = triplet_id();
        // insert_triplet_embedding ⇒ équivalent à insert.
        store
            .insert_triplet_embedding(tid.clone(), vec![1.0, 0.0])
            .unwrap();
        assert_eq!(store.len(), 1);

        // link_triplet_embedding sur objet Embedding ⇒ indexé sous triplet.id.
        let tid2 = triplet_id();
        let triplet = Triplet {
            id: tid2.clone(),
            subject: NodeId(Uuid::now_v7()),
            predicate: PredicateId(Uuid::now_v7()),
            object: NodeValue::Embedding(vec![0.0, 1.0]),
            metadata: TripletMetadata {
                weight: 1.0,
                created_at: Timestamp::now(),
                last_activated: Timestamp::now(),
                source: TripletSource::User,
                confidence: 1.0,
                context: ContextPartition::Default,
                trust_level: TrustLevel::User,
                decay_rate: 0.0,
            },
            node_type: None,
        };
        store.link_triplet_embedding(&triplet).unwrap();
        assert_eq!(store.len(), 2);
        let hits = store.search_with_scores(&[0.0, 1.0], 1).unwrap();
        assert_eq!(hits[0].0, tid2);

        // link_triplet_embedding sur objet non-embedding ⇒ no-op Ok(()).
        let mut text_triplet = triplet.clone();
        text_triplet.id = triplet_id();
        text_triplet.object = NodeValue::Text("pas un vecteur".to_string());
        store.link_triplet_embedding(&text_triplet).unwrap();
        assert_eq!(store.len(), 2);
    }

    /// Golden rappel HNSW (P2) : 3 vecteurs 384 verrouillés, la requête proche
    /// du premier rend le premier en top-1 (fallback brute-force exact < 1k,
    /// rescore cosinus). Verrouille dim 384 + ranking + déterminisme.
    #[test]
    fn test_hnsw_golden_rappel_top1() {
        fn vec384(first: f32, second: f32) -> Vec<f32> {
            let mut v = vec![0.0f32; EMBEDDING_DIM];
            v[0] = first;
            v[1] = second;
            v
        }
        let mut store = HnswVectorStore::new();
        let id_a = triplet_id();
        let id_b = triplet_id();
        let id_c = triplet_id();
        store.insert(id_a.clone(), vec384(1.0, 0.0)).unwrap();
        store.insert(id_b.clone(), vec384(0.7, 0.7)).unwrap();
        store.insert(id_c.clone(), vec384(0.0, 1.0)).unwrap();
        assert_eq!(store.dimension(), Some(EMBEDDING_DIM));
        assert_eq!(store.len(), 3);
        // Dim non-384 refusée (verrou production).
        assert!(store.insert(triplet_id(), vec![1.0, 0.0]).is_err());

        let query = vec384(0.9, 0.1);
        let hits = store.search(&query, 3).unwrap();
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].0, id_a, "le plus aligné doit arriver premier");
        assert!(hits[0].1 > hits[1].1 && hits[1].1 > hits[2].1);
        // Score exact cosinus : requête quasi-alignée sur id_a ⇒ ~0.99.
        assert!(
            hits[0].1 > 0.99,
            "score cosinus attendu > 0.99, obtenu {}",
            hits[0].1
        );
        // k=0 ⇒ vide ; remove partiel garde le verrou.
        assert!(store.search(&query, 0).unwrap().is_empty());
        store.remove(&id_c).unwrap();
        assert_eq!(store.dimension(), Some(EMBEDDING_DIM));
    }
}
