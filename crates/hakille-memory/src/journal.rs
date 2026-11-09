//! Journal épisodique et checkpoint cognitif du démon. Une transaction redb
//! conserve ensemble les faits, les libellés, les conflits et le contexte court.
//! Les épisodes intégraux vivent séparément : compacter le contexte ne les efface pas.

use crate::{ContextPartition, GraphSnapshot, KnowledgeGraph, Triplet};
use hakille_core::{HakilleError, Result, Timestamp, TripletId, TrustLevel};
use redb::{Database, ReadableTable, TableDefinition, TableHandle};
use serde::{Deserialize, Serialize};
use std::path::Path;

const STATE: TableDefinition<&str, &[u8]> = TableDefinition::new("cognitive_checkpoint");
const EPISODES: TableDefinition<&str, &[u8]> = TableDefinition::new("cognitive_episodes");
const VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryCheckpoint {
    pub version: u32,
    pub graph: GraphSnapshot,
    pub pending_resolutions: Vec<(Triplet, Triplet)>,
    /// Le noyau valide son propre schéma ; aucune dépendance mémoire → noyau.
    pub context: serde_json::Value,
    #[serde(default)]
    pub revisions: Vec<FactRevision>,
    #[serde(default = "personal_partition")]
    pub active_partition: ContextPartition,
}

fn personal_partition() -> ContextPartition {
    ContextPartition::Personal
}

/// Historique des décisions ; jamais injecté comme un fait actuel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FactRevision {
    pub existing: Triplet,
    pub incoming: Triplet,
    pub accepted_incoming: bool,
    pub resolved_at: Timestamp,
}

impl MemoryCheckpoint {
    pub fn new(
        graph: &KnowledgeGraph,
        pending_resolutions: &[(Triplet, Triplet)],
        context: serde_json::Value,
    ) -> Self {
        Self {
            version: VERSION,
            graph: graph.snapshot(),
            pending_resolutions: pending_resolutions.to_vec(),
            context,
            revisions: Vec::new(),
            active_partition: personal_partition(),
        }
    }

    fn validate(&self) -> Result<()> {
        if self.version != VERSION {
            return Err(storage(format!(
                "version mémoire {} non supportée",
                self.version
            )));
        }
        KnowledgeGraph::from_snapshot(self.graph.clone())?;
        for (existing, incoming) in &self.pending_resolutions {
            existing.validate()?;
            incoming.validate()?;
        }
        for revision in &self.revisions {
            revision.existing.validate()?;
            revision.incoming.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EpisodeStatus {
    /// Un arrêt pendant l'inférence laisse cet état, jamais un succès inventé.
    Started,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryEpisode {
    pub id: String,
    pub created_at: Timestamp,
    pub trust_level: TrustLevel,
    pub partition: ContextPartition,
    pub prompt: String,
    pub response: Option<String>,
    pub error: Option<String>,
    pub status: EpisodeStatus,
    pub recalled_fact_ids: Vec<TripletId>,
    /// Trace d'outils et sources externes ; schéma validé par le noyau.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_trace: Option<serde_json::Value>,
}

impl MemoryEpisode {
    pub fn start(prompt: &str, trust: TrustLevel, partition: ContextPartition) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            created_at: Timestamp::now(),
            trust_level: trust,
            partition,
            prompt: prompt.to_owned(),
            response: None,
            error: None,
            status: EpisodeStatus::Started,
            recalled_fact_ids: Vec::new(),
            tool_trace: None,
        }
    }
}

pub struct MemoryJournal {
    db: Database,
}

fn storage(error: impl std::fmt::Display) -> HakilleError {
    HakilleError::StorageError(format!("journal mémoire : {error}"))
}

fn encode(value: &impl Serialize) -> Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(storage)
}

impl MemoryJournal {
    /// Une base illisible, verrouillée ou de version inconnue est refusée.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(storage)?;
        }
        let journal = Self {
            db: Database::create(path).map_err(storage)?,
        };
        if journal.checkpoint()?.is_none() {
            let txn = journal.db.begin_read().map_err(storage)?;
            for table in txn.list_tables().map_err(storage)? {
                if table.name() != "cognitive_episodes" {
                    return Err(storage(format!(
                        "table historique {} sans checkpoint cognitif : import explicite requis",
                        table.name()
                    )));
                }
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(storage)?;
        }
        Ok(journal)
    }

    pub fn checkpoint(&self) -> Result<Option<MemoryCheckpoint>> {
        let txn = self.db.begin_read().map_err(storage)?;
        let table = match txn.open_table(STATE) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(error) => return Err(storage(error)),
        };
        let Some(value) = table.get("current").map_err(storage)? else {
            return Err(storage("checkpoint absent dans une table existante"));
        };
        let checkpoint: MemoryCheckpoint =
            serde_json::from_slice(value.value()).map_err(storage)?;
        checkpoint.validate()?;
        Ok(Some(checkpoint))
    }

    /// Commit ACID unique. Le graphe complet est sérialisé en O(N) ; les
    /// archives ne sont jamais recopiées. Une optimisation incrémentale du
    /// checkpoint pourra conserver ce contrat sans changer le noyau.
    pub fn commit(
        &self,
        checkpoint: &MemoryCheckpoint,
        episode: Option<&MemoryEpisode>,
    ) -> Result<()> {
        self.commit_changes(checkpoint, episode, &[])
    }

    /// Checkpoint et suppressions d'archives atomiques (oubli sans résurrection).
    pub fn commit_changes(
        &self,
        checkpoint: &MemoryCheckpoint,
        episode: Option<&MemoryEpisode>,
        removed: &[String],
    ) -> Result<()> {
        checkpoint.validate()?;
        let state = encode(checkpoint)?;
        let episode_bytes = episode.map(encode).transpose()?;
        let txn = self.db.begin_write().map_err(storage)?;
        {
            txn.open_table(STATE)
                .map_err(storage)?
                .insert("current", state.as_slice())
                .map_err(storage)?;
            if let (Some(episode), Some(bytes)) = (episode, episode_bytes) {
                txn.open_table(EPISODES)
                    .map_err(storage)?
                    .insert(episode.id.as_str(), bytes.as_slice())
                    .map_err(storage)?;
            }
            if !removed.is_empty() {
                let mut table = txn.open_table(EPISODES).map_err(storage)?;
                for id in removed {
                    table.remove(id.as_str()).map_err(storage)?;
                }
            }
        }
        txn.commit().map_err(storage)
    }

    /// Écrit l'entrée AVANT l'inférence : même un tour interrompu reste lisible.
    pub fn record_started(&self, episode: &MemoryEpisode) -> Result<()> {
        let bytes = encode(episode)?;
        let txn = self.db.begin_write().map_err(storage)?;
        {
            let mut table = txn.open_table(EPISODES).map_err(storage)?;
            if table.get(episode.id.as_str()).map_err(storage)?.is_some() {
                return Err(storage("identifiant d'épisode déjà présent"));
            }
            table
                .insert(episode.id.as_str(), bytes.as_slice())
                .map_err(storage)?;
        }
        txn.commit().map_err(storage)
    }

    /// Recherche lexicale des archives, ou récence si `query` est vide.
    /// Mémoire de sélection bornée à `limit` (maximum 100), scan disque O(N).
    /// Les épisodes Guest/Untrusted ne sont jamais injectés dans un tour User.
    pub fn recall_episodes(
        &self,
        query: &str,
        limit: usize,
        partition: &ContextPartition,
        exclude: Option<&str>,
    ) -> Result<Vec<MemoryEpisode>> {
        let limit = limit.min(100);
        if limit == 0 {
            return Ok(Vec::new());
        }
        let txn = self.db.begin_read().map_err(storage)?;
        let table = match txn.open_table(EPISODES) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(error) => return Err(storage(error)),
        };
        let terms = crate::retrieval::lexical_terms(query);
        let mut selected: Vec<(usize, MemoryEpisode)> = Vec::new();
        for item in table.iter().map_err(storage)?.rev() {
            let (_, raw) = item.map_err(storage)?;
            let episode: MemoryEpisode = serde_json::from_slice(raw.value()).map_err(storage)?;
            if exclude == Some(episode.id.as_str())
                || !matches!(episode.trust_level, TrustLevel::User | TrustLevel::System)
                || !crate::retrieval::visible(&episode.partition, partition)
            {
                continue;
            }
            let words = crate::retrieval::lexical_terms(&format!(
                "{} {} {}",
                episode.prompt,
                episode.response.as_deref().unwrap_or_default(),
                episode
                    .tool_trace
                    .as_ref()
                    .map(|v| v.to_string())
                    .unwrap_or_default()
            ));
            let score = terms.intersection(&words).count();
            if !query.trim().is_empty() && score == 0 {
                continue;
            }
            selected.push((score, episode));
            selected.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.id.cmp(&a.1.id)));
            selected.truncate(limit);
            if query.trim().is_empty() && selected.len() == limit {
                break;
            }
        }
        Ok(selected.into_iter().map(|(_, e)| e).collect())
    }

    pub fn episode(&self, id: &str) -> Result<Option<MemoryEpisode>> {
        let txn = self.db.begin_read().map_err(storage)?;
        let table = match txn.open_table(EPISODES) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(error) => return Err(storage(error)),
        };
        table
            .get(id)
            .map_err(storage)?
            .map(|v| serde_json::from_slice(v.value()).map_err(storage))
            .transpose()
    }

    /// Suppression logique durable ; ne promet pas l'effacement physique des
    /// pages redb ni celui des exports et du journal d'audit distinct.
    pub fn forget_episode(&self, id: &str) -> Result<bool> {
        let txn = self.db.begin_write().map_err(storage)?;
        let removed = {
            let mut table = txn.open_table(EPISODES).map_err(storage)?;
            table.remove(id).map_err(storage)?.is_some()
        };
        txn.commit().map_err(storage)?;
        Ok(removed)
    }

    /// Références explicites et occurrences textuelles exactes d'un fait.
    /// Ne prétend pas détecter toutes les paraphrases sémantiques.
    pub fn episodes_mentioning(&self, fact: &Triplet) -> Result<Vec<String>> {
        let txn = self.db.begin_read().map_err(storage)?;
        let table = match txn.open_table(EPISODES) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(error) => return Err(storage(error)),
        };
        let object = crate::retrieval::object_text(&fact.object).to_lowercase();
        let mut ids = Vec::new();
        for item in table.iter().map_err(storage)? {
            let (_, raw) = item.map_err(storage)?;
            let episode: MemoryEpisode = serde_json::from_slice(raw.value()).map_err(storage)?;
            if episode.recalled_fact_ids.contains(&fact.id)
                || (!object.is_empty()
                    && (episode.prompt.to_lowercase().contains(&object)
                        || episode
                            .response
                            .as_deref()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&object)
                        || episode
                            .tool_trace
                            .as_ref()
                            .is_some_and(|v| v.to_string().to_lowercase().contains(&object))))
            {
                ids.push(episode.id);
            }
        }
        Ok(ids)
    }
}
