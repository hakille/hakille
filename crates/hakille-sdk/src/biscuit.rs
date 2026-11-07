//! Helpers pour l'atténuation et la vérification de capabilities Biscuit dans le SDK (§36).
//!
//! Facilite la gestion cryptographique des permissions déléguées aux agents tiers.

use hakille_core::{CapabilityAction, CapabilityScope, Timestamp};
use serde::{Deserialize, Serialize};

/// Atténuation d'accès pour un agent souverain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPermissionRequest {
    pub agent_id: String,
    pub scopes: Vec<CapabilityScope>,
    pub actions: Vec<CapabilityAction>,
    pub valid_duration_secs: u64,
}

impl AgentPermissionRequest {
    pub fn new(
        agent_id: impl Into<String>,
        scopes: Vec<CapabilityScope>,
        actions: Vec<CapabilityAction>,
        valid_duration_secs: u64,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            scopes,
            actions,
            valid_duration_secs,
        }
    }

    /// Calcule l'horodatage d'expiration absolu.
    pub fn compute_expiry(&self) -> Timestamp {
        Timestamp(chrono::Utc::now() + chrono::Duration::seconds(self.valid_duration_secs as i64))
    }
}
