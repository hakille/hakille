use crate::audio::capture::AudioChunk;
use async_trait::async_trait;
use hakille_core::{HakilleError, Result};
use hakille_inference::CampplusOnnx;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceBiometricEmbedding {
    pub vector: Vec<f32>,
    pub dimension: usize,
}

impl VoiceBiometricEmbedding {
    pub fn cosine_similarity(&self, other: &Self) -> f32 {
        if self.dimension != self.vector.len() || other.dimension != other.vector.len() {
            return 0.0;
        }
        hakille_inference::speaker_onnx::cosine_similarity(&self.vector, &other.vector)
    }
}

#[async_trait]
pub trait SpeakerIdentifier: Send + Sync {
    async fn extract_embedding(&self, chunk: &AudioChunk) -> Result<VoiceBiometricEmbedding>;
}

/// Biométrie locuteur RÉELLE : embeddings CAM++ via forward `ort`
/// (`CampplusOnnx`), fbank 80×200 + 12+ couches TDNN-CAM exécutées.
/// Erreur LOUD si le checkpoint est absent — aucun fallback simulé.
pub struct CamPlusPlusIdentifier {
    embedding_dim: usize,
    model_path: Option<PathBuf>,
    cache: std::sync::Mutex<Option<CampplusOnnx>>,
}

impl CamPlusPlusIdentifier {
    pub fn new() -> Self {
        Self {
            embedding_dim: 192,
            model_path: None,
            cache: std::sync::Mutex::new(None),
        }
    }

    pub fn with_model_path(path: impl AsRef<Path>) -> Self {
        Self {
            embedding_dim: 192,
            model_path: Some(path.as_ref().to_path_buf()),
            cache: std::sync::Mutex::new(None),
        }
    }

    pub fn model_name(&self) -> &str {
        "CAM++ (zh-en 16k advanced, ONNX réel)"
    }

    pub fn model_path(&self) -> Option<&Path> {
        self.model_path.as_deref()
    }

    /// Chemin effectif : explicite > défaut. Erreur LOUD si introuvable.
    fn resolve_path(&self) -> Result<PathBuf> {
        if let Some(ref p) = self.model_path {
            if p.exists() {
                return Ok(p.clone());
            }
            return Err(HakilleError::InferenceError(format!(
                "CamPlusPlusIdentifier: modèle CAM++ absent '{}' — provisionnez models/campplus_zh_en_16k_advanced.onnx (RÉEL exigé, pas de sine-hash)",
                p.display()
            )));
        }
        CampplusOnnx::default_model_path().ok_or_else(|| {
            HakilleError::InferenceError(
                "CamPlusPlusIdentifier: checkpoint CAM++ introuvable (models/campplus_zh_en_16k_advanced.onnx absent) — biométrie RÉELLE impossible, aucun fallback simulé"
                    .to_string(),
            )
        })
    }

    fn mono_16k(chunk: &AudioChunk) -> Result<Vec<f32>> {
        hakille_inference::audio_resample::try_to_mono_16k(
            &chunk.samples,
            chunk.channels,
            chunk.sample_rate,
        )
    }
}

impl Default for CamPlusPlusIdentifier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SpeakerIdentifier for CamPlusPlusIdentifier {
    async fn extract_embedding(&self, chunk: &AudioChunk) -> Result<VoiceBiometricEmbedding> {
        let pcm_16k = Self::mono_16k(chunk)?;
        if pcm_16k.is_empty() {
            return Err(HakilleError::InferenceError(
                "CamPlusPlusIdentifier: chunk vide — embedding RÉEL impossible".to_string(),
            ));
        }
        // Session mise en cache (ouverture paresseuse, réutilisée).
        // `CampplusOnnx::open` est bloquant : on l'appelle hors verrou long.
        let path = self.resolve_path()?;
        let mut guard = self.cache.lock().map_err(|e| {
            HakilleError::InferenceError(format!("CamPlusPlusIdentifier: mutex empoisonné : {e}"))
        })?;
        if guard.is_none() {
            let sess = CampplusOnnx::open(&path)?;
            *guard = Some(sess);
        }
        let vector = guard
            .as_ref()
            .expect("session CAM++ initialisée")
            .embed(&pcm_16k)?;
        Ok(VoiceBiometricEmbedding {
            dimension: self.embedding_dim,
            vector,
        })
    }
}
