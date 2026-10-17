use annora_core::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Blocked,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskPriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStep {
    pub step_index: u32,
    pub description: String,
    pub capability_required: Option<String>,
    pub completed: bool,
    pub completed_at: Option<Timestamp>,
    pub result_summary: Option<String>,
}

/// A short-to-medium term bounded action sequence (§28e).
/// Unlike long-term [`Objective`], a `Task` has explicit operational boundaries,
/// step sequences, and immediate completion conditions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub parent_objective_id: Option<String>,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub steps: Vec<TaskStep>,
    pub created_at: Timestamp,
    pub deadline: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
}

impl Task {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: description.into(),
            parent_objective_id: None,
            status: TaskStatus::Pending,
            priority: TaskPriority::Normal,
            steps: Vec::new(),
            created_at: Timestamp::now(),
            deadline: None,
            completed_at: None,
        }
    }

    /// Variante validée : refuse les titres vides / blancs.
    pub fn try_new(
        id: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<Self, String> {
        let task = Self::new(id, title, description);
        task.validate()?;
        Ok(task)
    }

    /// Valide la tâche : titre non vide (sinon `Err`). Une `deadline`
    /// antérieure à `created_at` n'est qu'un avertissement (log `tracing` +
    /// `eprintln`), pas une erreur — l'échéance reste enregistrée.
    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty() {
            return Err("task title must not be empty".to_string());
        }
        if let Some(deadline) = &self.deadline {
            if *deadline < self.created_at {
                tracing::warn!(
                    task_id = %self.id,
                    "task deadline is before created_at"
                );
                eprintln!("warning: task '{}' deadline is before created_at", self.id);
            }
        }
        Ok(())
    }

    pub fn with_parent_objective(mut self, objective_id: impl Into<String>) -> Self {
        self.parent_objective_id = Some(objective_id.into());
        self
    }

    pub fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_deadline(mut self, deadline: Timestamp) -> Self {
        if deadline < self.created_at {
            tracing::warn!(
                task_id = %self.id,
                "task deadline is before created_at"
            );
            eprintln!("warning: task '{}' deadline is before created_at", self.id);
        }
        self.deadline = Some(deadline);
        self
    }

    pub fn add_step(
        &mut self,
        description: impl Into<String>,
        capability_required: Option<String>,
    ) -> u32 {
        let step_index = self.steps.len() as u32;
        self.steps.push(TaskStep {
            step_index,
            description: description.into(),
            capability_required,
            completed: false,
            completed_at: None,
            result_summary: None,
        });
        step_index
    }

    /// Retire l'étape `index` et réindexe les suivantes ; `false` si hors bornes.
    pub fn remove_step(&mut self, index: usize) -> bool {
        if index >= self.steps.len() {
            return false;
        }
        self.steps.remove(index);
        for (i, step) in self.steps.iter_mut().enumerate() {
            step.step_index = i as u32;
        }
        true
    }

    pub fn complete_step(&mut self, index: usize, summary: Option<String>) -> bool {
        if let Some(step) = self.steps.get_mut(index) {
            step.completed = true;
            step.completed_at = Some(Timestamp::now());
            step.result_summary = summary;

            // Auto-advance task state
            if self.steps.iter().all(|s| s.completed) {
                self.status = TaskStatus::Completed;
                self.completed_at = Some(Timestamp::now());
            } else {
                self.status = TaskStatus::InProgress;
            }
            true
        } else {
            false
        }
    }

    /// Marque la tâche en échec (terminal).
    pub fn fail(&mut self) {
        self.status = TaskStatus::Failed;
    }

    /// Marque la tâche bloquée (sauf si déjà `Completed` / `Failed`).
    pub fn block(&mut self) {
        if self.status != TaskStatus::Completed && self.status != TaskStatus::Failed {
            self.status = TaskStatus::Blocked;
        }
    }

    /// Rouvre la tâche : `InProgress` si une étape est terminée, sinon
    /// `Pending` ; efface `completed_at`.
    pub fn reopen(&mut self) {
        self.completed_at = None;
        if self.steps.iter().any(|s| s.completed) {
            self.status = TaskStatus::InProgress;
        } else {
            self.status = TaskStatus::Pending;
        }
    }

    pub fn progress_ratio(&self) -> f64 {
        if self.steps.is_empty() {
            return if self.status == TaskStatus::Completed {
                1.0
            } else {
                0.0
            };
        }
        let completed = self.steps.iter().filter(|s| s.completed).count();
        completed as f64 / self.steps.len() as f64
    }
}

/// Table redb des tâches (`id → Task` bincode).
pub const TASKS_TABLE: redb::TableDefinition<&[u8], &[u8]> = redb::TableDefinition::new("tasks");

/// Store persistant des tâches (lien `Task.parent_objective_id` conservé tel
/// quel — la jointure vers `ObjectiveStore` se fait par cet ID).
#[derive(Debug, Default)]
pub struct TaskStore {
    tasks: std::collections::HashMap<String, Task>,
}

impl TaskStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert(&mut self, task: Task) {
        self.tasks.insert(task.id.clone(), task);
    }

    pub fn get(&self, id: &str) -> Option<&Task> {
        self.tasks.get(id)
    }

    pub fn remove(&mut self, id: &str) -> bool {
        self.tasks.remove(id).is_some()
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Tâches rattachées à un objectif (`Task.parent_objective_id == Some(id)`).
    pub fn tasks_for_objective(&self, objective_id: &str) -> Vec<&Task> {
        self.tasks
            .values()
            .filter(|t| t.parent_objective_id.as_deref() == Some(objective_id))
            .collect()
    }

    /// Persiste tout le store en redb (écrase à l'identique, idempotent).
    pub fn save_to_db(&self, db: &redb::Database) -> annora_core::Result<()> {
        let txn = db
            .begin_write()
            .map_err(|e| annora_core::AnnoraError::Internal(e.to_string()))?;
        {
            let mut table = txn
                .open_table(TASKS_TABLE)
                .map_err(|e| annora_core::AnnoraError::Internal(e.to_string()))?;
            for (id, task) in &self.tasks {
                let bytes = bincode::serialize(task)
                    .map_err(|e| annora_core::AnnoraError::Internal(e.to_string()))?;
                table
                    .insert(id.as_bytes(), bytes.as_slice())
                    .map_err(|e| annora_core::AnnoraError::Internal(e.to_string()))?;
            }
        }
        txn.commit()
            .map_err(|e| annora_core::AnnoraError::Internal(e.to_string()))?;
        Ok(())
    }

    /// Recharge depuis redb (base fraîche ⇒ store vide, pas d'erreur).
    pub fn load_from_db(db: &redb::Database) -> annora_core::Result<Self> {
        let txn = db
            .begin_read()
            .map_err(|e| annora_core::AnnoraError::Internal(e.to_string()))?;
        let table = match txn.open_table(TASKS_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Self::new()),
            Err(e) => return Err(annora_core::AnnoraError::Internal(e.to_string())),
        };
        use redb::ReadableTable;
        let mut store = Self::new();
        for item in table
            .iter()
            .map_err(|e| annora_core::AnnoraError::Internal(e.to_string()))?
        {
            let (_, v) = item.map_err(|e| annora_core::AnnoraError::Internal(e.to_string()))?;
            let task: Task = bincode::deserialize(v.value())
                .map_err(|e| annora_core::AnnoraError::Internal(e.to_string()))?;
            store.tasks.insert(task.id.clone(), task);
        }
        Ok(store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_title_validation() {
        assert!(Task::try_new("t1", "", "desc").is_err());
        assert!(Task::try_new("t1", "   ", "desc").is_err());
        assert!(Task::try_new("t1", "Titre", "desc").is_ok());
        assert!(Task::new("t1", "", "desc").validate().is_err());
    }

    #[test]
    fn test_deadline_before_created_warns_but_keeps() {
        let task = Task::new("t1", "Titre", "desc");
        let past = Timestamp(task.created_at.0 - Duration::hours(1));
        // Pas de panique : avertissement seulement, échéance conservée.
        let task = task.with_deadline(past.clone());
        assert_eq!(task.deadline, Some(past));
        assert!(task.validate().is_ok());
    }

    #[test]
    fn test_fail_block_reopen() {
        let mut task = Task::new("t1", "Titre", "desc");
        task.block();
        assert_eq!(task.status, TaskStatus::Blocked);
        task.reopen();
        assert_eq!(task.status, TaskStatus::Pending);

        task.add_step("étape 1", None);
        task.complete_step(0, None);
        assert_eq!(task.status, TaskStatus::Completed);
        // Bloquer une tâche terminée : ignoré.
        task.block();
        assert_eq!(task.status, TaskStatus::Completed);
        task.reopen();
        assert_eq!(task.status, TaskStatus::InProgress);
        assert!(task.completed_at.is_none());

        task.fail();
        assert_eq!(task.status, TaskStatus::Failed);
        // Bloquer une tâche en échec : ignoré.
        task.block();
        assert_eq!(task.status, TaskStatus::Failed);
    }

    #[test]
    fn test_remove_step_reindexes() {
        let mut task = Task::new("t1", "Titre", "desc");
        task.add_step("a", None);
        task.add_step("b", None);
        task.add_step("c", None);
        assert!(task.remove_step(1));
        assert_eq!(task.steps.len(), 2);
        assert_eq!(task.steps[0].description, "a");
        assert_eq!(task.steps[1].description, "c");
        assert_eq!(task.steps[0].step_index, 0);
        assert_eq!(task.steps[1].step_index, 1);
        assert!(!task.remove_step(5));
    }
}
