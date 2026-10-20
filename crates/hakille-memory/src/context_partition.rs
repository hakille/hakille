use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ContextPartition {
    Default,
    Professional,
    Personal,
    Shared,
    Custom(String),
}

pub struct ContextManager {
    pub active_partition: ContextPartition,
}

impl ContextManager {
    /// Construit un gestionnaire avec la partition active donnée.
    pub fn new(active: ContextPartition) -> Self {
        Self {
            active_partition: active,
        }
    }

    /// Alias de [`Self::new`] (construction directe).
    pub fn with_partition(active: ContextPartition) -> Self {
        Self::new(active)
    }

    /// Bascule historique (compatibilité) : affecte sans validation.
    pub fn activate_partition(&mut self, partition: ContextPartition) {
        self.active_partition = partition;
    }

    /// Bascule validée : refuse les `Custom` vides/blancs (retourne `false`
    /// sans rien changer), applique sinon (retourne `true`).
    pub fn switch(&mut self, partition: ContextPartition) -> bool {
        if let ContextPartition::Custom(ref name) = partition
            && !Self::validate_custom(name)
        {
            return false;
        }
        self.active_partition = partition;
        true
    }

    /// Partition active courante.
    pub fn active(&self) -> &ContextPartition {
        &self.active_partition
    }

    /// Partitions connues : les 4 natives + l'active si c'est un `Custom`
    /// distinct.
    pub fn list_partitions(&self) -> Vec<ContextPartition> {
        let mut out = vec![
            ContextPartition::Default,
            ContextPartition::Professional,
            ContextPartition::Personal,
            ContextPartition::Shared,
        ];
        if let ContextPartition::Custom(_) = self.active_partition
            && !out.contains(&self.active_partition)
        {
            out.push(self.active_partition.clone());
        }
        out
    }

    /// Valide un nom de partition personnalisée (non vide / non blanc).
    pub fn validate_custom(name: &str) -> bool {
        !name.trim().is_empty()
    }
}

/// Contrôle d'accès lecture : un triplet est visible depuis `active` si sa
/// partition égale l'active, ou — quand `include_shared` vaut `true`
/// (défaut d'usage) — si l'un des deux côtés est `Shared` (espace commun
/// lisible de partout, et réciproquement l'espace `Shared` voit tout).
pub fn is_visible(
    triplet_partition: &ContextPartition,
    active: &ContextPartition,
    include_shared: bool,
) -> bool {
    if triplet_partition == active {
        return true;
    }
    if include_shared
        && (*triplet_partition == ContextPartition::Shared || *active == ContextPartition::Shared)
    {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_and_switch() {
        let mut mgr = ContextManager::new(ContextPartition::Personal);
        assert_eq!(mgr.active(), &ContextPartition::Personal);

        assert!(mgr.switch(ContextPartition::Professional));
        assert_eq!(mgr.active(), &ContextPartition::Professional);

        // Custom vide ⇒ refus, partition inchangée.
        assert!(!mgr.switch(ContextPartition::Custom("   ".to_string())));
        assert_eq!(mgr.active(), &ContextPartition::Professional);

        // Custom valide ⇒ accepté.
        assert!(mgr.switch(ContextPartition::Custom("projet-x".to_string())));
        assert_eq!(
            mgr.active(),
            &ContextPartition::Custom("projet-x".to_string())
        );
    }

    #[test]
    fn test_list_partitions_includes_custom() {
        let mgr = ContextManager::with_partition(ContextPartition::Default);
        let list = mgr.list_partitions();
        assert!(list.contains(&ContextPartition::Default));
        assert!(list.contains(&ContextPartition::Shared));
        assert_eq!(list.len(), 4);

        let custom = ContextManager::new(ContextPartition::Custom("x".to_string()));
        let list = custom.list_partitions();
        assert_eq!(list.len(), 5);
        assert!(list.contains(&ContextPartition::Custom("x".to_string())));
    }

    #[test]
    fn test_validate_custom() {
        assert!(ContextManager::validate_custom("projet-x"));
        assert!(!ContextManager::validate_custom(""));
        assert!(!ContextManager::validate_custom("   "));
    }

    #[test]
    fn test_is_visible() {
        // Même partition ⇒ visible.
        assert!(is_visible(
            &ContextPartition::Personal,
            &ContextPartition::Personal,
            true
        ));
        // Partition différente ⇒ invisible.
        assert!(!is_visible(
            &ContextPartition::Personal,
            &ContextPartition::Professional,
            true
        ));
        // Shared lisible de partout (défaut include_shared=true).
        assert!(is_visible(
            &ContextPartition::Shared,
            &ContextPartition::Personal,
            true
        ));
        // … mais exclu si include_shared=false.
        assert!(!is_visible(
            &ContextPartition::Shared,
            &ContextPartition::Personal,
            false
        ));
        // L'espace Shared voit tout quand include_shared=true.
        assert!(is_visible(
            &ContextPartition::Personal,
            &ContextPartition::Shared,
            true
        ));
    }
}
