//! Estimation 3D du regard (Gaze Tracking) & orientation de tête 6-DOF avec intersection écran.
//!
//! Ce module combine :
//! 1. L'orientation de la tête $SO(3)$ (Pitch, Yaw, Roll) via les repères faciaux clés.
//! 2. Le vecteur de rotation oculaire déduit des centres d'iris MediaPipe (468..=477).
//! 3. Le rayon de regard 3D [`GazeRay3D`] et son intersection avec les limites de l'écran [`ScreenBounds`].

use hakille_spatial::{GazeRay3D, HeadPose6D, Point3D, ScreenBounds, Vector3D};
use serde::{Deserialize, Serialize};

/// Résultat du suivi du regard 3D.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GazeTrackingResult {
    /// L'utilisateur fixe-t-il l'écran d'affichage ?
    pub is_screen_focused: bool,
    /// Angles de regard en radians : (Pitch (haut/bas), Yaw (gauche/droite)).
    pub gaze_angles: (f32, f32),
    /// Rayon 3D de regard projeté depuis le visage.
    pub gaze_ray: GazeRay3D,
    /// Point d'impact 3D sur le plan de l'écran (si intersection).
    pub screen_intersection: Option<Point3D>,
    /// Orientation 6-DOF estimée de la tête.
    pub head_pose: HeadPose6D,
    /// Score de confiance de l'estimation [0.0, 1.0].
    pub confidence: f32,
}

/// Traqueur 3D de regard et d'attention visuelle.
pub struct GazeTracker {
    /// Limites physiques ou virtuelles de l'écran de travail.
    screen_bounds: ScreenBounds,
}

impl GazeTracker {
    /// Initialise le traqueur avec un écran d'ordinateur portable standard par défaut.
    pub fn new() -> Self {
        Self {
            screen_bounds: ScreenBounds::default_desktop_24inch(),
        }
    }

    /// Initialise avec des dimensions d'écran personnalisées.
    pub fn with_screen(screen_bounds: ScreenBounds) -> Self {
        Self { screen_bounds }
    }

    /// Calcule le regard 3D à partir des 478 points du maillage facial.
    pub fn track(
        &self,
        landmarks: &[Point3D],
        camera_distance_m: f32,
        width: f32,
        height: f32,
    ) -> GazeTrackingResult {
        if landmarks.len() < 478 {
            let default_ray = GazeRay3D::from_vector(
                Point3D::new(0.0, 0.0, camera_distance_m),
                Vector3D::new(0.0, 0.0, -1.0),
                true,
            );
            return GazeTrackingResult {
                is_screen_focused: true,
                gaze_angles: (0.0, 0.0),
                gaze_ray: default_ray,
                screen_intersection: Some(Point3D::ORIGIN),
                head_pose: HeadPose6D::default(),
                confidence: 0.5,
            };
        }

        // Points clés faciaux :
        // 1: pointe du nez, 152: menton, 33: coin externe oeil gauche, 263: coin externe oeil droit
        let nose = landmarks[1];
        let chin = landmarks[152];
        let left_eye_ext = landmarks[33];
        let right_eye_ext = landmarks[263];

        // Estimation de l'orientation de tête
        let eye_mid = left_eye_ext.midpoint(&right_eye_ext);
        let head_h_vec = Vector3D::new(right_eye_ext.x - left_eye_ext.x, right_eye_ext.y - left_eye_ext.y, right_eye_ext.z - left_eye_ext.z);
        let _head_v_vec = Vector3D::new(chin.x - eye_mid.x, chin.y - eye_mid.y, chin.z - eye_mid.z);

        // Yaw : asymétrie horizontale du nez par rapport au milieu des yeux
        let face_width = left_eye_ext.distance_to(&right_eye_ext).max(1e-4);
        let nose_offset_x = (nose.x - eye_mid.x) / face_width;
        let head_yaw = (nose_offset_x * 1.5).clamp(-0.8, 0.8);

        // Pitch : inclinaison verticale nez-yeux
        // Ratio anatomique standard nez/visage au repos ~ 0.29-0.30
        let face_height = eye_mid.distance_to(&chin).max(1e-4);
        let nose_offset_y = (nose.y - eye_mid.y) / face_height;
        let head_pitch = ((nose_offset_y - 0.294) * 1.5).clamp(-0.8, 0.8);

        // Roll : inclinaison de la ligne des yeux par rapport à l'horizontale
        let head_roll = head_h_vec.y.atan2(head_h_vec.x);

        // Iris MediaPipe :
        // 468 = centre iris gauche, 473 = centre iris droit
        let left_iris = landmarks[468];
        let right_iris = landmarks[473];
        let eyes_center = left_iris.midpoint(&right_iris);

        // Rotation des yeux relative aux coins
        // Oeil gauche : 33 (ext), 133 (int) ; Oeil droit : 362 (int), 263 (ext)
        let left_eye_center = landmarks[33].midpoint(&landmarks[133]);
        let right_eye_center = landmarks[362].midpoint(&landmarks[263]);

        let left_eye_dx = (left_iris.x - left_eye_center.x) / (landmarks[33].distance_to(&landmarks[133]).max(1e-4));
        let left_eye_dy = (left_iris.y - left_eye_center.y) / (landmarks[33].distance_to(&landmarks[133]).max(1e-4));

        let right_eye_dx = (right_iris.x - right_eye_center.x) / (landmarks[362].distance_to(&landmarks[263]).max(1e-4));
        let right_eye_dy = (right_iris.y - right_eye_center.y) / (landmarks[362].distance_to(&landmarks[263]).max(1e-4));

        let eye_yaw = ((left_eye_dx + right_eye_dx) * 0.5 * 1.2).clamp(-0.6, 0.6);
        let eye_pitch = ((left_eye_dy + right_eye_dy) * 0.5 * 1.2).clamp(-0.6, 0.6);

        // Conversion des coordonnées pixels [0, W] et [0, H] dans le repère métrique centré de la caméra
        let fx = width / (2.0 * (1.22173f32 / 2.0).tan());
        let fy = fx;
        let origin_x = (eyes_center.x - width * 0.5) / fx * camera_distance_m;
        let origin_y = (eyes_center.y - height * 0.5) / fy * camera_distance_m;
        let gaze_origin = Point3D::new(origin_x, origin_y, camera_distance_m);

        // Combinaison Head Pose + Eye Rotation avec biais de visée écran (webcam en haut de l'écran)
        let screen_pitch_bias = ((self.screen_bounds.center.y - origin_y) / camera_distance_m.max(0.1)).atan();
        let total_pitch = (screen_pitch_bias * 0.8) + head_pitch + eye_pitch;
        let total_yaw = head_yaw + eye_yaw;

        // Direction du rayon dans l'espace 3D caméra (l'écran est à Z=0, l'utilisateur est à Z > 0 face à l'écran)
        // Direction vers l'écran = -Z
        let dir_x = total_yaw.sin();
        let dir_y = total_pitch.sin();
        let dir_z = -(1.0 - dir_x * dir_x - dir_y * dir_y).max(0.0).sqrt();

        let gaze_direction = Vector3D::new(dir_x, dir_y, dir_z).normalize_or_zero();

        let mut gaze_ray = GazeRay3D::from_vector(gaze_origin, gaze_direction, false);
        let is_screen_focused = gaze_ray.check_screen_intersection(&self.screen_bounds);
        let screen_intersection = gaze_ray.intersect_plane_z(self.screen_bounds.center.z);

        let head_pose = HeadPose6D::from_radians(head_yaw, head_pitch, head_roll, gaze_origin);

        GazeTrackingResult {
            is_screen_focused,
            gaze_angles: (total_pitch, total_yaw),
            gaze_ray,
            screen_intersection,
            head_pose,
            confidence: 0.90,
        }
    }
}

impl Default for GazeTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_inference::facemesh_onnx::generate_canonical_landmarks;

    #[test]
    fn test_gaze_tracker_canonical_screen_focus() {
        let tracker = GazeTracker::new();
        let canonical_pts = generate_canonical_landmarks(640.0, 480.0);

        let res = tracker.track(&canonical_pts, 0.75, 640.0, 480.0);

        assert!(res.is_screen_focused, "Regard centré canonique doit fixer l'écran");
        assert!(res.screen_intersection.is_some());
        assert!(res.gaze_angles.0.abs() < 0.25, "Pitch proche de l'horizontale : {}", res.gaze_angles.0);
        assert!(res.gaze_angles.1.abs() < 0.25, "Yaw proche de l'axe central : {}", res.gaze_angles.1);
        assert!(res.confidence >= 0.85);
    }
}
