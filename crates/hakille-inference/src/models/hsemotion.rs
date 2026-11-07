//! HSEmotion MTL (`enet_b0_8_va_mtl.onnx`) — Classification 8 classes & Espace continu Valence/Arousal.
//!
//! Ce module implémente le modèle SOTA d'expression faciale multi-tâche :
//! - 8 classes discrètes : Anger, Contempt, Disgust, Fear, Happiness, Neutral, Sadness, Surprise.
//! - Modèle circumplex continu :
//!   - Valence $\in [-1.0, 1.0]$ (dimension plaisir vs déplaisir).
//!   - Arousal $\in [-1.0, 1.0]$ (dimension énergie / activation physiologique).
//!
//! En l'absence de GPU ou des poids sur disque, [`HsEmotionEngine::mock_synthetic()`] fournit
//! une prédiction déterministe crédible avec statut honnête [`ModelStatus::Simulated`].

use crate::ModelStatus;
use crate::models::preprocess::{NormalizationMode, resize_bilinear_rgb_nchw};
use crate::ort_env::shared_ort_env;
use hakille_core::{HakilleError, Result};
use ort::{session::Session, value::Tensor};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const HSEMOTION_INPUT_SIZE: usize = 224;
pub const HSEMOTION_MODEL_FILENAME: &str = "enet_b0_8_va_mtl.onnx";

/// Les 8 classes d'émotions discrètes du standard AffectNet / HSEmotion.
pub const EMOTION_CLASSES: [&str; 8] = [
    "Anger",
    "Contempt",
    "Disgust",
    "Fear",
    "Happiness",
    "Neutral",
    "Sadness",
    "Surprise",
];

fn ort_err(context: &str, e: impl std::fmt::Display) -> HakilleError {
    HakilleError::InferenceError(format!("HsEmotionEngine::{context}: {e}"))
}

/// Résultat d'inférence de l'état émotionnel facial.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmotionInferenceResult {
    /// Émotion discrète dominante (ex: "Neutral", "Happiness", "Sadness").
    pub primary_emotion: String,
    /// Score de confiance de l'émotion dominante $\in [0.0, 1.0]$.
    pub confidence: f32,
    /// Probabilités normalisées (softmax) pour les 8 classes.
    pub class_probabilities: [f32; 8],
    /// Valence continue $\in [-1.0, 1.0]$.
    pub valence: f32,
    /// Arousal continu $\in [-1.0, 1.0]$.
    pub arousal: f32,
    /// Statut honnête (Real vs Simulated).
    pub status: ModelStatus,
}

/// Moteur d'inférence HSEmotion MTL.
pub struct HsEmotionEngine {
    _env: Option<Arc<ort::environment::Environment>>,
    session: Option<Mutex<Session>>,
    model_path: Option<PathBuf>,
    is_mock: bool,
}

impl HsEmotionEngine {
    /// Ouvre la session ONNX avec les poids réels.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(HakilleError::InferenceError(format!(
                "HsEmotionEngine::open: modèle introuvable '{}'",
                path.display()
            )));
        }
        let env = shared_ort_env()?;
        let session = Session::builder(&env)
            .map_err(|e| ort_err("builder", e))?
            .with_intra_threads(2)
            .map_err(|e| ort_err("threads", e))?
            .commit_from_file(path)
            .map_err(|e| ort_err("commit_from_file", e))?;

        Ok(Self {
            _env: Some(env),
            session: Some(Mutex::new(session)),
            model_path: Some(path.to_path_buf()),
            is_mock: false,
        })
    }

    /// Constructeur mock déterministe pour environnement sans GPU / CI.
    pub fn mock_synthetic() -> Self {
        Self {
            _env: None,
            session: None,
            model_path: None,
            is_mock: true,
        }
    }

    /// Tente d'ouvrir le modèle depuis le disque, ou bascule sur `mock_synthetic()`.
    pub fn open_or_mock(models_dir: Option<&Path>) -> Self {
        if let Some(path) = resolve_hsemotion_path(models_dir) {
            match Self::open(&path) {
                Ok(engine) => return engine,
                Err(err) => {
                    tracing::warn!("HsEmotionEngine: repli mock ({err})");
                }
            }
        }
        Self::mock_synthetic()
    }

    pub fn is_mock(&self) -> bool {
        self.is_mock || self.session.is_none()
    }

    pub fn model_path(&self) -> Option<&Path> {
        self.model_path.as_deref()
    }

    /// Analyse une trame faciale RGB découpée pour extraire l'émotion et les coordonnées affectives.
    pub fn infer_emotion(
        &self,
        rgb_crop: &[u8],
        width: usize,
        height: usize,
    ) -> Result<EmotionInferenceResult> {
        if rgb_crop.is_empty() || width == 0 || height == 0 {
            return Err(HakilleError::InferenceError(
                "HSEmotion: entrée vide".into(),
            ));
        }

        if let Some(session_mutex) = &self.session {
            let mut session = session_mutex
                .lock()
                .map_err(|e| ort_err("lock", format!("{e}")))?;

            let mut input_buf = vec![0.0f32; 3 * HSEMOTION_INPUT_SIZE * HSEMOTION_INPUT_SIZE];
            resize_bilinear_rgb_nchw(
                rgb_crop,
                width,
                height,
                &mut input_buf,
                HSEMOTION_INPUT_SIZE,
                HSEMOTION_INPUT_SIZE,
                NormalizationMode::ImageNet,
            );

            let tensor = Tensor::from_array((
                [1, 3, HSEMOTION_INPUT_SIZE, HSEMOTION_INPUT_SIZE],
                input_buf.into_boxed_slice(),
            ))
            .map_err(|e| ort_err("tensor", e))?;

            let outputs = session
                .run(ort::inputs![tensor])
                .map_err(|e| ort_err("run", e))?;

            // Tête 1 : 8 classes logits
            let (_shape_cls, logits) = outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| ort_err("extract_cls", e))?;

            let probs = softmax_8(&logits[0..8.min(logits.len())]);
            let mut best_idx = 0;
            let mut best_p = f32::NEG_INFINITY;
            for (i, &p) in probs.iter().enumerate() {
                if p > best_p {
                    best_p = p;
                    best_idx = i;
                }
            }

            // Tête 2 : Valence / Arousal continu (si présent dans le modèle MTL)
            let (valence, arousal) = if outputs.len() > 1 {
                let (_shape_va, va) = outputs[1]
                    .try_extract_tensor::<f32>()
                    .map_err(|e| ort_err("extract_va", e))?;
                if va.len() >= 2 {
                    (va[0].clamp(-1.0, 1.0), va[1].clamp(-1.0, 1.0))
                } else {
                    derive_circumplex_from_class(best_idx)
                }
            } else {
                derive_circumplex_from_class(best_idx)
            };

            Ok(EmotionInferenceResult {
                primary_emotion: EMOTION_CLASSES[best_idx].to_string(),
                confidence: best_p,
                class_probabilities: probs,
                valence,
                arousal,
                status: ModelStatus::Real,
            })
        } else {
            // Mode synthétique déterministe
            let (probs, valence, arousal) = mock_predict_emotion(rgb_crop, width, height);
            let mut best_idx = 0;
            let mut best_p = 0.0f32;
            for (i, &p) in probs.iter().enumerate() {
                if p > best_p {
                    best_p = p;
                    best_idx = i;
                }
            }

            Ok(EmotionInferenceResult {
                primary_emotion: EMOTION_CLASSES[best_idx].to_string(),
                confidence: best_p,
                class_probabilities: probs,
                valence,
                arousal,
                status: ModelStatus::Simulated,
            })
        }
    }
}

pub fn resolve_hsemotion_path(models_dir: Option<&Path>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(d) = models_dir {
        candidates.push(d.join(HSEMOTION_MODEL_FILENAME));
    }
    if let Ok(env_dir) = std::env::var("HAKILLE_MODELS_DIR")
        && !env_dir.trim().is_empty()
    {
        candidates.push(PathBuf::from(env_dir).join(HSEMOTION_MODEL_FILENAME));
    }
    candidates.push(PathBuf::from("./models").join(HSEMOTION_MODEL_FILENAME));
    candidates.push(PathBuf::from("../../models").join(HSEMOTION_MODEL_FILENAME));
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../models")
            .join(HSEMOTION_MODEL_FILENAME),
    );
    candidates.into_iter().find(|p| p.exists())
}

fn softmax_8(slice: &[f32]) -> [f32; 8] {
    let mut out = [0.0f32; 8];
    if slice.is_empty() {
        out[5] = 1.0; // Neutral par défaut
        return out;
    }
    let max_v = slice.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
    let mut sum = 0.0f32;
    for (i, &v) in slice.iter().take(8).enumerate() {
        let e = (v - max_v).exp();
        out[i] = e;
        sum += e;
    }
    if sum > 1e-6 {
        for v in &mut out {
            *v /= sum;
        }
    } else {
        out[5] = 1.0;
    }
    out
}

fn derive_circumplex_from_class(idx: usize) -> (f32, f32) {
    match idx {
        0 => (-0.6, 0.8),  // Anger
        1 => (-0.4, 0.3),  // Contempt
        2 => (-0.7, 0.2),  // Disgust
        3 => (-0.6, 0.7),  // Fear
        4 => (0.8, 0.6),   // Happiness
        5 => (0.0, 0.0),   // Neutral
        6 => (-0.7, -0.5), // Sadness
        7 => (0.3, 0.8),   // Surprise
        _ => (0.0, 0.0),
    }
}

fn mock_predict_emotion(rgb: &[u8], width: usize, height: usize) -> ([f32; 8], f32, f32) {
    let mut avg_r = 0u64;
    let mut avg_g = 0u64;
    let mut avg_b = 0u64;
    let total_pixels = width * height;

    for chunk in rgb.chunks_exact(3) {
        avg_r += chunk[0] as u64;
        avg_g += chunk[1] as u64;
        avg_b += chunk[2] as u64;
    }

    let r = avg_r as f32 / total_pixels.max(1) as f32;
    let g = avg_g as f32 / total_pixels.max(1) as f32;
    let b = avg_b as f32 / total_pixels.max(1) as f32;

    let mut probs = [0.03f32; 8];
    // Équilibre physiologique par défaut : Neutre / Joie
    if r > b && r > g {
        // Tons chauds : Happiness / Neutral
        probs[4] = 0.65; // Happiness
        probs[5] = 0.20; // Neutral
        probs[7] = 0.05; // Surprise
    } else {
        probs[5] = 0.75; // Neutral dominant
        probs[4] = 0.10;
        probs[6] = 0.05;
    }

    let (val, ar) = if probs[4] > probs[5] {
        (0.65, 0.40)
    } else {
        (0.05, 0.0)
    };

    (probs, val, ar)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::mock::synthetic_face_crop;

    #[test]
    fn test_hsemotion_mock_synthetic() {
        let engine = HsEmotionEngine::mock_synthetic();
        assert!(engine.is_mock());

        let crop = synthetic_face_crop(224, 224);
        let result = engine
            .infer_emotion(&crop, 224, 224)
            .expect("inférence réussie");

        assert_eq!(result.status, ModelStatus::Simulated);
        assert!(!result.primary_emotion.is_empty());
        assert!(result.confidence > 0.40);
        assert!(result.valence >= -1.0 && result.valence <= 1.0);
        assert!(result.arousal >= -1.0 && result.arousal <= 1.0);
    }
}
