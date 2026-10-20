pub mod context_partition;
pub mod contradiction;
pub mod decay;
pub mod export;
pub mod graph;
pub mod heritage;
pub mod objective;
pub mod query_lang;
pub mod runtime;
pub mod storage;
pub mod task;
pub mod triplet;
pub mod vector;

pub use context_partition::*;
pub use contradiction::*;
pub use decay::*;
pub use export::*;
pub use graph::*;
pub use heritage::*;
pub use objective::*;
pub use query_lang::*;
pub use runtime::*;
pub use task::*;
pub use triplet::*;

// Ré-exports explicites des tiers persistant et vectoriel (nominatifs plutôt
// que globaux : `storage::edge` et `vector::edge` partagent le même nom de
// module, un `pub use storage::*` + `pub use vector::*` serait ambigu).
pub use storage::GraphStorage;
pub use storage::cluster::ClusterStore;
pub use storage::edge::EdgeStore;
pub use vector::BruteForceVectorStore;
pub use vector::HnswVectorStore;
pub use vector::VectorIndex;
pub use vector::cluster::ClusterVectorStore;
pub use vector::edge::EdgeVectorStore;
pub use vector::{EMBEDDING_DIM, HNSW_BRUTE_FORCE_THRESHOLD};

// Nota : pas de `use` privé ici — les noms ci-dessous (`KnowledgeGraph`,
// `EdgeStore`, `DecayEngine`, `Triplet`, …) sont déjà en portée via les
// `pub use` globaux/nominatifs ci-dessus (un `use` redondant déclencherait
// `hidden_glob_reexports`).

/// Façade unique câblant le graphe RAM ([`KnowledgeGraph`]) et le tier edge
/// redb ([`EdgeStore`]).
///
/// Cycle de vie :
/// - [`Self::new`] : graphe RAM seul (sans persistance — `persist` rend
///   alors `Err`, voir [`Self::is_persistent`]) ;
/// - [`Self::open`] : crée/ouvre la base redb au chemin donné et hydrate le
///   graphe depuis le contenu existant ;
/// - [`Self::persist`] : RAM → redb (sans ramasse-miettes, idempotent) ;
/// - [`Self::flush`] : no-op documenté (redb rend chaque commit durable dès
///   `persist`, rien à vider) ;
/// - [`Self::close`] : `persist` puis libération (consomme `self`).
///
/// Le store vectoriel ([`EdgeVectorStore`]) est câblé en structure mais non
/// alimenté : aucun encodeur d'embeddings n'est branché à ce stade, donc
/// `insert_text` n'indexe que le graphe symbolique (recherche via
/// `recall_text` = `search_text` trié par poids). Le champ existe pour que
/// la façade compile et que l'indexation vectorielle se branche ici sans
/// changer l'API publique.
pub struct MemoryManager {
    graph: KnowledgeGraph,
    edge: Option<EdgeStore>,
    vectors: EdgeVectorStore,
    path: Option<std::path::PathBuf>,
}

impl MemoryManager {
    /// Façade RAM seule (aucune persistance).
    pub fn new() -> Self {
        Self {
            graph: KnowledgeGraph::new(),
            edge: None,
            vectors: EdgeVectorStore::new(),
            path: None,
        }
    }

    /// Ouvre (ou crée) la base redb au chemin donné et hydrate le graphe RAM
    /// depuis son contenu. Les répertoires parents manquants sont créés.
    pub async fn open(path: impl AsRef<std::path::Path>) -> hakille_core::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| {
                hakille_core::HakilleError::StorageError(format!(
                    "MemoryManager::open : création du répertoire parent impossible : {e}"
                ))
            })?;
        }
        let db = redb::Database::create(&path).map_err(|e| {
            hakille_core::HakilleError::StorageError(format!("MemoryManager::open : {e}"))
        })?;
        let edge = EdgeStore::new(std::sync::Arc::new(db));
        let mut graph = KnowledgeGraph::new();
        graph.hydrate_from(&edge).await?;
        Ok(Self {
            graph,
            edge: Some(edge),
            vectors: EdgeVectorStore::new(),
            path: Some(path),
        })
    }

    /// Accès lecture au graphe RAM sous-jacent.
    pub fn graph(&self) -> &KnowledgeGraph {
        &self.graph
    }

    /// Accès mutable au graphe RAM sous-jacent (insertions avancées,
    /// `activate`, `remove`, decay manuel…).
    pub fn graph_mut(&mut self) -> &mut KnowledgeGraph {
        &mut self.graph
    }

    /// Accès lecture au store vectoriel (aujourd'hui vide : aucun encodeur
    /// d'embeddings n'est branché ; l'indexation sémantique s'écrira ici via
    /// le trait [`VectorIndex`] sans changer l'API de la façade).
    pub fn vector_store(&self) -> &EdgeVectorStore {
        &self.vectors
    }

    /// Vrai si une base redb est câblée (`open`) et donc si `persist`/`close`
    /// peuvent écrire.
    pub fn is_persistent(&self) -> bool {
        self.edge.is_some()
    }

    /// Chemin de la base redb câblée, le cas échéant.
    pub fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }

    /// Insère un fait texte « sujet — prédicat — objet » dans le graphe RAM
    /// (sujet/prédicat résolus via les registres, ID déterministe S/P/O,
    /// routage contradiction de `KnowledgeGraph::insert`).
    ///
    /// Métadonnées par défaut (pré-validées, dans `[0, 1]`) : poids `1.0`,
    /// confiance `0.9`, source `User`, contexte `Default`, niveau `User`,
    /// décroissance `0.01`, type [`NodeType::Fact`]. RAM seule : appeler
    /// [`Self::persist`] pour écrire en base.
    pub fn insert_text(&mut self, subject: &str, predicate: &str, object: &str) -> InsertResult {
        let subject_id = self.graph.resolve_subject(subject);
        let predicate_id = self.graph.resolve_predicate(predicate);
        let object_value = NodeValue::Text(object.to_string());
        let triplet_id =
            KnowledgeGraph::deterministic_triplet_id(&subject_id, &predicate_id, &object_value);
        let now = hakille_core::Timestamp::now();
        let triplet = Triplet {
            id: triplet_id,
            subject: subject_id,
            predicate: predicate_id,
            object: object_value,
            metadata: TripletMetadata {
                weight: 1.0,
                created_at: now.clone(),
                last_activated: now,
                source: TripletSource::User,
                confidence: 0.9,
                context: crate::context_partition::ContextPartition::Default,
                trust_level: hakille_core::TrustLevel::User,
                decay_rate: 0.01,
            },
            node_type: Some(NodeType::Fact),
        };
        self.graph.insert(triplet)
    }

    /// Rappelle les faits texte contenant `query` (insensible à la casse),
    /// triés par poids décroissant (égalité ⇒ `TripletId`). Lecture seule :
    /// n'active pas les souvenirs (appeler `graph_mut().activate(&id)` pour
    /// renforcer explicitement).
    pub fn recall_text(&self, query: &str) -> Vec<Triplet> {
        let mut out: Vec<Triplet> = self.graph.search_text(query).into_iter().cloned().collect();
        out.sort_by(|a, b| {
            b.metadata
                .weight
                .partial_cmp(&a.metadata.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });
        out
    }

    /// Applique la décroissance temporelle au graphe RAM (délègue à
    /// [`DecayEngine::apply_decay` : historique préservé par défaut).
    pub fn apply_decay(&mut self, cutoff_weight: f64) -> DecayReport {
        DecayEngine::apply_decay(&mut self.graph, cutoff_weight)
    }

    /// Persiste le graphe RAM vers redb (idempotent, sans ramasse-miettes).
    /// Rend `Err` si la façade est RAM seule (construite via [`Self::new`]).
    pub async fn persist(&self) -> hakille_core::Result<()> {
        match &self.edge {
            Some(edge) => self.graph.persist_to(edge).await,
            None => Err(hakille_core::HakilleError::StorageError(
                "MemoryManager::persist sans base câblée : construire via MemoryManager::open(path)"
                    .to_string(),
            )),
        }
    }

    /// Assure la durabilité : no-op — redb rend chaque transaction `commit`
    /// durable dès [`Self::persist`], il n'y a aucun tampon à vider. Conservé
    /// pour l'API (symétrie `persist`/`flush`/`close`).
    pub fn flush(&self) -> hakille_core::Result<()> {
        Ok(())
    }

    /// Persiste puis libère la façade (la base redb est fermée en tombant le
    /// dernier `Arc<Database>`).
    pub async fn close(self) -> hakille_core::Result<()> {
        self.persist().await?;
        Ok(())
    }
}

impl Default for MemoryManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_core::{NodeId, PredicateId, Timestamp, TripletId, TrustLevel};

    #[test]
    fn test_task_lifecycle_and_bounded_steps() {
        let mut task = Task::new(
            "task-42",
            "Envoyer compte-rendu",
            "Envoyer par email après réunion",
        )
        .with_priority(TaskPriority::High)
        .with_parent_objective("obj-q3-marketing");

        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.progress_ratio(), 0.0);

        let s1 = task.add_step("Rédiger les notes", None);
        let s2 = task.add_step("Relire le plan d'action", None);
        let _s3 = task.add_step("Envoyer email", Some("capability:email-send".to_string()));

        assert_eq!(task.steps.len(), 3);

        task.complete_step(s1 as usize, Some("Notes rédigées".to_string()));
        assert_eq!(task.status, TaskStatus::InProgress);
        assert!((task.progress_ratio() - 0.333).abs() < 0.01);

        task.complete_step(s2 as usize, None);
        task.complete_step(2, Some("Email transmis".to_string()));
        assert_eq!(task.status, TaskStatus::Completed);
        assert_eq!(task.progress_ratio(), 1.0);
        assert!(task.completed_at.is_some());
    }

    #[test]
    fn test_declarative_graph_query_lang() {
        let mut graph = KnowledgeGraph::new();

        let subj_alice = NodeId(uuid::Uuid::new_v4());
        let pred_aime = PredicateId(uuid::Uuid::new_v4());
        // Prédicat distinct documenté : depuis le routage contradiction (§8,
        // mission 2), même sujet + même prédicat + objets différents =
        // conflit direct (2e insert refusé). Deux faits compatibles exigent
        // donc deux prédicats distincts — l'ancien test utilisait le même et
        // attendait l'insertion aveugle (stub corrigé).
        let pred_boit = PredicateId(uuid::Uuid::new_v4());

        let t1 = Triplet {
            id: TripletId(uuid::Uuid::new_v4()),
            subject: subj_alice.clone(),
            predicate: pred_aime.clone(),
            object: NodeValue::Text("Programmation en Rust".to_string()),
            metadata: TripletMetadata {
                weight: 0.95,
                created_at: Timestamp::now(),
                last_activated: Timestamp::now(),
                source: TripletSource::User,
                confidence: 0.98,
                context: ContextPartition::Default,
                trust_level: TrustLevel::User,
                decay_rate: 0.01,
            },
            node_type: Some(NodeType::Fact),
        };

        let t2 = Triplet {
            id: TripletId(uuid::Uuid::new_v4()),
            subject: subj_alice.clone(),
            predicate: pred_boit.clone(),
            object: NodeValue::Text("Café espresso".to_string()),
            metadata: TripletMetadata {
                weight: 0.40, // Lower weight
                created_at: Timestamp::now(),
                last_activated: Timestamp::now(),
                source: TripletSource::User,
                confidence: 0.85,
                context: ContextPartition::Personal,
                trust_level: TrustLevel::User,
                decay_rate: 0.01,
            },
            node_type: Some(NodeType::Fact),
        };

        graph.insert(t1);
        graph.insert(t2);

        // Query with text filter and weight cutoff
        let query = GraphQueryBuilder::select()
            .where_subject(subj_alice)
            .where_object_contains("Rust")
            .where_weight_above(0.5)
            .build();

        let results = query.execute(&graph);
        assert_eq!(results.len(), 1);
        match &results[0].object {
            NodeValue::Text(s) => assert!(s.contains("Rust")),
            _ => panic!("Expected text object"),
        }

        // Query by partition
        let query_personal = GraphQueryBuilder::select()
            .where_partition(ContextPartition::Personal)
            .build();
        let results_personal = query_personal.execute(&graph);
        assert_eq!(results_personal.len(), 1);
        match &results_personal[0].object {
            NodeValue::Text(s) => assert!(s.contains("Café")),
            _ => panic!("Expected café object"),
        }
    }

    #[test]
    fn test_memory_manager_insert_and_recall_text() {
        let mut manager = MemoryManager::new();
        assert!(!manager.is_persistent());
        assert!(manager.path().is_none());

        let first = manager.insert_text("Alice", "aime", "le café");
        let first_id = match first {
            InsertResult::Inserted(id) => id,
            other => panic!("insertion attendue, obtenue : {:?}", other),
        };
        // Idempotence : même fait ⇒ même ID.
        match manager.insert_text("Alice", "aime", "le café") {
            InsertResult::Inserted(id) => assert_eq!(id, first_id),
            other => panic!("doublon idempotent attendu, obtenu : {:?}", other),
        }

        let hits = manager.recall_text("café");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, first_id);
        assert_eq!(hits[0].node_type_or_default(), NodeType::Fact);
        assert!(manager.recall_text("introuvable").is_empty());
    }

    #[tokio::test]
    async fn test_memory_manager_open_persist_close_roundtrip() {
        let dir = std::env::temp_dir().join(format!("hakille-manager-{}", uuid::Uuid::now_v7()));
        let db_path = dir.join("memory.redb");

        let manager = MemoryManager::open(&db_path).await.unwrap();
        assert!(manager.is_persistent());
        assert_eq!(manager.path(), Some(db_path.as_path()));
        drop(manager);

        let mut manager = MemoryManager::open(&db_path).await.unwrap();
        manager.insert_text("Bob", "boit", "du thé");
        manager.apply_decay(0.01);
        manager.persist().await.unwrap();
        manager.flush().unwrap();
        manager.close().await.unwrap();

        // Réouverture : le fait survit (registres reconstruits au pire en repli).
        let reopened = MemoryManager::open(&db_path).await.unwrap();
        assert_eq!(reopened.graph().len(), 1);
        assert_eq!(reopened.recall_text("thé").len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_memory_manager_persist_without_edge_errors() {
        let manager = MemoryManager::new();
        assert!(manager.persist().await.is_err());
        // `flush` reste Ok même sans base (no-op).
        assert!(manager.flush().is_ok());
    }
}
