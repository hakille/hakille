use std::path::{Path, PathBuf};
use annora_core::{AnnoraError, Result};
use serde::{Deserialize, Serialize};

use crate::catalog::{ModelCatalog, ModelEntry, ModelTier};
use crate::downloader::{ModelDownloader, ProgressCallback};

/// High-level provision status of a model in the local system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProvisionStatus {
    pub entry: ModelEntry,
    pub is_installed: bool,
    pub primary_path: Option<PathBuf>,
    pub total_disk_bytes: u64,
    pub missing_artifacts: Vec<String>,
}

/// Coordinates local model storage, integrity verification, and zero-touch autonomous downloads.
#[derive(Clone)]
pub struct ModelProvisioner {
    cache_dir: PathBuf,
    downloader: ModelDownloader,
    catalog: Vec<ModelEntry>,
}

impl Default for ModelProvisioner {
    fn default() -> Self {
        let default_cache = std::env::var("ANNORA_MODELS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./models"));
        Self::new(default_cache)
    }
}

impl ModelProvisioner {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            downloader: ModelDownloader::new(),
            catalog: ModelCatalog::official_catalog(),
        }
    }

    pub fn with_catalog(cache_dir: PathBuf, catalog: Vec<ModelEntry>) -> Self {
        Self {
            cache_dir,
            downloader: ModelDownloader::new(),
            catalog,
        }
    }

    /// The base directory where model artifacts are cached.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Full local path for an artifact by its filename.
    pub fn artifact_path(&self, filename: &str) -> PathBuf {
        self.cache_dir.join(filename)
    }

    /// Access the registered model catalog entries.
    pub fn catalog(&self) -> &[ModelEntry] {
        &self.catalog
    }

    /// Check the local installation status of a specific model entry.
    pub fn status(&self, entry: &ModelEntry) -> ModelProvisionStatus {
        let mut missing = Vec::new();
        let mut total_bytes = 0u64;

        // Check primary artifact
        let primary_path = self.artifact_path(&entry.primary_artifact.filename);
        if primary_path.exists() {
            if let Ok(meta) = std::fs::metadata(&primary_path) {
                total_bytes += meta.len();
            }
        } else {
            missing.push(entry.primary_artifact.filename.clone());
        }

        // Check auxiliary artifacts
        for aux in &entry.auxiliary_artifacts {
            let aux_path = self.artifact_path(&aux.filename);
            if aux_path.exists() {
                if let Ok(meta) = std::fs::metadata(&aux_path) {
                    total_bytes += meta.len();
                }
            } else {
                missing.push(aux.filename.clone());
            }
        }

        let is_installed = missing.is_empty();
        ModelProvisionStatus {
            entry: entry.clone(),
            is_installed,
            primary_path: if is_installed { Some(primary_path) } else { None },
            total_disk_bytes: total_bytes,
            missing_artifacts: missing,
        }
    }

    /// List installation status for all models in the catalog.
    pub fn list_all_status(&self) -> Vec<ModelProvisionStatus> {
        self.catalog.iter().map(|m| self.status(m)).collect()
    }

    /// Verify cryptographic integrity of all artifacts for a model if present on disk.
    pub fn verify_integrity(&self, entry: &ModelEntry) -> Result<bool> {
        let status = self.status(entry);
        if !status.is_installed {
            return Ok(false);
        }

        // Check primary artifact
        let primary_path = self.artifact_path(&entry.primary_artifact.filename);
        if let Some(ref expected_hash) = entry.primary_artifact.sha256_checksum {
            if !ModelDownloader::verify_sha256(&primary_path, expected_hash)? {
                return Ok(false);
            }
        }

        // Check auxiliary artifacts
        for aux in &entry.auxiliary_artifacts {
            let aux_path = self.artifact_path(&aux.filename);
            if let Some(ref expected_hash) = aux.sha256_checksum {
                if !ModelDownloader::verify_sha256(&aux_path, expected_hash)? {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    /// Download and provision all artifacts for a given model entry.
    pub async fn provision_model(
        &self,
        entry: &ModelEntry,
        progress: Option<ProgressCallback>,
    ) -> Result<PathBuf> {
        // Ensure destination directory exists
        tokio::fs::create_dir_all(&self.cache_dir)
            .await
            .map_err(|e| AnnoraError::StorageError(format!("Failed to create model cache dir {}: {}", self.cache_dir.display(), e)))?;

        // 1. Download auxiliary artifacts first (e.g., tokens.txt, configs)
        for aux in &entry.auxiliary_artifacts {
            let aux_dest = self.artifact_path(&aux.filename);
            if !aux_dest.exists() {
                tracing::info!(model = %entry.id, file = %aux.filename, "Downloading auxiliary model artifact");
                let res = self.downloader.download_file(
                    &aux.primary_url,
                    &aux_dest,
                    Some(aux.size_bytes),
                    aux.sha256_checksum.as_deref(),
                    progress.clone(),
                ).await;

                if let Err(e) = res {
                    if let Some(ref fallback) = aux.fallback_url {
                        tracing::warn!(url = %aux.primary_url, fallback = %fallback, "Primary URL failed, retrying with fallback");
                        self.downloader.download_file(
                            fallback,
                            &aux_dest,
                            Some(aux.size_bytes),
                            aux.sha256_checksum.as_deref(),
                            progress.clone(),
                        ).await?;
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        // 2. Download primary model artifact
        let primary_dest = self.artifact_path(&entry.primary_artifact.filename);
        if !primary_dest.exists() {
            tracing::info!(model = %entry.id, file = %entry.primary_artifact.filename, "Downloading primary model artifact");
            let res = self.downloader.download_file(
                &entry.primary_artifact.primary_url,
                &primary_dest,
                Some(entry.primary_artifact.size_bytes),
                entry.primary_artifact.sha256_checksum.as_deref(),
                progress.clone(),
            ).await;

            if let Err(e) = res {
                if let Some(ref fallback) = entry.primary_artifact.fallback_url {
                    tracing::warn!(url = %entry.primary_artifact.primary_url, fallback = %fallback, "Primary URL failed, retrying with fallback");
                    self.downloader.download_file(
                        fallback,
                        &primary_dest,
                        Some(entry.primary_artifact.size_bytes),
                        entry.primary_artifact.sha256_checksum.as_deref(),
                        progress,
                    ).await?;
                } else {
                    return Err(e);
                }
            }
        }

        Ok(primary_dest)
    }

    /// Provision a model by its identifier.
    pub async fn provision_by_id(
        &self,
        id: &str,
        progress: Option<ProgressCallback>,
    ) -> Result<PathBuf> {
        let entry = self.catalog.iter().find(|m| m.id == id).cloned()
            .ok_or_else(|| AnnoraError::InferenceError(format!("Model ID '{}' not found in catalog", id)))?;
        self.provision_model(&entry, progress).await
    }

    /// Provision all models required for a given deployment tier (Guest, Native, Sovereign).
    pub async fn provision_tier(
        &self,
        tier: ModelTier,
        progress: Option<ProgressCallback>,
    ) -> Result<Vec<PathBuf>> {
        let target_models = ModelCatalog::models_for_tier(tier);
        let mut provisioned = Vec::new();

        for entry in target_models {
            let path = self.provision_model(&entry, progress.clone()).await?;
            provisioned.push(path);
        }

        Ok(provisioned)
    }

    /// Provision all models defined in the official catalog.
    pub async fn provision_all(
        &self,
        progress: Option<ProgressCallback>,
    ) -> Result<Vec<PathBuf>> {
        let mut provisioned = Vec::new();
        for entry in &self.catalog {
            let path = self.provision_model(entry, progress.clone()).await?;
            provisioned.push(path);
        }
        Ok(provisioned)
    }

    /// Remove a model and its associated artifacts from local disk storage.
    pub fn remove_model(&self, entry: &ModelEntry) -> Result<()> {
        let primary_dest = self.artifact_path(&entry.primary_artifact.filename);
        if primary_dest.exists() {
            std::fs::remove_file(&primary_dest)
                .map_err(|e| AnnoraError::StorageError(format!("Failed to delete {}: {}", primary_dest.display(), e)))?;
        }

        for aux in &entry.auxiliary_artifacts {
            let aux_dest = self.artifact_path(&aux.filename);
            if aux_dest.exists() {
                std::fs::remove_file(&aux_dest)
                    .map_err(|e| AnnoraError::StorageError(format!("Failed to delete {}: {}", aux_dest.display(), e)))?;
            }
        }

        Ok(())
    }
}
