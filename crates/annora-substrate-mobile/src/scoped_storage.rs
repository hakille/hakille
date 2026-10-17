use annora_core::{AnnoraError, Result, VirtualPath};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StorageScope {
    Documents,
    Cache,
    ScopedExternalMedia,
    SecureKeystore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopedStorageSandbox {
    app_container_root: PathBuf,
    permitted_media_uris: HashSet<String>,
}

impl ScopedStorageSandbox {
    pub fn new(app_container_root: impl Into<PathBuf>) -> Self {
        Self {
            app_container_root: app_container_root.into(),
            permitted_media_uris: HashSet::new(),
        }
    }

    pub fn grant_media_access(&mut self, uri: impl Into<String>) {
        self.permitted_media_uris.insert(uri.into());
    }

    pub fn resolve_path(
        &self,
        scope: StorageScope,
        relative_path: &VirtualPath,
    ) -> Result<PathBuf> {
        // Enforce sandbox isolation: forbid directory traversal and absolute paths
        let path_str = &relative_path.0;
        let p = Path::new(path_str);
        if path_str.is_empty()
            || path_str.contains("..")
            || path_str.starts_with('/')
            || path_str.starts_with('\\')
            || path_str.contains('\0')
            || p.is_absolute()
            || path_str.chars().nth(1) == Some(':')
        {
            return Err(AnnoraError::SubstrateError(format!(
                "Security violation: path traversal detected in '{}'",
                path_str
            )));
        }

        let base = match scope {
            StorageScope::Documents => self.app_container_root.join("Documents"),
            StorageScope::Cache => self.app_container_root.join("Cache"),
            StorageScope::ScopedExternalMedia => {
                if !self.permitted_media_uris.contains(path_str) {
                    return Err(AnnoraError::SubstrateError(format!(
                        "External media access denied for '{}': user permission required via SAF / PhotoPicker",
                        path_str
                    )));
                }
                self.app_container_root.join("ExternalMedia")
            }
            StorageScope::SecureKeystore => self.app_container_root.join("Keystore"),
        };

        let resolved = base.join(path_str);
        if !resolved.starts_with(&base) {
            return Err(AnnoraError::SubstrateError(format!(
                "Security violation: resolved path escapes sandbox base: {:?}",
                resolved
            )));
        }

        Ok(resolved)
    }

    pub fn container_root(&self) -> &Path {
        &self.app_container_root
    }
}

impl Default for ScopedStorageSandbox {
    fn default() -> Self {
        Self::new(PathBuf::from("/data/user/0/com.annora.ai/files"))
    }
}
