//! Hybrid Access Control Engine (F10):
//! Fusion of ReBAC (Zanzibar graph) + RBAC (static role) + ABAC (continuous confidence tau & device tier) + Biscuit.

use crate::hierarchy::TenantId;
use crate::rebac::{RebacEngine, RelationTuple};
use hakille_core::{CapabilityAction, CapabilityScope};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeviceTrustTier {
    Tier3UntrustedWeb,
    Tier2SoftwareHost,
    Tier1HardwareEnclave,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    pub subject: String,
    pub base_role: String,
    pub confidence_score: f64,
    pub device_tier: DeviceTrustTier,
    pub tenant_id: TenantId,
    pub scope: CapabilityScope,
    pub action: CapabilityAction,
    pub resource: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny(String),
}

impl Decision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Decision::Allow)
    }
}

pub struct AccessEngine {
    pub rebac: RebacEngine,
}

impl Default for AccessEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AccessEngine {
    pub fn new() -> Self {
        Self {
            rebac: RebacEngine::new(),
        }
    }

    /// Multi-engine evaluation: ReBAC && RBAC && ABAC
    pub fn check_permission(&self, ctx: &AuthContext, tuple: &RelationTuple) -> Decision {
        // Validate continuous confidence score in [0.0, 1.0]
        if ctx.confidence_score.is_nan() || !(0.0..=1.0).contains(&ctx.confidence_score) {
            return Decision::Deny(format!(
                "ABAC rejection: invalid continuous confidence score ({:.2} must be within [0.0, 1.0])",
                ctx.confidence_score
            ));
        }

        // 1. ReBAC check (Zanzibar relationship graph)
        let rebac_ok = self.rebac.check_relation_transitive(
            &tuple.entity_type,
            &tuple.entity_id,
            &tuple.relation,
            &tuple.subject_type,
            &tuple.subject_id,
        );
        if !rebac_ok {
            return Decision::Deny(format!(
                "ReBAC rejection: relationship '{}' missing between {}:{} and {}:{}",
                tuple.relation,
                tuple.entity_type,
                tuple.entity_id,
                tuple.subject_type,
                tuple.subject_id
            ));
        }

        // 2. RBAC check (Base role compatibility and minor protection Rule 7)
        let is_guest = ctx.base_role.eq_ignore_ascii_case("Guest");
        let is_minor = ctx.base_role.to_lowercase().contains("minor") || ctx.base_role == "Dependent";

        if is_guest && matches!(ctx.scope, CapabilityScope::Wallet | CapabilityScope::FullSystem) {
            return Decision::Deny(
                "RBAC rejection: Guest role can never access Wallet or FullSystem".to_string(),
            );
        }

        if is_minor && matches!(ctx.scope, CapabilityScope::Wallet | CapabilityScope::FullSystem) {
            return Decision::Deny(
                "RBAC rejection: Minor accounts are constitutionally prohibited from Wallet and FullSystem access (Rule 7)"
                    .to_string(),
            );
        }

        // 3. ABAC check (Continuous confidence score tau and hardware trust tier)
        let min_tau = match (&ctx.scope, &ctx.action) {
            (CapabilityScope::Wallet | CapabilityScope::FullSystem, _) => 0.90,
            (CapabilityScope::ModelInference, _) => 0.70,
            (CapabilityScope::Filesystem, CapabilityAction::Write | CapabilityAction::Delete) => {
                0.70
            }
            (CapabilityScope::Filesystem, _) => 0.40,
            _ => 0.40,
        };

        if ctx.confidence_score < min_tau {
            return Decision::Deny(format!(
                "ABAC rejection: insufficient continuous confidence score ({:.2} < required {:.2})",
                ctx.confidence_score, min_tau
            ));
        }

        // Hardware device trust check for high-security scopes
        if matches!(ctx.scope, CapabilityScope::Wallet | CapabilityScope::FullSystem)
            && ctx.device_tier < DeviceTrustTier::Tier2SoftwareHost
        {
            return Decision::Deny(
                "ABAC rejection: high-security operation requires at least Tier2 host trust"
                    .to_string(),
            );
        }

        Decision::Allow
    }

    /// Unified 4-pillar evaluation: ReBAC && RBAC && ABAC && Biscuit capability verification
    pub fn check_permission_with_token(
        &self,
        ctx: &AuthContext,
        tuple: &RelationTuple,
        biscuit_mgr: &crate::biscuit::BiscuitCapabilityManager,
        token: &hakille_core::capability::CapabilityToken,
    ) -> Decision {
        let base_decision = self.check_permission(ctx, tuple);
        if !base_decision.is_allowed() {
            return base_decision;
        }

        // 4. Biscuit Datalog capability token check
        if let Err(e) = biscuit_mgr.verify_token(token, ctx.scope.clone(), ctx.action.clone()) {
            return Decision::Deny(format!("Biscuit capability rejection: {e}"));
        }

        Decision::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::biscuit::BiscuitCapabilityManager;
    use hakille_core::types::Timestamp;

    #[test]
    fn test_access_engine_invalid_confidence_denied() {
        let engine = AccessEngine::new();
        let ctx = AuthContext {
            subject: "user:u".into(),
            base_role: "Member".into(),
            confidence_score: f64::NAN,
            device_tier: DeviceTrustTier::Tier1HardwareEnclave,
            tenant_id: TenantId::new(),
            scope: CapabilityScope::Filesystem,
            action: CapabilityAction::Read,
            resource: "res".into(),
        };
        let tuple = RelationTuple::new("res", "r1", "read", "user", "u");
        let dec = engine.check_permission(&ctx, &tuple);
        assert!(!dec.is_allowed(), "NaN confidence must be denied");
    }

    #[test]
    fn test_access_engine_minor_wallet_denied() {
        let mut engine = AccessEngine::new();
        engine.rebac.add_tuple(RelationTuple::new("wallet", "w1", "owner", "user", "minor_user"));

        let ctx = AuthContext {
            subject: "user:minor_user".into(),
            base_role: "MinorDependent".into(),
            confidence_score: 1.0,
            device_tier: DeviceTrustTier::Tier1HardwareEnclave,
            tenant_id: TenantId::new(),
            scope: CapabilityScope::Wallet,
            action: CapabilityAction::Execute,
            resource: "w1".into(),
        };
        let tuple = RelationTuple::new("wallet", "w1", "owner", "user", "minor_user");
        let dec = engine.check_permission(&ctx, &tuple);
        assert!(!dec.is_allowed(), "Rule 7 must block minor wallet access");
    }

    #[test]
    fn test_access_engine_unified_check_with_token() {
        let mut engine = AccessEngine::new();
        let biscuit_mgr = BiscuitCapabilityManager::new();
        let exp = Timestamp(chrono::Utc::now() + chrono::Duration::hours(1));

        engine.rebac.add_tuple(RelationTuple::new("doc", "d1", "reader", "user", "alice"));

        let token = biscuit_mgr
            .mint_root_token(
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Read],
                exp,
            )
            .unwrap();

        let ctx = AuthContext {
            subject: "user:alice".into(),
            base_role: "Reader".into(),
            confidence_score: 0.85,
            device_tier: DeviceTrustTier::Tier1HardwareEnclave,
            tenant_id: TenantId::new(),
            scope: CapabilityScope::Filesystem,
            action: CapabilityAction::Read,
            resource: "d1".into(),
        };
        let tuple = RelationTuple::new("doc", "d1", "reader", "user", "alice");

        // Valid token passes
        let dec = engine.check_permission_with_token(&ctx, &tuple, &biscuit_mgr, &token);
        assert!(dec.is_allowed());

        // Context with scope not in token (e.g. Wallet) fails
        let ctx_wallet = AuthContext {
            scope: CapabilityScope::Wallet,
            action: CapabilityAction::Execute,
            ..ctx
        };
        let dec_wallet = engine.check_permission_with_token(&ctx_wallet, &tuple, &biscuit_mgr, &token);
        assert!(!dec_wallet.is_allowed());
    }
}
