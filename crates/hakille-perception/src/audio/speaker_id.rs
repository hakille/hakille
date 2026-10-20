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
        if self.vector.len() != other.vector.len() || self.vector.is_empty() {
            return 0.0;
        }

        let dot_product: f32 = self
            .vector
            .iter()
            .zip(&other.vector)
            .map(|(&a, &b)| {
                if a.is_finite() && b.is_finite() {
                    a * b
                } else {
                    0.0
                }
            })
            .sum();

        let norm_a: f32 = self
            .vector
            .iter()
            .map(|&a| if a.is_finite() { a * a } else { 0.0 })
            .sum::<f32>()
            .sqrt();
        let norm_b: f32 = other
            .vector
            .iter()
            .map(|&b| if b.is_finite() { b * b } else { 0.0 })
            .sum::<f32>()
            .sqrt();

        if norm_a <= 0.0 || norm_b <= 0.0 {
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
        if chunk.sample_rate == 0 {
            return Err(HakilleError::InferenceError(
                "CamPlusPlusIdentifier: sample_rate nul".to_string(),
            ));
        }
        // Dé-interlace multi-canaux par moyenne.
        let mono: Vec<f32> = if chunk.channels > 1 {
            let ch = chunk.channels as usize;
            let frames = chunk.samples.len() / ch;
            let mut m = Vec::with_capacity(frames);
            for f in 0..frames {
                let mut acc = 0.0f32;
                for c in 0..ch {
                    let s = chunk.samples[f * ch + c];
                    acc += if s.is_finite() { s } else { 0.0 };
                }
                m.push((acc / ch as f32).clamp(-1.0, 1.0));
            }
            m
        } else {
            chunk
                .samples
                .iter()
                .map(|s| {
                    if s.is_finite() {
                        s.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    }
                })
                .collect()
        };
        if chunk.sample_rate == 16_000 {
            return Ok(mono);
        }
        // Ré-échantillonnage linéaire pur Rust.
        let ratio = 16_000.0 / chunk.sample_rate as f64;
        let out_len = ((mono.len() as f64 * ratio).round() as usize).max(1);
        let mut out = Vec::with_capacity(out_len);
        for i in 0..out_len {
            let pos = i as f64 / ratio;
            let i0 = pos.floor() as usize;
            let frac = (pos - i0 as f64) as f32;
            let s0 = mono.get(i0).copied().unwrap_or(0.0);
            let s1 = mono.get(i0 + 1).copied().unwrap_or(s0);
            out.push(s0 + (s1 - s0) * frac);
        }
        Ok(out)
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
