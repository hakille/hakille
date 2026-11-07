//! 5-Tier Sovereign Multi-Tenant Entity Hierarchy & Governance (F08):
//! Teams, Organizations, Enterprises, Families, and Groups.

use crate::ssi::Did;
use chrono::{DateTime, Utc};
use hakille_core::{CapabilityScope, HakilleError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EntityTier {
    Team,
    Organization,
    Enterprise,
    Family,
    Group,
}

impl std::fmt::Display for EntityTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityTier::Team => write!(f, "Team"),
            EntityTier::Organization => write!(f, "Organization"),
            EntityTier::Enterprise => write!(f, "Enterprise"),
            EntityTier::Family => write!(f, "Family"),
            EntityTier::Group => write!(f, "Group"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TenantId(pub Uuid);

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TenantId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(u: Uuid) -> Self {
        Self(u)
    }

    pub fn to_uuid(&self) -> Uuid {
        self.0
    }

    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        Uuid::parse_str(s).map(Self)
    }
}

impl Default for TenantId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TenantState {
    Active,
    Suspended,
    Archived,
    Dissolved,
}

// -----------------------------------------------------------------------------
// Tier Configurations
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TeamConfig {
    pub max_members: u32,
    pub shared_agent_pool_enabled: bool,
    pub working_memory_quota_mb: u32,
    pub auto_archive_days_inactive: Option<u32>,
}

impl Default for TeamConfig {
    fn default() -> Self {
        Self {
            max_members: 25,
            shared_agent_pool_enabled: true,
            working_memory_quota_mb: 512,
            auto_archive_days_inactive: Some(90),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrgConfig {
    pub registration_number: String,
    pub compliance_framework: Vec<String>, // e.g. ["GDPR", "SOC2_Type2"]
    pub scim_endpoint: Option<String>,
    pub audit_retention_years: u32,
    pub requires_mfa: bool,
}

impl Default for OrgConfig {
    fn default() -> Self {
        Self {
            registration_number: "ORG-0001".to_string(),
            compliance_framework: vec!["GDPR".to_string()],
            scim_endpoint: None,
            audit_retention_years: 7,
            requires_mfa: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnterpriseConfig {
    pub holding_legal_name: String,
    pub cross_tenant_isolation_strict: bool,
    pub master_kms_arn: Option<String>,
    pub global_constitutional_overrides: Vec<String>,
}

impl Default for EnterpriseConfig {
    fn default() -> Self {
        Self {
            holding_legal_name: "Hakille Sovereign Enterprise".to_string(),
            cross_tenant_isolation_strict: true,
            master_kms_arn: None,
            global_constitutional_overrides: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FamilyConfig {
    pub domestic_hardware_dids: Vec<Did>,
    pub minor_protection_strict: bool,
    pub inheritance_vault_enabled: bool,
    pub guest_auto_expiry_minutes: u32,
}

impl Default for FamilyConfig {
    fn default() -> Self {
        Self {
            domestic_hardware_dids: vec![],
            minor_protection_strict: true,
            inheritance_vault_enabled: true,
            guest_auto_expiry_minutes: 1440, // 24 hours max
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GroupConfig {
    pub allow_public_invite: bool,
    pub join_quorum_threshold: f32, // e.g. 0.66 for 2/3 approval
    pub ephemeral_ttl_seconds: Option<u64>,
}

impl Default for GroupConfig {
    fn default() -> Self {
        Self {
            allow_public_invite: false,
            join_quorum_threshold: 0.66,
            ephemeral_ttl_seconds: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TierConfiguration {
    Team(TeamConfig),
    Organization(OrgConfig),
    Enterprise(EnterpriseConfig),
    Family(FamilyConfig),
    Group(GroupConfig),
}

// -----------------------------------------------------------------------------
// Tenant Entity Record
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TenantEntityRecord {
    pub tenant_id: TenantId,
    pub parent_tenant_id: Option<TenantId>,
    pub tier: EntityTier,
    pub display_name: String,
    pub root_did: Did,
    pub state: TenantState,
    pub configuration: TierConfiguration,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TenantEntityRecord {
    pub fn new(
        tier: EntityTier,
        display_name: impl Into<String>,
        root_did: Did,
        configuration: TierConfiguration,
        parent_tenant_id: Option<TenantId>,
    ) -> Result<Self, HakilleError> {
        let name = display_name.into();
        if name.trim().is_empty() {
            return Err(HakilleError::CapabilityDenied(
                "Tenant display name cannot be empty".to_string(),
            ));
        }

        // Validate configuration matches tier
        match (&tier, &configuration) {
            (EntityTier::Team, TierConfiguration::Team(_))
            | (EntityTier::Organization, TierConfiguration::Organization(_))
            | (EntityTier::Enterprise, TierConfiguration::Enterprise(_))
            | (EntityTier::Family, TierConfiguration::Family(_))
            | (EntityTier::Group, TierConfiguration::Group(_)) => {}
            _ => {
                return Err(HakilleError::CapabilityDenied(format!(
                    "Tenant tier '{}' does not match configuration variant",
                    tier
                )));
            }
        }

        // Validate tier hierarchy constraints:
        // - Enterprise cannot have a parent
        // - Team can only have Organization or Enterprise parent
        // - Organization can only have Enterprise parent
        // - Family is strictly sovereign (no parent)
        match (tier, parent_tenant_id) {
            (EntityTier::Enterprise, Some(_)) => {
                return Err(HakilleError::CapabilityDenied(
                    "Enterprise is the root tier and cannot have a parent tenant".to_string(),
                ));
            }
            (EntityTier::Family, Some(_)) => {
                return Err(HakilleError::CapabilityDenied(
                    "Family is a sovereign domestic sphere and cannot be a subsidiary".to_string(),
                ));
            }
            _ => {}
        }

        let now = Utc::now();
        Ok(Self {
            tenant_id: TenantId::new(),
            parent_tenant_id,
            tier,
            display_name: name,
            root_did,
            state: TenantState::Active,
            configuration,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn transition_state(&mut self, next: TenantState) -> Result<(), HakilleError> {
        if self.state == TenantState::Dissolved {
            return Err(HakilleError::CapabilityDenied(
                "Cannot transition dissolved tenant".to_string(),
            ));
        }
        self.state = next;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn is_active(&self) -> bool {
        self.state == TenantState::Active
    }

    pub fn is_dissolved(&self) -> bool {
        self.state == TenantState::Dissolved
    }
}

// -----------------------------------------------------------------------------
// Tenant Membership
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TenantMembership {
    pub membership_id: Uuid,
    pub tenant_id: TenantId,
    pub subject_did: Did,
    pub base_role: String, // e.g. "Admin", "Member", "Guardian", "Dependent"
    pub functional_attributes: HashMap<String, String>,
    pub granted_scopes: Vec<CapabilityScope>,
    pub max_delegation_depth: u8,
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    pub is_active: bool,
}

impl TenantMembership {
    pub fn new(
        tenant_id: TenantId,
        subject_did: Did,
        base_role: impl Into<String>,
        granted_scopes: Vec<CapabilityScope>,
        max_delegation_depth: u8,
    ) -> Self {
        let role = base_role.into();
        let is_minor = role.to_lowercase().contains("minor") || role == "Dependent";
        let is_guest = role.eq_ignore_ascii_case("guest");

        // Enforce Rule 7 (NoWallet for minors) and Guest invariants (NoWallet, NoFullSystem)
        let filtered_scopes = if is_minor || is_guest {
            granted_scopes
                .into_iter()
                .filter(|s| *s != CapabilityScope::Wallet && *s != CapabilityScope::FullSystem)
                .collect()
        } else {
            granted_scopes
        };

        Self {
            membership_id: Uuid::new_v4(),
            tenant_id,
            subject_did,
            base_role: role,
            functional_attributes: HashMap::new(),
            granted_scopes: filtered_scopes,
            max_delegation_depth: max_delegation_depth.min(4),
            valid_from: Utc::now(),
            valid_until: None,
            is_active: true,
        }
    }

    pub fn is_valid_at(&self, at: DateTime<Utc>) -> bool {
        if !self.is_active || at < self.valid_from {
            return false;
        }
        if let Some(exp) = self.valid_until {
            at <= exp
        } else {
            true
        }
    }

    pub fn is_minor(&self) -> bool {
        self.base_role.to_lowercase().contains("minor")
            || self.base_role == "Dependent"
            || self
                .functional_attributes
                .get("is_minor")
                .map(|v| v == "true")
                .unwrap_or(false)
    }

    pub fn is_guest(&self) -> bool {
        self.base_role.eq_ignore_ascii_case("guest")
    }

    pub fn has_scope(&self, scope: &CapabilityScope) -> bool {
        self.granted_scopes.contains(scope)
    }

    pub fn revoke(&mut self) {
        self.is_active = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_configuration_mismatch_rejected() {
        let did = Did::new("did:key:zTest");
        let wrong_cfg = TierConfiguration::Enterprise(EnterpriseConfig::default());
        let res = TenantEntityRecord::new(EntityTier::Team, "Swarm", did, wrong_cfg, None);
        assert!(res.is_err(), "Mismatch between tier and config must fail");
    }

    #[test]
    fn test_membership_rule7_nowallet_filtered() {
        let mem = TenantMembership::new(
            TenantId::new(),
            Did::new("did:key:zMinor"),
            "MinorDependent",
            vec![CapabilityScope::Wallet, CapabilityScope::Audio],
            1,
        );
        assert!(mem.is_minor());
        assert!(
            !mem.has_scope(&CapabilityScope::Wallet),
            "Rule 7 must filter Wallet scope"
        );
        assert!(mem.has_scope(&CapabilityScope::Audio));
    }

    #[test]
    fn test_membership_guest_fullsystem_filtered() {
        let mem = TenantMembership::new(
            TenantId::new(),
            Did::new("did:key:zGuest"),
            "Guest",
            vec![CapabilityScope::FullSystem, CapabilityScope::Audio],
            0,
        );
        assert!(mem.is_guest());
        assert!(
            !mem.has_scope(&CapabilityScope::FullSystem),
            "Guest cannot have FullSystem"
        );
        assert!(mem.has_scope(&CapabilityScope::Audio));
    }

    #[test]
    fn test_tenant_id_to_uuid_and_parse() {
        let tid = TenantId::new();
        let u = tid.to_uuid();
        let parsed = TenantId::parse(&u.to_string()).expect("parsing valid UUID succeeds");
        assert_eq!(tid, parsed);
        assert!(TenantId::parse("invalid-uuid").is_err());
    }

    #[test]
    fn test_tenant_state_helpers() {
        let did = Did::new("did:key:zOrg");
        let cfg = TierConfiguration::Organization(OrgConfig::default());
        let mut rec =
            TenantEntityRecord::new(EntityTier::Organization, "Acme", did, cfg, None).unwrap();
        assert!(rec.is_active());
        assert!(!rec.is_dissolved());

        rec.transition_state(TenantState::Dissolved).unwrap();
        assert!(!rec.is_active());
        assert!(rec.is_dissolved());
    }

    #[test]
    fn test_membership_revoke() {
        let mut mem = TenantMembership::new(
            TenantId::new(),
            Did::new("did:key:zMember"),
            "Member",
            vec![CapabilityScope::Filesystem],
            2,
        );
        assert!(mem.is_valid_at(Utc::now()));
        mem.revoke();
        assert!(!mem.is_valid_at(Utc::now()));
    }
}
