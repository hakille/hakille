//! Cross-Tenant Firewalls & Cryptographic Isolation (F13).
//! Strict inter-tenant cryptographic separation, conglomerate inter-org boundaries,
//! and domestic/enterprise airgap guarantees.

use crate::hierarchy::{EntityTier, TenantId};
use hakille_core::HakilleError;
use std::collections::{HashMap, HashSet};

pub struct CrossTenantFirewall {
    tenants: HashMap<TenantId, EntityTier>,
    bilateral_grants: HashSet<(TenantId, TenantId)>,
}

impl Default for CrossTenantFirewall {
    fn default() -> Self {
        Self::new()
    }
}

impl CrossTenantFirewall {
    pub fn new() -> Self {
        Self {
            tenants: HashMap::new(),
            bilateral_grants: HashSet::new(),
        }
    }

    pub fn register_tenant(&mut self, id: TenantId, tier: EntityTier) {
        self.tenants.insert(id, tier);
    }

    pub fn grant_bilateral_delegation(
        &mut self,
        source: TenantId,
        target: TenantId,
    ) -> Result<(), HakilleError> {
        // Personal family spheres cannot establish cross-tenant corporate bridges
        let source_tier = self.tenants.get(&source).ok_or_else(|| {
            HakilleError::CapabilityDenied(format!("Source tenant {} not found", source))
        })?;
        let target_tier = self.tenants.get(&target).ok_or_else(|| {
            HakilleError::CapabilityDenied(format!("Target tenant {} not found", target))
        })?;

        if *source_tier == EntityTier::Family || *target_tier == EntityTier::Family {
            return Err(HakilleError::CapabilityDenied(
                "Cross-tenant firewall violation: Personal Family sphere is strictly airgapped and cannot bridge to enterprise organizations"
                    .to_string(),
            ));
        }

        self.bilateral_grants.insert((source, target));
        Ok(())
    }

    /// Grant bidirectional (mutual) delegation between two non-family tenants
    pub fn grant_mutual_delegation(
        &mut self,
        a: TenantId,
        b: TenantId,
    ) -> Result<(), HakilleError> {
        self.grant_bilateral_delegation(a, b)?;
        self.grant_bilateral_delegation(b, a)?;
        Ok(())
    }

    /// Revoke a previously granted bilateral delegation
    pub fn revoke_bilateral_delegation(&mut self, source: TenantId, target: TenantId) -> bool {
        self.bilateral_grants.remove(&(source, target))
    }

    /// Check if a tenant is registered in the firewall
    pub fn is_registered(&self, id: &TenantId) -> bool {
        self.tenants.contains_key(id)
    }

    /// Get the tier of a registered tenant
    pub fn get_tier(&self, id: &TenantId) -> Option<EntityTier> {
        self.tenants.get(id).copied()
    }

    /// Check if a bilateral grant exists between source and target
    pub fn has_grant(&self, source: &TenantId, target: &TenantId) -> bool {
        self.bilateral_grants.contains(&(*source, *target))
    }

    /// Count of registered tenants
    pub fn registered_count(&self) -> usize {
        self.tenants.len()
    }

    /// Count of active bilateral grants
    pub fn grant_count(&self) -> usize {
        self.bilateral_grants.len()
    }

    /// Clear all registered tenants and grants
    pub fn clear(&mut self) {
        self.tenants.clear();
        self.bilateral_grants.clear();
    }

    pub fn check_cross_tenant_access(
        &self,
        requesting_tenant: &TenantId,
        target_tenant: &TenantId,
    ) -> Result<(), HakilleError> {
        // Invariant 1: Intra-tenant requests are always allowed
        if requesting_tenant == target_tenant {
            return Ok(());
        }

        let req_tier = self.tenants.get(requesting_tenant).ok_or_else(|| {
            HakilleError::CapabilityDenied(format!(
                "Requesting tenant {} not found",
                requesting_tenant
            ))
        })?;
        let tgt_tier = self.tenants.get(target_tenant).ok_or_else(|| {
            HakilleError::CapabilityDenied(format!("Target tenant {} not found", target_tenant))
        })?;

        // Invariant 2: Family airgap check
        if *req_tier == EntityTier::Family || *tgt_tier == EntityTier::Family {
            return Err(HakilleError::CapabilityDenied(
                "Cross-tenant firewall blocked: strict airgap between domestic family and external tenants"
                    .to_string(),
            ));
        }

        // Invariant 3: Inter-org boundary requires explicit bilateral grant
        if self
            .bilateral_grants
            .contains(&(*requesting_tenant, *target_tenant))
        {
            Ok(())
        } else {
            Err(HakilleError::CapabilityDenied(format!(
                "Cross-tenant firewall blocked: no explicit bilateral delegation between {} and {}",
                requesting_tenant, target_tenant
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_firewall_mutual_delegation() {
        let mut fw = CrossTenantFirewall::new();
        let org_a = TenantId::new();
        let org_b = TenantId::new();

        fw.register_tenant(org_a, EntityTier::Organization);
        fw.register_tenant(org_b, EntityTier::Organization);

        fw.grant_mutual_delegation(org_a, org_b).unwrap();
        assert!(fw.check_cross_tenant_access(&org_a, &org_b).is_ok());
        assert!(fw.check_cross_tenant_access(&org_b, &org_a).is_ok());
    }

    #[test]
    fn test_firewall_revocation() {
        let mut fw = CrossTenantFirewall::new();
        let org_a = TenantId::new();
        let org_b = TenantId::new();

        fw.register_tenant(org_a, EntityTier::Organization);
        fw.register_tenant(org_b, EntityTier::Organization);

        fw.grant_bilateral_delegation(org_a, org_b).unwrap();
        assert!(fw.check_cross_tenant_access(&org_a, &org_b).is_ok());

        assert!(fw.revoke_bilateral_delegation(org_a, org_b));
        assert!(fw.check_cross_tenant_access(&org_a, &org_b).is_err());
    }

    #[test]
    fn test_firewall_query_helpers() {
        let mut fw = CrossTenantFirewall::new();
        let org = TenantId::new();

        assert!(!fw.is_registered(&org));
        fw.register_tenant(org, EntityTier::Enterprise);

        assert!(fw.is_registered(&org));
        assert_eq!(fw.get_tier(&org), Some(EntityTier::Enterprise));
        assert_eq!(fw.registered_count(), 1);

        fw.clear();
        assert_eq!(fw.registered_count(), 0);
        assert!(!fw.is_registered(&org));
    }
}
