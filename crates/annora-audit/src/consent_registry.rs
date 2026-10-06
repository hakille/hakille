use annora_core::{ActorId, CapabilityAction, CapabilityScope, Timestamp};
use serde::{Deserialize, Serialize};

/// An explicit cryptographic record of authorized capability permissions (§20i).
/// Distinct from individual action execution entries; provides an instant, tamper-evident
/// overview of "Which powers have I granted, to whom, and since when?".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRecord {
    pub grant_id: String,
    pub subject_actor: ActorId,
    pub granted_scopes: Vec<CapabilityScope>,
    pub granted_actions: Vec<CapabilityAction>,
    pub purpose: String,
    pub valid_from: Timestamp,
    pub valid_until: Option<Timestamp>,
    pub revoked_at: Option<Timestamp>,
    pub revocation_reason: Option<String>,
}

impl ConsentRecord {
    pub fn is_currently_active(&self) -> bool {
        if self.revoked_at.is_some() {
            return false;
        }
        if let Some(ref until) = self.valid_until {
            let now = chrono::Utc::now();
            if now > until.0 {
                return false;
            }
        }
        true
    }

    pub fn covers_scope(&self, scope: &CapabilityScope) -> bool {
        self.is_currently_active() && self.granted_scopes.contains(scope)
    }
}

/// Consent Registry (§20i).
/// Dedicated registry answering in one query "What permissions did I grant?",
/// without requiring forensic reconstruction across millions of action log entries.
pub struct ConsentRegistry {
    records: Vec<ConsentRecord>,
}

impl Default for ConsentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsentRegistry {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    /// Record a newly granted capability permission.
    pub fn register_grant(
        &mut self,
        grant_id: impl Into<String>,
        actor: ActorId,
        scopes: Vec<CapabilityScope>,
        actions: Vec<CapabilityAction>,
        purpose: impl Into<String>,
        valid_until: Option<Timestamp>,
    ) -> String {
        let id_str = grant_id.into();
        let record = ConsentRecord {
            grant_id: id_str.clone(),
            subject_actor: actor,
            granted_scopes: scopes,
            granted_actions: actions,
            purpose: purpose.into(),
            valid_from: Timestamp::now(),
            valid_until,
            revoked_at: None,
            revocation_reason: None,
        };
        self.records.push(record);
        id_str
    }

    /// Revoke an active permission immediately.
    pub fn revoke_grant(&mut self, grant_id: &str, reason: impl Into<String>) -> bool {
        if let Some(record) = self.records.iter_mut().find(|r| r.grant_id == grant_id) {
            record.revoked_at = Some(Timestamp::now());
            record.revocation_reason = Some(reason.into());
            true
        } else {
            false
        }
    }

    /// Instant query: list all currently active permissions across the entire system.
    pub fn list_active_consents(&self) -> Vec<&ConsentRecord> {
        self.records.iter().filter(|r| r.is_currently_active()).collect()
    }

    /// Query active permissions for a specific agent/actor.
    pub fn get_actor_consents(&self, actor: &ActorId) -> Vec<&ConsentRecord> {
        self.records
            .iter()
            .filter(|r| &r.subject_actor == actor && r.is_currently_active())
            .collect()
    }

    /// Verify if actor has active consent for a scope.
    pub fn has_active_consent(&self, actor: &ActorId, scope: &CapabilityScope) -> bool {
        self.records
            .iter()
            .any(|r| &r.subject_actor == actor && r.covers_scope(scope))
    }
}
