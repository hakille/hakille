use crate::crypto::error::CryptoError;
use crate::crypto::subkeys::{SubkeyHierarchy, derive_subkeys};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::{RngCore, rngs::OsRng};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Parameters for Argon2id Key Derivation Function (RFC 9106)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Argon2Params {
    /// Memory cost in KiB (standard production default: 65536 KiB = 64 MiB)
    pub memory_kib: u32,
    /// Number of passes / iterations (standard production default: 3)
    pub passes: u32,
    /// Degree of parallelism / threads (standard production default: 4)
    pub parallelism: u32,
    /// Output key length in bytes (standard: 32 bytes for 256-bit key)
    pub output_len: usize,
}

impl Default for Argon2Params {
    fn default() -> Self {
        Self {
            memory_kib: 65536, // 64 MiB standard RFC 9106 production profile
            passes: 3,         // t = 3
            parallelism: 4,    // p = 4 threads
            output_len: 32,    // 256 bits
        }
    }
}

impl Argon2Params {
    /// Lightweight configuration for fast deterministic unit tests
    pub fn test_params() -> Self {
        Self {
            memory_kib: 64, // 64 KiB
            passes: 1,
            parallelism: 1,
            output_len: 32,
        }
    }
}

/// 32-byte Master Key protected by memory zeroization upon drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct MasterKey {
    pub key: [u8; 32],
}

/// Type alias for storage master key
pub type MasterStorageKey = MasterKey;

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MasterKey([REDACTED])")
    }
}

impl MasterKey {
    pub fn new(key: [u8; 32]) -> Self {
        Self { key }
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.key
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.key
    }

    pub fn from_bytes(key: [u8; 32]) -> Self {
        Self { key }
    }
}

/// Generate a cryptographically secure 32-byte salt using OsRng
pub fn generate_salt() -> [u8; 32] {
    let mut salt = [0u8; 32];
    OsRng.fill_bytes(&mut salt);
    salt
}

/// RFC 9106 compliant Master Key Derivation Function
pub struct MasterKeyKdf;

impl MasterKeyKdf {
    /// Generate a 32-byte CSPRNG salt
    pub fn generate_salt() -> [u8; 32] {
        generate_salt()
    }

    /// Derive master key using default RFC 9106 production parameters (64 MiB, t=3, p=4)
    pub fn derive_master_key(passphrase: &[u8], salt: &[u8; 32]) -> Result<MasterKey, CryptoError> {
        Self::derive_master_key_with_params(passphrase, salt, &Argon2Params::default())
    }

    /// Memory-hard key derivation adhering to Argon2id RFC 9106 with custom parameters
    pub fn derive_master_key_with_params(
        passphrase: &[u8],
        salt: &[u8; 32],
        params: &Argon2Params,
    ) -> Result<MasterKey, CryptoError> {
        if params.memory_kib == 0 || params.passes == 0 || params.parallelism == 0 {
            return Err(CryptoError::KdfError(
                "Argon2 parameters must be non-zero".to_string(),
            ));
        }

        let argon2_params = Params::new(
            params.memory_kib,
            params.passes,
            params.parallelism,
            Some(params.output_len),
        )
        .map_err(|e| CryptoError::KdfError(e.to_string()))?;

        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon2_params);

        let mut out = [0u8; 32];
        argon2
            .hash_password_into(passphrase, salt, &mut out)
            .map_err(|e| CryptoError::KdfError(e.to_string()))?;

        Ok(MasterKey::new(out))
    }

    /// Derive default subkey hierarchy from a derived master key
    pub fn derive_subkeys(master: &MasterKey) -> SubkeyHierarchy {
        derive_subkeys(master)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_argon2id_derivation_deterministic() {
        let passphrase = b"correct horse battery staple sovereign OS 2026";
        let salt = [7u8; 32];
        let params = Argon2Params::test_params();

        let key1 = MasterKeyKdf::derive_master_key_with_params(passphrase, &salt, &params).unwrap();
        let key2 = MasterKeyKdf::derive_master_key_with_params(passphrase, &salt, &params).unwrap();

        assert_eq!(key1.as_bytes(), key2.as_bytes());
        assert_ne!(key1.as_bytes(), &[0u8; 32]);
    }

    #[test]
    fn test_argon2id_different_salt_differs() {
        let passphrase = b"secure passphrase";
        let salt1 = [1u8; 32];
        let salt2 = [2u8; 32];
        let params = Argon2Params::test_params();

        let key1 =
            MasterKeyKdf::derive_master_key_with_params(passphrase, &salt1, &params).unwrap();
        let key2 =
            MasterKeyKdf::derive_master_key_with_params(passphrase, &salt2, &params).unwrap();

        assert_ne!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_argon2id_zeroize_on_drop() {
        let mut key = MasterKey::new([0x42; 32]);
        assert_eq!(key.as_bytes(), &[0x42; 32]);
        key.zeroize();
        assert_eq!(key.as_bytes(), &[0u8; 32]);
    }
}
