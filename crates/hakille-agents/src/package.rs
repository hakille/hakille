//! Format de paquet souverain `.hap` (Hakille Agent Package) et vérificateur d'intégrité.
//!
//! Principes souverains et contrat d'honnêteté :
//! - Paquet hermétique auto-contenu : manifeste vérifié + binaire WebAssembly + empreintes SHA-256 scellées.
//! - Signature obligatoire Ed25519 du développeur : aucun code tiers non signé ne peut être installé.
//! - Dépaquetage avec vérification fail-closed de l'intégrité binaire avant toute exécution.
//! - Stockage content-addressed du binaire WASM sous `<data_dir>/agents/<sha256>.wasm` avec contrôle d'empreinte.

use crate::reference::write_atomic;
use crate::registry::AgentManifest;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hakille_core::HakilleError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};

/// Taille maximale d'un paquet `.hap` sérialisé (16 Mio).
pub const MAX_PACKAGE_BYTES: usize = 16 * 1024 * 1024;
/// Taille maximale d'un binaire WASM embarqué dans un `.hap` (8 Mio).
pub const MAX_WASM_BYTES: usize = 8 * 1024 * 1024;
/// Domaine de séparation cryptographique des signatures `.hap` v1.
const HAP_SIGNING_DOMAIN: &[u8] = b"hakille-agent-package-v1";

/// Erreurs spécifiques au traitement et à la vérification d'un paquet `.hap`.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PackageError {
    #[error("Erreur de manifeste d'agent : {0}")]
    Manifest(String),
    #[error("Incohérence d'empreinte SHA-256 du binaire WASM")]
    WasmHashMismatch,
    #[error("Signature développeur Ed25519 invalide ou corrompue")]
    InvalidDeveloperSignature,
    #[error("Clé publique de vérification Ed25519 invalide")]
    InvalidVerifyingKey,
    #[error("Clé développeur Ed25519 non reconnue dans le trousseau épinglé")]
    UntrustedDeveloperKey,
    #[error("Paquet .hap ou binaire WASM trop volumineux ({0} octets)")]
    PackageTooLarge(usize),
    #[error("Erreur de sérialisation ou désérialisation du paquet : {0}")]
    Serialization(String),
    #[error("Erreur d'entrée/sortie sur le paquet : {0}")]
    Io(String),
}

pub type PackageResult<T> = std::result::Result<T, PackageError>;

impl From<PackageError> for HakilleError {
    fn from(err: PackageError) -> Self {
        match err {
            PackageError::Manifest(msg) => HakilleError::CapabilityDenied(msg),
            PackageError::WasmHashMismatch => {
                HakilleError::TrustViolation("empreinte SHA-256 du binaire WASM incohérente".into())
            }
            PackageError::InvalidDeveloperSignature => HakilleError::AuthenticationFailed,
            PackageError::InvalidVerifyingKey => {
                HakilleError::CryptoError("clé publique développeur Ed25519 invalide".into())
            }
            PackageError::UntrustedDeveloperKey => HakilleError::TrustViolation(
                "clé développeur Ed25519 absente du trousseau épinglé".into(),
            ),
            PackageError::PackageTooLarge(sz) => {
                HakilleError::CapabilityDenied(format!("paquet .hap trop volumineux ({sz} octets)"))
            }
            PackageError::Serialization(msg) => HakilleError::SerializationError(msg),
            PackageError::Io(msg) => HakilleError::StorageError(msg),
        }
    }
}

/// Paquet d'agent souverain `.hap` complet et signé (format d'échange du SDK).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HakilleAgentPackage {
    pub manifest_toml: String,
    pub wasm_bytes: Vec<u8>,
    pub wasm_sha256: [u8; 32],
    pub developer_verifying_key_bytes: [u8; 32],
    pub developer_signature: Vec<u8>,
    pub packaged_at_ms: u64,
}

/// Contenu vérifié et validé d'un paquet `.hap`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedAgentPackage {
    pub manifest: AgentManifest,
    pub wasm_bytes: Vec<u8>,
    pub wasm_sha256: [u8; 32],
    pub wasm_sha256_hex: String,
    pub developer_verifying_key_bytes: [u8; 32],
    pub packaged_at_ms: u64,
}

fn canonical_signing_payload(
    manifest_toml: &str,
    wasm_sha256: &[u8; 32],
    packaged_at_ms: u64,
) -> Vec<u8> {
    let manifest_bytes = manifest_toml.as_bytes();
    let mut payload =
        Vec::with_capacity(HAP_SIGNING_DOMAIN.len() + 8 + manifest_bytes.len() + 32 + 8);
    payload.extend_from_slice(HAP_SIGNING_DOMAIN);
    payload.extend_from_slice(&(manifest_bytes.len() as u64).to_le_bytes());
    payload.extend_from_slice(manifest_bytes);
    payload.extend_from_slice(wasm_sha256);
    payload.extend_from_slice(&packaged_at_ms.to_le_bytes());
    payload
}

impl HakilleAgentPackage {
    /// Crée et signe un paquet `.hap` (utilisé par le SDK et les tests de conformité).
    pub fn pack(
        manifest_toml: impl Into<String>,
        wasm_bytes: &[u8],
        signing_key: &SigningKey,
    ) -> PackageResult<Self> {
        if wasm_bytes.len() > MAX_WASM_BYTES {
            return Err(PackageError::PackageTooLarge(wasm_bytes.len()));
        }
        let manifest_str = manifest_toml.into();
        if manifest_str.len() > MAX_PACKAGE_BYTES {
            return Err(PackageError::PackageTooLarge(manifest_str.len()));
        }
        // Vérifie le manifeste dès la création
        let _ = AgentManifest::from_toml(&manifest_str)
            .map_err(|e| PackageError::Manifest(e.to_string()))?;

        let mut hasher = Sha256::new();
        hasher.update(wasm_bytes);
        let hash_output = hasher.finalize();
        let mut wasm_sha256 = [0u8; 32];
        wasm_sha256.copy_from_slice(&hash_output);

        let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
        let payload_to_sign = canonical_signing_payload(&manifest_str, &wasm_sha256, now_ms);

        let signature = signing_key.sign(&payload_to_sign);
        let verifying_key = signing_key.verifying_key();

        Ok(Self {
            manifest_toml: manifest_str,
            wasm_bytes: wasm_bytes.to_vec(),
            wasm_sha256,
            developer_verifying_key_bytes: verifying_key.to_bytes(),
            developer_signature: signature.to_bytes().to_vec(),
            packaged_at_ms: now_ms,
        })
    }

    /// Dépaquette et vérifie l'intégrité cryptographique contre un trousseau de
    /// clés publiques développeur épinglées (`trusted_keys`, fail-closed si vide)
    /// ainsi que la conformité du manifeste.
    pub fn unpack_and_verify(
        &self,
        trusted_keys: &[VerifyingKey],
    ) -> PackageResult<VerifiedAgentPackage> {
        if trusted_keys.is_empty() {
            return Err(PackageError::UntrustedDeveloperKey);
        }
        if self.wasm_bytes.len() > MAX_WASM_BYTES {
            return Err(PackageError::PackageTooLarge(self.wasm_bytes.len()));
        }
        if self.manifest_toml.len() > MAX_PACKAGE_BYTES {
            return Err(PackageError::PackageTooLarge(self.manifest_toml.len()));
        }

        // 1. Vérification stricte du SHA-256 du binaire
        let mut hasher = Sha256::new();
        hasher.update(&self.wasm_bytes);
        let actual_hash = hasher.finalize();
        if actual_hash.as_slice() != self.wasm_sha256 {
            return Err(PackageError::WasmHashMismatch);
        }

        // 2. Vérification de l'ancrage dans le trousseau épinglé + verify_strict Ed25519
        let verifying_key = VerifyingKey::from_bytes(&self.developer_verifying_key_bytes)
            .map_err(|_| PackageError::InvalidVerifyingKey)?;
        if !trusted_keys.contains(&verifying_key) {
            return Err(PackageError::UntrustedDeveloperKey);
        }

        let sig_bytes: [u8; 64] = self
            .developer_signature
            .as_slice()
            .try_into()
            .map_err(|_| PackageError::InvalidDeveloperSignature)?;
        let signature = Signature::from_bytes(&sig_bytes);

        let payload_to_verify =
            canonical_signing_payload(&self.manifest_toml, &self.wasm_sha256, self.packaged_at_ms);

        verifying_key
            .verify_strict(&payload_to_verify, &signature)
            .map_err(|_| PackageError::InvalidDeveloperSignature)?;

        // 3. Validation fail-closed du manifeste via AgentManifest::from_toml
        let manifest = AgentManifest::from_toml(&self.manifest_toml)
            .map_err(|e| PackageError::Manifest(e.to_string()))?;

        let wasm_sha256_hex = hex_encode(&self.wasm_sha256);

        Ok(VerifiedAgentPackage {
            manifest,
            wasm_bytes: self.wasm_bytes.clone(),
            wasm_sha256: self.wasm_sha256,
            wasm_sha256_hex,
            developer_verifying_key_bytes: self.developer_verifying_key_bytes,
            packaged_at_ms: self.packaged_at_ms,
        })
    }

    /// Sérialise le paquet en octets JSON hermétiques.
    pub fn to_bytes(&self) -> PackageResult<Vec<u8>> {
        serde_json::to_vec(self).map_err(|e| PackageError::Serialization(e.to_string()))
    }

    /// Désérialise et vérifie un paquet `.hap` contre les clés publiques épinglées.
    pub fn from_bytes(bytes: &[u8], trusted_keys: &[VerifyingKey]) -> PackageResult<Self> {
        if bytes.len() > MAX_PACKAGE_BYTES {
            return Err(PackageError::PackageTooLarge(bytes.len()));
        }
        let package: Self = serde_json::from_slice(bytes)
            .map_err(|e| PackageError::Serialization(e.to_string()))?;
        let _ = package.unpack_and_verify(trusted_keys)?;
        Ok(package)
    }

    /// Lit un fichier `.hap` depuis le disque (lecture bornée à [`MAX_PACKAGE_BYTES`]),
    /// le désérialise et vérifie son intégrité contre `trusted_keys`.
    pub fn from_file(path: impl AsRef<Path>, trusted_keys: &[VerifyingKey]) -> PackageResult<Self> {
        let file = fs::File::open(path.as_ref()).map_err(|e| PackageError::Io(e.to_string()))?;
        let meta = file
            .metadata()
            .map_err(|e| PackageError::Io(e.to_string()))?;
        if meta.len() > MAX_PACKAGE_BYTES as u64 {
            return Err(PackageError::PackageTooLarge(
                usize::try_from(meta.len()).unwrap_or(usize::MAX),
            ));
        }
        let mut bytes = Vec::new();
        file.take((MAX_PACKAGE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| PackageError::Io(e.to_string()))?;
        if bytes.len() > MAX_PACKAGE_BYTES {
            return Err(PackageError::PackageTooLarge(bytes.len()));
        }
        Self::from_bytes(&bytes, trusted_keys)
    }

    /// Installe le paquet vérifié dans le répertoire d'agents cible (`<data_dir>/agents/`).
    /// Écrit atomiquement le binaire WASM adressé par son hash SHA-256 (`<sha256>.wasm`),
    /// relit et re-vérifie l'empreinte sur disque.
    pub fn install_into(
        &self,
        agents_dir: &Path,
        trusted_keys: &[VerifyingKey],
    ) -> PackageResult<(PathBuf, VerifiedAgentPackage)> {
        let verified = self.unpack_and_verify(trusted_keys)?;
        fs::create_dir_all(agents_dir).map_err(|e| PackageError::Io(e.to_string()))?;

        let wasm_filename = format!("{}.wasm", verified.wasm_sha256_hex);
        let dest_path = agents_dir.join(&wasm_filename);

        write_atomic(&dest_path, &verified.wasm_bytes)
            .map_err(|e| PackageError::Io(e.to_string()))?;

        let written = fs::read(&dest_path).map_err(|e| PackageError::Io(e.to_string()))?;
        let actual = Sha256::digest(&written);
        if actual.as_slice() != verified.wasm_sha256 {
            return Err(PackageError::WasmHashMismatch);
        }

        Ok((dest_path, verified))
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{:02x}", b);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_AGENT_TOML: &str = r#"
id = "agent-weather"
name = "Weather Agent"
version = "1.0.0"
description = "Agent de calcul météo local"
isolated = true
required_capabilities = ["Filesystem"]
"#;

    #[test]
    fn test_hap_package_lifecycle_and_verification() {
        let signing_key = SigningKey::from_bytes(&[42u8; 32]);
        let trusted = [signing_key.verifying_key()];
        let untrusted_key = SigningKey::from_bytes(&[99u8; 32]);
        let wasm_code = b"\x00asm\x01\x00\x00\x00";

        let package = HakilleAgentPackage::pack(VALID_AGENT_TOML, wasm_code, &signing_key)
            .expect("pack doit réussir");

        // Empty trusted keyring is rejected fail-closed
        assert_eq!(
            package.unpack_and_verify(&[]).unwrap_err(),
            PackageError::UntrustedDeveloperKey
        );

        // Self-signed package with untrusted key is rejected fail-closed
        assert_eq!(
            package
                .unpack_and_verify(&[untrusted_key.verifying_key()])
                .unwrap_err(),
            PackageError::UntrustedDeveloperKey
        );

        let verified = package
            .unpack_and_verify(&trusted)
            .expect("unpack_and_verify doit réussir avec clé épinglée");
        assert_eq!(verified.manifest.id, "agent-weather");
        assert_eq!(verified.manifest.name, "Weather Agent");
        assert_eq!(verified.manifest.version, "1.0.0");
        assert_eq!(verified.wasm_bytes, wasm_code);

        // Serialization roundtrip
        let bytes = package.to_bytes().expect("to_bytes");
        let restored = HakilleAgentPackage::from_bytes(&bytes, &trusted).expect("from_bytes");
        assert_eq!(restored, package);

        // Tampering with WASM bytes fails
        let mut tampered_wasm = package.clone();
        tampered_wasm.wasm_bytes = b"\x00asm\x02\x00\x00\x00".to_vec();
        assert_eq!(
            tampered_wasm.unpack_and_verify(&trusted).unwrap_err(),
            PackageError::WasmHashMismatch
        );

        // Tampering with manifest fails signature
        let mut tampered_manifest = package.clone();
        tampered_manifest.manifest_toml = VALID_AGENT_TOML.replace("1.0.0", "2.0.0");
        assert_eq!(
            tampered_manifest.unpack_and_verify(&trusted).unwrap_err(),
            PackageError::InvalidDeveloperSignature
        );

        // Non-isolated manifest is rejected fail-closed
        let non_isolated_toml = VALID_AGENT_TOML.replace("isolated = true", "isolated = false");
        let err =
            HakilleAgentPackage::pack(non_isolated_toml, wasm_code, &signing_key).unwrap_err();
        assert!(matches!(err, PackageError::Manifest(_)));
    }

    #[test]
    fn test_hap_package_installation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let signing_key = SigningKey::from_bytes(&[88u8; 32]);
        let trusted = [signing_key.verifying_key()];
        let wasm_code = b"\x00asm\x01\x00\x00\x00";

        let package = HakilleAgentPackage::pack(VALID_AGENT_TOML, wasm_code, &signing_key).unwrap();
        let (installed_path, verified) = package.install_into(temp_dir.path(), &trusted).unwrap();

        assert!(installed_path.exists());
        assert_eq!(fs::read(&installed_path).unwrap(), wasm_code);
        assert_eq!(
            installed_path.file_name().unwrap().to_str().unwrap(),
            format!("{}.wasm", verified.wasm_sha256_hex)
        );
    }
}
