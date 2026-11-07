//! RTMPose-t / YOLOv8-Pose (`rtmpose_tiny.onnx`) — Extraction du squelette 17 points COCO & Posture.
//!
//! Ce module extrait les 17 repères anatomiques standards COCO :
//! (Nez, Yeux G/D, Oreilles G/D, Épaules G/D, Coudes G/D, Poignets G/D, Hanches G/D, Genoux G/D, Chevilles G/D).
//!
//! Les points détectés sont convertis directement dans le type souverain [`hakille_spatial::BodySkeleton3D`].
//! Les métriques posturales (angle de cyphose cervicale / Tech-Neck, affaissement rachidien)
//! sont automatiquement calculées sans allocation superflue.

use crate::ort_env::shared_ort_env;
use crate::ModelStatus;
use hakille_core::{HakilleError, Result};
use hakille_spatial::{BodySkeleton3D, CocoJointId, Point3D, PostureKind, SkeletonJoint3D, Vector3D};
use ort::{session::Session, value::Tensor};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const POSE_INPUT_SIZE: usize = 256;
pub const POSE_MODEL_FILENAME: &str = "rtmpose_tiny.onnx";

fn ort_err(context: &str, e: impl std::fmt::Display) -> HakilleError {
    HakilleError::InferenceError(format!("PoseExtractor::{context}: {e}"))
}

/// Résultat d'inférence de la pose corporelle et de la posture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoseInferenceResult {
    /// Squelette corporel 3D 17 points COCO.
    pub skeleton: BodySkeleton3D,
    /// Classification discrète de la posture.
    pub posture: PostureKind,
    /// Angle du cou par rapport à la verticale en degrés.
    pub neck_angle_deg: f32,
    /// Indice de confiance moyen des articulations visibles.
    pub confidence: f32,
    /// Statut honnête de l'inférence.
    pub status: ModelStatus,
}

/// Résout le chemin du modèle de pose.
pub fn resolve_pose_model_path(models_dir: Option<&Path>) -> Option<PathBuf> {
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

/// Session ONNX / simulateur de pose corporelle.
pub struct PoseSession {
    _env: Option<Arc<ort::environment::Environment>>,
    session: Option<Mutex<Session>>,
    model_path: Option<PathBuf>,
}

impl PoseSession {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(HakilleError::InferenceError(format!(
                "PoseSession::open: modèle introuvable '{}'",
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

    pub fn open_or_simulated(models_dir: Option<&Path>) -> Self {
        if let Some(path) = resolve_pose_model_path(models_dir) {
            match Self::open(&path) {
                Ok(s) => return s,
                Err(e) => {
                    tracing::warn!("PoseSession: échec ouverture ONNX ({e}), repli simulé");
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

    /// Extrait les 17 articulations à partir d'une image RGB.
    pub fn infer(&self, rgb_data: &[u8], width: usize, height: usize) -> Result<PoseInferenceResult> {
        if rgb_data.is_empty() || width == 0 || height == 0 {
            return Err(HakilleError::InferenceError(
                "PoseSession::infer: image vide ou dimensions nulles".into(),
            ));
        }

        if let Some(session_mutex) = &self.session {
            let tensor_data = preprocess_image_to_chw(rgb_data, width, height, POSE_INPUT_SIZE, POSE_INPUT_SIZE);
            let input_tensor = Tensor::<f32>::from_array(([1usize, 3usize, POSE_INPUT_SIZE, POSE_INPUT_SIZE], tensor_data))
                .map_err(|e| ort_err("input_tensor", e))?;

            let mut session = session_mutex.lock().map_err(|_| ort_err("lock", "mutex empoisonné"))?;
            let outputs = session.run(ort::inputs![input_tensor]).map_err(|e| ort_err("forward", e))?;

            let (_shape, values) = outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| ort_err("extract_output", e))?;

            let skeleton = parse_pose_outputs(values, width as f32, height as f32)?;
            let neck_angle_deg = calculate_neck_angle(&skeleton);
            let posture = skeleton.posture;
            let conf = average_confidence(&skeleton);

            return Ok(PoseInferenceResult {
                skeleton,
                posture,
                neck_angle_deg,
                confidence: conf,
                status: ModelStatus::Real,
            });
        }

        // Mode simulation canonique déterministe
        let skeleton = generate_canonical_seated_skeleton(width as f32, height as f32);
        let neck_angle_deg = calculate_neck_angle(&skeleton);
        let posture = skeleton.posture;
        let conf = average_confidence(&skeleton);

        Ok(PoseInferenceResult {
            skeleton,
            posture,
            neck_angle_deg,
            confidence: conf,
            status: ModelStatus::Simulated,
        })
    }
}

fn preprocess_image_to_chw(
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
                let r = (src[src_idx] as f32) / 255.0;
                let g = (src[src_idx + 1] as f32) / 255.0;
                let b = (src[src_idx + 2] as f32) / 255.0;

                let dst_idx = dy * dst_w + dx;
                out[dst_idx] = r;
                out[plane_size + dst_idx] = g;
                out[2 * plane_size + dst_idx] = b;
            }
        }
    }
    out
}

fn parse_pose_outputs(values: &[f32], width: f32, height: f32) -> Result<BodySkeleton3D> {
    // RTMPose / YOLOv8-pose produit soit [1, 17, 3] (x, y, conf), soit [1, 17, 2] + confidences
    let mut joints = [SkeletonJoint3D::default(); 17];
    if values.len() >= 17 * 3 {
        for i in 0..17 {
            let x = values[i * 3] / POSE_INPUT_SIZE as f32 * width;
            let y = values[i * 3 + 1] / POSE_INPUT_SIZE as f32 * height;
            let conf = values[i * 3 + 2].clamp(0.0, 1.0);
            joints[i] = SkeletonJoint3D::new(i as u8, Point3D::new(x, y, 0.0), conf);
        }
    } else if values.len() >= 17 * 2 {
        for i in 0..17 {
            let x = values[i * 2] / POSE_INPUT_SIZE as f32 * width;
            let y = values[i * 2 + 1] / POSE_INPUT_SIZE as f32 * height;
            joints[i] = SkeletonJoint3D::new(i as u8, Point3D::new(x, y, 0.0), 0.85);
        }
    } else {
        return Err(HakilleError::InferenceError("Pose: tenseur trop court".into()));
    }

    let posture = classify_skeleton_posture(&joints);
    Ok(BodySkeleton3D::new(joints, posture))
}

/// Calcule l'angle du cou (degrés) entre l'axe vertical et le vecteur allant du milieu des épaules aux oreilles.
pub fn calculate_neck_angle(skeleton: &BodySkeleton3D) -> f32 {
    let left_ear = skeleton.joint(CocoJointId::LeftEar);
    let right_ear = skeleton.joint(CocoJointId::RightEar);
    let left_shoulder = skeleton.joint(CocoJointId::LeftShoulder);
    let right_shoulder = skeleton.joint(CocoJointId::RightShoulder);

    if left_ear.is_confident(0.2)
        && right_ear.is_confident(0.2)
        && left_shoulder.is_confident(0.2)
        && right_shoulder.is_confident(0.2)
    {
        let ear_mid = left_ear.position.midpoint(&right_ear.position);
        let shoulder_mid = left_shoulder.position.midpoint(&right_shoulder.position);

        let neck_vec = Vector3D::new(
            ear_mid.x - shoulder_mid.x,
            ear_mid.y - shoulder_mid.y,
            ear_mid.z - shoulder_mid.z,
        );
        // Axe vertical vers le haut (dans le repère image où Y pointe vers le bas, le haut est -Y)
        let vertical = Vector3D::new(0.0, -1.0, 0.0);
        let angle_rad = neck_vec.angle_between(&vertical);
        angle_rad.to_degrees()
    } else {
        12.0 // Angle nominal standard par défaut
    }
}

/// Classifie la posture corporelle à partir des 17 articulations.
fn classify_skeleton_posture(joints: &[SkeletonJoint3D; 17]) -> PostureKind {
    let left_hip = &joints[CocoJointId::LeftHip as usize];
    let _right_hip = &joints[CocoJointId::RightHip as usize];
    let left_knee = &joints[CocoJointId::LeftKnee as usize];
    let _right_knee = &joints[CocoJointId::RightKnee as usize];

    // Si les genoux et hanches sont visibles avec un angle significatif -> Seated vs Standing
    if left_knee.is_confident(0.3) && left_hip.is_confident(0.3) {
        let dy = (left_knee.position.y - left_hip.position.y).abs();
        let dx = (left_knee.position.x - left_hip.position.x).abs();
        if dy > dx * 1.5 {
            PostureKind::Standing
        } else {
            PostureKind::Seated
        }
    } else {
        // En vue bureau standard (buste seul visible) :
        PostureKind::Seated
    }
}

fn average_confidence(skeleton: &BodySkeleton3D) -> f32 {
    let sum: f32 = skeleton.joints.iter().map(|j| j.confidence).sum();
    sum / (skeleton.joints.len() as f32)
}

/// Génère un squelette canonique assis pour les tests déterministes.
pub fn generate_canonical_seated_skeleton(w: f32, h: f32) -> BodySkeleton3D {
    let mut joints = [SkeletonJoint3D::default(); 17];
    let cx = w * 0.5;

    // Tête
    joints[CocoJointId::Nose as usize] = SkeletonJoint3D::new(0, Point3D::new(cx, h * 0.16, 0.6), 0.95);
    joints[CocoJointId::LeftEye as usize] = SkeletonJoint3D::new(1, Point3D::new(cx - w * 0.04, h * 0.14, 0.6), 0.95);
    joints[CocoJointId::RightEye as usize] = SkeletonJoint3D::new(2, Point3D::new(cx + w * 0.04, h * 0.14, 0.6), 0.95);
    joints[CocoJointId::LeftEar as usize] = SkeletonJoint3D::new(3, Point3D::new(cx - w * 0.09, h * 0.15, 0.6), 0.90);
    joints[CocoJointId::RightEar as usize] = SkeletonJoint3D::new(4, Point3D::new(cx + w * 0.09, h * 0.15, 0.6), 0.90);

    // Torse et épaules
    joints[CocoJointId::LeftShoulder as usize] = SkeletonJoint3D::new(5, Point3D::new(cx - w * 0.18, h * 0.30, 0.6), 0.92);
    joints[CocoJointId::RightShoulder as usize] = SkeletonJoint3D::new(6, Point3D::new(cx + w * 0.18, h * 0.30, 0.6), 0.92);

    // Bras
    joints[CocoJointId::LeftElbow as usize] = SkeletonJoint3D::new(7, Point3D::new(cx - w * 0.24, h * 0.50, 0.6), 0.85);
    joints[CocoJointId::RightElbow as usize] = SkeletonJoint3D::new(8, Point3D::new(cx + w * 0.24, h * 0.50, 0.6), 0.85);
    joints[CocoJointId::LeftWrist as usize] = SkeletonJoint3D::new(9, Point3D::new(cx - w * 0.20, h * 0.70, 0.6), 0.80);
    joints[CocoJointId::RightWrist as usize] = SkeletonJoint3D::new(10, Point3D::new(cx + w * 0.20, h * 0.70, 0.6), 0.80);

    // Hanches
    joints[CocoJointId::LeftHip as usize] = SkeletonJoint3D::new(11, Point3D::new(cx - w * 0.14, h * 0.82, 0.6), 0.90);
    joints[CocoJointId::RightHip as usize] = SkeletonJoint3D::new(12, Point3D::new(cx + w * 0.14, h * 0.82, 0.6), 0.90);

    // Membres inférieurs (position assise)
    joints[CocoJointId::LeftKnee as usize] = SkeletonJoint3D::new(13, Point3D::new(cx - w * 0.15, h * 0.95, 0.6), 0.70);
    joints[CocoJointId::RightKnee as usize] = SkeletonJoint3D::new(14, Point3D::new(cx + w * 0.15, h * 0.95, 0.6), 0.70);
    joints[CocoJointId::LeftAnkle as usize] = SkeletonJoint3D::new(15, Point3D::new(cx - w * 0.15, h * 1.05, 0.6), 0.20);
    joints[CocoJointId::RightAnkle as usize] = SkeletonJoint3D::new(16, Point3D::new(cx + w * 0.15, h * 1.05, 0.6), 0.20);

    BodySkeleton3D::new(joints, PostureKind::Seated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pose_simulated_seated() {
        let session = PoseSession::open_or_simulated(None);
        assert_eq!(session.status(), ModelStatus::Simulated);

        let dummy_img = vec![128u8; 640 * 480 * 3];
        let res = session.infer(&dummy_img, 640, 480).expect("inférence réussie");

        assert_eq!(res.status, ModelStatus::Simulated);
        assert_eq!(res.posture, PostureKind::Seated);
        assert!(res.confidence > 0.75);
        assert!(res.neck_angle_deg < 25.0, "Angle du cou nominal assis < 25° : {}", res.neck_angle_deg);
        assert_eq!(res.skeleton.joints.len(), 17);
    }
}
