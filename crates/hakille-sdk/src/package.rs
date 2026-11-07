//! Format de paquet souverain `.hap` (Hakille Agent Package) (§36).
//!
//! Principes souverains de Hakille :
//! - Paquet hermétique auto-contenu : manifeste vérifié + binaire WebAssembly + empreintes SHA-256 scellées.
//! - Signature obligatoire Ed25519 du développeur : aucun code tiers non signé ne peut être installé.
//! - Dépaquetage avec vérification fail-closed de l'intégrité binaire avant toute exécution.

use crate::manifest::{ManifestError, SdkManifest};
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PackageError {
    #[error("Erreur de manifeste : {0}")]
    Manifest(#[from] ManifestError),
    #[error("Incohérence de hachage SHA-256 du binaire WASM")]
    WasmHashMismatch,
    #[error("Signature du développeur invalide ou corrompue")]
    InvalidDeveloperSignature,
    #[error("Clé de vérification du développeur invalide")]
    InvalidVerifyingKey,
    #[error("Erreur de sérialisation du paquet : {0}")]
    Serialization(String),
}

/// Paquet d'agent souverain `.hap` complet et signé.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HakilleAgentPackage {
    pub manifest_toml: String,
    pub wasm_bytes: Vec<u8>,
    pub wasm_sha256: [u8; 32],
    pub developer_verifying_key_bytes: [u8; 32],
    pub developer_signature: Vec<u8>,
    pub packaged_at_ms: u64,
}

impl HakilleAgentPackage {
    /// Crée et signe un paquet `.hap` depuis le manifeste et le binaire WASM compilé.
    pub fn pack(
        manifest_toml: impl Into<String>,
        wasm_bytes: &[u8],
        signing_key: &SigningKey,
    ) -> Result<Self, PackageError> {
        let manifest_str = manifest_toml.into();
        // Vérifie la validité du manifeste dès la création
        let _ = SdkManifest::parse_and_validate(&manifest_str)?;

        // Calcul de l'empreinte SHA-256 du binaire WASM
        let mut hasher = Sha256::new();
        hasher.update(wasm_bytes);
        let hash_output = hasher.finalize();
        let mut wasm_sha256 = [0u8; 32];
        wasm_sha256.copy_from_slice(&hash_output);

        let now_ms = hakille_core::Timestamp::now().0.timestamp_millis().max(0) as u64;

        // Signature du paquet = Sign(manifest_toml + wasm_sha256 + packaged_at_ms)
        let mut payload_to_sign = Vec::new();
        payload_to_sign.extend_from_slice(manifest_str.as_bytes());
        payload_to_sign.extend_from_slice(&wasm_sha256);
        payload_to_sign.extend_from_slice(&now_ms.to_le_bytes());

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

    /// Dépaquette et vérifie l'intégrité mathématique et la signature du développeur.
    pub fn unpack_and_verify(&self) -> Result<SdkManifest, PackageError> {
        // 1. Vérification de l'empreinte SHA-256 du binaire
        let mut hasher = Sha256::new();
        hasher.update(&self.wasm_bytes);
        let actual_hash = hasher.finalize();
        if actual_hash.as_slice() != self.wasm_sha256 {
            return Err(PackageError::WasmHashMismatch);
        }

        // 2. Vérification de la signature du développeur
        let verifying_key = VerifyingKey::from_bytes(&self.developer_verifying_key_bytes)
            .map_err(|_| PackageError::InvalidVerifyingKey)?;

        let sig_array: [u8; 64] = self
            .developer_signature
            .as_slice()
            .try_into()
            .map_err(|_| PackageError::InvalidDeveloperSignature)?;
        let signature = ed25519_dalek::Signature::from_bytes(&sig_array);

        let mut payload_to_verify = Vec::new();
        payload_to_verify.extend_from_slice(self.manifest_toml.as_bytes());
        payload_to_verify.extend_from_slice(&self.wasm_sha256);
        payload_to_verify.extend_from_slice(&self.packaged_at_ms.to_le_bytes());

        verifying_key
            .verify(&payload_to_verify, &signature)
            .map_err(|_| PackageError::InvalidDeveloperSignature)?;

        // 3. Validation du manifeste
        let manifest = SdkManifest::parse_and_validate(&self.manifest_toml)?;
        Ok(manifest)
    }

    /// Sérialise le paquet en flux binaire JSON prêt à être stocké ou échangé.
    pub fn to_bytes(&self) -> Result<Vec<u8>, PackageError> {
        serde_json::to_vec(self).map_err(|e| PackageError::Serialization(e.to_string()))
    }

    /// Désérialise et vérifie un paquet `.hap`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PackageError> {
        let package: Self = serde_json::from_slice(bytes)
            .map_err(|e| PackageError::Serialization(e.to_string()))?;
        let _ = package.unpack_and_verify()?;
        Ok(package)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_TOML: &str = r#"
id = "weather-radar"
name = "Local Weather Radar"
version = "2.0.1"
description = "Calcul météo local"
author = "Atmosphere Lab"
license = "MIT"
entrypoint = "radar.wasm"
capabilities = ["filesystem:read"]
"#;

    #[test]
    fn test_package_pack_unpack_and_signature_verification() {
        let signing_key = SigningKey::from_bytes(&[55u8; 32]);
        let wasm_code = b"\x00asm\x01\x00\x00\x00";

        let package = HakilleAgentPackage::pack(SAMPLE_TOML, wasm_code, &signing_key).unwrap();
        assert_eq!(package.wasm_bytes, wasm_code);

        let manifest = package.unpack_and_verify().unwrap();
        assert_eq!(manifest.id, "weather-radar");

        // Tampering with wasm code must trigger WasmHashMismatch
        let mut tampered_wasm = package.clone();
        tampered_wasm.wasm_bytes = b"\x00asm\x02\x00\x00\x00".to_vec();
        assert_eq!(
            tampered_wasm.unpack_and_verify().unwrap_err(),
            PackageError::WasmHashMismatch
        );

        // Tampering with manifest must trigger InvalidDeveloperSignature
        let mut tampered_manifest = package.clone();
        tampered_manifest.manifest_toml = SAMPLE_TOML.replace("2.0.1", "3.0.0");
        assert_eq!(
            tampered_manifest.unpack_and_verify().unwrap_err(),
            PackageError::InvalidDeveloperSignature
        );
    }
}
