pub mod consent_registry;
pub mod entry;
pub mod journal;
pub mod query;

pub use consent_registry::*;
pub use entry::*;
pub use journal::*;
pub use query::*;

#[cfg(test)]
mod tests {
    use super::*;
    use annora_core::{ActorId, CapabilityAction, CapabilityScope};

    #[test]
    fn test_consent_registry_grant_and_revocation() {
        let mut registry = ConsentRegistry::new();
        let agent_actor = ActorId::Agent("mail-agent".to_string());

        // Grant Filesystem read access
        let grant_id = registry
            .register_grant(
                "grant-mail-fs",
                agent_actor.clone(),
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Read],
                "Lecture des courriels archivés",
                None,
            )
            .unwrap();

        assert!(registry.has_active_consent(
            &agent_actor,
            &CapabilityScope::Filesystem,
            &CapabilityAction::Read
        ));
        // Même scope mais action non accordée : refus.
        assert!(!registry.has_active_consent(
            &agent_actor,
            &CapabilityScope::Filesystem,
            &CapabilityAction::Write
        ));
        assert!(!registry.has_active_consent(
            &agent_actor,
            &CapabilityScope::Wallet,
            &CapabilityAction::Read
        ));
        assert_eq!(registry.list_active_consents().len(), 1);

        // Revoke grant
        let revoked =
            registry.revoke_grant(&grant_id, "Utilisateur a révoqué l'accès aux fichiers");
        assert!(revoked);
        assert!(!registry.has_active_consent(
            &agent_actor,
            &CapabilityScope::Filesystem,
            &CapabilityAction::Read
        ));
        assert_eq!(registry.list_active_consents().len(), 0);
    }
}
