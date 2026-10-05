use crate::audio::capture::AudioChunk;
use annora_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

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

pub struct CamPlusPlusIdentifier {
    embedding_dim: usize,
}

impl CamPlusPlusIdentifier {
    pub fn new() -> Self {
        Self {
            embedding_dim: 192,
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
        let mut vector = vec![0.0f32; self.embedding_dim];
        if !chunk.samples.is_empty() {
            for (i, &sample) in chunk.samples.iter().enumerate() {
                vector[i % self.embedding_dim] += sample;
            }
            // Normalize L2
            let norm: f32 = vector.iter().map(|&x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for x in &mut vector {
                    *x /= norm;
                }
            }
        }

        Ok(VoiceBiometricEmbedding {
            vector,
            dimension: self.embedding_dim,
        })
    }
}
