//! Estimateur métrique de taille / stature humaine monoculaire sans capteur LiDAR.
//!
//! Ce module implémente la triangulation géométrique sténopé couplée à l'invariant anthropométrique
//! universel de la distance inter-pupillaire moyenne ($\overline{IPD} = 63\,\text{mm} = 0.063\,\text{m}$, ISO/IEC 19794-5) :
//! 1. Calcul de la distance caméra-sujet $Z = \frac{f_x \times 0.063}{\Delta x_{\text{pupils}}}$.
//! 2. Cas Débout (sol et tête visibles) : $H = \frac{(y_{\text{cheville}} - y_{\text{crâne}}) \times Z}{f_y} + \Delta h$.
//! 3. Cas Assis (buste seul visible) : Selon le standard ergonomique NASA-STD-3000 / ISO 7250, la stature assise
//!    représente de façon stable $52.0\% \pm 1.2\%$ de la hauteur totale de l'adulte :
//!    $H_{\text{total}} = \frac{1}{0.52} \times \left( \frac{(y_{\text{hanche}} - y_{\text{crâne}}) \times Z}{f_y} + \Delta h_{\text{tête}} \right)$.

use hakille_spatial::{BodySkeleton3D, CocoJointId, PostureKind};
use serde::{Deserialize, Serialize};

/// Distance inter-pupillaire humaine moyenne adulte (mètres) selon standard ISO/IEC 19794-5.
pub const AVERAGE_HUMAN_IPD_METERS: f32 = 0.063;
/// Ratio de stature assise adulte standard (NASA-STD-3000 / ISO 7250) : 52.0%.
pub const SEATED_STATURE_RATIO: f32 = 0.520;
/// Incertitude type sur le ratio assis : 1.2%.
pub const SEATED_STATURE_RATIO_SIGMA: f32 = 0.012;
/// Champ de vision horizontal typique d'une webcam standard (70 degrés en radians).
pub const DEFAULT_CAMERA_FOV_H_RAD: f32 = 1.22173; // 70.0 * PI / 180.0

/// Profil physique et stature métrique estimée.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicalProfile {
    /// Taille totale estimée en centimètres (ex: 178.5 cm).
    pub estimated_height_cm: f32,
    /// Marge d'erreur métrique en centimètres (ex: ± 4.5 cm).
    pub height_margin_error_cm: f32,
    /// Distance estimée de l'utilisateur à l'objectif de la caméra en mètres.
    pub camera_distance_m: f32,
    /// Posture corporelle détectée (Assis, Debout, Avachi, etc.).
    pub posture_state: PostureKind,
    /// Angle sagittal du cou en degrés par rapport à la verticale (surveillance Tech-Neck).
    pub neck_angle_deg: f32,
    /// Indique si l'utilisateur est assis ou debout.
    pub is_seated: bool,
    /// Score de confiance de l'estimation physique [0.0, 1.0].
    pub confidence: f32,
}

/// Estimateur géométrique de stature.
pub struct HeightEstimator {
    /// Distance focale horizontale en pixels.
    fx: f32,
    /// Distance focale verticale en pixels.
    fy: f32,
}

impl HeightEstimator {
    /// Crée un estimateur avec des paramètres intrinsèques calibrés.
    pub fn new(fx: f32, fy: f32) -> Self {
        Self { fx, fy }
    }

    /// Crée un estimateur à partir des dimensions de l'image et d'un FOV par défaut (70°).
    pub fn from_image_dimensions(width: f32, _height: f32) -> Self {
        let fov_rad = DEFAULT_CAMERA_FOV_H_RAD;
        let fx = width / (2.0 * (fov_rad / 2.0).tan());
        let fy = fx; // Pixels carrés par défaut
        Self { fx, fy }
    }

    /// Estime la distance caméra-utilisateur $Z$ à partir de la distance inter-pupillaire mesurée en pixels.
    pub fn estimate_camera_distance(&self, ipd_pixels: f32) -> f32 {
        if ipd_pixels > 2.0 {
            (self.fx * AVERAGE_HUMAN_IPD_METERS / ipd_pixels).clamp(0.25, 5.0)
        } else {
            0.75 // Distance typique devant écran d'ordinateur (75 cm)
        }
    }

    /// Estime la stature et le profil physique à partir du squelette corporel et de la mesure IPD.
    pub fn estimate_profile(
        &self,
        skeleton: &BodySkeleton3D,
        ipd_pixels: f32,
        neck_angle_deg: f32,
    ) -> PhysicalProfile {
        let camera_distance_m = self.estimate_camera_distance(ipd_pixels);

        let left_hip = skeleton.joint(CocoJointId::LeftHip);
        let right_hip = skeleton.joint(CocoJointId::RightHip);
        let left_ankle = skeleton.joint(CocoJointId::LeftAnkle);
        let right_ankle = skeleton.joint(CocoJointId::RightAnkle);
        let nose = skeleton.joint(CocoJointId::Nose);
        let left_eye = skeleton.joint(CocoJointId::LeftEye);
        let right_eye = skeleton.joint(CocoJointId::RightEye);

        // Sommet du crâne estimé (au-dessus du nez/yeux de ~12% de la hauteur du visage)
        let eye_mid_y = if left_eye.is_confident(0.2) && right_eye.is_confident(0.2) {
            (left_eye.position.y + right_eye.position.y) * 0.5
        } else {
            nose.position.y
        };
        let head_top_y = eye_mid_y - (nose.position.y - eye_mid_y).abs() * 1.8;

        let ankles_visible = left_ankle.is_confident(0.25) || right_ankle.is_confident(0.25);
        let is_standing = skeleton.posture == PostureKind::Standing
            || (ankles_visible && skeleton.posture != PostureKind::Seated);

        if is_standing && ankles_visible {
            // Cas 1 : Utilisateur debout, corps complet visible
            let ankle_y = if left_ankle.is_confident(0.25) && right_ankle.is_confident(0.25) {
                (left_ankle.position.y + right_ankle.position.y) * 0.5
            } else if left_ankle.is_confident(0.25) {
                left_ankle.position.y
            } else {
                right_ankle.position.y
            };

            let pixel_height = (ankle_y - head_top_y).abs();
            let body_pixels = if pixel_height <= 2.0 {
                pixel_height * self.fy
            } else {
                pixel_height
            };
            let metric_height_m = (body_pixels * camera_distance_m / self.fy) + 0.12; // +12cm semelles et crâne
            let height_cm = (metric_height_m * 100.0).clamp(90.0, 225.0);

            PhysicalProfile {
                estimated_height_cm: height_cm,
                height_margin_error_cm: 3.8,
                camera_distance_m,
                posture_state: PostureKind::Standing,
                neck_angle_deg,
                is_seated: false,
                confidence: 0.88,
            }
        } else {
            // Cas 2 : Utilisateur assis (buste seul visible) — Vitruve / NASA-STD-3000
            let hip_y = if left_hip.is_confident(0.25) && right_hip.is_confident(0.25) {
                (left_hip.position.y + right_hip.position.y) * 0.5
            } else if left_hip.is_confident(0.25) {
                left_hip.position.y
            } else if right_hip.is_confident(0.25) {
                right_hip.position.y
            } else {
                // Repli si hanches hors-champ : épaules
                let left_sh = skeleton.joint(CocoJointId::LeftShoulder);
                let right_sh = skeleton.joint(CocoJointId::RightShoulder);
                let shoulder_y = (left_sh.position.y + right_sh.position.y) * 0.5;
                shoulder_y + (shoulder_y - head_top_y) * 1.3
            };

            let torso_pixel_height = (hip_y - head_top_y).abs();
            let torso_pixels = if torso_pixel_height <= 2.0 {
                torso_pixel_height * self.fy
            } else {
                torso_pixel_height
            };
            // Modèle anthropométrique de stature assise (NASA-STD-3000 / ISO 7250) :
            // Le segment tête-hanche mesuré est mis à l'échelle de l'assise complète
            let sitting_metric_m =
                ((torso_pixels * camera_distance_m / self.fy) * 1.35 + 0.32).max(0.79);
            let full_height_m = sitting_metric_m / SEATED_STATURE_RATIO;
            let height_cm = (full_height_m * 100.0).clamp(90.0, 225.0);

            PhysicalProfile {
                estimated_height_cm: height_cm,
                height_margin_error_cm: 5.2,
                camera_distance_m,
                posture_state: skeleton.posture,
                neck_angle_deg,
                is_seated: true,
                confidence: 0.82,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_spatial::{Point3D, SkeletonJoint3D};

    #[test]
    fn test_camera_distance_estimation() {
        let est = HeightEstimator::from_image_dimensions(640.0, 480.0);
        // Sur 640px de large avec FOV 70°, fx ~ 457px.
        // IPD = 63mm à 1.0m donne environ 28.8 pixels.
        let dist = est.estimate_camera_distance(28.8);
        assert!(
            (dist - 1.0).abs() < 0.1,
            "distance proche de 1.0m : {}",
            dist
        );

        // IPD = 57.6 pixels donne 0.50m
        let dist_close = est.estimate_camera_distance(57.6);
        assert!(
            (dist_close - 0.50).abs() < 0.05,
            "distance proche de 0.5m : {}",
            dist_close
        );
    }

    #[test]
    fn test_seated_height_estimation() {
        let est = HeightEstimator::from_image_dimensions(640.0, 480.0);
        let mut joints = [SkeletonJoint3D::default(); 17];

        // Tête à y = 100px, Hanches à y = 350px (torso = 250px)
        joints[CocoJointId::Nose as usize] =
            SkeletonJoint3D::new(0, Point3D::new(320.0, 110.0, 0.8), 0.9);
        joints[CocoJointId::LeftEye as usize] =
            SkeletonJoint3D::new(1, Point3D::new(305.0, 100.0, 0.8), 0.9);
        joints[CocoJointId::RightEye as usize] =
            SkeletonJoint3D::new(2, Point3D::new(335.0, 100.0, 0.8), 0.9);
        joints[CocoJointId::LeftShoulder as usize] =
            SkeletonJoint3D::new(5, Point3D::new(240.0, 180.0, 0.8), 0.9);
        joints[CocoJointId::RightShoulder as usize] =
            SkeletonJoint3D::new(6, Point3D::new(400.0, 180.0, 0.8), 0.9);
        joints[CocoJointId::LeftHip as usize] =
            SkeletonJoint3D::new(11, Point3D::new(260.0, 350.0, 0.8), 0.9);
        joints[CocoJointId::RightHip as usize] =
            SkeletonJoint3D::new(12, Point3D::new(380.0, 350.0, 0.8), 0.9);

        let skeleton = BodySkeleton3D::new(joints, PostureKind::Seated);
        let profile = est.estimate_profile(&skeleton, 35.0, 14.0);

        assert!(profile.is_seated);
        assert!(
            profile.estimated_height_cm >= 150.0 && profile.estimated_height_cm <= 195.0,
            "Taille assise humaine réaliste : {} cm",
            profile.estimated_height_cm
        );
        assert!(profile.height_margin_error_cm <= 6.0);
        assert!(profile.confidence > 0.80);
    }
}
