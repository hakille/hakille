//! Biscuit v5 Capabilities and Offline Attenuation Engine (F11).

use hakille_core::capability::{
    CapabilityAction, CapabilityAuthority, CapabilityScope, CapabilityToken, PinnedRootPublicKey,
};
use hakille_core::types::Timestamp;
use hakille_core::HakilleError;
use std::collections::HashSet;

pub struct BiscuitCapabilityManager {
    pub authority: CapabilityAuthority,
    revocation_list: HashSet<Vec<u8>>,
}

impl Default for BiscuitCapabilityManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BiscuitCapabilityManager {
    pub fn new() -> Self {
        Self {
            authority: CapabilityAuthority::new(),
            revocation_list: HashSet::new(),
        }
    }

    pub fn root_public_key(&self) -> PinnedRootPublicKey {
        self.authority.root_public_key()
    }

    /// Mint a root capability token
    pub fn mint_root_token(
        &self,
        scopes: Vec<CapabilityScope>,
        actions: Vec<CapabilityAction>,
        expires_at: Timestamp,
    ) -> Result<CapabilityToken, HakilleError> {
        self.authority
            .create_root_token(scopes, actions, expires_at)
    }

    /// Attenuate an existing capability token with strictly monotonic scope narrowing.
    /// Invariant: Scope_child ⊆ Scope_parent, Action_child ⊆ Action_parent, Expiry_child ≤ Expiry_parent.
    pub fn attenuate_token(
        &self,
        parent: &CapabilityToken,
        parent_scopes: &[CapabilityScope],
        parent_actions: &[CapabilityAction],
        child_scopes: &[CapabilityScope],
        child_actions: &[CapabilityAction],
        child_expires_at: Timestamp,
    ) -> Result<CapabilityToken, HakilleError> {
        // Enforce maximum attenuation depth (D_max = 4)
        // Root token has 1 block; each attenuation adds 1 block.
        // If parent has > MAX_DELEGATION_DEPTH blocks, further attenuation exceeds D_max.
        if parent.biscuit.block_count() > hakille_core::auth::MAX_DELEGATION_DEPTH {
            return Err(HakilleError::CapabilityDenied(format!(
                "Biscuit delegation depth exceeded: current blocks {} > max allowed {}",
                parent.biscuit.block_count(),
                hakille_core::auth::MAX_DELEGATION_DEPTH
            )));
        }

        // Enforce Scope Monotonicity
        for s in child_scopes {
            if !parent_scopes.contains(s) {
                return Err(HakilleError::CapabilityDenied(format!(
                    "Monotonicity violation: child scope {:?} not present in parent",
                    s
                )));
            }
        }

        // Enforce Action Monotonicity
        for a in child_actions {
            if !parent_actions.contains(a) {
                return Err(HakilleError::CapabilityDenied(format!(
                    "Monotonicity violation: child action {:?} not present in parent",
                    a
                )));
            }
        }

        // Enforce Temporal Monotonicity
        if parent
            .min_expiry_timestamp()
            .is_some_and(|parent_exp| child_expires_at.0.timestamp() > parent_exp)
        {
            return Err(HakilleError::CapabilityDenied(
                "Monotonicity violation: child token expiry exceeds parent token expiry"
                    .to_string(),
            ));
        }

        let child_biscuit = self.authority.attenuate(
            parent,
            child_scopes.to_vec(),
            child_actions.to_vec(),
            child_expires_at,
        )?;
        Ok(child_biscuit)
    }

    pub fn revoke_token(&mut self, token: &CapabilityToken) -> Result<(), HakilleError> {
        let bytes = token.to_vec()?;
        self.revocation_list.insert(bytes);
        Ok(())
    }

    pub fn revoke_token_bytes(&mut self, token_bytes: Vec<u8>) {
        self.revocation_list.insert(token_bytes);
    }

    pub fn is_revoked(&self, token: &CapabilityToken) -> bool {
        if let Ok(bytes) = token.to_vec() {
            self.revocation_list.contains(&bytes)
        } else {
            true
        }
    }

    pub fn is_revoked_bytes(&self, token_bytes: &[u8]) -> bool {
        self.revocation_list.contains(token_bytes)
    }

    pub fn revocation_count(&self) -> usize {
        self.revocation_list.len()
    }

    pub fn verify_token(
        &self,
        token: &CapabilityToken,
        scope: CapabilityScope,
        action: CapabilityAction,
    ) -> Result<(), HakilleError> {
        if self.is_revoked(token) {
            return Err(HakilleError::CapabilityDenied(
                "Capability token is revoked".to_string(),
            ));
        }
        self.authority.verify_pinned(token, scope, action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_biscuit_attenuation_depth_bounding() {
        let mgr = BiscuitCapabilityManager::new();
        let exp = Timestamp(chrono::Utc::now() + chrono::Duration::hours(5));

        let mut token = mgr
            .mint_root_token(
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Read],
                exp.clone(),
            )
            .unwrap();

        // 4 attenuations succeed
        for _ in 1..=4 {
            token = mgr
                .attenuate_token(
                    &token,
                    &[CapabilityScope::Filesystem],
                    &[CapabilityAction::Read],
                    &[CapabilityScope::Filesystem],
                    &[CapabilityAction::Read],
                    exp.clone(),
                )
                .expect("attenuation up to depth 4 must succeed");
        }

        // 5th attenuation must fail with CapabilityDenied
        let d5_res = mgr.attenuate_token(
            &token,
            &[CapabilityScope::Filesystem],
            &[CapabilityAction::Read],
            &[CapabilityScope::Filesystem],
            &[CapabilityAction::Read],
            exp,
        );
        assert!(d5_res.is_err(), "5th attenuation must exceed max delegation depth");
    }

    #[test]
    fn test_biscuit_byte_revocation_helpers() {
        let mut mgr = BiscuitCapabilityManager::new();
        let raw_token = vec![0x12, 0x34, 0x56, 0x78];

        assert!(!mgr.is_revoked_bytes(&raw_token));
        assert_eq!(mgr.revocation_count(), 0);

        mgr.revoke_token_bytes(raw_token.clone());
        assert!(mgr.is_revoked_bytes(&raw_token));
        assert_eq!(mgr.revocation_count(), 1);
    }
}
