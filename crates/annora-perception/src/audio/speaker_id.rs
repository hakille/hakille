use crate::audio::capture::AudioChunk;
use annora_core::Result;
use annora_inference::CampplusSpeakerEngine;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceBiometricEmbedding {
    pub vector: Vec<f32>,
    pub dimension: usize,
}

impl VoiceBiometricEmbedding {
    pub fn cosine_similarity(&self, other: &Self) -> f32 {
        if self.vector.len() != other.vector.len() || self.vector.is_empty() {
            return 0.0;
        }

        let dot_product: f32 = self
            .vector
            .iter()
            .zip(&other.vector)
            .map(|(&a, &b)| a * b)
            .sum();

        let norm_a: f32 = self.vector.iter().map(|&a| a * a).sum::<f32>().sqrt();
        let norm_b: f32 = other.vector.iter().map(|&b| b * b).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            (dot_product / (norm_a * norm_b)).clamp(-1.0, 1.0)
        }
    }
}

#[async_trait]
pub trait SpeakerIdentifier: Send + Sync {
    async fn extract_embedding(&self, chunk: &AudioChunk) -> Result<VoiceBiometricEmbedding>;
}

/// CAM++ neural speaker verification engine (192-dim biometric embeddings).
pub struct CamPlusPlusIdentifier {
    embedding_dim: usize,
    model_path: Option<PathBuf>,
}

impl CamPlusPlusIdentifier {
    pub fn new() -> Self {
        Self {
            embedding_dim: 192,
            model_path: None,
        }
    }

    pub fn with_model_path(path: impl AsRef<Path>) -> Self {
        Self {
            embedding_dim: 192,
            model_path: Some(path.as_ref().to_path_buf()),
        }
    }

    pub fn model_name(&self) -> &str {
        "CAM++ (zh-en 16k advanced)"
    }

    pub fn model_path(&self) -> Option<&Path> {
        self.model_path.as_deref()
    }

    pub fn speaker_engine(&self) -> CampplusSpeakerEngine {
        if let Some(ref p) = self.model_path {
            CampplusSpeakerEngine::with_model(p)
        } else {
            CampplusSpeakerEngine::new()
        }
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
        let engine = self.speaker_engine();
        let vector = engine.extract_speaker_embedding(&chunk.samples);

        Ok(VoiceBiometricEmbedding {
            vector,
            dimension: self.embedding_dim,
        })
    }
}
