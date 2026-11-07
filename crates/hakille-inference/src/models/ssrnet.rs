//! SSR-Net (`ssrnet_age.onnx`) — Estimation d'âge biologique continue (0-100), genre et protection des mineurs.
//!
//! Ce module implémente la régression souple par étapes (Soft Stagewise Regression) :
//! - Modèle ultra-léger (320 Ko, inférence CPU < 1 ms).
//! - Âge continu $\in [0.0, 100.0]$ avec intervalle de confiance.
//! - Catégorisation démographique en tranche [`AgeGroup`].
//! - Détection du genre biologique facial [`GenderKind`].
//! - Application stricte de la **Règle Constitutionnelle 7** : Si $P(\text{âge} < 18) \ge 0.85$,
//!   le drapeau `is_minor` est levé pour activer les restrictions de protection de l'enfance.

use crate::ModelStatus;
use crate::models::preprocess::{NormalizationMode, resize_bilinear_rgb_nchw};
use crate::ort_env::shared_ort_env;
use hakille_core::{HakilleError, Result};
use ort::{session::Session, value::Tensor};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const SSRNET_INPUT_SIZE: usize = 64;
pub const SSRNET_MODEL_FILENAME: &str = "ssrnet_age.onnx";

/// Seuil légal de minorité en années.
pub const MINOR_AGE_THRESHOLD: f32 = 18.0;
/// Seuil statistique pour activer la protection des mineurs (Règle Constitutionnelle 7).
pub const MINOR_CONFIDENCE_THRESHOLD: f32 = 0.85;

/// Tranche d'âge biologique catégorisée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgeGroup {
    /// Enfant (0 à 11 ans).
    Child,
    /// Adolescent (12 à 17 ans).
    Adolescent,
    /// Jeune adulte (18 à 35 ans).
    YoungAdult,
    /// Adulte mûr (36 à 60 ans).
    Adult,
    /// Senior (> 60 ans).
    Senior,
}

impl AgeGroup {
    pub fn from_age(age: f32) -> Self {
        if age < 12.0 {
            Self::Child
        } else if age < 18.0 {
            Self::Adolescent
        } else if age <= 35.0 {
            Self::YoungAdult
        } else if age <= 60.0 {
            Self::Adult
        } else {
            Self::Senior
        }
    }

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Child => "Child",
            Self::Adolescent => "Adolescent",
            Self::YoungAdult => "YoungAdult",
            Self::Adult => "Adult",
            Self::Senior => "Senior",
        }
    }
}

/// Estimation du genre biologique facial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GenderKind {
    Male,
    Female,
    Unknown,
}

impl GenderKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Male => "Male",
            Self::Female => "Female",
            Self::Unknown => "Unknown",
        }
    }
}

fn ort_err(context: &str, e: impl std::fmt::Display) -> HakilleError {
    HakilleError::InferenceError(format!("SsrNetEngine::{context}: {e}"))
}

/// Résultat complet de l'estimation d'âge et démographique.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgeEstimationResult {
    /// Âge continu estimé en années [0.0, 100.0].
    pub estimated_age: f32,
    /// Intervalle d'incertitude [âge_min, âge_max].
    pub age_bracket: (f32, f32),
    /// Groupe démographique d'âge.
    pub age_group: AgeGroup,
    /// Genre biologique estimé.
    pub gender: GenderKind,
    /// Indique si l'utilisateur est considéré mineur selon la Règle 7.
    pub is_minor: bool,
    /// Probabilité estimée que l'âge soit < 18 ans.
    pub minor_probability: f32,
    /// Confiance globale de la prédiction [0.0, 1.0].
    pub confidence: f32,
    /// Statut honnête de l'inférence.
    pub status: ModelStatus,
}

/// Moteur d'estimation d'âge SSR-Net.
pub struct SsrNetEngine {
    _env: Option<Arc<ort::environment::Environment>>,
    session: Option<Mutex<Session>>,
    model_path: Option<PathBuf>,
    is_mock: bool,
}

impl SsrNetEngine {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(HakilleError::InferenceError(format!(
                "SsrNetEngine::open: modèle introuvable '{}'",
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
            is_mock: false,
        })
    }

    pub fn mock_synthetic() -> Self {
        Self {
            _env: None,
            session: None,
            model_path: None,
            is_mock: true,
        }
    }

    pub fn open_or_mock(models_dir: Option<&Path>) -> Self {
        if let Some(path) = resolve_ssrnet_path(models_dir) {
            match Self::open(&path) {
                Ok(engine) => return engine,
                Err(err) => {
                    tracing::warn!("SsrNetEngine: repli mock ({err})");
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

    /// Estime l'âge et le genre à partir du crop du visage RGB.
    pub fn estimate_age(
        &self,
        rgb_crop: &[u8],
        width: usize,
        height: usize,
    ) -> Result<AgeEstimationResult> {
        if rgb_crop.is_empty() || width == 0 || height == 0 {
            return Err(HakilleError::InferenceError("SSRNet: entrée vide".into()));
        }

        if let Some(session_mutex) = &self.session {
            let mut session = session_mutex
                .lock()
                .map_err(|e| ort_err("lock", format!("{e}")))?;

            let mut input_buf = vec![0.0f32; 3 * SSRNET_INPUT_SIZE * SSRNET_INPUT_SIZE];
            resize_bilinear_rgb_nchw(
                rgb_crop,
                width,
                height,
                &mut input_buf,
                SSRNET_INPUT_SIZE,
                SSRNET_INPUT_SIZE,
                NormalizationMode::Centered,
            );

            let tensor = Tensor::from_array((
                [1, 3, SSRNET_INPUT_SIZE, SSRNET_INPUT_SIZE],
                input_buf.into_boxed_slice(),
            ))
            .map_err(|e| ort_err("tensor", e))?;

            let outputs = session
                .run(ort::inputs![tensor])
                .map_err(|e| ort_err("run", e))?;

            let (_shape, values) = outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| ort_err("extract", e))?;

            let raw_age = if !values.is_empty() { values[0] } else { 30.0 };
            let estimated_age = raw_age.clamp(1.0, 95.0);

            // Estimation de genre si une 2e tête ou valeur est présente
            let gender = if values.len() > 1 {
                if values[1] > 0.5 {
                    GenderKind::Female
                } else {
                    GenderKind::Male
                }
            } else {
                GenderKind::Unknown
            };

            let (is_minor, minor_prob) = calculate_minor_flag(estimated_age);

            Ok(AgeEstimationResult {
                estimated_age,
                age_bracket: (
                    (estimated_age - 3.2).max(0.0),
                    (estimated_age + 3.2).min(100.0),
                ),
                age_group: AgeGroup::from_age(estimated_age),
                gender,
                is_minor,
                minor_probability: minor_prob,
                confidence: 0.90,
                status: ModelStatus::Real,
            })
        } else {
            // Mode synthétique déterministe
            let (estimated_age, gender) = mock_estimate_age(rgb_crop, width, height);
            let (is_minor, minor_prob) = calculate_minor_flag(estimated_age);

            Ok(AgeEstimationResult {
                estimated_age,
                age_bracket: (
                    (estimated_age - 3.5).max(0.0),
                    (estimated_age + 3.5).min(100.0),
                ),
                age_group: AgeGroup::from_age(estimated_age),
                gender,
                is_minor,
                minor_probability: minor_prob,
                confidence: 0.88,
                status: ModelStatus::Simulated,
            })
        }
    }
}

pub fn resolve_ssrnet_path(models_dir: Option<&Path>) -> Option<PathBuf> {
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

/// Calcule le drapeau de minorité selon la loi normale $\mathcal{N}(\mu, \sigma^2)$ avec $\sigma \approx 3.2$ ans.
fn calculate_minor_flag(age: f32) -> (bool, f32) {
    let sigma = 3.2f32;
    let z = (MINOR_AGE_THRESHOLD - age) / sigma;
    // Approximation sigmoïdale de la fonction CDF normale
    let p_minor = 1.0 / (1.0 + (-1.702 * z).exp());
    let is_minor = p_minor >= MINOR_CONFIDENCE_THRESHOLD;
    (is_minor, p_minor)
}

fn mock_estimate_age(rgb: &[u8], width: usize, height: usize) -> (f32, GenderKind) {
    let mut avg_lum = 0u64;
    for chunk in rgb.chunks_exact(3) {
        let lum = (chunk[0] as u64 * 299 + chunk[1] as u64 * 587 + chunk[2] as u64 * 114) / 1000;
        avg_lum += lum;
    }
    let total = (width * height).max(1) as u64;
    let lum = (avg_lum / total) as f32;

    // Déterminisme stable pour adulte typique (~28 ans)
    let age = (25.0 + (lum % 15.0)).clamp(18.0, 65.0);
    (age, GenderKind::Male)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::mock::synthetic_face_crop;

    #[test]
    fn test_ssrnet_mock_synthetic() {
        let engine = SsrNetEngine::mock_synthetic();
        assert!(engine.is_mock());

        let crop = synthetic_face_crop(64, 64);
        let result = engine
            .estimate_age(&crop, 64, 64)
            .expect("estimation réussie");

        assert_eq!(result.status, ModelStatus::Simulated);
        assert!(result.estimated_age >= 18.0 && result.estimated_age <= 50.0);
        assert_eq!(result.is_minor, false);
        assert!(result.age_bracket.0 < result.estimated_age);
        assert!(result.age_bracket.1 > result.estimated_age);
    }

    #[test]
    fn test_minor_protection_rule_7() {
        // Enfant de 10 ans : minor_prob doit dépasser 0.85
        let (is_minor_child, prob_child) = calculate_minor_flag(10.0);
        assert!(is_minor_child, "Enfant de 10 ans doit déclencher is_minor");
        assert!(prob_child > 0.95);

        // Adulte de 30 ans : minor_prob doit être quasi-nulle
        let (is_minor_adult, prob_adult) = calculate_minor_flag(30.0);
        assert!(
            !is_minor_adult,
            "Adulte de 30 ans ne doit pas être classé mineur"
        );
        assert!(prob_adult < 0.05);
    }
}
