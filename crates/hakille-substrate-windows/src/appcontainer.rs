use hakille_core::{CapabilityScope, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Represents a secured Windows AppContainer sandbox profile for an isolated WASM agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppContainerProfile {
    pub package_name: String,
    pub package_sid: String,
    pub allowed_capabilities: Vec<CapabilityScope>,
    pub capability_sids: Vec<String>,
    pub isolated_folder: PathBuf,
    pub is_active: bool,
}

/// Valide le nom de profil AppContainer (un seul segment sûr sans traversée).
pub fn validate_container_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 128
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.contains(':')
        || name.contains('\0')
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        return Err(hakille_core::HakilleError::CapabilityDenied(format!(
            "nom de profil AppContainer invalide : '{name}'"
        )));
    }
    Ok(())
}

impl AppContainerProfile {
    pub fn try_new(
        name: &str,
        capabilities: Vec<CapabilityScope>,
        base_dir: Option<PathBuf>,
    ) -> Result<Self> {
        validate_container_name(name)?;
        Ok(Self::new(name, capabilities, base_dir))
    }

    pub fn new(name: &str, capabilities: Vec<CapabilityScope>, base_dir: Option<PathBuf>) -> Self {
        let safe_name: String = name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            .collect();
        let package_name = format!("Hakille.AppContainer.{}", safe_name);
        // Deterministic AppContainer SID generation format: S-1-15-2-<hash1>-<hash2>-<hash3>-<hash4>
        let hash = safe_name.bytes().fold(0x811c9dc5u32, |acc, b| {
            (acc ^ b as u32).wrapping_mul(0x01000193)
        });
        let package_sid = format!(
            "S-1-15-2-{}-{}-{}-{}",
            hash,
            hash.wrapping_add(1),
            hash.wrapping_add(2),
            hash.wrapping_add(3)
        );

        let mut capability_sids = Vec::new();
        for cap in &capabilities {
            match cap {
                CapabilityScope::Network => {
                    // WinCapabilityInternetClientSid: S-1-15-3-1
                    capability_sids.push("S-1-15-3-1".to_string());
                }
                CapabilityScope::Filesystem => {
                    // WinCapabilityDocumentsLibrarySid: S-1-15-3-3
                    capability_sids.push("S-1-15-3-3".to_string());
                }
                CapabilityScope::Audio => {
                    // WinCapabilityMicrophoneSid: S-1-15-3-2
                    capability_sids.push("S-1-15-3-2".to_string());
                }
                CapabilityScope::Camera => {
                    // WinCapabilityWebcamSid: S-1-15-3-4
                    capability_sids.push("S-1-15-3-4".to_string());
                }
                _ => {}
            }
        }

        let base = base_dir.unwrap_or_else(|| {
            std::env::var("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("C:\\ProgramData\\Hakille"))
                .join("AppContainers")
        });

        let isolated_folder = base.join(&package_name);

        Self {
            package_name,
            package_sid,
            allowed_capabilities: capabilities,
            capability_sids,
            isolated_folder,
            is_active: false,
        }
    }

    /// Check if a given capability is granted within this sandbox.
    pub fn has_capability(&self, scope: CapabilityScope) -> bool {
        self.allowed_capabilities.contains(&scope)
    }

    /// Ensure the AppContainer isolated working directory exists on disk with proper folder structure.
    pub fn ensure_storage(&self) -> Result<()> {
        let Some(suffix) = self.package_name.strip_prefix("Hakille.AppContainer.") else {
            return Err(hakille_core::HakilleError::CapabilityDenied(
                "préfixe package_name AppContainer invalide".into(),
            ));
        };
        validate_container_name(suffix)?;
        if !self.isolated_folder.exists() {
            std::fs::create_dir_all(&self.isolated_folder).map_err(|e| {
                hakille_core::HakilleError::StorageError(format!(
                    "Failed to create AppContainer directory {}: {}",
                    self.isolated_folder.display(),
                    e
                ))
            })?;
        }
        Ok(())
    }
}

pub struct AppContainerSandbox;

impl AppContainerSandbox {
    pub fn create_profile(
        name: &str,
        capabilities: Vec<CapabilityScope>,
        isolated_folder: &str,
    ) -> Result<AppContainerProfile> {
        let mut profile =
            AppContainerProfile::try_new(name, capabilities, Some(PathBuf::from(isolated_folder)))?;
        profile.ensure_storage()?;
        profile.is_active = true;
        Ok(profile)
    }

    pub fn teardown(profile: &mut AppContainerProfile) -> Result<()> {
        let Some(suffix) = profile.package_name.strip_prefix("Hakille.AppContainer.") else {
            return Err(hakille_core::HakilleError::CapabilityDenied(
                "teardown AppContainer refusé : package_name invalide".into(),
            ));
        };
        validate_container_name(suffix)?;
        profile.is_active = false;
        if let Ok(meta) = std::fs::symlink_metadata(&profile.isolated_folder)
            && !meta.file_type().is_symlink()
        {
            let _ = std::fs::remove_dir_all(&profile.isolated_folder);
        }
        Ok(())
    }
}
