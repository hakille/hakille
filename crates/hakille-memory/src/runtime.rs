//! Façade mémoire « prête à l'emploi » pour le démon : graphe SPO + tier
//! redb + recherche lexicale OU vectorielle réelle (HNSW `instant-distance`,
//! persisté dans la table redb des embeddings).
//!
//! # Pourquoi cette façade
//!
//! Le démon persistait jusqu'ici son graphe en JSON (`./hakille-memory.json`,
//! produit par [`KnowledgeGraph::save_to_file`] / restauré par
//! [`KnowledgeGraph::load_from_file`]). [`MemoryRuntime`] remplace ce chemin
//! par une base redb durable et ajoute la recherche sémantique quand un
//! encodeur d'embeddings est fourni. La migration des données existantes se
//! fait via [`MemoryRuntime::import_legacy_json`] (format JSON exact du démon,
//! sans perte ni doublon).
//!
//! # Recherche : lexicale par défaut, vectorielle si embedder
//!
//! - **Sans embedder** ([`MemoryRuntime::set_embedder`] jamais appelé) :
//!   recherche lexicale à comportement identique au démon actuel — sous-chaîne
//!   insensible à la casse sur l'objet textuel, tri par poids décroissant puis
//!   identifiant (sémantique de `KnowledgeGraph::search_text` +
//!   `MemoryManager::recall_text`).
//! - **Avec embedder** : l'encodeur est appelé sur la requête et sur chaque
//!   souvenir ; la recherche passe par [`HnswVectorStore`] (brute-force exact
//!   sous [`HNSW_BRUTE_FORCE_THRESHOLD`] vecteurs, ANN HNSW + rescore cosinus
//!   au-dessus), les embeddings étant persistés dans la table redb dédiée.
//!
//! **HONNÊTETÉ** : l'embedder neural (MiniLM 384 dims via `ort`) reste à
//! **brancher par le démon**. Cette façade ne fabrique AUCUN embedding par
//! défaut : sans `set_embedder`, elle ne fait pas semblant de comprendre, elle
//! cherche lexicalement. Le contrat attendu d'un embedder est :
//! `Fn(&str) -> Vec<f32>` **384 dimensions** (verrou hérité de
//! [`HnswVectorStore`] et d'[`EdgeStore::save_embedding`]) — un vecteur d'une
//! autre dimension est refusé AVANT toute écriture (erreur
//! [`MemoryRuntimeError::Embedding`], jamais de troncature silencieuse).
//!
//! # Modèle de données dans le graphe
//!
//! Chaque mémoire créée par [`MemoryRuntime::remember`] est un **sujet
//! déterministe** porteur de trois familles de triplets (espace de noms
//! réservé `hakille-memory:`) :
//!
//! - contenu : prédicat `hakille-memory:content`, objet `Text(texte)` ;
//! - source : prédicat `hakille-memory:source`, objet `Text(source)` ;
//! - étiquettes : un triplet par tag, prédicat dérivé de la chaîne
//!   `hakille-memory-tag:<tag>` (sensible à la casse), objet `Text(tag)`.
//!
//! Le sujet est dérivé (FNV-1a, sensible à la casse) de
//! `texte + source + tags triés` : deux `remember` au contenu identique
//! retombent sur les mêmes identifiants (idempotent par construction), deux
//! contenus différents ne se contredisent jamais. L'espace `hakille-memory:` est
//! réservé : un triplet historique utilisant exactement ces prédicats serait
//! traité comme triplet auxiliaire par [`MemoryRuntime::recent`] /
//! [`MemoryRuntime::recall`] (collision en pratique impossible hors
//! fabrication manuelle).
//!
//! # Limites documentées
//!
//! - Les libellés nominaux (`Alice` → UUID) ne sont pas persistés par le tier
//!   redb : après réouverture, `hydrate_from` retombe sur les UUID comme
//!   étiquettes (limite héritée du store edge, voir sa documentation).
//!   `recent`/`recall` n'en dépendent pas ; l'import JSON re-seede les
//!   libellés du snapshot en mémoire.
//! - redb n'autorise qu'une seule [`redb::Database`] ouverte par fichier et
//!   par processus : un seul [`MemoryRuntime`] vivant par chemin.
//! - Pas encore d'API `forget`/`remove` : le ramasse-miettes reste côté
//!   graphe avancé (`KnowledgeGraph::persist_to_with_gc`).
//! - L'index HNSW ne sert qu'au-delà de [`HNSW_BRUTE_FORCE_THRESHOLD`]
//!   vecteurs ; en deçà, le brute-force exact garantit un ordre déterministe.

use crate::graph::KnowledgeGraph;
use crate::storage::GraphStorage;
use crate::storage::edge::EdgeStore;
use crate::triplet::{
    InsertResult, NodeType, NodeValue, TriplePattern, Triplet, TripletMetadata, TripletSource,
};
use crate::vector::{EMBEDDING_DIM, HnswVectorStore, VectorIndex};
use hakille_core::{NodeId, PredicateId, Timestamp, TripletId, TrustLevel};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Erreurs honnêtes de la façade : chaque variante dit d'où vient l'échec,
/// aucune n'est convertie en succès partiel silencieux.
#[derive(Debug, thiserror::Error)]
pub enum MemoryRuntimeError {
    /// Ouverture / lecture du tier redb (base absente, verrouillée, corrompue…).
    #[error("stockage redb : {0}")]
    Storage(String),
    /// Écriture durable du graphe ou des embeddings en échec.
    #[error("persistance : {0}")]
    Persist(String),
    /// Snapshot JSON legacy illisible ou invalide.
    #[error("import du snapshot JSON legacy : {0}")]
    LegacyImport(String),
    /// Embedder absent/incohérent (dimension ≠ 384, vecteur vide).
    #[error("embedding : {0}")]
    Embedding(String),
    /// Insertion refusée par le routage contradictions (jamais silencieux).
    #[error("conflit de mémoire non arbitré : {0}")]
    Conflict(String),
}

/// Entrée mémoire affichable (HUD, JSON IPC) — sérialisable telle quelle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Identifiant stable (UUID du triplet de contenu, en clair).
    pub id: String,
    /// Texte du souvenir (objet rendu pour les triplets legacy : nombres,
    /// booléens, horodatages, entités).
    pub text: String,
    /// Étiquettes triées (vide pour les triplets legacy importés).
    pub tags: Vec<String>,
    /// Source : chaîne exacte fournie à `remember`, ou forme rendue du
    /// `TripletSource` pour les triplets legacy.
    pub source: String,
    /// Poids courant du souvenir (1.0 à la création, renforcé par activation).
    pub weight: f64,
    /// Horodatage de création du triplet porteur.
    pub created_at: Timestamp,
    /// Dernière activation (recall/re-remember).
    pub last_activated: Timestamp,
}

/// Résultat de rappel : l'entrée + le score de la stratégie utilisée.
///
/// HONNÊTETÉ sur `score` : en mode vectoriel c'est la similarité cosinus
/// exacte (rescore) ; en mode lexical c'est le **poids** du triplet (proxy de
/// pertinence hérité du tri du démon), les deux modes n'étant donc pas
/// comparables entre eux.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecallHit {
    pub entry: MemoryEntry,
    pub score: f32,
}

/// Encodeur d'embeddings : texte → vecteur 384 dims.
pub type Embedder = Box<dyn Fn(&str) -> Vec<f32> + Send + Sync>;

/// Façade unique redb + graphe + index vectoriel pour le démon.
pub struct MemoryRuntime {
    db: Arc<redb::Database>,
    edge: EdgeStore,
    graph: KnowledgeGraph,
    vectors: HnswVectorStore,
    embedder: Option<Embedder>,
    path: PathBuf,
}

impl std::fmt::Debug for MemoryRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryRuntime")
            .field("path", &self.path)
            .field("memories", &self.memory_count())
            .field("embedder", &self.has_embedder())
            .finish()
    }
}

impl MemoryRuntime {
    /// Ouvre (ou crée) la base redb au chemin donné, hydrate le graphe depuis
    /// son contenu et recharge l'index vectoriel persistant.
    ///
    /// - Base fraîche : tables créées, schéma versionné par [`EdgeStore`].
    /// - Base existante écrite par cette façade : graphe + embeddings
    ///   restaurés (l'embedder, lui, doit être re-fourni via
    ///   [`Self::set_embedder`] après ouverture).
    /// - Fichier corrompu / verrouillé / version de schéma incompatible ⇒
    ///   `Err` explicite (jamais d'ouverture en mode dégradé muet).
    pub fn open(db_path: &Path) -> Result<Self, MemoryRuntimeError> {
        if let Some(parent) = db_path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| {
                MemoryRuntimeError::Storage(format!(
                    "création du répertoire parent de {} impossible : {e}",
                    db_path.display()
                ))
            })?;
        }
        let db = redb::Database::create(db_path)
            .map_err(|e| MemoryRuntimeError::Storage(format!("{}: {e}", db_path.display())))?;
        let db = Arc::new(db);
        let edge = EdgeStore::try_new(Arc::clone(&db)).map_err(storage_error)?;
        let mut graph = KnowledgeGraph::new();
        block_on(graph.hydrate_from(&edge)).map_err(storage_error)?;
        let mut vectors = HnswVectorStore::new();
        vectors.load(&db).map_err(storage_error)?;
        Ok(Self {
            db,
            edge,
            graph,
            vectors,
            embedder: None,
            path: db_path.to_path_buf(),
        })
    }

    /// Chemin de la base redb câblée.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Accès lecture au graphe (requêtes avancées, affichage) — les mutations
    /// doivent passer par [`Self::remember`] / [`Self::import_legacy_json`]
    /// pour rester persistées.
    pub fn graph(&self) -> &KnowledgeGraph {
        &self.graph
    }

    /// Vrai si un encodeur d'embeddings est branché (recherche vectorielle).
    pub fn has_embedder(&self) -> bool {
        self.embedder.is_some()
    }

    /// Nombre de souvenirs visibles (triplets hors auxiliaires source/tags).
    pub fn memory_count(&self) -> usize {
        self.graph
            .all_triplets()
            .into_iter()
            .filter(|t| !is_helper(t))
            .count()
    }

    /// Vrai si aucun souvenir visible.
    pub fn is_empty(&self) -> bool {
        self.memory_count() == 0
    }

    /// Branche un encodeur d'embeddings (neural ou déterministe de test) et
    /// **réindexe immédiatement** les souvenirs texte existants.
    ///
    /// Contrat : `Fn(&str) -> Vec<f32>` de **384 dimensions** (verrou
    /// [`HnswVectorStore`], identique à la production MiniLM). Un embedder
    /// produisant une autre dimension est refusé par souvenir (log `warn!`,
    /// souvenir non réindexé) : la méthode ne peut pas rendre `Err` (signature
    /// imposée) et ne fait donc jamais échouer l'appel — les souvenirs restent
    /// intacts.
    ///
    /// L'embedder n'est PAS persisté (c'est une closure) : le démon doit le
    /// rebrancher après chaque [`Self::open`].
    pub fn set_embedder(&mut self, embedder: Embedder) {
        self.embedder = Some(embedder);
        self.reindex_embeddings();
    }

    /// Mémorise un texte, retourne l'identifiant stable du souvenir.
    ///
    /// Idempotent : même `(texte, tags, source)` ⇒ même identifiant (les tags
    /// sont nettoyés, triés et dédupliqués). Persisté immédiatement (triplets
    /// en UNE transaction redb ; embedding en écriture ciblée si un embedder
    /// est branché). Un embedder de mauvaise dimension provoque une erreur
    /// AVANT toute écriture (aucun état partiel).
    pub fn remember(
        &mut self,
        text: &str,
        tags: &[String],
        source: &str,
    ) -> Result<String, MemoryRuntimeError> {
        let mut cleaned_tags: Vec<String> = tags
            .iter()
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect();
        cleaned_tags.sort();
        cleaned_tags.dedup();

        let subject = memory_subject_id(text, &cleaned_tags, source);
        let metadata = memory_metadata(source);
        let predicate = content_predicate_id();
        let object = NodeValue::Text(text.to_string());
        let content_id = KnowledgeGraph::deterministic_triplet_id(&subject, &predicate, &object);

        let embedding = match self.embedder.as_ref() {
            Some(embed) => {
                let vector = embed(text);
                validate_embedding(&vector, "MemoryRuntime::remember")?;
                Some(vector)
            }
            None => None,
        };

        let mut batch = Vec::with_capacity(2 + cleaned_tags.len());
        batch.push(Triplet {
            id: content_id.clone(),
            subject: subject.clone(),
            predicate,
            object,
            metadata: metadata.clone(),
            node_type: Some(NodeType::Fact),
        });
        batch.push(auxiliary_triplet(
            &subject,
            source_predicate_id(),
            source.to_string(),
            &metadata,
        ));
        for tag in &cleaned_tags {
            batch.push(auxiliary_triplet(
                &subject,
                tag_predicate_id(tag),
                tag.clone(),
                &metadata,
            ));
        }

        for triplet in &batch {
            match self.graph.insert(triplet.clone()) {
                InsertResult::Inserted(id) => {
                    debug_assert_eq!(id, triplet.id);
                }
                InsertResult::ContradictionResolved { new, .. } => {
                    debug_assert_eq!(new, triplet.id);
                }
                InsertResult::NeedsUserResolution { existing, incoming } => {
                    return Err(MemoryRuntimeError::Conflict(format!(
                        "remember refusé : {:?} et {:?} se contredisent (espace réservé `hakille-memory:` détourné ?)",
                        existing.id, incoming.id
                    )));
                }
            }
        }
        block_on(self.edge.batch_store(batch)).map_err(persist_error)?;

        if let Some(vector) = embedding {
            self.vectors
                .insert(content_id.clone(), vector.clone())
                .map_err(embedding_error)?;
            self.edge
                .save_embedding(&content_id, &vector)
                .map_err(persist_error)?;
        }
        Ok(content_id.0.to_string())
    }

    /// Rappelle les souvenirs pertinents pour `query`, au plus `limit`.
    ///
    /// - Sans embedder : recherche lexicale, comportement identique au démon
    ///   actuel (sous-chaîne casse-insensible sur le texte ; nombres et
    ///   booléens inclus comme `search_text`), tri poids décroissant puis id.
    /// - Avec embedder : recherche vectorielle HNSW/brute-force exacte (384
    ///   dims, ordre déterministe score décroissant puis id) ; requête vide ou
    ///   blanche ⇒ résultat vide documenté.
    pub fn recall(&self, query: &str, limit: usize) -> Result<Vec<RecallHit>, MemoryRuntimeError> {
        if let Some(embed) = self.embedder.as_ref() {
            if query.trim().is_empty() {
                return Ok(Vec::new());
            }
            let query_vector = embed(query);
            validate_embedding(&query_vector, "MemoryRuntime::recall")?;
            let scored = self
                .vectors
                .search(&query_vector, limit)
                .map_err(embedding_error)?;
            let mut hits = Vec::with_capacity(scored.len());
            for (id, score) in scored {
                match self.graph.get(&id) {
                    Some(triplet) if !is_helper(triplet) => {
                        hits.push(RecallHit {
                            entry: self.entry_from_triplet(triplet),
                            score,
                        });
                    }
                    Some(_) => {}
                    None => {
                        tracing::warn!(
                            id = %id.0,
                            "recall : embedding orphelin (triplet absent du graphe), ignoré"
                        );
                    }
                }
            }
            return Ok(hits);
        }

        let lowered = query.to_lowercase();
        let mut matched: Vec<&Triplet> = self
            .graph
            .all_triplets()
            .into_iter()
            .filter(|t| !is_helper(t) && lexical_matches(&t.object, &lowered))
            .collect();
        matched.sort_by(|a, b| {
            b.metadata
                .weight
                .partial_cmp(&a.metadata.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });
        matched.truncate(limit);
        Ok(matched
            .into_iter()
            .map(|t| RecallHit {
                entry: self.entry_from_triplet(t),
                score: t.metadata.weight as f32,
            })
            .collect())
    }

    /// Souvenirs les plus récemment activés (égalité : création, puis id),
    /// auxiliaires source/tags exclus, au plus `limit`.
    pub fn recent(&self, limit: usize) -> Result<Vec<MemoryEntry>, MemoryRuntimeError> {
        let mut entries: Vec<MemoryEntry> = self
            .graph
            .all_triplets()
            .into_iter()
            .filter(|t| !is_helper(t))
            .map(|t| self.entry_from_triplet(t))
            .collect();
        entries.sort_by(|a, b| {
            b.last_activated
                .cmp(&a.last_activated)
                .then_with(|| b.created_at.cmp(&a.created_at))
                .then_with(|| a.id.cmp(&b.id))
        });
        entries.truncate(limit);
        Ok(entries)
    }

    /// Persistance complète : resynchronise le graphe RAM vers redb et réécrit
    /// tous les embeddings de l'index vectoriel. Les écritures de `remember`/
    /// `import_legacy_json` étant déjà durables, `flush` est un filet de
    /// sécurité (fin de session, avant sauvegarde externe).
    pub fn flush(&self) -> Result<(), MemoryRuntimeError> {
        block_on(self.graph.persist_to(&self.edge)).map_err(persist_error)?;
        self.vectors.save(&self.db).map_err(persist_error)?;
        Ok(())
    }

    /// Importe un snapshot JSON au format exact du démon
    /// (`KnowledgeGraph::save_to_file`, cf. `persist_memory` du démon) :
    /// `{triplets, subject_names, predicate_names, subject_labels,
    /// predicate_labels}`.
    ///
    /// Retourne le nombre de triplets **nouveaux** (jamais un total : un second
    /// import du même fichier rend 0). Garanties :
    /// - **sans doublon** : les identifiants sont déterministes, `batch_store`
    ///   écrase à l'identique puis `hydrate_from` recharge l'union ;
    /// - **sans perte** : l'import contourne le routage contradictions (deux
    ///   triplets historiques en conflit direct cohabitent comme dans le
    ///   JSON), au contraire d'un `insert()` qui demanderait un arbitrage ;
    /// - **idempotent** : ni activation de poids ni duplication à la
    ///   réimportation.
    ///
    /// Les libellés nominaux du snapshot sont re-seedés en mémoire (meilleur
    /// effort : un libellé dont l'UUID ne dérive pas du nom est ignoré, cf.
    /// limites du module). Si un embedder est branché, les souvenirs texte
    /// importés sont indexés au passage.
    pub fn import_legacy_json(&mut self, path: &Path) -> Result<usize, MemoryRuntimeError> {
        let bytes = std::fs::read(path).map_err(|e| {
            MemoryRuntimeError::LegacyImport(format!(
                "lecture de {} impossible : {e}",
                path.display()
            ))
        })?;
        let snapshot: crate::graph::GraphSnapshot =
            serde_json::from_slice(&bytes).map_err(|e| {
                MemoryRuntimeError::LegacyImport(format!(
                    "snapshot JSON invalide ({}): {e}",
                    path.display()
                ))
            })?;
        let crate::graph::GraphSnapshot {
            triplets,
            subject_names,
            predicate_names,
            subject_labels,
            predicate_labels,
        } = snapshot;

        // Libellés AVANT hydration : `hydrate_from` les conserve pour les IDs
        // présents et ne remplace pas un libellé connu par le repli UUID.
        for (name, _) in &subject_names {
            self.graph.resolve_subject(name);
        }
        for (name, _) in &predicate_names {
            self.graph.resolve_predicate(name);
        }
        for (id, label) in &subject_labels {
            if &KnowledgeGraph::subject_id_for_name(label) == id {
                self.graph.resolve_subject(label);
            }
        }
        for (id, label) in &predicate_labels {
            if &KnowledgeGraph::predicate_id_for_name(label) == id {
                self.graph.resolve_predicate(label);
            }
        }

        let before: std::collections::HashSet<TripletId> = self.graph.ids().into_iter().collect();
        let imported_count = triplets.iter().filter(|t| !before.contains(&t.id)).count();

        block_on(self.edge.batch_store(triplets)).map_err(persist_error)?;
        block_on(self.graph.hydrate_from(&self.edge)).map_err(storage_error)?;

        let imported_ids: Vec<TripletId> = self
            .graph
            .ids()
            .into_iter()
            .filter(|id| !before.contains(id))
            .collect();
        debug_assert_eq!(imported_ids.len(), imported_count);
        if self.embedder.is_some() {
            self.index_imported(&imported_ids);
        }
        Ok(imported_count)
    }

    /// Réindexe tous les souvenirs texte (set_embedder). Best-effort :
    /// un vecteur de dimension invalide est loggé et ignoré, jamais propagé
    /// (signature `set_embedder` infaillible).
    fn reindex_embeddings(&mut self) {
        if self.embedder.is_none() {
            return;
        }
        let targets: Vec<(TripletId, String)> = self
            .graph
            .all_triplets()
            .into_iter()
            .filter(|t| !is_helper(t))
            .filter_map(|t| match &t.object {
                NodeValue::Text(text) => Some((t.id.clone(), text.clone())),
                _ => None,
            })
            .collect();
        let mut indexed = false;
        for (id, text) in targets {
            let vector = match self.embedder.as_ref() {
                Some(embed) => embed(&text),
                None => return,
            };
            if let Err(e) = validate_embedding(&vector, "set_embedder") {
                tracing::warn!(id = %id.0, "set_embedder : {e} — souvenir non indexé");
                continue;
            }
            if let Err(e) = self.vectors.insert(id, vector) {
                tracing::warn!("set_embedder : indexation refusée : {e}");
                continue;
            }
            indexed = true;
        }
        if indexed && let Err(e) = self.vectors.save(&self.db) {
            tracing::warn!("set_embedder : persistence des embeddings impossible : {e}");
        }
    }

    /// Indexe les souvenirs importés (texte uniquement), best-effort comme
    /// [`Self::reindex_embeddings`].
    fn index_imported(&mut self, ids: &[TripletId]) {
        let mut indexed = false;
        for id in ids {
            let text = match self.graph.get(id) {
                Some(triplet) if !is_helper(triplet) => match &triplet.object {
                    NodeValue::Text(text) => text.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            let vector = match self.embedder.as_ref() {
                Some(embed) => embed(&text),
                None => return,
            };
            if let Err(e) = validate_embedding(&vector, "import_legacy_json") {
                tracing::warn!(id = %id.0, "import_legacy_json : {e} — souvenir non indexé");
                continue;
            }
            if let Err(e) = self.vectors.insert(id.clone(), vector) {
                tracing::warn!("import_legacy_json : indexation refusée : {e}");
                continue;
            }
            indexed = true;
        }
        if indexed && let Err(e) = self.vectors.save(&self.db) {
            tracing::warn!("import_legacy_json : persistence des embeddings impossible : {e}");
        }
    }

    /// Rend une entrée affichable depuis un triplet. Pour un triplet de
    /// contenu (`hakille-memory:content`), les auxiliaires du même sujet sont
    /// relus pour restituer tags et source ; pour un triplet legacy, le texte
    /// est l'objet rendu et la source le `TripletSource` du triplet.
    fn entry_from_triplet(&self, triplet: &Triplet) -> MemoryEntry {
        let text = render_object(&triplet.object);
        let mut tags = Vec::new();
        let mut source = render_source(&triplet.metadata.source);
        if triplet.predicate == content_predicate_id() {
            let pattern = TriplePattern {
                subject: Some(triplet.subject.clone()),
                predicate: None,
                object: None,
            };
            for sibling in self.graph.query(&pattern) {
                match &sibling.object {
                    NodeValue::Text(value) if sibling.predicate == source_predicate_id() => {
                        source = value.clone();
                    }
                    NodeValue::Text(value)
                        if sibling.predicate == tag_predicate_id(value)
                            && !tags.contains(value) =>
                    {
                        tags.push(value.clone());
                    }
                    _ => {}
                }
            }
            tags.sort();
        }
        MemoryEntry {
            id: triplet.id.0.to_string(),
            text,
            tags,
            source,
            weight: triplet.metadata.weight,
            created_at: triplet.metadata.created_at.clone(),
            last_activated: triplet.metadata.last_activated.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Internes : dérivation des identifiants, espace réservé, rendu, erreurs.
// ---------------------------------------------------------------------------

/// Triplet auxiliaire (source ou tag) : objet `Text(valeur)`, métadonnées du
/// souvenir porteuses, ID déterministe comme tout triplet du graphe.
fn auxiliary_triplet(
    subject: &NodeId,
    predicate: PredicateId,
    value: String,
    metadata: &TripletMetadata,
) -> Triplet {
    let object = NodeValue::Text(value);
    let id = KnowledgeGraph::deterministic_triplet_id(subject, &predicate, &object);
    Triplet {
        id,
        subject: subject.clone(),
        predicate,
        object,
        metadata: metadata.clone(),
        node_type: Some(NodeType::Fact),
    }
}

/// Métadonnées par défaut d'un souvenir : poids 1.0, confiance 0.9, source
/// mappée sur [`TripletSource`], décroissance 0.01, partition `Default`.
fn memory_metadata(source: &str) -> TripletMetadata {
    let now = Timestamp::now();
    TripletMetadata {
        weight: 1.0,
        created_at: now.clone(),
        last_activated: now,
        source: map_source(source),
        confidence: 0.9,
        context: crate::context_partition::ContextPartition::Default,
        trust_level: TrustLevel::User,
        decay_rate: 0.01,
    }
}

/// `source: &str` → [`TripletSource`] (variantes historiques reconnues,
/// toute autre chaîne conservée intégralement en `External`).
fn map_source(source: &str) -> TripletSource {
    let trimmed = source.trim();
    match trimmed.to_ascii_lowercase().as_str() {
        "user" | "utilisateur" => TripletSource::User,
        "system" | "systeme" | "système" => TripletSource::System,
        "inferred" | "infere" | "inféré" => TripletSource::Inferred,
        _ => TripletSource::External(trimmed.to_string()),
    }
}

/// Rendu affichable d'un [`TripletSource`] (forme stable pour le HUD).
fn render_source(source: &TripletSource) -> String {
    match source {
        TripletSource::User => "user".to_string(),
        TripletSource::System => "system".to_string(),
        TripletSource::Inferred => "inferred".to_string(),
        TripletSource::External(value) => value.clone(),
    }
}

/// Rendu affichable d'un [`NodeValue`] (aucune information inventée :
/// l'UUID est montré tel quel, l'embedding résumé par sa dimension).
fn render_object(object: &NodeValue) -> String {
    match object {
        NodeValue::Text(text) => text.clone(),
        NodeValue::Number(number) => number.to_string(),
        NodeValue::Boolean(flag) => flag.to_string(),
        NodeValue::Timestamp(timestamp) => timestamp.0.to_rfc3339(),
        NodeValue::Entity(id) => id.0.to_string(),
        NodeValue::Embedding(vector) => format!("<embedding {} dimension(s)>", vector.len()),
    }
}

/// Correspondance lexicale IDENTIQUE au démon actuel
/// (`KnowledgeGraph::search_text`) : sous-chaîne casse-insensible pour le
/// texte, `to_string().contains` pour nombres/booléens, jamais pour les
/// entités/horodatages/embeddings. `lowered` est la requête déjà minusculée.
fn lexical_matches(object: &NodeValue, lowered: &str) -> bool {
    match object {
        NodeValue::Text(text) => text.to_lowercase().contains(lowered),
        NodeValue::Number(number) => number.to_string().contains(lowered),
        NodeValue::Boolean(flag) => flag.to_string().contains(lowered),
        _ => false,
    }
}

/// Vrai si le triplet est un auxiliaire interne (`hakille-memory:source` ou
/// tag dérivé de la valeur portée). Les triplets de contenu restent visibles.
fn is_helper(triplet: &Triplet) -> bool {
    match &triplet.object {
        NodeValue::Text(value) => {
            triplet.predicate == source_predicate_id()
                || triplet.predicate == tag_predicate_id(value)
        }
        _ => triplet.predicate == source_predicate_id(),
    }
}

/// Embedder de production verrouillé à 384 dims (MiniLM) : toute autre
/// dimension est une erreur honnête AVANT écriture.
fn validate_embedding(vector: &[f32], what: &str) -> Result<(), MemoryRuntimeError> {
    if vector.len() != EMBEDDING_DIM {
        return Err(MemoryRuntimeError::Embedding(format!(
            "{what} : dimension {} refusée (verrouillée à {EMBEDDING_DIM})",
            vector.len()
        )));
    }
    Ok(())
}

/// Prédicat du triplet de contenu.
fn content_predicate_id() -> PredicateId {
    KnowledgeGraph::predicate_id_for_name("hakille-memory:content")
}

/// Prédicat du triplet de source.
fn source_predicate_id() -> PredicateId {
    KnowledgeGraph::predicate_id_for_name("hakille-memory:source")
}

/// Prédicat d'un tag : sensible à la casse (deux tags `Work`/`work` ne se
/// contredisent donc pas), espace de noms réservé.
fn tag_predicate_id(tag: &str) -> PredicateId {
    PredicateId(hash_uuid("hakille-memory-tag", tag))
}

/// Sujet déterministe d'une mémoire : FNV-1a 128 sensible à la casse sur
/// `texte \u{1f} source \u{1f} tags triés`. Volontairement distinct de
/// [`KnowledgeGraph::deterministic_uuid`] (qui normalise en minuscules) :
/// `Alice` et `alice` restent deux souvenirs distincts.
fn memory_subject_id(text: &str, tags: &[String], source: &str) -> NodeId {
    let mut key = String::with_capacity(text.len() + source.len() + 2 + tags.len());
    key.push_str(text);
    key.push('\u{1f}');
    key.push_str(source);
    for tag in tags {
        key.push('\u{1f}');
        key.push_str(tag);
    }
    NodeId(hash_uuid("hakille-memory-subject", &key))
}

/// FNV-1a 64 (mêmes bases que le graphe, implémentation locale car celle de
/// `KnowledgeGraph` est privée).
fn fnv1a64(data: &[u8], basis: u64) -> u64 {
    const PRIME: u64 = 1_099_511_628_211;
    let mut hash = basis;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// UUID stable dérivé de `(namespace, value)` par double FNV-1a (même schéma
/// documenté que `KnowledgeGraph::deterministic_uuid`, SANS normalisation de
/// casse).
fn hash_uuid(namespace: &str, value: &str) -> uuid::Uuid {
    let mut first = Vec::with_capacity(namespace.len() + value.len() + 1);
    first.extend_from_slice(namespace.as_bytes());
    first.push(0x1F);
    first.extend_from_slice(value.as_bytes());
    let mut second = Vec::with_capacity(value.len() + namespace.len() + 1);
    second.extend_from_slice(value.as_bytes());
    second.push(0x1E);
    second.extend_from_slice(namespace.as_bytes());
    let h1 = fnv1a64(&first, 14_695_958_103_934_665_637);
    let h2 = fnv1a64(&second, 0x9E37_79B9_7F4A_7C15);
    uuid::Uuid::from_u128((u128::from(h1) << 64) | u128::from(h2))
}

fn storage_error(error: hakille_core::HakilleError) -> MemoryRuntimeError {
    MemoryRuntimeError::Storage(error.to_string())
}

fn persist_error(error: hakille_core::HakilleError) -> MemoryRuntimeError {
    MemoryRuntimeError::Persist(error.to_string())
}

fn embedding_error(error: hakille_core::HakilleError) -> MemoryRuntimeError {
    MemoryRuntimeError::Embedding(error.to_string())
}

/// Mini-exécuteur bloquant honnête : les futures `async` du graphe/edge
/// n'attendent jamais réellement (redb est bloquant enveloppé en `async`),
/// mais on boucle proprement jusqu'au `Ready` au lieu de `poll` une fois.
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    struct NoopWaker;
    impl std::task::Wake for NoopWaker {
        fn wake(self: Arc<Self>) {}
    }
    let waker = std::task::Waker::from(Arc::new(NoopWaker));
    let mut context = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => return value,
            std::task::Poll::Pending => std::thread::yield_now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_partition::ContextPartition;
    use crate::graph::GraphSnapshot;
    use uuid::Uuid;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hakille-runtime-{name}-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&dir).expect("création répertoire temp");
        dir
    }

    /// Embedder déterministe de test : sac de mots hachés (FNV) projeté sur
    /// 384 dims + composante de base (jamais de vecteur nul). Déterministe
    /// entre runs, sans dépendance ni modèle neural.
    fn hash_embedder() -> Embedder {
        Box::new(|text: &str| {
            let mut vector = vec![0.0f32; EMBEDDING_DIM];
            vector[EMBEDDING_DIM - 1] = 1.0;
            for token in text.to_lowercase().split_whitespace() {
                let hash = fnv1a64(token.as_bytes(), 0xcbf2_9ce4_8422_2325);
                let index = (hash % (EMBEDDING_DIM - 1) as u64) as usize;
                vector[index] += 1.0;
            }
            vector
        })
    }

    fn legacy_triplet(
        subject: NodeId,
        predicate: PredicateId,
        text: &str,
        confidence: f64,
    ) -> Triplet {
        let object = NodeValue::Text(text.to_string());
        let id = KnowledgeGraph::deterministic_triplet_id(&subject, &predicate, &object);
        Triplet {
            id,
            subject,
            predicate,
            object,
            metadata: TripletMetadata {
                weight: 0.8,
                created_at: Timestamp::now(),
                last_activated: Timestamp::now(),
                source: TripletSource::User,
                confidence,
                context: ContextPartition::Default,
                trust_level: TrustLevel::User,
                decay_rate: 0.01,
            },
            node_type: Some(NodeType::Fact),
        }
    }

    #[test]
    fn test_remember_recall_lexical_roundtrip() {
        let dir = temp_dir("lexical");
        let db = dir.join("memory.redb");
        let mut runtime = MemoryRuntime::open(&db).unwrap();
        assert!(runtime.is_empty());
        assert_eq!(runtime.path(), db.as_path());
        assert!(!runtime.has_embedder());

        let first = runtime
            .remember("Le café est prêt", &["perso".to_string()], "user")
            .unwrap();
        let again = runtime
            .remember("Le café est prêt", &["perso".to_string()], "user")
            .unwrap();
        assert_eq!(first, again, "remember est idempotent");
        runtime
            .remember("Réunion demain matin", &[], "daemon")
            .unwrap();
        assert_eq!(runtime.memory_count(), 2);

        let hits = runtime.recall("café", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].entry.id, first);
        assert_eq!(hits[0].entry.text, "Le café est prêt");
        assert_eq!(hits[0].entry.tags, vec!["perso".to_string()]);
        assert_eq!(hits[0].entry.source, "user");
        assert!(hits[0].score > 0.0);

        assert!(runtime.recall("introuvable", 10).unwrap().is_empty());
        // Requête vide = tout (comportement historique `search_text`), limité.
        assert_eq!(runtime.recall("", 10).unwrap().len(), 2);
        assert_eq!(runtime.recall("", 1).unwrap().len(), 1);
        assert_eq!(runtime.recent(10).unwrap().len(), 2);

        runtime.flush().unwrap();
        drop(runtime);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_remember_survives_reopen_with_tags_and_source() {
        let dir = temp_dir("reopen");
        let db = dir.join("memory.redb");
        {
            let mut runtime = MemoryRuntime::open(&db).unwrap();
            runtime
                .remember("souvenir persistant", &["test".to_string()], "daemon")
                .unwrap();
        }

        let runtime = MemoryRuntime::open(&db).unwrap();
        assert_eq!(runtime.memory_count(), 1);
        let hits = runtime.recall("persistant", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].entry.tags, vec!["test".to_string()]);
        assert_eq!(hits[0].entry.source, "daemon");
        assert_eq!(runtime.recent(5).unwrap().len(), 1);
        drop(runtime);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_vector_recall_with_deterministic_embedder() {
        let dir = temp_dir("vector");
        let db = dir.join("memory.redb");
        let mut runtime = MemoryRuntime::open(&db).unwrap();
        // Souvenir posé AVANT l'embedder : `set_embedder` doit le réindexer.
        runtime.remember("alpha sujet", &[], "user").unwrap();
        runtime.set_embedder(hash_embedder());
        assert!(runtime.has_embedder());
        runtime.remember("alpha beta mélange", &[], "user").unwrap();
        runtime.remember("gamma seule", &[], "user").unwrap();

        let hits = runtime.recall("alpha", 3).unwrap();
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].entry.text, "alpha sujet");
        assert_eq!(hits[1].entry.text, "alpha beta mélange");
        assert_eq!(hits[2].entry.text, "gamma seule");
        assert!(hits[0].score > hits[1].score);
        assert!(hits[1].score > hits[2].score);

        // Persistance des embeddings : réouverture + rebranchement.
        drop(runtime);
        let mut reopened = MemoryRuntime::open(&db).unwrap();
        reopened.set_embedder(hash_embedder());
        let hits = reopened.recall("alpha", 1).unwrap();
        assert_eq!(hits[0].entry.text, "alpha sujet");
        drop(reopened);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_import_legacy_json_idempotent_without_loss() {
        let dir = temp_dir("import");
        let db = dir.join("memory.redb");

        let alice = KnowledgeGraph::subject_id_for_name("Alice");
        let aime = KnowledgeGraph::predicate_id_for_name("aime");
        let ville = KnowledgeGraph::subject_id_for_name("Ville");
        let capitale = KnowledgeGraph::predicate_id_for_name("capitale");
        let t1 = legacy_triplet(alice.clone(), aime.clone(), "le café", 0.9);
        let t2 = legacy_triplet(ville.clone(), capitale.clone(), "Paris", 0.9);
        let t3 = legacy_triplet(ville.clone(), capitale.clone(), "Dakar", 0.9);

        // Le routage contradictions refuserait t3 (AskUser) : l'import doit
        // pourtant le conserver — c'est le test « sans perte ».
        let mut scratch = KnowledgeGraph::new();
        assert!(matches!(
            scratch.insert(t2.clone()),
            InsertResult::Inserted(_)
        ));
        assert!(matches!(
            scratch.insert(t3.clone()),
            InsertResult::NeedsUserResolution { .. }
        ));

        let snapshot = GraphSnapshot {
            triplets: vec![t1, t2, t3],
            subject_names: vec![
                ("Alice".to_string(), alice.clone()),
                ("Ville".to_string(), ville.clone()),
            ],
            predicate_names: vec![
                ("aime".to_string(), aime.clone()),
                ("capitale".to_string(), capitale.clone()),
            ],
            subject_labels: vec![
                (alice.clone(), "Alice".to_string()),
                (ville.clone(), "Ville".to_string()),
            ],
            predicate_labels: vec![
                (aime.clone(), "aime".to_string()),
                (capitale.clone(), "capitale".to_string()),
            ],
        };
        let json = dir.join("legacy.json");
        std::fs::write(&json, serde_json::to_vec_pretty(&snapshot).unwrap()).unwrap();

        let mut runtime = MemoryRuntime::open(&db).unwrap();
        assert_eq!(runtime.import_legacy_json(&json).unwrap(), 3);
        assert_eq!(
            runtime.memory_count(),
            3,
            "aucune perte : Paris ET Dakar importés"
        );
        assert_eq!(runtime.recall("Paris", 5).unwrap().len(), 1);
        assert_eq!(runtime.recall("Dakar", 5).unwrap().len(), 1);
        assert_eq!(runtime.recall("café", 5).unwrap().len(), 1);
        assert_eq!(runtime.graph().lookup_subject("alice"), Some(alice.clone()));
        assert_eq!(
            runtime.graph().lookup_predicate("capitale"),
            Some(capitale.clone())
        );

        // Réimport : idempotent, rien de neuf, aucun doublon.
        assert_eq!(runtime.import_legacy_json(&json).unwrap(), 0);
        assert_eq!(runtime.memory_count(), 3);
        assert_eq!(runtime.graph().len(), 3);

        // Fichier absent : erreur honnête.
        assert!(matches!(
            runtime
                .import_legacy_json(&dir.join("absent.json"))
                .unwrap_err(),
            MemoryRuntimeError::LegacyImport(_)
        ));

        drop(runtime);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_open_corrupted_database_is_an_honest_error() {
        let dir = temp_dir("corrupt");
        let db = dir.join("corrupt.redb");
        std::fs::write(&db, b"ceci n'est pas une base redb").unwrap();
        let error = MemoryRuntime::open(&db).expect_err("base corrompue doit rendre Err");
        assert!(
            matches!(error, MemoryRuntimeError::Storage(_)),
            "attendu Storage, obtenu : {error:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_wrong_dimension_embedder_rejected_before_any_write() {
        let dir = temp_dir("dim");
        let db = dir.join("memory.redb");
        let mut runtime = MemoryRuntime::open(&db).unwrap();
        runtime.set_embedder(Box::new(|_| vec![0.0f32; 8]));
        let error = runtime.remember("texte", &[], "user").unwrap_err();
        assert!(matches!(error, MemoryRuntimeError::Embedding(_)));
        assert!(
            runtime.is_empty(),
            "aucune écriture partielle après erreur d'embedding"
        );
        drop(runtime);
        std::fs::remove_dir_all(&dir).ok();
    }
}
