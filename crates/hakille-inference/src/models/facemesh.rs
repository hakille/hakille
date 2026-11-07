//! Moteur MediaPipe FaceMesh Iris (478 repères 3D) — Pur Rust & ONNX Runtime.
//!
//! Ce module extrait 478 points 3D sub-pixels :
//! - 468 repères faciaux denses (contours yeux, bouche, sourcils, nez, ovale).
//! - 10 repères d'iris (468..=472 pour l'œil gauche, 473..=477 pour l'œil droit).
//!
//! Métriques calculées :
//! - Eye Aspect Ratio (EAR) gauche et droit (pour clignements et somnolence PERCLOS).
//! - Mouth Aspect Ratio (MAR) (pour bâillements).
//! - Distance inter-pupillaire (IPD) normalisée (pour estimation de taille et distance).
//! - 6 points PnP canoniques (repères sténopé pour pose de tête 6-DOF).
//!
//! En l'absence de GPU / poids ONNX, le constructeur [`FaceMeshEngine::mock_synthetic()`]
//! fournit une géométrie canonique déterministe avec statut honnête [`ModelStatus::Simulated`].

use crate::ModelStatus;
use crate::models::preprocess::{NormalizationMode, resize_bilinear_rgb_nchw};
use crate::ort_env::shared_ort_env;
use hakille_core::{HakilleError, Result};
use hakille_spatial::Point3D;
use ort::{session::Session, value::Tensor};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const FACEMESH_POINTS_COUNT: usize = 478;
pub const FACEMESH_INPUT_WIDTH: usize = 192;
pub const FACEMESH_INPUT_HEIGHT: usize = 192;
pub const FACEMESH_MODEL_FILENAME: &str = "facemesh_iris.onnx";

// Indices canoniques MediaPipe :
pub const LEFT_EYE_INDICES: [usize; 6] = [33, 160, 158, 133, 153, 144];
pub const RIGHT_EYE_INDICES: [usize; 6] = [362, 385, 387, 263, 373, 380];
pub const MOUTH_LEFT: usize = 61;
pub const MOUTH_RIGHT: usize = 291;
pub const MOUTH_TOP: usize = 13;
pub const MOUTH_BOTTOM: usize = 14;

pub const NOSE_TIP_INDEX: usize = 1;
pub const CHIN_INDEX: usize = 152;
pub const LEFT_IRIS_CENTER_INDEX: usize = 468;
pub const RIGHT_IRIS_CENTER_INDEX: usize = 473;

fn ort_err(context: &str, e: impl std::fmt::Display) -> HakilleError {
    HakilleError::InferenceError(format!("FaceMeshEngine::{context}: {e}"))
}

/// Les 6 points de repère canoniques pour la résolution PnP (Perspective-n-Point) 6-DOF.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CanonicalPnPPoints {
    pub nose_tip: Point3D,
    pub chin: Point3D,
    pub left_eye_corner: Point3D,
    pub right_eye_corner: Point3D,
    pub left_mouth_corner: Point3D,
    pub right_mouth_corner: Point3D,
}

/// Résultat complet de l'extraction des repères du maillage facial.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaceMeshOutput {
    /// 478 repères 3D normalisés [0.0, 1.0] ou métriques relatifs.
    pub landmarks: Vec<Point3D>,
    /// Centre de l'iris gauche.
    pub left_iris: Point3D,
    /// Centre de l'iris droit.
    pub right_iris: Point3D,
    /// Distance inter-pupillaire (IPD) normalisée.
    pub ipd_distance: f32,
    /// Eye Aspect Ratio (EAR) œil gauche.
    pub left_ear: f32,
    /// Eye Aspect Ratio (EAR) œil droit.
    pub right_ear: f32,
    /// Mouth Aspect Ratio (MAR).
    pub mar: f32,
    /// Points PnP canoniques pour orientation 6-DOF.
    pub pnp_points: CanonicalPnPPoints,
    /// Score de confiance globale.
    pub confidence: f32,
    /// Statut honnête de l'inférence.
    pub status: ModelStatus,
}

/// Moteur d'inférence MediaPipe FaceMesh 478 points.
pub struct FaceMeshEngine {
    _env: Option<Arc<ort::environment::Environment>>,
    session: Option<Mutex<Session>>,
    model_path: Option<PathBuf>,
    is_mock: bool,
}

impl FaceMeshEngine {
    /// Ouvre une session ONNX réelle à partir du fichier de modèle.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(HakilleError::InferenceError(format!(
                "FaceMeshEngine::open: modèle introuvable '{}'",
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

    /// Constructeur de repli déterministe / synthétique pour environnement CI sans GPU.
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
        if let Some(path) = resolve_facemesh_path(models_dir) {
            match Self::open(&path) {
                Ok(engine) => return engine,
                Err(err) => {
                    tracing::warn!("FaceMeshEngine: repli mock ({err})");
                }
            }
        }
        Self::mock_synthetic()
    }

    /// Retourne `true` si le moteur tourne en simulation synthétique.
    pub fn is_mock(&self) -> bool {
        self.is_mock || self.session.is_none()
    }

    /// Chemin du fichier de modèle chargé le cas échéant.
    pub fn model_path(&self) -> Option<&Path> {
        self.model_path.as_deref()
    }

    /// Extrait les 478 landmarks à partir d'une image RGB brute `[width * height * 3]`.
    pub fn detect_landmarks(
        &self,
        rgb_bytes: &[u8],
        width: usize,
        height: usize,
    ) -> Result<FaceMeshOutput> {
        if rgb_bytes.is_empty() || width == 0 || height == 0 {
            return Err(HakilleError::InferenceError(
                "FaceMesh: trame d'entrée vide".into(),
            ));
        }

        if let Some(session_mutex) = &self.session {
            let mut session = session_mutex
                .lock()
                .map_err(|e| ort_err("lock", format!("{e}")))?;

            let mut input_buf = vec![0.0f32; 3 * FACEMESH_INPUT_WIDTH * FACEMESH_INPUT_HEIGHT];
            resize_bilinear_rgb_nchw(
                rgb_bytes,
                width,
                height,
                &mut input_buf,
                FACEMESH_INPUT_WIDTH,
                FACEMESH_INPUT_HEIGHT,
                NormalizationMode::Centered,
            );

            let tensor = Tensor::from_array((
                [1, 3, FACEMESH_INPUT_HEIGHT, FACEMESH_INPUT_WIDTH],
                input_buf.into_boxed_slice(),
            ))
            .map_err(|e| ort_err("tensor", e))?;

            let outputs = session
                .run(ort::inputs![tensor])
                .map_err(|e| ort_err("run", e))?;

            let output_tensor = &outputs[0];
            let (_shape, values) = output_tensor
                .try_extract_tensor::<f32>()
                .map_err(|e| ort_err("extract", e))?;

            let landmarks = parse_landmarks_dynamic(values)?;
            compute_facemesh_metrics(landmarks, ModelStatus::Real)
        } else {
            // Mode synthétique déterministe
            let landmarks = generate_canonical_landmarks();
            compute_facemesh_metrics(landmarks, ModelStatus::Simulated)
        }
    }
}

/// Résout le chemin du modèle `facemesh_iris.onnx`.
pub fn resolve_facemesh_path(models_dir: Option<&Path>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(d) = models_dir {
        candidates.push(d.join(FACEMESH_MODEL_FILENAME));
    }
    if let Ok(env_dir) = std::env::var("HAKILLE_MODELS_DIR")
        && !env_dir.trim().is_empty()
    {
        candidates.push(PathBuf::from(env_dir).join(FACEMESH_MODEL_FILENAME));
    }
    candidates.push(PathBuf::from("./models").join(FACEMESH_MODEL_FILENAME));
    candidates.push(PathBuf::from("../../models").join(FACEMESH_MODEL_FILENAME));
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../models")
            .join(FACEMESH_MODEL_FILENAME),
    );
    candidates.into_iter().find(|p| p.exists())
}

/// Parse la sortie brute ONNX pour extraire les repères Point3D.
fn parse_landmarks_dynamic(values: &[f32]) -> Result<Vec<Point3D>> {
    let total_elements = values.len();
    if total_elements < FACEMESH_POINTS_COUNT * 3 {
        return Err(HakilleError::InferenceError(format!(
            "FaceMesh: tenseur de sortie trop court ({} éléments)",
            total_elements
        )));
    }

    let mut points = Vec::with_capacity(FACEMESH_POINTS_COUNT);
    for i in 0..FACEMESH_POINTS_COUNT {
        let idx = i * 3;
        points.push(Point3D::new(
            values[idx] / FACEMESH_INPUT_WIDTH as f32,
            values[idx + 1] / FACEMESH_INPUT_HEIGHT as f32,
            values[idx + 2] / FACEMESH_INPUT_WIDTH as f32,
        ));
    }
    Ok(points)
}

/// Calcule les métriques biométriques (EAR, MAR, IPD, PnP) à partir des repères 3D.
pub fn compute_facemesh_metrics(
    landmarks: Vec<Point3D>,
    status: ModelStatus,
) -> Result<FaceMeshOutput> {
    if landmarks.len() < FACEMESH_POINTS_COUNT {
        return Err(HakilleError::InferenceError(format!(
            "landmarks count insuffisant : {}",
            landmarks.len()
        )));
    }

    // Calcul EAR gauche
    let left_ear = calculate_ear(&landmarks, &LEFT_EYE_INDICES);
    // Calcul EAR droit
    let right_ear = calculate_ear(&landmarks, &RIGHT_EYE_INDICES);

    // Calcul MAR
    let mar = calculate_mar(&landmarks, MOUTH_LEFT, MOUTH_RIGHT, MOUTH_TOP, MOUTH_BOTTOM);

    // Iris & IPD
    let left_iris = landmarks[LEFT_IRIS_CENTER_INDEX];
    let right_iris = landmarks[RIGHT_IRIS_CENTER_INDEX];
    let ipd_distance = left_iris.distance_to(&right_iris);

    // Points PnP canoniques
    let pnp_points = CanonicalPnPPoints {
        nose_tip: landmarks[NOSE_TIP_INDEX],
        chin: landmarks[CHIN_INDEX],
        left_eye_corner: landmarks[LEFT_EYE_INDICES[0]],
        right_eye_corner: landmarks[RIGHT_EYE_INDICES[0]],
        left_mouth_corner: landmarks[MOUTH_LEFT],
        right_mouth_corner: landmarks[MOUTH_RIGHT],
    };

    Ok(FaceMeshOutput {
        landmarks,
        left_iris,
        right_iris,
        ipd_distance,
        left_ear,
        right_ear,
        mar,
        pnp_points,
        confidence: 0.95,
        status,
    })
}

/// Eye Aspect Ratio : EAR = (||p2 - p6|| + ||p3 - p5||) / (2 * ||p1 - p4||)
fn calculate_ear(pts: &[Point3D], idx: &[usize; 6]) -> f32 {
    let p1 = pts[idx[0]];
    let p2 = pts[idx[1]];
    let p3 = pts[idx[2]];
    let p4 = pts[idx[3]];
    let p5 = pts[idx[4]];
    let p6 = pts[idx[5]];

    let v1 = p2.distance_to(&p6);
    let v2 = p3.distance_to(&p5);
    let h = p1.distance_to(&p4);

    if h > 1e-5 { (v1 + v2) / (2.0 * h) } else { 0.0 }
}

/// Mouth Aspect Ratio : MAR = ||top - bottom|| / ||left - right||
fn calculate_mar(pts: &[Point3D], left: usize, right: usize, top: usize, bottom: usize) -> f32 {
    let p_left = pts[left];
    let p_right = pts[right];
    let p_top = pts[top];
    let p_bottom = pts[bottom];

    let v = p_top.distance_to(&p_bottom);
    let h = p_left.distance_to(&p_right);

    if h > 1e-5 { v / h } else { 0.0 }
}

/// Génère 478 repères canoniques d'un visage humain neutre typique.
pub fn generate_canonical_landmarks() -> Vec<Point3D> {
    let mut pts = Vec::with_capacity(FACEMESH_POINTS_COUNT);
    for i in 0..FACEMESH_POINTS_COUNT {
        let norm_i = i as f32 / FACEMESH_POINTS_COUNT as f32;
        let angle = norm_i * std::f32::consts::TAU;
        let x = 0.5 + 0.25 * angle.cos() * (0.8 + 0.2 * norm_i);
        let y = 0.5 + 0.35 * angle.sin() * (0.8 + 0.2 * norm_i);
        let z = 0.05 * (angle * 2.0).cos();
        pts.push(Point3D::new(x, y, z));
    }

    // Assignation explicite des points critiques pour que les métriques soient physiologiquement plausibles
    // Oeil gauche
    pts[33] = Point3D::new(0.30, 0.38, 0.02);
    pts[160] = Point3D::new(0.35, 0.36, 0.03);
    pts[158] = Point3D::new(0.40, 0.36, 0.03);
    pts[133] = Point3D::new(0.44, 0.38, 0.02);
    pts[153] = Point3D::new(0.40, 0.40, 0.02);
    pts[144] = Point3D::new(0.35, 0.40, 0.02);

    // Oeil droit
    pts[362] = Point3D::new(0.70, 0.38, 0.02);
    pts[385] = Point3D::new(0.65, 0.36, 0.03);
    pts[387] = Point3D::new(0.60, 0.36, 0.03);
    pts[263] = Point3D::new(0.56, 0.38, 0.02);
    pts[373] = Point3D::new(0.60, 0.40, 0.02);
    pts[380] = Point3D::new(0.65, 0.40, 0.02);

    // Iris
    pts[468] = Point3D::new(0.37, 0.38, 0.03);
    pts[473] = Point3D::new(0.63, 0.38, 0.03);

    // Nez & Menton
    pts[1] = Point3D::new(0.50, 0.52, 0.08); // Bout du nez
    pts[152] = Point3D::new(0.50, 0.85, 0.01); // Menton

    // Bouche
    pts[61] = Point3D::new(0.42, 0.68, 0.03); // Coin gauche
    pts[291] = Point3D::new(0.58, 0.68, 0.03); // Coin droit
    pts[13] = Point3D::new(0.50, 0.66, 0.04); // Lèvre sup
    pts[14] = Point3D::new(0.50, 0.70, 0.03); // Lèvre inf

    pts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::mock::synthetic_face_crop;

    #[test]
    fn test_facemesh_mock_synthetic() {
        let engine = FaceMeshEngine::mock_synthetic();
        assert!(engine.is_mock());

        let crop = synthetic_face_crop(192, 192);
        let output = engine
            .detect_landmarks(&crop, 192, 192)
            .expect("extraction réussie");

        assert_eq!(output.landmarks.len(), 478);
        assert_eq!(output.status, ModelStatus::Simulated);
        assert!(
            output.left_ear > 0.15 && output.left_ear < 0.45,
            "EAR gauche: {}",
            output.left_ear
        );
        assert!(
            output.right_ear > 0.15 && output.right_ear < 0.45,
            "EAR droit: {}",
            output.right_ear
        );
        assert!(
            output.mar > 0.10 && output.mar < 0.40,
            "MAR: {}",
            output.mar
        );
        assert!(
            output.ipd_distance > 0.20 && output.ipd_distance < 0.35,
            "IPD: {}",
            output.ipd_distance
        );
    }
}
