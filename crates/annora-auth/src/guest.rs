use annora_core::{CapabilityScope, Timestamp, UserId};
use serde::{Deserialize, Serialize};

/// Temporary guest profile with bounded time and capability scope (§30e).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestSession {
    pub guest_id: String,
    pub host_user_id: UserId,
    pub allowed_scopes: Vec<CapabilityScope>,
    pub purpose: String,
    pub created_at: Timestamp,
    pub expires_at: Timestamp,
    pub is_active: bool,
}

impl GuestSession {
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now();
        now > self.expires_at.0
    }

    pub fn allows_scope(&self, scope: &CapabilityScope) -> bool {
        if self.is_expired() || !self.is_active {
            return false;
        }
        // Strict boundary: Guest can NEVER access Wallet or FullSystem capabilities (§30e, §19e)
        if matches!(scope, CapabilityScope::Wallet | CapabilityScope::FullSystem) {
            return false;
        }
        self.allowed_scopes.contains(scope)
    }
}

/// Manager for temporary guest delegations (§30e).
pub struct GuestSessionManager {
    sessions: Vec<GuestSession>,
}

impl Default for GuestSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GuestSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
        }
    }

    /// Provision a time-bounded guest session.
    pub fn create_guest_session(
        &mut self,
        guest_id: impl Into<String>,
        host_user_id: UserId,
        allowed_scopes: Vec<CapabilityScope>,
        purpose: impl Into<String>,
        duration_minutes: u32,
    ) -> Result<GuestSession, String> {
        // Enforce strict security invariant: Guest can never hold Wallet or FullSystem
        if allowed_scopes.contains(&CapabilityScope::Wallet) || allowed_scopes.contains(&CapabilityScope::FullSystem) {
            return Err("Un invité ne peut en aucun cas recevoir les capabilities Wallet ou FullSystem (§30e).".to_string());
        }

        let now = chrono::Utc::now();
        let expires_at = now + chrono::Duration::minutes(duration_minutes as i64);

        let session = GuestSession {
            guest_id: guest_id.into(),
            host_user_id,
            allowed_scopes,
            purpose: purpose.into(),
            created_at: Timestamp(now),
            expires_at: Timestamp(expires_at),
            is_active: true,
        };

        self.sessions.push(session.clone());
        Ok(session)
    }

    /// Check if an active guest can perform an action in a scope.
    pub fn can_guest_access(&self, guest_id: &str, scope: &CapabilityScope) -> bool {
        if let Some(session) = self.sessions.iter().find(|s| s.guest_id == guest_id) {
            session.allows_scope(scope)
        } else {
            false
        }
    }

    /// Cleanup expired sessions automatically.
    pub fn purge_expired(&mut self) -> usize {
        let initial_count = self.sessions.len();
        self.sessions.retain(|s| !s.is_expired() && s.is_active);
        initial_count - self.sessions.len()
    }
}
