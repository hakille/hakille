//! Biscuit v5 Datalog authorization verification against pinned root public keys.

use biscuit_auth::{AuthorizerLimits, Biscuit};
use hakille_core::{
    CapabilityAction, CapabilityScope, CapabilityToken, HakilleError, PinnedRootPublicKey,
};

/// Authorizer for verifying Biscuit v5 tokens with pinned root public key validation.
pub struct BiscuitVerifier;

impl BiscuitVerifier {
    /// Verifies token authenticity against pinned root public key, evaluating required scope and action.
    ///
    /// Monotonically verifies:
    /// 1. Token is not expired according to its temporal checks.
    /// 2. Token was signed by the expected `PinnedRootPublicKey` (rejecting attacker self-signed tokens).
    /// 3. Datalog evaluation satisfies `right(scope, action)`.
    pub fn verify_token(
        token: &CapabilityToken,
        root_public_key: &PinnedRootPublicKey,
        required_scope: &CapabilityScope,
        required_action: CapabilityAction,
    ) -> Result<(), HakilleError> {
        if token.is_expired() {
            return Err(HakilleError::CapabilityExpired);
        }

        let token_bytes = token.to_vec().map_err(|e| {
            HakilleError::CapabilityDenied(format!("Token serialization failed: {e}"))
        })?;

        let pinned_biscuit = Biscuit::from(token_bytes, *root_public_key).map_err(|e| {
            HakilleError::CapabilityDenied(format!("Pinned root key mismatch: {e}"))
        })?;

        let mut authorizer = pinned_biscuit
            .authorizer()
            .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;

        let now_ts = chrono::Utc::now().timestamp();
        authorizer
            .add_fact(format!("time({now_ts})").as_str())
            .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;

        let scope_name = required_scope.datalog_name();
        let action_name = format!("{:?}", required_action).to_lowercase();
        authorizer
            .add_fact(format!("operation(\"{scope_name}\", \"{action_name}\")").as_str())
            .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;

        authorizer
            .add_check(format!("check if right(\"{scope_name}\", \"{action_name}\")").as_str())
            .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;

        authorizer
            .allow()
            .map_err(|e| HakilleError::CapabilityDenied(e.to_string()))?;

        let limits = AuthorizerLimits {
            max_time: std::time::Duration::from_millis(500),
            ..Default::default()
        };

        authorizer.authorize_with_limits(limits).map_err(|e| {
            if token.is_expired() {
                HakilleError::CapabilityExpired
            } else {
                HakilleError::CapabilityDenied(format!("Biscuit authorization failed: {e}"))
            }
        })?;

        Ok(())
    }
}
