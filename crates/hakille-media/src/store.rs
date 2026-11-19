//! Stockage content-addressed (`<data_dir>/media/`) avec index atomique,
//! sidecars Ed25519 et ramasse-miettes LRU borné.

use crate::catalog::{sha256_bytes, sha256_file};
use crate::error::{MediaError, MediaResult};
use crate::provenance::{
    MediaProvenance, MediaProvenancePayload, PROVENANCE_PNG_KEYWORD, PROVENANCE_SCHEMA_VERSION,
    inject_png_itxt_chunk, sign_provenance, write_provenance_sidecar,
};
use crate::types::{EngineKind, MediaArtifact, MediaDataExit, MediaModality, PreviewImage};
use chrono::Utc;
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Paramètres techniques nécessaires pour persister et signer un artefact généré.
#[derive(Debug, Clone)]
pub struct StoreArtifactParams {
    pub artifact_id: Uuid,
    pub modality: MediaModality,
    pub extension: String,
    pub mime_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_secs: Option<f32>,
    pub fps: Option<u16>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub seed: u64,
    pub prompt: String,
    pub engine_id: String,
    pub engine_kind: EngineKind,
    pub data_exit: MediaDataExit,
    pub model_id: Option<String>,
    pub model_sha256: Option<String>,
    pub parent_artifacts: Vec<String>,
    pub visemes_json: Option<Vec<u8>>,
    pub preview: Option<PreviewImage>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct MediaIndex {
    version: u32,
    artifacts: Vec<MediaArtifact>,
}

/// Dépôt local content-addressed pour tous les médias générés par Hakille.
#[derive(Debug, Clone)]
pub struct MediaStore {
    root_dir: PathBuf,
    max_store_bytes: u64,
}

impl MediaStore {
    pub fn new(root_dir: impl Into<PathBuf>, max_store_bytes: u64) -> MediaResult<Self> {
        let root_dir = root_dir.into();
        std::fs::create_dir_all(root_dir.join("blobs"))?;
        Ok(Self {
            root_dir,
            max_store_bytes,
        })
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn index_path(&self) -> PathBuf {
        self.root_dir.join("index.json")
    }

    /// Stocke des octets bruts (PNG, JPEG, WAV, APNG, Y4M, MP4, WebM), embarque
    /// le chunk `iTXt` pour les PNG/APNG, signe le manifeste Ed25519, écrit le
    /// sidecar `.provenance.json` et met à jour `index.json` atomiquement.
    pub fn store_bytes(
        &self,
        raw_bytes: Vec<u8>,
        params: StoreArtifactParams,
        signing_key: &SigningKey,
    ) -> MediaResult<(MediaArtifact, MediaProvenance)> {
        let ext = sanitize_extension(&params.extension);
        let created_at = Utc::now();
        let prompt_sha256 = sha256_bytes(params.prompt.as_bytes());

        // Pour PNG/APNG, injecte un résumé provenance `iTXt` dans le flux binaire
        // avant le calcul du SHA-256 final afin que le fichier sur disque et la
        // signature couvrent tous les octets y compris le chunk embarqué.
        let final_bytes = if matches!(ext.as_str(), "png" | "apng") {
            let embedded_meta = serde_json::json!({
                "schema_version": PROVENANCE_SCHEMA_VERSION,
                "artifact_id": params.artifact_id.to_string(),
                "modality": params.modality,
                "engine_id": params.engine_id,
                "engine_kind": params.engine_kind,
                "seed": params.seed,
                "prompt_sha256": prompt_sha256,
                "synthetic_ai_disclosure": true,
                "created_at": created_at,
            });
            inject_png_itxt_chunk(
                &raw_bytes,
                PROVENANCE_PNG_KEYWORD,
                &embedded_meta.to_string(),
            )?
        } else {
            raw_bytes
        };

        let content_sha256 = sha256_bytes(&final_bytes);
        let prefix = &content_sha256[..2];
        let shard_dir = self.root_dir.join("blobs").join(prefix);
        std::fs::create_dir_all(&shard_dir)?;

        let filename = format!("{}-{}.{ext}", &content_sha256[..24], params.artifact_id);
        let blob_path = shard_dir.join(filename);
        let tmp_blob = blob_path.with_extension(format!("{ext}.tmp"));
        std::fs::write(&tmp_blob, &final_bytes)?;
        std::fs::rename(&tmp_blob, &blob_path)?;

        let visemes_path = if let Some(visemes) = &params.visemes_json {
            let vp = blob_path.with_extension("visemes.json");
            let tmp_vp = blob_path.with_extension("visemes.tmp");
            std::fs::write(&tmp_vp, visemes)?;
            std::fs::rename(&tmp_vp, &vp)?;
            Some(vp)
        } else {
            None
        };

        let payload = MediaProvenancePayload {
            schema_version: PROVENANCE_SCHEMA_VERSION,
            artifact_id: params.artifact_id.to_string(),
            modality: params.modality,
            mime_type: params.mime_type.clone(),
            content_sha256: content_sha256.clone(),
            bytes_len: final_bytes.len() as u64,
            prompt_sha256,
            engine_id: params.engine_id.clone(),
            engine_kind: params.engine_kind,
            data_exit: params.data_exit.clone(),
            model_id: params.model_id.clone(),
            model_sha256: params.model_sha256.clone(),
            seed: params.seed,
            parent_artifacts: params.parent_artifacts.clone(),
            synthetic_ai_disclosure: true,
            created_at,
            signer_public_key_hex: String::new(),
        };
        let provenance = sign_provenance(payload, signing_key)?;
        let provenance_path = write_provenance_sidecar(&blob_path, &provenance)?;

        let artifact = MediaArtifact {
            id: params.artifact_id,
            modality: params.modality,
            path: blob_path,
            mime_type: params.mime_type,
            sha256: content_sha256,
            bytes_len: final_bytes.len() as u64,
            width: params.width,
            height: params.height,
            duration_secs: params.duration_secs,
            fps: params.fps,
            sample_rate: params.sample_rate,
            channels: params.channels,
            seed: params.seed,
            prompt_summary: summarize_prompt(&params.prompt, 120),
            engine_id: params.engine_id,
            engine_kind: params.engine_kind,
            data_exit: params.data_exit,
            model_id: params.model_id,
            provenance_path,
            provenance_signature_hex: provenance.signature_hex.clone(),
            visemes_path,
            parent_artifacts: params.parent_artifacts,
            preview: params.preview,
            created_at,
            elapsed_ms: params.elapsed_ms,
        };

        self.append_index(artifact.clone())?;
        let _ = self.enforce_quota();
        Ok((artifact, provenance))
    }

    /// Stocke un fichier déjà présent sur disque (ex. sortie MP4/WebM de `ffmpeg`)
    /// en le déplaçant dans le dépôt content-addressed et en signant sa provenance.
    pub fn store_existing_file(
        &self,
        source_file: &Path,
        params: StoreArtifactParams,
        signing_key: &SigningKey,
    ) -> MediaResult<(MediaArtifact, MediaProvenance)> {
        if !source_file.is_file() {
            return Err(MediaError::Storage(format!(
                "fichier média source introuvable : {}",
                source_file.display()
            )));
        }
        let ext = sanitize_extension(&params.extension);
        let content_sha256 = sha256_file(source_file)?;
        let bytes_len = std::fs::metadata(source_file)?.len();
        let prefix = &content_sha256[..2];
        let shard_dir = self.root_dir.join("blobs").join(prefix);
        std::fs::create_dir_all(&shard_dir)?;

        let filename = format!("{}-{}.{ext}", &content_sha256[..24], params.artifact_id);
        let blob_path = shard_dir.join(filename);
        std::fs::copy(source_file, &blob_path)?;

        let created_at = Utc::now();
        let prompt_sha256 = sha256_bytes(params.prompt.as_bytes());
        let payload = MediaProvenancePayload {
            schema_version: PROVENANCE_SCHEMA_VERSION,
            artifact_id: params.artifact_id.to_string(),
            modality: params.modality,
            mime_type: params.mime_type.clone(),
            content_sha256: content_sha256.clone(),
            bytes_len,
            prompt_sha256,
            engine_id: params.engine_id.clone(),
            engine_kind: params.engine_kind,
            data_exit: params.data_exit.clone(),
            model_id: params.model_id.clone(),
            model_sha256: params.model_sha256.clone(),
            seed: params.seed,
            parent_artifacts: params.parent_artifacts.clone(),
            synthetic_ai_disclosure: true,
            created_at,
            signer_public_key_hex: String::new(),
        };
        let provenance = sign_provenance(payload, signing_key)?;
        let provenance_path = write_provenance_sidecar(&blob_path, &provenance)?;

        let artifact = MediaArtifact {
            id: params.artifact_id,
            modality: params.modality,
            path: blob_path,
            mime_type: params.mime_type,
            sha256: content_sha256,
            bytes_len,
            width: params.width,
            height: params.height,
            duration_secs: params.duration_secs,
            fps: params.fps,
            sample_rate: params.sample_rate,
            channels: params.channels,
            seed: params.seed,
            prompt_summary: summarize_prompt(&params.prompt, 120),
            engine_id: params.engine_id,
            engine_kind: params.engine_kind,
            data_exit: params.data_exit,
            model_id: params.model_id,
            provenance_path,
            provenance_signature_hex: provenance.signature_hex.clone(),
            visemes_path: None,
            parent_artifacts: params.parent_artifacts,
            preview: params.preview,
            created_at,
            elapsed_ms: params.elapsed_ms,
        };

        self.append_index(artifact.clone())?;
        let _ = self.enforce_quota();
        Ok((artifact, provenance))
    }

    /// Liste les artefacts les plus récents en premier.
    pub fn list(&self, limit: usize) -> MediaResult<Vec<MediaArtifact>> {
        let mut idx = self.load_index()?;
        idx.artifacts
            .sort_by_key(|a| std::cmp::Reverse(a.created_at));
        if limit > 0 && idx.artifacts.len() > limit {
            idx.artifacts.truncate(limit);
        }
        Ok(idx.artifacts)
    }

    /// Recherche un artefact par UUID complet ou préfixe hexadécimal.
    pub fn get(&self, id_or_prefix: &str) -> MediaResult<Option<MediaArtifact>> {
        let idx = self.load_index()?;
        let needle = id_or_prefix.trim().to_ascii_lowercase();
        Ok(idx.artifacts.into_iter().find(|a| {
            a.id.to_string().to_ascii_lowercase().starts_with(&needle)
                || a.sha256.to_ascii_lowercase().starts_with(&needle)
        }))
    }

    /// Nombre total d'octets stockés d'après l'index.
    pub fn total_bytes(&self) -> MediaResult<u64> {
        let idx = self.load_index()?;
        Ok(idx.artifacts.iter().map(|a| a.bytes_len).sum())
    }

    fn load_index(&self) -> MediaResult<MediaIndex> {
        let path = self.index_path();
        if !path.is_file() {
            return Ok(MediaIndex {
                version: 1,
                artifacts: Vec::new(),
            });
        }
        let raw = std::fs::read(&path)?;
        let idx: MediaIndex = serde_json::from_slice(&raw)?;
        Ok(idx)
    }

    fn save_index(&self, idx: &MediaIndex) -> MediaResult<()> {
        let path = self.index_path();
        let tmp = self.root_dir.join("index.json.tmp");
        let bytes = serde_json::to_vec_pretty(idx)?;
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    fn append_index(&self, mut artifact: MediaArtifact) -> MediaResult<()> {
        // Ne stocke pas les gros pixels RGBA bruts dans index.json pour garder
        // l'index léger et rapide à charger.
        artifact.preview = None;
        let mut idx = self.load_index()?;
        idx.version = 1;
        idx.artifacts.push(artifact);
        self.save_index(&idx)
    }

    /// Éviction LRU si le total dépasse `max_store_bytes`.
    fn enforce_quota(&self) -> MediaResult<()> {
        if self.max_store_bytes == 0 {
            return Ok(());
        }
        let mut idx = self.load_index()?;
        let mut total: u64 = idx.artifacts.iter().map(|a| a.bytes_len).sum();
        if total <= self.max_store_bytes {
            return Ok(());
        }
        // Trie du plus ancien au plus récent pour évincer les plus anciens.
        idx.artifacts.sort_by_key(|a| a.created_at);
        let mut kept = Vec::with_capacity(idx.artifacts.len());
        for art in idx.artifacts {
            if total > self.max_store_bytes && kept.len() + 1 < 1_000_000 {
                // Conserve toujours au moins le dernier artefact ajouté.
                if art.path.is_file() {
                    let _ = std::fs::remove_file(&art.path);
                }
                if art.provenance_path.is_file() {
                    let _ = std::fs::remove_file(&art.provenance_path);
                }
                if let Some(vp) = &art.visemes_path
                    && vp.is_file()
                {
                    let _ = std::fs::remove_file(vp);
                }
                total = total.saturating_sub(art.bytes_len);
            } else {
                kept.push(art);
            }
        }
        idx.artifacts = kept;
        self.save_index(&idx)
    }
}

fn sanitize_extension(ext: &str) -> String {
    let clean: String = ext
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if clean.is_empty() {
        "bin".to_string()
    } else {
        clean
    }
}

fn summarize_prompt(prompt: &str, max_chars: usize) -> String {
    let single: String = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    if single.chars().count() <= max_chars {
        single
    } else {
        let mut out: String = single.chars().take(max_chars.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
