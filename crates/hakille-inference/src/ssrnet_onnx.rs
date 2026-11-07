//! SSR-Net (`ssrnet_age.onnx`) — Estimation d'âge biologique ultralégère (< 1 ms CPU) & Protection des Mineurs.
//!
//! Ce module implémente la régression souple par étapes (Soft Stagewise Regression) :
//! - Entrée : Recadrage facial RGB $64 \times 64$, normalisé $[-1.0, 1.0]$.
//! - Sortie : Âge continu estimé en années $[0.0, 100.0]$ avec intervalle de confiance.
//! - Règle Constitutionnelle 7 : Si l'âge estimé est inférieur à 18 ans avec une marge de confiance
//!   statistique ($P(\text{âge} < 18) \ge 0.85$), le drapeau `is_minor` est activé pour verrouiller
//!   immédiatement les privilèges sensibles conformément à la Constitution de Hakille.

use crate::ort_env::shared_ort_env;
use crate::ModelStatus;
use hakille_core::{HakilleError, Result};
use ort::{session::Session, value::Tensor};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const SSRNET_INPUT_SIZE: usize = 64;
pub const SSRNET_MODEL_FILENAME: &str = "ssrnet_age.onnx";

/// Seuil légal de minorité en années.
pub const MINOR_AGE_THRESHOLD: f32 = 18.0;
/// Seuil de probabilité pour déclencher la protection constitutionnelle des mineurs.
pub const MINOR_CONFIDENCE_THRESHOLD: f32 = 0.85;

fn ort_err(context: &str, e: impl std::fmt::Display) -> HakilleError {
    HakilleError::InferenceError(format!("SSRNetAge::{context}: {e}"))
}

/// Résultat d'estimation d'âge démographique.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgeEstimationResult {
    /// Âge biologique estimé en années (ex: 28.4 ans).
    pub estimated_age: f32,
    /// Fourchette d'incertitude [âge_min, âge_max] à 1 écart-type (ex: [25.4, 31.4]).
    pub age_bracket: (f32, f32),
    /// Indique si l'utilisateur est considéré mineur (< 18 ans) selon la Règle 7.
    pub is_minor: bool,
    /// Probabilité estimée que l'utilisateur soit mineur $P(\text{âge} < 18)$.
    pub minor_probability: f32,
    /// Score de confiance global dans l'estimation $[0.0, 1.0]$.
    pub confidence: f32,
    /// Statut honnête (Real vs Simulated).
    pub status: ModelStatus,
}

/// Résout le chemin du modèle SSR-Net.
pub fn resolve_ssrnet_model_path(models_dir: Option<&Path>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(d) = models_dir {
        candidates.push(d.join(SSRNET_MODEL_FILENAME));
    }
    if let Ok(env_dir) = std::env::var("HAKILLE_MODELS_DIR")
        && !env_dir.trim().is_empty()
    {
        candidates.push(PathBuf::from(env_dir).join(SSRNET_MODEL_FILENAME));
    }
    candidates.push(PathBuf::from("./models").join(SSRNET_MODEL_FILENAME));
    candidates.push(PathBuf::from("../../models").join(SSRNET_MODEL_FILENAME));
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../models")
            .join(SSRNET_MODEL_FILENAME),
    );
    candidates.into_iter().find(|p| p.exists())
}

/// Session SSR-Net ONNX / simulateur canonique.
pub struct SSRNetSession {
    _env: Option<Arc<ort::environment::Environment>>,
    session: Option<Mutex<Session>>,
    model_path: Option<PathBuf>,
}

impl SSRNetSession {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(HakilleError::InferenceError(format!(
                "SSRNet::open: modèle introuvable '{}'",
                path.display()
            )));
        }
        let env = shared_ort_env()?;
        let session = Session::builder(&env)
            .map_err(|e| ort_err("builder", e))?
            .with_intra_threads(1)
            .map_err(|e| ort_err("threads", e))?
            .commit_from_file(path)
            .map_err(|e| ort_err("commit_from_file", e))?;

        Ok(Self {
            _env: Some(env),
            session: Some(Mutex::new(session)),
            model_path: Some(path.to_path_buf()),
        })
    }

    pub fn open_or_simulated(models_dir: Option<&Path>) -> Self {
        if let Some(path) = resolve_ssrnet_model_path(models_dir) {
            match Self::open(&path) {
                Ok(s) => return s,
                Err(e) => {
                    tracing::warn!("SSRNet: échec ouverture ONNX ({e}), repli simulé");
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

    /// Estime l'âge à partir d'un recadrage de visage RGB.
    pub fn infer(&self, rgb_crop: &[u8], width: usize, height: usize) -> Result<AgeEstimationResult> {
        if rgb_crop.is_empty() || width == 0 || height == 0 {
            return Err(HakilleError::InferenceError(
                "SSRNet::infer: crop facial vide ou dimensions nulles".into(),
            ));
        }

        if let Some(session_mutex) = &self.session {
            let tensor_data = preprocess_image_normalized(rgb_crop, width, height, SSRNET_INPUT_SIZE, SSRNET_INPUT_SIZE);
            let input_tensor = Tensor::<f32>::from_array(([1usize, 3usize, SSRNET_INPUT_SIZE, SSRNET_INPUT_SIZE], tensor_data))
                .map_err(|e| ort_err("input_tensor", e))?;

            let mut session = session_mutex.lock().map_err(|_| ort_err("lock", "mutex empoisonné"))?;
            let outputs = session.run(ort::inputs![input_tensor]).map_err(|e| ort_err("forward", e))?;

            let (_shape, values) = outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| ort_err("extract_output", e))?;

            let raw_age = if !values.is_empty() {
                values[0].clamp(0.0, 100.0)
            } else {
                return Err(HakilleError::InferenceError("SSRNet: sortie vide".into()));
            };

            return Ok(build_age_result(raw_age, 0.90, ModelStatus::Real));
        }

        // Mode simulation canonique déterministe
        // Calibré sur un profil adulte type (28.5 ans) pour les tests nominaux
        let simulated_age = 28.5f32;
        Ok(build_age_result(simulated_age, 0.92, ModelStatus::Simulated))
    }
}

/// Redimensionne bilinéairement et normalise dans [-1.0, 1.0].
fn preprocess_image_normalized(
    src: &[u8],
    src_w: usize,
    src_h: usize,
    dst_w: usize,
    dst_h: usize,
) -> Vec<f32> {
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
                // (val / 127.5) - 1.0 donne [-1.0, 1.0]
                let r = (src[src_idx] as f32) / 127.5 - 1.0;
                let g = (src[src_idx + 1] as f32) / 127.5 - 1.0;
                let b = (src[src_idx + 2] as f32) / 127.5 - 1.0;

                let dst_idx = dy * dst_w + dx;
                out[dst_idx] = r;
                out[plane_size + dst_idx] = g;
                out[2 * plane_size + dst_idx] = b;
            }
        }
    }
    out
}

/// Construit le résultat avec application de la Règle Constitutionnelle 7.
pub fn build_age_result(estimated_age: f32, confidence: f32, status: ModelStatus) -> AgeEstimationResult {
    // Ecart-type d'incertitude typique SSR-Net : ~3.0 ans
    let sigma = 3.16f32;
    let min_age = (estimated_age - sigma).max(0.0);
    let max_age = estimated_age + sigma;

    // Calcul de la probabilité CDF normale P(Age < 18)
    // z = (18 - estimated_age) / sigma
    let z = (MINOR_AGE_THRESHOLD - estimated_age) / sigma;
    let minor_probability = approximate_normal_cdf(z);

    // Règle 7 : Si P(âge < 18) > 0.85 ou l'âge moyen < 18, protection obligatoire
    let is_minor = minor_probability >= MINOR_CONFIDENCE_THRESHOLD || estimated_age < MINOR_AGE_THRESHOLD;

    AgeEstimationResult {
        estimated_age,
        age_bracket: (min_age, max_age),
        is_minor,
        minor_probability,
        confidence,
        status,
    }
}

/// Approximation de la fonction de répartition de la loi normale centrée réduite (CDF Gaussienne).
fn approximate_normal_cdf(z: f32) -> f32 {
    // Approximation logistique rapide : CDF(z) approx 1 / (1 + exp(-1.702 * z))
    let exp_term = (-1.702 * z).exp();
    1.0 / (1.0 + exp_term)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssrnet_simulated_nominal() {
        let session = SSRNetSession::open_or_simulated(None);
        assert_eq!(session.status(), ModelStatus::Simulated);

        let dummy_crop = vec![128u8; 64 * 64 * 3];
        let res = session.infer(&dummy_crop, 64, 64).expect("inférence réussie");

        assert_eq!(res.status, ModelStatus::Simulated);
        assert!((res.estimated_age - 28.5).abs() < 1e-3);
        assert!(!res.is_minor, "un adulte de 28.5 ans n'est pas un mineur");
        assert!(res.minor_probability < 0.05);
        assert!(res.age_bracket.0 < res.estimated_age);
        assert!(res.age_bracket.1 > res.estimated_age);
    }

    #[test]
    fn test_minor_protection_enforcement_rule_7() {
        // Test d'un enfant de 12 ans -> doit activer is_minor
        let child_res = build_age_result(12.0, 0.95, ModelStatus::Simulated);
        assert!(child_res.is_minor, "Enfant de 12 ans doit être protégé (Règle 7)");
        assert!(child_res.minor_probability > 0.95);

        // Test d'un jeune de 17.5 ans -> incertitude penche vers mineur
        let borderline_res = build_age_result(17.5, 0.90, ModelStatus::Simulated);
        assert!(borderline_res.is_minor, "17.5 ans < 18 ans active la protection");

        // Test d'un adulte de 35 ans -> adulte confirmé
        let adult_res = build_age_result(35.0, 0.95, ModelStatus::Simulated);
        assert!(!adult_res.is_minor);
        assert!(adult_res.minor_probability < 1e-3);
    }
}
