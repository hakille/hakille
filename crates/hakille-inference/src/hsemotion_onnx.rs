//! HSEmotion MTL (`enet_b0_8_va_mtl.onnx`) — Classification d'émotions 8 classes & régression Valence/Arousal.
//!
//! Ce module implémente l'inférence conjointe catégorielle et dimensionnelle :
//! - 8 classes d'émotions discrètes (Anger, Contempt, Disgust, Fear, Happiness, Neutral, Sadness, Surprise).
//! - 2 dimensions affectives continues du modèle de Russell :
//!   - Valence $\in [-1.0, 1.0]$ (axe plaisir/déplaisir).
//!   - Arousal $\in [-1.0, 1.0]$ (axe énergie/activation physiologique).
//!
//! En l'absence des poids ONNX (CI / sandbox), un mode simulé déterministe et honnête
//! [`ModelStatus::Simulated`] est fourni, tandis que le chargement du modèle réel produit [`ModelStatus::Real`].

use crate::ModelStatus;
use crate::ort_env::shared_ort_env;
use hakille_core::{HakilleError, Result};
use ort::{session::Session, value::Tensor};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const HSEMOTION_INPUT_SIZE: usize = 224;
pub const HSEMOTION_MODEL_FILENAME: &str = "enet_b0_8_va_mtl.onnx";

/// Les 8 classes discrètes d'émotions du standard AffectNet / HSEmotion.
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
    HakilleError::InferenceError(format!("HSEmotion::{context}: {e}"))
}

/// Résultat d'inférence de l'état émotionnel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmotionInferenceResult {
    /// Émotion discrète dominante (ex: "Neutral", "Happiness", "Sadness").
    pub primary_emotion: String,
    /// Score de confiance associé à l'émotion dominante $\in [0.0, 1.0]$.
    pub confidence: f32,
    /// Probabilités normalisées (softmax) pour les 8 classes.
    pub class_probabilities: [f32; 8],
    /// Valence continue $\in [-1.0, 1.0]$ (plaisir / déplaisir).
    pub valence: f32,
    /// Arousal continu $\in [-1.0, 1.0]$ (calme / excitation).
    pub arousal: f32,
    /// Statut honnête (Real vs Simulated).
    pub status: ModelStatus,
}

/// Résout le chemin du modèle HSEmotion.
pub fn resolve_hsemotion_model_path(models_dir: Option<&Path>) -> Option<PathBuf> {
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

/// Session HSEmotion ONNX / simulateur canonique.
pub struct HSEmotionSession {
    _env: Option<Arc<ort::environment::Environment>>,
    session: Option<Mutex<Session>>,
    model_path: Option<PathBuf>,
}

impl HSEmotionSession {
    /// Tente d'ouvrir la session ONNX avec les poids réels.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(HakilleError::InferenceError(format!(
                "HSEmotion::open: modèle introuvable '{}'",
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
        })
    }

    /// Ouvre la session ou bascule honnêtement en mode simulation.
    pub fn open_or_simulated(models_dir: Option<&Path>) -> Self {
        if let Some(path) = resolve_hsemotion_model_path(models_dir) {
            match Self::open(&path) {
                Ok(s) => return s,
                Err(e) => {
                    tracing::warn!("HSEmotion: échec ouverture ONNX ({e}), repli simulé");
                }
            }
        }
        Self {
            _env: None,
            session: None,
            model_path: None,
        }
    }

    pub fn model_path(&self) -> Option<&Path> {
        self.model_path.as_deref()
    }

    pub fn is_real(&self) -> bool {
        self.session.is_some()
    }

    pub fn status(&self) -> ModelStatus {
        if self.is_real() {
            ModelStatus::Real
        } else {
            ModelStatus::Simulated
        }
    }

    /// Exécute l'inférence émotionnelle sur un recadrage facial RGB.
    pub fn infer(
        &self,
        rgb_crop: &[u8],
        width: usize,
        height: usize,
    ) -> Result<EmotionInferenceResult> {
        if rgb_crop.is_empty() || width == 0 || height == 0 {
            return Err(HakilleError::InferenceError(
                "HSEmotion::infer: crop facial vide ou dimensions nulles".into(),
            ));
        }

        let Some(session_mutex) = &self.session else {
            return Err(HakilleError::InferenceError(
                "HSEmotion::infer: modèle ONNX enet_b0_8_va_mtl.onnx absent — classification émotionnelle simulée refusée (contrat d'honnêteté)".into(),
            ));
        };

        let expected_bytes = width
            .checked_mul(height)
            .and_then(|n| n.checked_mul(3))
            .ok_or_else(|| {
                HakilleError::InferenceError("HSEmotion::infer: dimensions débordantes".into())
            })?;
        if rgb_crop.len() < expected_bytes {
            return Err(HakilleError::InferenceError(format!(
                "HSEmotion::infer: tampon RGB trop court ({} < {expected_bytes})",
                rgb_crop.len()
            )));
        }

        // Normalisation ImageNet (mean: [0.485, 0.456, 0.406], std: [0.229, 0.224, 0.225])
        let tensor_data = preprocess_imagenet_chw(
            rgb_crop,
            width,
            height,
            HSEMOTION_INPUT_SIZE,
            HSEMOTION_INPUT_SIZE,
        );
        let input_tensor = Tensor::<f32>::from_array((
            [1usize, 3usize, HSEMOTION_INPUT_SIZE, HSEMOTION_INPUT_SIZE],
            tensor_data,
        ))
        .map_err(|e| ort_err("input_tensor", e))?;

        let mut session = session_mutex
            .lock()
            .map_err(|_| ort_err("lock", "mutex empoisonné"))?;
        let outputs = session
            .run(ort::inputs![input_tensor])
            .map_err(|e| ort_err("forward", e))?;

        // Le modèle MTL produit soit 2 tenseurs (logits 8 et va 2), soit un tenseur concaténé de taille 10.
        parse_mtl_outputs(&outputs)
    }
}

/// Normalisation ImageNet bilinéaire vers [3, target_h, target_w] en format NCHW.
fn preprocess_imagenet_chw(
    src: &[u8],
    src_w: usize,
    src_h: usize,
    dst_w: usize,
    dst_h: usize,
) -> Vec<f32> {
    const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
    const STD: [f32; 3] = [0.229, 0.224, 0.225];

    let mut out = vec![0.0f32; 3 * dst_w * dst_h];
    let plane_size = dst_w * dst_h;

    let x_ratio = (src_w as f32) / (dst_w as f32);
    let y_ratio = (src_h as f32) / (dst_h as f32);

    for dy in 0..dst_h {
        let sy = (dy as f32 * y_ratio).clamp(0.0, (src_h.saturating_sub(1)) as f32) as usize;
        for dx in 0..dst_w {
            let sx = (dx as f32 * x_ratio).clamp(0.0, (src_w.saturating_sub(1)) as f32) as usize;
            let src_idx = (sy * src_w + sx) * 3;
            if src_idx + 2 < src.len() {
                let r = ((src[src_idx] as f32) / 255.0 - MEAN[0]) / STD[0];
                let g = ((src[src_idx + 1] as f32) / 255.0 - MEAN[1]) / STD[1];
                let b = ((src[src_idx + 2] as f32) / 255.0 - MEAN[2]) / STD[2];

                let dst_idx = dy * dst_w + dx;
                out[dst_idx] = r;
                out[plane_size + dst_idx] = g;
                out[2 * plane_size + dst_idx] = b;
            }
        }
    }
    out
}

fn parse_mtl_outputs(outputs: &ort::session::SessionOutputs) -> Result<EmotionInferenceResult> {
    let mut logits = [0.0f32; 8];
    let mut valence = 0.0f32;
    let mut arousal = 0.0f32;

    if outputs.len() >= 2 {
        // Deux sorties distinctes : logits 8 classes et va 2 dims
        let (_s1, v1) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| ort_err("out0", e))?;
        let (_s2, v2) = outputs[1]
            .try_extract_tensor::<f32>()
            .map_err(|e| ort_err("out1", e))?;

        if v1.len() >= 8 {
            logits.copy_from_slice(&v1[..8]);
        }
        if v2.len() >= 2 {
            valence = v2[0].clamp(-1.0, 1.0);
            arousal = v2[1].clamp(-1.0, 1.0);
        }
    } else if outputs.len() == 1 {
        // Tenseur unique concaténé de 10 valeurs [8 logits, valence, arousal]
        let (_s, v) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| ort_err("out0", e))?;
        if v.len() >= 10 {
            logits.copy_from_slice(&v[..8]);
            valence = v[8].clamp(-1.0, 1.0);
            arousal = v[9].clamp(-1.0, 1.0);
        } else if v.len() >= 8 {
            logits.copy_from_slice(&v[..8]);
        }
    } else {
        return Err(HakilleError::InferenceError(
            "HSEmotion: aucune sortie".into(),
        ));
    }

    let mut probs = logits;
    normalize_softmax(&mut probs);

    let mut best_idx = 0;
    let mut best_val = -1.0;
    for (i, &p) in probs.iter().enumerate() {
        if p > best_val {
            best_val = p;
            best_idx = i;
        }
    }

    Ok(EmotionInferenceResult {
        primary_emotion: EMOTION_CLASSES[best_idx].to_string(),
        confidence: best_val.clamp(0.0, 1.0),
        class_probabilities: probs,
        valence,
        arousal,
        status: ModelStatus::Real,
    })
}

pub fn normalize_softmax(values: &mut [f32; 8]) {
    let max_val = values
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(f32::NEG_INFINITY, f32::max);
    if !max_val.is_finite() {
        *values = [0.0; 8];
        values[5] = 1.0;
        return;
    }
    let mut sum = 0.0;
    for v in values.iter_mut() {
        *v = if v.is_finite() {
            (*v - max_val).exp()
        } else {
            0.0
        };
        sum += *v;
    }
    if sum > 1e-6 {
        for v in values.iter_mut() {
            *v /= sum;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hsemotion_missing_model_refuses_simulated_output() {
        let session = HSEmotionSession::open_or_simulated(None);
        assert_eq!(session.status(), ModelStatus::Simulated);

        let dummy_crop = vec![120u8; 224 * 224 * 3];
        let err = session
            .infer(&dummy_crop, 224, 224)
            .expect_err("modèle absent : aucune émotion simulée autorisée");
        assert!(err.to_string().contains("absent"));
    }

    #[test]
    fn test_hsemotion_softmax_normalization_and_nan_safety() {
        let mut probs = [0.02, 0.01, 0.01, 0.02, 0.05, 0.82, 0.04, 0.03];
        normalize_softmax(&mut probs);
        let sum: f32 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-3, "softmax somme à 1.0 : {}", sum);

        let mut nan_logits = [f32::NAN; 8];
        normalize_softmax(&mut nan_logits);
        assert!(nan_logits.iter().all(|v| v.is_finite()));
    }
}
