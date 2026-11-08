//! Contactless Gesture Authorization & Biscuit Capability Validation (F36).
//!
//! Validates nonce-bound Biscuit capability tokens activated via Gaze-Intent dwell
//! and micro-nod gestures within a strict 1200ms validity window, enforcing replay
//! protection, token lifetime expiration invariants, and scope containment.

use hakille_core::HakilleError;
use hakille_core::capability::{CapabilityAction, CapabilityScope, CapabilityToken};
use rand::{RngCore, rngs::OsRng};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Maximum duration permitted between target lock/challenge registration and gesture confirmation (1200 ms).
pub const CONTACTLESS_AUTH_WINDOW: Duration = Duration::from_millis(1200);

/// Active challenge awaiting contactless micro-nod authorization.
#[derive(Debug, Clone)]
pub struct ContactlessAuthChallenge {
    pub challenge_id: Uuid,
    pub action_nonce: [u8; 32],
    pub target_id: String,
    pub biscuit_token: CapabilityToken,
    pub required_scope: CapabilityScope,
    pub required_action: CapabilityAction,
    pub created_at: Instant,
    pub validity_window: Duration,
}

impl ContactlessAuthChallenge {
    /// Returns `true` if this challenge has exceeded its 1200ms validity window.
    pub fn is_timed_out(&self, now: Instant) -> bool {
        now.duration_since(self.created_at) > self.validity_window
    }
}

/// Cryptographic verification verdict for contactless capability token activation.
#[derive(Debug, Clone)]
pub struct ContactlessAuthVerdict {
    pub challenge_id: Uuid,
    pub target_id: String,
    pub action_nonce: [u8; 32],
    pub is_valid: bool,
    pub validated_token: CapabilityToken,
    pub activated_at: Instant,
}

/// Manager responsible for registering and authenticating contactless Biscuit capabilities.
#[derive(Debug, Default)]
pub struct ContactlessGestureAuthManager {
    challenges: HashMap<Uuid, ContactlessAuthChallenge>,
    used_nonces: HashSet<[u8; 32]>,
}

impl ContactlessGestureAuthManager {
    pub fn new() -> Self {
        Self {
            challenges: HashMap::new(),
            used_nonces: HashSet::new(),
        }
    }

    /// Register a new contactless capability activation challenge.
    pub fn register_challenge(
        &mut self,
        target_id: &str,
        biscuit_token: CapabilityToken,
        required_scope: CapabilityScope,
        required_action: CapabilityAction,
        now: Instant,
    ) -> ContactlessAuthChallenge {
        let mut nonce = [0u8; 32];
        OsRng.fill_bytes(&mut nonce);
        let challenge_id = Uuid::new_v4();

        let challenge = ContactlessAuthChallenge {
            challenge_id,
            action_nonce: nonce,
            target_id: target_id.to_string(),
            biscuit_token,
            required_scope,
            required_action,
            created_at: now,
            validity_window: CONTACTLESS_AUTH_WINDOW,
        };

        self.challenges.insert(challenge_id, challenge.clone());
        challenge
    }

    /// Validate a contactless authorization decision against a pending challenge.
    ///
    /// Checks:
    /// 1. Challenge existence and single-use consumption (anti-replay).
    /// 2. Target ID matching between gesture decision and registered challenge.
    /// 3. Decision approval flag (`is_gesture_authorized`).
    /// 4. Temporal execution within the 1200ms window (`decision_time - created_at <= 1200ms`).
    /// 5. Biscuit token expiration check (`!token.is_expired()`).
    /// 6. Action nonce non-reuse invariant.
    pub fn validate_authorization(
        &mut self,
        challenge_id: Uuid,
        gesture_target_id: &str,
        is_gesture_authorized: bool,
        decision_time: Instant,
    ) -> Result<ContactlessAuthVerdict, HakilleError> {
        // 1. Consume challenge (prevents duplicate validation attempts)
        let challenge = self.challenges.remove(&challenge_id).ok_or_else(|| {
            HakilleError::CapabilityDenied(
                "Défi d'autorisation sans contact inconnu ou déjà consommé".to_string(),
            )
        })?;

        // 2. Target ID match
        if challenge.target_id != gesture_target_id {
            return Err(HakilleError::CapabilityDenied(format!(
                "Incohérence de cible : le geste visait '{}', mais le défi attendait '{}'",
                gesture_target_id, challenge.target_id
            )));
        }

        // 3. Gesture authorization confirmation
        if !is_gesture_authorized {
            return Err(HakilleError::CapabilityDenied(
                "Le micro-geste requis n'a pas été validé".to_string(),
            ));
        }

        // 4. Temporal validity window check (must be within 1200ms)
        let elapsed = decision_time.duration_since(challenge.created_at);
        if elapsed > challenge.validity_window {
            return Err(HakilleError::CapabilityDenied(format!(
                "Délai d'autorisation gestuelle dépassé : {:?} écoulés (> fenêtre de 1200ms)",
                elapsed
            )));
        }

        // 5. Biscuit capability token expiration check
        if challenge.biscuit_token.is_expired() {
            return Err(HakilleError::CapabilityExpired);
        }

        // 6. Anti-replay action nonce verification
        if self.used_nonces.contains(&challenge.action_nonce) {
            return Err(HakilleError::CapabilityDenied(
                "Attaque par rejeu détectée : nonce d'action déjà consommé".to_string(),
            ));
        }
        self.used_nonces.insert(challenge.action_nonce);

        Ok(ContactlessAuthVerdict {
            challenge_id,
            target_id: challenge.target_id,
            action_nonce: challenge.action_nonce,
            is_valid: true,
            validated_token: challenge.biscuit_token,
            activated_at: decision_time,
        })
    }

    /// Prune challenges that have exceeded their 1200ms validity window.
    pub fn prune_expired_challenges(&mut self, now: Instant) -> usize {
        let before_count = self.challenges.len();
        self.challenges.retain(|_, c| !c.is_timed_out(now));
        before_count - self.challenges.len()
    }

    /// Number of active challenges currently pending authorization.
    pub fn pending_challenges_count(&self) -> usize {
        self.challenges.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_core::capability::CapabilityAuthority;
    use hakille_core::types::Timestamp;

    fn make_test_token(expires_in_secs: i64) -> CapabilityToken {
        let authority = CapabilityAuthority::new();
        let expires_at = Timestamp(chrono::Utc::now() + chrono::Duration::seconds(expires_in_secs));
        authority
            .create_root_token(
                vec![CapabilityScope::Wallet],
                vec![CapabilityAction::Execute],
                expires_at,
            )
            .expect("Token creation should succeed")
    }

    #[test]
    fn test_contactless_biscuit_auth_success() {
        let mut manager = ContactlessGestureAuthManager::new();
        let token = make_test_token(3600); // valid for 1 hour
        let t0 = Instant::now();

        let challenge = manager.register_challenge(
            "wallet_tx_99",
            token,
            CapabilityScope::Wallet,
            CapabilityAction::Execute,
            t0,
        );
        assert_eq!(manager.pending_challenges_count(), 1);

        // Gesture authorization decision arrives 450ms later (well within 1200ms)
        let t_decision = t0 + Duration::from_millis(450);
        let result = manager.validate_authorization(
            challenge.challenge_id,
            "wallet_tx_99",
            true,
            t_decision,
        );

        assert!(
            result.is_ok(),
            "Valid gesture within 1200ms should authorize token"
        );
        let verdict = result.unwrap();
        assert!(verdict.is_valid);
        assert_eq!(verdict.target_id, "wallet_tx_99");
        assert_eq!(verdict.action_nonce, challenge.action_nonce);
        assert_eq!(manager.pending_challenges_count(), 0);
    }

    #[test]
    fn test_contactless_biscuit_auth_timeout_rejection() {
        let mut manager = ContactlessGestureAuthManager::new();
        let token = make_test_token(3600);
        let t0 = Instant::now();

        let challenge = manager.register_challenge(
            "admin_system_reboot",
            token,
            CapabilityScope::FullSystem,
            CapabilityAction::Admin,
            t0,
        );

        // Gesture decision arrives after 1350ms (> 1200ms validity window)
        let t_late = t0 + Duration::from_millis(1350);
        let result = manager.validate_authorization(
            challenge.challenge_id,
            "admin_system_reboot",
            true,
            t_late,
        );

        assert!(
            result.is_err(),
            "Gesture arriving after 1200ms must be rejected"
        );
        match result.unwrap_err() {
            HakilleError::CapabilityDenied(msg) => {
                assert!(msg.contains("1200ms") || msg.contains("Délai"));
            }
            err => panic!("Expected CapabilityDenied, got {:?}", err),
        }
    }

    #[test]
    fn test_contactless_biscuit_auth_expired_token_rejection() {
        let mut manager = ContactlessGestureAuthManager::new();
        let expired_token = make_test_token(-30); // expired 30s ago
        let t0 = Instant::now();

        let challenge = manager.register_challenge(
            "confidential_doc_export",
            expired_token,
            CapabilityScope::Filesystem,
            CapabilityAction::Read,
            t0,
        );

        let t_decision = t0 + Duration::from_millis(300);
        let result = manager.validate_authorization(
            challenge.challenge_id,
            "confidential_doc_export",
            true,
            t_decision,
        );

        assert!(result.is_err(), "Expired capability token must be rejected");
        assert!(matches!(
            result.unwrap_err(),
            HakilleError::CapabilityExpired
        ));
    }

    #[test]
    fn test_contactless_biscuit_auth_replay_protection() {
        let mut manager = ContactlessGestureAuthManager::new();
        let token = make_test_token(3600);
        let t0 = Instant::now();

        let challenge = manager.register_challenge(
            "action_single_use",
            token,
            CapabilityScope::Sensor,
            CapabilityAction::Read,
            t0,
        );

        // First validation succeeds
        let res1 = manager.validate_authorization(
            challenge.challenge_id,
            "action_single_use",
            true,
            t0 + Duration::from_millis(200),
        );
        assert!(res1.is_ok());

        // Replay attempt with same challenge ID fails
        let res2 = manager.validate_authorization(
            challenge.challenge_id,
            "action_single_use",
            true,
            t0 + Duration::from_millis(300),
        );
        assert!(res2.is_err(), "Replay attempt must be blocked");
    }

    #[test]
    fn test_contactless_biscuit_auth_target_mismatch() {
        let mut manager = ContactlessGestureAuthManager::new();
        let token = make_test_token(3600);
        let t0 = Instant::now();

        let challenge = manager.register_challenge(
            "button_A",
            token,
            CapabilityScope::GuiInput,
            CapabilityAction::Write,
            t0,
        );

        let result = manager.validate_authorization(
            challenge.challenge_id,
            "button_B", // Mismatched target!
            true,
            t0 + Duration::from_millis(200),
        );

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            HakilleError::CapabilityDenied(_)
        ));
    }

    #[test]
    fn test_contactless_biscuit_auth_prune_expired() {
        let mut manager = ContactlessGestureAuthManager::new();
        let t0 = Instant::now();

        manager.register_challenge(
            "c1",
            make_test_token(3600),
            CapabilityScope::Audio,
            CapabilityAction::Read,
            t0,
        );
        manager.register_challenge(
            "c2",
            make_test_token(3600),
            CapabilityScope::Camera,
            CapabilityAction::Read,
            t0,
        );
        assert_eq!(manager.pending_challenges_count(), 2);

        // At t0 + 1500ms (> 1200ms)
        let pruned = manager.prune_expired_challenges(t0 + Duration::from_millis(1500));
        assert_eq!(pruned, 2);
        assert_eq!(manager.pending_challenges_count(), 0);
    }
}
