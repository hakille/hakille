use hakille_core::{CapabilityScope, HakilleError, Timestamp, UserId};
use serde::{Deserialize, Serialize};

/// Strict 24-hour maximum duration cap for all guest delegations (§30e, F23).
pub const MAX_GUEST_DURATION_MINUTES: u32 = 1440;

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

    /// Remaining valid duration in minutes, or 0 if expired
    pub fn remaining_minutes(&self) -> i64 {
        if self.is_expired() || !self.is_active {
            return 0;
        }
        let now = chrono::Utc::now();
        (self.expires_at.0 - now).num_minutes().max(0)
    }

    pub fn allows_scope(&self, scope: &CapabilityScope) -> bool {
        if self.is_expired() || !self.is_active {
            return false;
        }
        // Strict boundary: Guest can NEVER access Wallet or FullSystem capabilities (§30e, §19e, F23)
        if matches!(scope, CapabilityScope::Wallet | CapabilityScope::FullSystem) {
            return false;
        }
        self.allowed_scopes.contains(scope)
    }
}

/// Manager for temporary guest delegations (§30e, F23).
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

    /// Enforce strict guest boundaries:
    /// - Duration capped strictly at 24 hours (1440 minutes)
    /// - Zero duration prohibited
    /// - Wallet and FullSystem scopes unconditionally forbidden
    pub fn validate_guest_constraints(
        scopes: &[CapabilityScope],
        duration_minutes: u32,
    ) -> Result<(), HakilleError> {
        if scopes.contains(&CapabilityScope::Wallet)
            || scopes.contains(&CapabilityScope::FullSystem)
        {
            return Err(HakilleError::CapabilityDenied(
                "Constitutional Guest Invariant: Guests are strictly barred from Wallet and FullSystem privileges (§30e).".to_string()
            ));
        }
        if duration_minutes == 0 {
            return Err(HakilleError::CapabilityDenied(
                "Guest session duration cannot be zero.".to_string(),
            ));
        }
        if duration_minutes > MAX_GUEST_DURATION_MINUTES {
            return Err(HakilleError::CapabilityDenied(format!(
                "Guest session duration exceeds strict 24h cap ({} > {} min).",
                duration_minutes, MAX_GUEST_DURATION_MINUTES
            )));
        }
        Ok(())
    }

    /// Provision a time-bounded guest session.
    ///
    /// Bornes fail-closed :
    /// - durée max 24 h (`duration_minutes <= 1440`), refus au-delà ;
    /// - `duration_minutes == 0` refusé ;
    /// - `guest_id` vide → token UUID unique généré (`guest-<uuidv4>`) ;
    /// - doublon d'`id` (session existante) refusé.
    pub fn create_guest_session(
        &mut self,
        guest_id: impl Into<String>,
        host_user_id: UserId,
        allowed_scopes: Vec<CapabilityScope>,
        purpose: impl Into<String>,
        duration_minutes: u32,
    ) -> Result<GuestSession, String> {
        // Enforce strict security invariant: Guest can never hold Wallet or FullSystem
        if allowed_scopes.contains(&CapabilityScope::Wallet)
            || allowed_scopes.contains(&CapabilityScope::FullSystem)
        {
            return Err("Un invité ne peut en aucun cas recevoir les capabilities Wallet ou FullSystem (§30e).".to_string());
        }

        if duration_minutes == 0 {
            return Err("Durée invité nulle refusée.".to_string());
        }
        if duration_minutes > MAX_GUEST_DURATION_MINUTES {
            return Err("Durée invité bornée à 24 h max (1440 min) — refus au-delà.".to_string());
        }

        let mut gid = guest_id.into();
        if gid.trim().is_empty() {
            gid = format!("guest-{}", uuid::Uuid::new_v4());
        }
        if self.sessions.iter().any(|s| s.guest_id == gid) {
            return Err(format!(
                "guest_id '{}' déjà utilisé (token unique exigé)",
                gid
            ));
        }

        let now = chrono::Utc::now();
        let expires_at = now + chrono::Duration::minutes(duration_minutes as i64);

        let session = GuestSession {
            guest_id: gid,
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

    /// Retrieve guest session by guest_id
    pub fn get_session(&self, guest_id: &str) -> Option<&GuestSession> {
        self.sessions.iter().find(|s| s.guest_id == guest_id)
    }

    /// Revoke an active guest session immediately
    pub fn revoke_session(&mut self, guest_id: &str) -> bool {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.guest_id == guest_id) {
            session.is_active = false;
            true
        } else {
            false
        }
    }

    /// Count of currently active (non-expired, non-revoked) guest sessions
    pub fn active_session_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|s| !s.is_expired() && s.is_active)
            .count()
    }

    /// Cleanup expired sessions automatically.
    pub fn purge_expired(&mut self) -> usize {
        let initial_count = self.sessions.len();
        self.sessions.retain(|s| !s.is_expired() && s.is_active);
        initial_count - self.sessions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_guest_strict_validation_helper() {
        assert!(
            GuestSessionManager::validate_guest_constraints(&[CapabilityScope::Audio], 60).is_ok()
        );
        assert!(
            GuestSessionManager::validate_guest_constraints(&[CapabilityScope::Wallet], 60)
                .is_err()
        );
        assert!(
            GuestSessionManager::validate_guest_constraints(&[CapabilityScope::FullSystem], 60)
                .is_err()
        );
        assert!(
            GuestSessionManager::validate_guest_constraints(&[CapabilityScope::Audio], 0).is_err()
        );
        assert!(
            GuestSessionManager::validate_guest_constraints(&[CapabilityScope::Audio], 1441)
                .is_err()
        );
    }

    #[test]
    fn test_guest_revocation_and_remaining_duration() {
        let mut mgr = GuestSessionManager::new();
        let host = UserId(Uuid::new_v4());
        let session = mgr
            .create_guest_session("g-rev", host, vec![CapabilityScope::Audio], "test", 120)
            .unwrap();

        assert!(session.remaining_minutes() > 100);
        assert_eq!(mgr.active_session_count(), 1);

        assert!(mgr.revoke_session("g-rev"));
        assert!(!mgr.can_guest_access("g-rev", &CapabilityScope::Audio));
        assert_eq!(mgr.active_session_count(), 0);
    }
}
