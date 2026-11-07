use crate::crypto::error::CryptoError;
use crate::crypto::kdf::MasterKey;
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Domains for cryptographic key separation in Hakille
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SubkeyDomain {
    Identity,
    MemoryPartition(String),
    Credentials,
    Biometrics,
    BlindIndex,
}

impl SubkeyDomain {
    pub fn as_str(&self) -> String {
        match self {
            SubkeyDomain::Identity => "identity".to_string(),
            SubkeyDomain::MemoryPartition(p) => format!("memory-partition:{}", p),
            SubkeyDomain::Credentials => "credentials".to_string(),
            SubkeyDomain::Biometrics => "biometrics".to_string(),
            SubkeyDomain::BlindIndex => "blind-index".to_string(),
        }
    }
}

/// Zeroized cryptographic subkey hierarchy for an entity or tenant
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SubkeyHierarchy {
    pub identity_key: [u8; 32],
    pub memory_key: [u8; 32],
    pub credentials_key: [u8; 32],
    pub biometrics_key: [u8; 32],
    pub blind_index_key: [u8; 32],
}

impl std::fmt::Debug for SubkeyHierarchy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SubkeyHierarchy([REDACTED])")
    }
}

impl SubkeyHierarchy {
    pub fn identity_key(&self) -> &[u8; 32] {
        &self.identity_key
    }

    pub fn memory_key(&self) -> &[u8; 32] {
        &self.memory_key
    }

    pub fn credentials_key(&self) -> &[u8; 32] {
        &self.credentials_key
    }

    pub fn biometrics_key(&self) -> &[u8; 32] {
        &self.biometrics_key
    }

    pub fn blind_index_key(&self) -> &[u8; 32] {
        &self.blind_index_key
    }
}

/// Derivation context salt for HKDF-SHA256 subkeys
pub const HKDF_DOMAIN_SALT: &[u8] = b"hakille:v1:domain-salt";

/// HKDF-SHA256 Subkey Tree engine (RFC 5869)
pub struct HkdfSubkeyTree;

impl HkdfSubkeyTree {
    /// Derive a single subkey with explicit domain and tenant binding
    pub fn derive_subkey(
        master: &MasterKey,
        domain: &SubkeyDomain,
        tenant_id: Option<&str>,
    ) -> Result<[u8; 32], CryptoError> {
        derive_subkey(master, domain, tenant_id)
    }

    /// Derive complete subkey hierarchy bound to a tenant
    pub fn derive_subkeys_for_tenant(
        master: &MasterKey,
        tenant_id: &str,
    ) -> Result<SubkeyHierarchy, CryptoError> {
        derive_subkeys_for_tenant(master, tenant_id)
    }

    /// Derive default root subkey hierarchy
    pub fn derive_subkeys(master: &MasterKey) -> SubkeyHierarchy {
        derive_subkeys(master)
    }
}

/// Derive a 32-byte domain-separated subkey:
/// context = "hakille:v1:" || Domain || ":" || TenantID
pub fn derive_subkey(
    master: &MasterKey,
    domain: &SubkeyDomain,
    tenant_id: Option<&str>,
) -> Result<[u8; 32], CryptoError> {
    let hk = Hkdf::<Sha256>::new(Some(HKDF_DOMAIN_SALT), master.as_bytes());
    let tenant_str = tenant_id.unwrap_or("default");
    let info = format!("hakille:v1:{}:{}", domain.as_str(), tenant_str);

    let mut okm = [0u8; 32];
    hk.expand(info.as_bytes(), &mut okm)
        .map_err(|e| CryptoError::SubkeyDerivationError(e.to_string()))?;

    Ok(okm)
}

/// Derive full subkey hierarchy bound to a specific tenant ID
pub fn derive_subkeys_for_tenant(
    master: &MasterKey,
    tenant_id: &str,
) -> Result<SubkeyHierarchy, CryptoError> {
    let identity_key = derive_subkey(master, &SubkeyDomain::Identity, Some(tenant_id))?;
    let memory_key = derive_subkey(
        master,
        &SubkeyDomain::MemoryPartition("default".to_string()),
        Some(tenant_id),
    )?;
    let credentials_key = derive_subkey(master, &SubkeyDomain::Credentials, Some(tenant_id))?;
    let biometrics_key = derive_subkey(master, &SubkeyDomain::Biometrics, Some(tenant_id))?;
    let blind_index_key = derive_subkey(master, &SubkeyDomain::BlindIndex, Some(tenant_id))?;

    Ok(SubkeyHierarchy {
        identity_key,
        memory_key,
        credentials_key,
        biometrics_key,
        blind_index_key,
    })
}

/// Derive default root subkey hierarchy
pub fn derive_subkeys(master: &MasterKey) -> SubkeyHierarchy {
    derive_subkeys_for_tenant(master, "default")
        .expect("Subkey derivation with 32-byte output is infallible in HKDF-SHA256")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subkey_derivation_domain_separation() {
        let master = MasterKey::new([0x55; 32]);
        let k_id = derive_subkey(&master, &SubkeyDomain::Identity, Some("tenant-A")).unwrap();
        let k_cred = derive_subkey(&master, &SubkeyDomain::Credentials, Some("tenant-A")).unwrap();
        let k_bio = derive_subkey(&master, &SubkeyDomain::Biometrics, Some("tenant-A")).unwrap();

        assert_ne!(k_id, k_cred);
        assert_ne!(k_id, k_bio);
        assert_ne!(k_cred, k_bio);
    }

    #[test]
    fn test_subkey_tenant_isolation() {
        let master = MasterKey::new([0x33; 32]);
        let k_tenant1 = derive_subkey(&master, &SubkeyDomain::Identity, Some("org-corp")).unwrap();
        let k_tenant2 =
            derive_subkey(&master, &SubkeyDomain::Identity, Some("org-subsidiary")).unwrap();

        assert_ne!(k_tenant1, k_tenant2);
    }

    #[test]
    fn test_subkey_hierarchy_generation() {
        let master = MasterKey::new([0x77; 32]);
        let hierarchy = derive_subkeys(&master);

        assert_ne!(hierarchy.identity_key, [0u8; 32]);
        assert_ne!(hierarchy.identity_key, hierarchy.blind_index_key);
    }
}
