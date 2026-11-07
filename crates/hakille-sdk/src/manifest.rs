//! Validation et analyse déclarative des manifestes d'agents souverains (`agent.toml`) (§36).
//!
//! Principes souverains de Hakille :
//! - Validation stricte fail-closed au chargement.
//! - Zéro permission par défaut : toute capability doit être explicitement déclarée et justifiée.
//! - Respect sémantique du versionnage (SemVer) et isolation hermétique.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error("Erreur de syntaxe TOML : {0}")]
    ParseError(String),
    #[error("Champ obligatoire manquant : {0}")]
    MissingField(&'static str),
    #[error("Version sémantique invalide : {0}")]
    InvalidVersion(String),
    #[error("Identifiant d'agent invalide (doit être alphanumérique avec tirets) : {0}")]
    InvalidId(String),
    #[error("Capability invalide ou interdite (wildcards interdits) : {0}")]
    ForbiddenCapability(String),
    #[error("Quota d'exécution invalide : {0}")]
    InvalidQuota(&'static str),
}

/// Quotas déclarés par l'agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestQuotas {
    pub max_invocations_per_minute: u32,
    pub max_payload_bytes_per_call: u32,
    pub max_memory_mb: u32,
    pub max_execution_time_ms: u32,
}

impl Default for ManifestQuotas {
    fn default() -> Self {
        Self {
            max_invocations_per_minute: 60,
            max_payload_bytes_per_call: 65536,
            max_memory_mb: 64,
            max_execution_time_ms: 1000,
        }
    }
}

/// Manifeste complet d'un agent souverain Hakille (`agent.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SdkManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub entrypoint: String,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub quotas: ManifestQuotas,
}

impl SdkManifest {
    /// Analyse et valide une chaîne `agent.toml`.
    pub fn parse_and_validate(toml_content: &str) -> Result<Self, ManifestError> {
        let manifest: Self =
            toml::from_str(toml_content).map_err(|e| ManifestError::ParseError(e.to_string()))?;

        manifest.validate()?;
        Ok(manifest)
    }

    /// Validation approfondie des invariants souverains.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.id.trim().is_empty() {
            return Err(ManifestError::MissingField("id"));
        }
        if !self
            .id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ManifestError::InvalidId(self.id.clone()));
        }

        if self.name.trim().is_empty() {
            return Err(ManifestError::MissingField("name"));
        }

        // Vérification SemVer
        semver::Version::parse(&self.version)
            .map_err(|e| ManifestError::InvalidVersion(format!("{}: {}", self.version, e)))?;

        if self.entrypoint.trim().is_empty() {
            return Err(ManifestError::MissingField("entrypoint"));
        }

        // Vérification des capabilities déclarées
        for cap in &self.capabilities {
            let cap_clean = cap.trim().to_lowercase();
            if cap_clean == "*" || cap_clean == "all" || cap_clean == "root" {
                return Err(ManifestError::ForbiddenCapability(cap.clone()));
            }
        }

        // Vérification des quotas
        if self.quotas.max_invocations_per_minute == 0
            || self.quotas.max_invocations_per_minute > 10_000
        {
            return Err(ManifestError::InvalidQuota(
                "max_invocations_per_minute doit être dans [1, 10000]",
            ));
        }
        if self.quotas.max_memory_mb == 0 || self.quotas.max_memory_mb > 1024 {
            return Err(ManifestError::InvalidQuota(
                "max_memory_mb doit être dans [1, 1024]",
            ));
        }
        if self.quotas.max_execution_time_ms == 0 || self.quotas.max_execution_time_ms > 30_000 {
            return Err(ManifestError::InvalidQuota(
                "max_execution_time_ms doit être dans [1, 30000]",
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_manifest_parses_cleanly() {
        let valid_toml = r#"
id = "notes-sync"
name = "Notes Synchronizer"
version = "1.2.0"
description = "Synchronise les notes locales en P2P"
author = "Sovereign Dev <dev@hakille.org>"
license = "Apache-2.0"
entrypoint = "notes_sync.wasm"
capabilities = ["filesystem:read", "mesh:broadcast"]

[quotas]
max_invocations_per_minute = 120
max_payload_bytes_per_call = 131072
max_memory_mb = 128
max_execution_time_ms = 1500
"#;
        let manifest = SdkManifest::parse_and_validate(valid_toml).unwrap();
        assert_eq!(manifest.id, "notes-sync");
        assert_eq!(manifest.version, "1.2.0");
        assert_eq!(manifest.capabilities.len(), 2);
        assert_eq!(manifest.quotas.max_memory_mb, 128);
    }

    #[test]
    fn test_wildcard_capability_rejected_fail_closed() {
        let bad_toml = r#"
id = "rogue-agent"
name = "Rogue Agent"
version = "0.1.0"
description = "Tentative d'escalade"
author = "Bad Actor"
license = "MIT"
entrypoint = "rogue.wasm"
capabilities = ["*"]
"#;
        assert!(matches!(
            SdkManifest::parse_and_validate(bad_toml),
            Err(ManifestError::ForbiddenCapability(_))
        ));
    }

    #[test]
    fn test_invalid_semver_rejected() {
        let bad_version = r#"
id = "bad-ver"
name = "Bad Version Agent"
version = "alpha-1"
description = "Version non conforme"
author = "Dev"
license = "Apache-2.0"
entrypoint = "agent.wasm"
capabilities = []
"#;
        assert!(matches!(
            SdkManifest::parse_and_validate(bad_version),
            Err(ManifestError::InvalidVersion(_))
        ));
    }
}
