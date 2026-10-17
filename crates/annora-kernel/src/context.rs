use annora_core::{Timestamp, UserId};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEntry {
    pub timestamp: Timestamp,
    pub speaker: String,
    pub content: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveGoal {
    pub id: String,
    pub description: String,
    pub created_at: Timestamp,
    pub progress: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionContext {
    pub user_id: UserId,
    pub conversation_stream: VecDeque<ContextEntry>,
    pub max_stream_entries: usize,
    pub active_goals: Vec<ActiveGoal>,
    pub current_focus: Option<String>,
    pub last_interaction: Timestamp,
    /// Résumé honnête des échanges archivés (heuristique, pas de LLM) :
    /// quand `conversation_stream` dépasse 20 entrées on ne garde que les
    /// 10 dernières et on comptabilise le reste ici.
    pub archived_summary: Option<String>,
    pub total_archived: usize,
}

impl InteractionContext {
    pub fn new(user_id: UserId, max_stream_entries: usize) -> Self {
        Self {
            user_id,
            conversation_stream: VecDeque::with_capacity(max_stream_entries),
            max_stream_entries,
            active_goals: Vec::new(),
            current_focus: None,
            last_interaction: Timestamp::now(),
            archived_summary: None,
            total_archived: 0,
        }
    }

    pub fn record_interaction(&mut self, speaker: &str, content: &str, tags: Vec<String>) {
        if self.conversation_stream.len() >= self.max_stream_entries {
            self.conversation_stream.pop_front();
        }
        self.conversation_stream.push_back(ContextEntry {
            timestamp: Timestamp::now(),
            speaker: speaker.to_string(),
            content: content.to_string(),
            tags,
        });
        self.last_interaction = Timestamp::now();
        self.maybe_summarize();
    }

    /// Résumé auto honnête (pas de LLM) : si plus de 20 échanges en mémoire,
    /// on garde les 10 derniers et on remplace le reste par un compteur.
    /// Le "résumé" est un comptage + rappel du focus, pas une synthèse
    /// sémantique — ne pas le présenter comme une compréhension.
    pub fn maybe_summarize(&mut self) {
        const KEEP_LAST: usize = 10;
        const TRIGGER: usize = 20;
        if self.conversation_stream.len() <= TRIGGER {
            return;
        }
        let excess = self.conversation_stream.len() - KEEP_LAST;
        // Draine les plus anciens, garde les 10 derniers.
        for _ in 0..excess {
            self.conversation_stream.pop_front();
        }
        self.total_archived += excess;
        let focus = self
            .current_focus
            .as_deref()
            .unwrap_or("sans focus explicite");
        self.archived_summary = Some(format!(
            "Résumé auto (heuristique): {} échanges archivés au total, {} derniers conservés. Focus: {}.",
            self.total_archived, KEEP_LAST, focus
        ));
    }

    /// Persistance JSON (string) — pour sauvegarde souveraine locale.
    pub fn to_json(&self) -> annora_core::Result<String> {
        serde_json::to_string(self)
            .map_err(|e| annora_core::AnnoraError::SerializationError(e.to_string()))
    }

    pub fn from_json(json: &str) -> annora_core::Result<Self> {
        serde_json::from_str(json)
            .map_err(|e| annora_core::AnnoraError::SerializationError(e.to_string()))
    }

    /// Persistance JSON (fichier) — chemin local souverain uniquement.
    pub fn save_to_file(&self, path: &std::path::Path) -> annora_core::Result<()> {
        let json = self.to_json()?;
        std::fs::write(path, json)
            .map_err(|e| annora_core::AnnoraError::StorageError(e.to_string()))
    }

    pub fn load_from_file(path: &std::path::Path) -> annora_core::Result<Self> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| annora_core::AnnoraError::StorageError(e.to_string()))?;
        Self::from_json(&raw)
    }

    pub fn add_goal(&mut self, id: impl Into<String>, description: impl Into<String>) {
        self.active_goals.push(ActiveGoal {
            id: id.into(),
            description: description.into(),
            created_at: Timestamp::now(),
            progress: 0.0,
        });
    }

    pub fn set_focus(&mut self, focus: impl Into<String>) {
        self.current_focus = Some(focus.into());
    }

    pub fn recent_exchanges(&self, count: usize) -> Vec<&ContextEntry> {
        self.conversation_stream.iter().rev().take(count).collect()
    }

    /// Sujets récents (contenus tronqués) pour `using_graph_orphans` :
    /// évite de relancer un sujet déjà abordé (initiative bornée).
    pub fn recent_topics(&self, count: usize) -> Vec<String> {
        self.recent_exchanges(count)
            .into_iter()
            .map(|e| e.content.chars().take(120).collect::<String>())
            .collect()
    }
}
