//! RTMPose-t / YOLOv8-Pose (`rtmpose_tiny.onnx`) — Squelette 17 points COCO, Décodage SimCC & Posture 3D.
//!
//! Ce module implémente l'extraction du squelette humain selon le standard COCO :
//! (Nose, L/R Eye, L/R Ear, L/R Shoulder, L/R Elbow, L/R Wrist, L/R Hip, L/R Knee, L/R Ankle).
//!
//! ### Décodage SimCC (Simple Coordinate Classification) :
//! Les modèles RTMPose découplent la régression 2D en deux distributions 1D à haute résolution
//! (`simcc_x` et `simcc_y`), permettant une précision sub-pixellique sans matrice de chaleur 2D lourde.
//!
//! ### Intégration Spatiale Souveraine :
//! Les repères sont directement projetés dans [`hakille_spatial::BodySkeleton3D`] avec classification
//! automatique de la posture corporelle ([`hakille_spatial::PostureKind`]) et calcul de l'angle cervical.

use crate::ort_env::shared_ort_env;
use crate::models::preprocess::{resize_bilinear_rgb_nchw, NormalizationMode};
use crate::ModelStatus;
use hakille_core::{HakilleError, Result};
use hakille_spatial::{
    BodySkeleton3D, CocoJointId, Point3D, PostureKind, SkeletonJoint3D,
    COCO_KEYPOINTS_COUNT,
};
use ort::{session::Session, value::Tensor};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const POSE_INPUT_SIZE: usize = 256;
pub const POSE_MODEL_FILENAME: &str = "rtmpose_tiny.onnx";
/// Ratio d'expansion de grille SimCC par défaut (RTMPose utilise 2.0x).
pub const DEFAULT_SIMCC_SPLIT_RATIO: f32 = 2.0;

fn ort_err(context: &str, e: impl std::fmt::Display) -> HakilleError {
    HakilleError::InferenceError(format!("RtmPoseEngine::{context}: {e}"))
}

/// Résultat d'inférence de la pose corporelle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoseInferenceResult {
    /// Squelette 3D complet 17 points COCO de `hakille-spatial`.
    pub skeleton: BodySkeleton3D,
    /// Posture dominante classifiée.
    pub posture: PostureKind,
    /// Angle du cou par rapport à la verticale en degrés.
    pub neck_angle_deg: f32,
    /// Confiance moyenne des repères visibles.
    pub confidence: f32,
    /// Statut honnête de l'inférence.
    pub status: ModelStatus,
}

/// Moteur de pose corporelle RTMPose.
pub struct RtmPoseEngine {
    _env: Option<Arc<ort::environment::Environment>>,
    session: Option<Mutex<Session>>,
    model_path: Option<PathBuf>,
    is_mock: bool,
}

impl RtmPoseEngine {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(HakilleError::InferenceError(format!(
                "RtmPoseEngine::open: modèle introuvable '{}'",
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

    pub fn mock_synthetic() -> Self {
        Self {
            _env: None,
            session: None,
            model_path: None,
            is_mock: true,
        }
    }

    pub fn open_or_mock(models_dir: Option<&Path>) -> Self {
        if let Some(path) = resolve_rtmpose_path(models_dir) {
            match Self::open(&path) {
                Ok(engine) => return engine,
                Err(err) => {
                    tracing::warn!("RtmPoseEngine: repli mock ({err})");
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

    /// Extrait le squelette corporel et la posture depuis une image RGB brute.
    pub fn extract_pose(&self, rgb_bytes: &[u8], width: usize, height: usize) -> Result<PoseInferenceResult> {
        if rgb_bytes.is_empty() || width == 0 || height == 0 {
            return Err(HakilleError::InferenceError("RTMPose: entrée vide".into()));
        }

        if let Some(session_mutex) = &self.session {
            let mut session = session_mutex
                .lock()
                .map_err(|e| ort_err("lock", format!("{e}")))?;

            let mut input_buf = vec![0.0f32; 3 * POSE_INPUT_SIZE * POSE_INPUT_SIZE];
            resize_bilinear_rgb_nchw(
                rgb_bytes,
                width,
                height,
                &mut input_buf,
                POSE_INPUT_SIZE,
                POSE_INPUT_SIZE,
                NormalizationMode::ImageNet,
            );

            let tensor = Tensor::from_array((
                [1, 3, POSE_INPUT_SIZE, POSE_INPUT_SIZE],
                input_buf.into_boxed_slice(),
            ))
            .map_err(|e| ort_err("tensor", e))?;

            let outputs = session
                .run(ort::inputs![tensor])
                .map_err(|e| ort_err("run", e))?;

            let joints = if outputs.len() >= 2 {
                // Décodage SimCC : output[0] = simcc_x, output[1] = simcc_y
                let (_sx, simcc_x) = outputs[0]
                    .try_extract_tensor::<f32>()
                    .map_err(|e| ort_err("extract_simcc_x", e))?;
                let (_sy, simcc_y) = outputs[1]
                    .try_extract_tensor::<f32>()
                    .map_err(|e| ort_err("extract_simcc_y", e))?;
                decode_simcc_joints(simcc_x, simcc_y, DEFAULT_SIMCC_SPLIT_RATIO)?
            } else {
                // Décodage standard (ex: YOLOv8-Pose tenseur [1, 17, 3] ou [1, 56, ...])
                let (_s, values) = outputs[0]
                    .try_extract_tensor::<f32>()
                    .map_err(|e| ort_err("extract_pose", e))?;
                decode_generic_joints(values)?
            };

            let (posture, neck_angle, conf) = analyze_skeleton_posture(&joints);
            let skeleton = BodySkeleton3D::new(joints, posture);

            Ok(PoseInferenceResult {
                skeleton,
                posture,
                neck_angle_deg: neck_angle,
                confidence: conf,
                status: ModelStatus::Real,
            })
        } else {
            // Mode synthétique déterministe
            let joints = generate_canonical_coco_joints();
            let (posture, neck_angle, conf) = analyze_skeleton_posture(&joints);
            let skeleton = BodySkeleton3D::new(joints, posture);

            Ok(PoseInferenceResult {
                skeleton,
                posture,
                neck_angle_deg: neck_angle,
                confidence: conf,
                status: ModelStatus::Simulated,
            })
        }
    }
}

pub fn resolve_rtmpose_path(models_dir: Option<&Path>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(d) = models_dir {
        candidates.push(d.join(POSE_MODEL_FILENAME));
    }
    if let Ok(env_dir) = std::env::var("HAKILLE_MODELS_DIR")
        && !env_dir.trim().is_empty()
    {
        candidates.push(PathBuf::from(env_dir).join(POSE_MODEL_FILENAME));
    }
    candidates.push(PathBuf::from("./models").join(POSE_MODEL_FILENAME));
    candidates.push(PathBuf::from("../../models").join(POSE_MODEL_FILENAME));
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../models")
            .join(POSE_MODEL_FILENAME),
    );
    candidates.into_iter().find(|p| p.exists())
}

/// Décode les 17 articulations depuis les deux vecteurs 1D SimCC (`simcc_x`, `simcc_y`).
pub fn decode_simcc_joints(
    simcc_x: &[f32],
    simcc_y: &[f32],
    split_ratio: f32,
) -> Result<[SkeletonJoint3D; 17]> {
    let x_bins = simcc_x.len() / COCO_KEYPOINTS_COUNT;
    let y_bins = simcc_y.len() / COCO_KEYPOINTS_COUNT;

    if x_bins == 0 || y_bins == 0 {
        return Err(HakilleError::InferenceError("SimCC bins insuffisants".into()));
    }

    let mut joints = [SkeletonJoint3D::default(); 17];
    let sr = split_ratio.max(1.0);

    for i in 0..COCO_KEYPOINTS_COUNT {
        let x_slice = &simcc_x[i * x_bins..(i + 1) * x_bins];
        let y_slice = &simcc_y[i * y_bins..(i + 1) * y_bins];

        let (best_x_idx, max_val_x) = argmax_and_max(x_slice);
        let (best_y_idx, max_val_y) = argmax_and_max(y_slice);

        // Subpixel refinement parabolique
        let refined_x = refine_subpixel(x_slice, best_x_idx);
        let refined_y = refine_subpixel(y_slice, best_y_idx);

        let norm_x = (refined_x / (sr * POSE_INPUT_SIZE as f32)).clamp(0.0, 1.0);
        let norm_y = (refined_y / (sr * POSE_INPUT_SIZE as f32)).clamp(0.0, 1.0);

        let conf = ((max_val_x + max_val_y) * 0.5).clamp(0.0, 1.0);

        joints[i] = SkeletonJoint3D::new(i as u8, Point3D::new(norm_x, norm_y, 0.0), conf);
    }

    Ok(joints)
}

fn decode_generic_joints(values: &[f32]) -> Result<[SkeletonJoint3D; 17]> {
    let mut joints = [SkeletonJoint3D::default(); 17];
    let stride = if values.len() >= 17 * 3 { 3 } else { 2 };
    for i in 0..17 {
        let idx = i * stride;
        if idx + stride <= values.len() {
            let x = values[idx].clamp(0.0, 1.0);
            let y = values[idx + 1].clamp(0.0, 1.0);
            let conf = if stride == 3 { values[idx + 2].clamp(0.0, 1.0) } else { 0.8 };
            joints[i] = SkeletonJoint3D::new(i as u8, Point3D::new(x, y, 0.0), conf);
        }
    }
    Ok(joints)
}

fn argmax_and_max(slice: &[f32]) -> (usize, f32) {
    let mut best_i = 0;
    let mut best_v = f32::NEG_INFINITY;
    for (i, &v) in slice.iter().enumerate() {
        if v > best_v {
            best_v = v;
            best_i = i;
        }
    }
    (best_i, best_v)
}

fn refine_subpixel(slice: &[f32], idx: usize) -> f32 {
    if idx > 0 && idx + 1 < slice.len() {
        let s0 = slice[idx - 1];
        let s1 = slice[idx];
        let s2 = slice[idx + 1];
        let denom = 2.0 * (2.0 * s1 - s0 - s2);
        if denom.abs() > 1e-6 {
            let delta = (s2 - s0) / denom;
            return idx as f32 + delta.clamp(-0.5, 0.5);
        }
    }
    idx as f32
}

/// Analyse ergonomique posturale de l'utilisateur à partir des 17 articulations COCO.
pub fn analyze_skeleton_posture(joints: &[SkeletonJoint3D; 17]) -> (PostureKind, f32, f32) {
    let nose = joints[CocoJointId::Nose as usize];
    let left_ear = joints[CocoJointId::LeftEar as usize];
    let right_ear = joints[CocoJointId::RightEar as usize];
    let left_shoulder = joints[CocoJointId::LeftShoulder as usize];
    let right_shoulder = joints[CocoJointId::RightShoulder as usize];
    let left_hip = joints[CocoJointId::LeftHip as usize];
    let right_hip = joints[CocoJointId::RightHip as usize];

    let conf_sum: f32 = joints.iter().map(|j| j.confidence).sum();
    let mean_conf = conf_sum / 17.0;

    // Point médian des oreilles et des épaules
    let mid_ear_x = (left_ear.position.x + right_ear.position.x) * 0.5;
    let mid_ear_y = (left_ear.position.y + right_ear.position.y) * 0.5;

    let mid_shoulder_x = (left_shoulder.position.x + right_shoulder.position.x) * 0.5;
    let mid_shoulder_y = (left_shoulder.position.y + right_shoulder.position.y) * 0.5;

    let mid_hip_y = (left_hip.position.y + right_hip.position.y) * 0.5;

    // Angle sagittal du cou : projection avant (dx = mid_ear - mid_shoulder)
    let dx = (mid_ear_x - mid_shoulder_x).abs();
    let dy = (mid_shoulder_y - mid_ear_y).max(0.01);
    let neck_angle_deg = (dx / dy).atan().to_degrees();

    // Classification de posture
    let posture = if neck_angle_deg > 28.0 {
        PostureKind::Slouched
    } else if (nose.position.y - mid_shoulder_y).abs() < 0.10 {
        PostureKind::LeaningForward
    } else if (mid_hip_y - mid_shoulder_y) > 0.40 {
        PostureKind::Standing
    } else {
        PostureKind::Upright
    };

    (posture, neck_angle_deg, mean_conf)
}

/// Génère 17 articulations COCO d'une posture assise nominale droite.
pub fn generate_canonical_coco_joints() -> [SkeletonJoint3D; 17] {
    let mut j = [SkeletonJoint3D::default(); 17];
    let conf = 0.95;

    j[0] = SkeletonJoint3D::new(0, Point3D::new(0.50, 0.22, 0.65), conf); // Nose
    j[1] = SkeletonJoint3D::new(1, Point3D::new(0.48, 0.20, 0.65), conf); // L Eye
    j[2] = SkeletonJoint3D::new(2, Point3D::new(0.52, 0.20, 0.65), conf); // R Eye
    j[3] = SkeletonJoint3D::new(3, Point3D::new(0.44, 0.21, 0.64), conf); // L Ear
    j[4] = SkeletonJoint3D::new(4, Point3D::new(0.56, 0.21, 0.64), conf); // R Ear

    j[5] = SkeletonJoint3D::new(5, Point3D::new(0.38, 0.35, 0.63), conf); // L Shoulder
    j[6] = SkeletonJoint3D::new(6, Point3D::new(0.62, 0.35, 0.63), conf); // R Shoulder
    j[7] = SkeletonJoint3D::new(7, Point3D::new(0.34, 0.48, 0.60), conf); // L Elbow
    j[8] = SkeletonJoint3D::new(8, Point3D::new(0.66, 0.48, 0.60), conf); // R Elbow
    j[9] = SkeletonJoint3D::new(9, Point3D::new(0.38, 0.58, 0.55), conf); // L Wrist
    j[10] = SkeletonJoint3D::new(10, Point3D::new(0.62, 0.58, 0.55), conf); // R Wrist

    j[11] = SkeletonJoint3D::new(11, Point3D::new(0.42, 0.70, 0.65), conf); // L Hip
    j[12] = SkeletonJoint3D::new(12, Point3D::new(0.58, 0.70, 0.65), conf); // R Hip
    j[13] = SkeletonJoint3D::new(13, Point3D::new(0.40, 0.85, 0.60), conf); // L Knee
    j[14] = SkeletonJoint3D::new(14, Point3D::new(0.60, 0.85, 0.60), conf); // R Knee
    j[15] = SkeletonJoint3D::new(15, Point3D::new(0.40, 0.98, 0.60), conf); // L Ankle
    j[16] = SkeletonJoint3D::new(16, Point3D::new(0.60, 0.98, 0.60), conf); // R Ankle

    j
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::mock::synthetic_face_crop;

    #[test]
    fn test_rtmpose_mock_synthetic() {
        let engine = RtmPoseEngine::mock_synthetic();
        assert!(engine.is_mock());

        let crop = synthetic_face_crop(256, 256);
        let result = engine.extract_pose(&crop, 256, 256).expect("extraction pose réussie");

        assert_eq!(result.status, ModelStatus::Simulated);
        assert_eq!(result.skeleton.joints.len(), 17);
        assert!(result.confidence > 0.8);
        assert!(result.neck_angle_deg >= 0.0 && result.neck_angle_deg < 60.0);
    }

    #[test]
    fn test_decode_simcc_joints() {
        // 17 keypoints avec 512 bins chacun
        let bins = 512;
        let mut simcc_x = vec![0.0f32; 17 * bins];
        let mut simcc_y = vec![0.0f32; 17 * bins];

        // Pour chaque keypoint, placer un pic à bin 256 (milieu)
        for i in 0..17 {
            simcc_x[i * bins + 256] = 10.0;
            simcc_y[i * bins + 256] = 10.0;
        }

        let joints = decode_simcc_joints(&simcc_x, &simcc_y, 2.0).expect("décodage SimCC");
        assert_eq!(joints.len(), 17);

        // Position normalisée attendue : 256 / (2.0 * 256) = 0.5
        for j in &joints {
            assert!((j.position.x - 0.5).abs() < 1e-3);
            assert!((j.position.y - 0.5).abs() < 1e-3);
        }
    }
}
