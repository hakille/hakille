use annora_core::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Objective {
    pub id: String,
    pub title: String,
    pub description: String,
    pub milestones: Vec<Milestone>,
    pub created_at: Timestamp,
    pub target_date: Option<Timestamp>,
    pub status: ObjectiveStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Milestone {
    pub id: String,
    pub description: String,
    pub completed: bool,
    pub completed_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ObjectiveStatus {
    Active,
    Paused,
    Completed,
    Abandoned,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ObjectiveError {
    #[error("objective title must not be empty")]
    EmptyTitle,
    #[error("milestone description must not be empty")]
    EmptyMilestone,
    #[error("milestone not found: {0}")]
    MilestoneNotFound(String),
}

impl Objective {
    /// Crée un objectif `Active` ; refuse les titres vides / blancs.
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<Self, ObjectiveError> {
        let title = title.into();
        if title.trim().is_empty() {
            return Err(ObjectiveError::EmptyTitle);
        }
        Ok(Self {
            id: id.into(),
            title,
            description: description.into(),
            milestones: Vec::new(),
            created_at: Timestamp::now(),
            target_date: None,
            status: ObjectiveStatus::Active,
        })
    }

    /// Statut courant.
    pub fn status(&self) -> &ObjectiveStatus {
        &self.status
    }

    pub fn set_status(&mut self, status: ObjectiveStatus) {
        self.status = status;
    }

    /// Ajoute un jalon ; refuse les descriptions vides. Rend l'ID du jalon.
    pub fn add_milestone(
        &mut self,
        description: impl Into<String>,
    ) -> Result<String, ObjectiveError> {
        let description = description.into();
        if description.trim().is_empty() {
            return Err(ObjectiveError::EmptyMilestone);
        }
        let id = uuid::Uuid::now_v7().to_string();
        self.milestones.push(Milestone {
            id: id.clone(),
            description,
            completed: false,
            completed_at: None,
        });
        Ok(id)
    }

    /// Termine un jalon ; passe l'objectif à `Completed` quand tous les
    /// jalons sont terminés. `Err(MilestoneNotFound)` si ID inconnu.
    pub fn complete_milestone(&mut self, milestone_id: &str) -> Result<(), ObjectiveError> {
        let m = self
            .milestones
            .iter_mut()
            .find(|m| m.id == milestone_id)
            .ok_or_else(|| ObjectiveError::MilestoneNotFound(milestone_id.to_string()))?;
        m.completed = true;
        m.completed_at = Some(Timestamp::now());
        if !self.milestones.is_empty() && self.milestones.iter().all(|m| m.completed) {
            self.status = ObjectiveStatus::Completed;
        }
        Ok(())
    }

    /// Ratio de jalons terminés (`1.0` si aucun jalon et `Completed`, sinon `0.0`).
    pub fn progress_ratio(&self) -> f64 {
        if self.milestones.is_empty() {
            return if self.status == ObjectiveStatus::Completed {
                1.0
            } else {
                0.0
            };
        }
        let done = self.milestones.iter().filter(|m| m.completed).count();
        done as f64 / self.milestones.len() as f64
    }
}

/// Table redb des objectifs (`id → Objective` bincode).
pub const OBJECTIVES_TABLE: redb::TableDefinition<&[u8], &[u8]> =
    redb::TableDefinition::new("objectives");

/// Store persistant des objectifs long terme. Les tâches s'y rattachent via
/// `Task.parent_objective_id` (côté `TaskStore::tasks_for_objective`).
#[derive(Debug, Default)]
pub struct ObjectiveStore {
    objectives: std::collections::HashMap<String, Objective>,
}

impl ObjectiveStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert(&mut self, objective: Objective) {
        self.objectives.insert(objective.id.clone(), objective);
    }

    pub fn get(&self, id: &str) -> Option<&Objective> {
        self.objectives.get(id)
    }

    pub fn remove(&mut self, id: &str) -> bool {
        self.objectives.remove(id).is_some()
    }

    pub fn len(&self) -> usize {
        self.objectives.len()
    }

    pub fn is_empty(&self) -> bool {
        self.objectives.is_empty()
    }

    pub fn save_to_db(&self, db: &redb::Database) -> annora_core::Result<()> {
        let txn = db
            .begin_write()
            .map_err(|e| annora_core::AnnoraError::Internal(e.to_string()))?;
        {
            let mut table = txn
                .open_table(OBJECTIVES_TABLE)
                .map_err(|e| annora_core::AnnoraError::Internal(e.to_string()))?;
            for (id, obj) in &self.objectives {
                let bytes = bincode::serialize(obj)
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

    pub fn load_from_db(db: &redb::Database) -> annora_core::Result<Self> {
        let txn = db
            .begin_read()
            .map_err(|e| annora_core::AnnoraError::Internal(e.to_string()))?;
        let table = match txn.open_table(OBJECTIVES_TABLE) {
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
            let obj: Objective = bincode::deserialize(v.value())
                .map_err(|e| annora_core::AnnoraError::Internal(e.to_string()))?;
            store.objectives.insert(obj.id.clone(), obj);
        }
        Ok(store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_validates_title() {
        assert!(Objective::new("o1", "", "desc").is_err());
        assert!(Objective::new("o1", "   ", "desc").is_err());
        let o = Objective::new("o1", "Apprendre Rust", "desc").unwrap();
        assert_eq!(o.title, "Apprendre Rust");
        assert_eq!(*o.status(), ObjectiveStatus::Active);
        assert_eq!(o.progress_ratio(), 0.0);
    }

    #[test]
    fn test_milestones_progress_and_auto_complete() {
        let mut o = Objective::new("o1", "Titre", "desc").unwrap();
        assert!(o.add_milestone("").is_err());
        let m1 = o.add_milestone("Jalon 1").unwrap();
        let m2 = o.add_milestone("Jalon 2").unwrap();
        assert!((o.progress_ratio() - 0.0).abs() < 1e-12);

        o.complete_milestone(&m1).unwrap();
        assert!((o.progress_ratio() - 0.5).abs() < 1e-12);
        assert_eq!(*o.status(), ObjectiveStatus::Active);

        o.complete_milestone(&m2).unwrap();
        assert!((o.progress_ratio() - 1.0).abs() < 1e-12);
        assert_eq!(*o.status(), ObjectiveStatus::Completed);

        assert!(matches!(
            o.complete_milestone("inconnu").unwrap_err(),
            ObjectiveError::MilestoneNotFound(_)
        ));
    }
}
