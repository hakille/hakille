//! Orientation et pose de tête 6-DOF ([`HeadPose6D`]) et projection sténopé ([`CameraIntrinsics`]).

use crate::geometry::{Point3D, Vector3D};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Pose de la tête à 6 degrés de liberté (6-DOF) dans le repère caméra.
///
/// Les angles d'Euler `yaw`, `pitch`, `roll` sont stockés en **radians** en interne.
/// Des méthodes d'accès et constructeurs en degrés sont fournis pour faciliter l'intégration.
///
/// Convention des repères caméra standard (OpenCV / Vision par ordinateur) :
/// - Axe X : vers la droite.
/// - Axe Y : vers le bas.
/// - Axe Z : vers l'avant (profondeur dans la scène).
///
/// Définition des rotations de tête :
/// - `yaw` (lacet) : rotation autour de l'axe vertical Y (négatif = tourne la tête vers sa droite / gauche caméra, positif = tourne la tête vers sa gauche).
/// - `pitch` (tangage) : rotation autour de l'axe transversal X (négatif = hochement vers le bas, positif = hochement vers le haut).
/// - `roll` (roulis) : rotation autour de l'axe optique Z (inclinaison latérale de la tête sur l'épaule).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HeadPose6D {
    /// Angle de lacet (yaw) en radians.
    pub yaw: f32,
    /// Angle de tangage (pitch) en radians.
    pub pitch: f32,
    /// Angle de roulis (roll) en radians.
    pub roll: f32,
    /// Translation du centre de la tête (en mètres) dans le repère caméra.
    pub translation: Point3D,
}

impl HeadPose6D {
    /// Seuil angulaire par défaut en degrés pour considérer que l'utilisateur fait face à la caméra.
    pub const DEFAULT_FACING_THRESHOLD_DEG: f32 = 20.0;

    /// Crée une nouvelle pose 6-DOF avec les angles exprimés en **radians**.
    pub const fn new(yaw: f32, pitch: f32, roll: f32, translation: Point3D) -> Self {
        Self {
            yaw,
            pitch,
            roll,
            translation,
        }
    }

    /// Crée une nouvelle pose 6-DOF à partir d'angles exprimés en **degrés**.
    pub fn from_degrees(yaw_deg: f32, pitch_deg: f32, roll_deg: f32, translation: Point3D) -> Self {
        Self {
            yaw: yaw_deg.to_radians(),
            pitch: pitch_deg.to_radians(),
            roll: roll_deg.to_radians(),
            translation,
        }
    }

    /// Crée une nouvelle pose 6-DOF à partir d'angles exprimés en **radians** (alias explicite de `new`).
    pub fn from_radians(yaw_rad: f32, pitch_rad: f32, roll_rad: f32, translation: Point3D) -> Self {
        Self::new(yaw_rad, pitch_rad, roll_rad, translation)
    }

    /// Retourne l'angle de lacet (yaw) en degrés.
    pub fn yaw_degrees(&self) -> f32 {
        self.yaw.to_degrees()
    }

    /// Retourne l'angle de tangage (pitch) en degrés.
    pub fn pitch_degrees(&self) -> f32 {
        self.pitch.to_degrees()
    }

    /// Retourne l'angle de roulis (roll) en degrés.
    pub fn roll_degrees(&self) -> f32 {
        self.roll.to_degrees()
    }

    /// Détermine si la tête fait face à la caméra selon des seuils personnalisés en degrés.
    pub fn is_facing_camera(&self, max_yaw_deg: f32, max_pitch_deg: f32) -> bool {
        self.yaw_degrees().abs() <= max_yaw_deg && self.pitch_degrees().abs() <= max_pitch_deg
    }

    /// Détermine si la tête fait face à la caméra avec le seuil par défaut de ±20°.
    pub fn is_facing_camera_default(&self) -> bool {
        self.is_facing_camera(Self::DEFAULT_FACING_THRESHOLD_DEG, Self::DEFAULT_FACING_THRESHOLD_DEG)
    }

    /// Calcule le vecteur unitaire pointant droit devant le visage dans le repère caméra.
    ///
    /// Lorsque le visage regarde droit devant (yaw=0, pitch=0), le vecteur pointe le long de l'axe Z
    /// vers l'observateur/écran ou la caméra selon le repère de pose.
    pub fn forward_vector(&self) -> Vector3D {
        let cy = self.yaw.cos();
        let sy = self.yaw.sin();
        let cp = self.pitch.cos();
        let sp = self.pitch.sin();

        // En repère caméra standard, un vecteur orienté selon la rotation Euler :
        Vector3D::new(sy * cp, -sp, cy * cp)
    }

    /// Matrice de rotation 3x3 combinée R = R_y(yaw) * R_x(pitch) * R_z(roll).
    pub fn rotation_matrix_3x3(&self) -> [[f32; 3]; 3] {
        let cy = self.yaw.cos();
        let sy = self.yaw.sin();
        let cp = self.pitch.cos();
        let sp = self.pitch.sin();
        let cr = self.roll.cos();
        let sr = self.roll.sin();

        [
            [cy * cr + sy * sp * sr, -cy * sr + sy * sp * cr, sy * cp],
            [cp * sr, cp * cr, -sp],
            [-sy * cr + cy * sp * sr, sy * sr + cy * sp * cr, cy * cp],
        ]
    }
}

impl Default for HeadPose6D {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            translation: Point3D::ORIGIN,
        }
    }
}

impl fmt::Display for HeadPose6D {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "HeadPose6D(yaw={:.1}°, pitch={:.1}°, roll={:.1}°, trans={})",
            self.yaw_degrees(),
            self.pitch_degrees(),
            self.roll_degrees(),
            self.translation
        )
    }
}

/// Paramètres intrinsèques d'une caméra à sténopé (pinhole camera model).
///
/// Modélise la matrice de projection $K$ pour SolvePnP et la reprojection 2D/3D :
/// $$K = \begin{pmatrix} f_x & 0 & c_x \\ 0 & f_y & c_y \\ 0 & 0 & 1 \end{pmatrix}$$
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraIntrinsics {
    /// Longueur focale horizontale en pixels ($f_x$).
    pub fx: f32,
    /// Longueur focale verticale en pixels ($f_y$).
    pub fy: f32,
    /// Coordonnée x du point principal (centre optique) en pixels ($c_x$).
    pub cx: f32,
    /// Coordonnée y du point principal (centre optique) en pixels ($c_y$).
    pub cy: f32,
}

impl CameraIntrinsics {
    /// Crée une nouvelle configuration intrinsèque de caméra sténopé.
    pub const fn new(fx: f32, fy: f32, cx: f32, cy: f32) -> Self {
        Self { fx, fy, cx, cy }
    }

    /// Projette un point 3D en coordonnées caméra vers l'espace image 2D (pixel $u, v$).
    /// Retourne `None` si le point est derrière ou sur le plan caméra ($Z \le 0$).
    pub fn project(&self, point: &Point3D) -> Option<(f32, f32)> {
        if point.z <= 1e-6 {
            return None;
        }
        let inv_z = 1.0 / point.z;
        let u = self.fx * point.x * inv_z + self.cx;
        let v = self.fy * point.y * inv_z + self.cy;
        Some((u, v))
    }

    /// Rétro-projette des coordonnées pixels $(u, v)$ et une profondeur $Z > 0$ en coordonnées 3D.
    pub fn unproject(&self, pixel_x: f32, pixel_y: f32, depth_z: f32) -> Option<Point3D> {
        if depth_z <= 1e-6 || self.fx.abs() <= 1e-6 || self.fy.abs() <= 1e-6 {
            return None;
        }
        let x = (pixel_x - self.cx) * depth_z / self.fx;
        let y = (pixel_y - self.cy) * depth_z / self.fy;
        Some(Point3D::new(x, y, depth_z))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_degrees_radians_conversion() {
        let pose = HeadPose6D::from_degrees(45.0, -30.0, 15.0, Point3D::new(0.0, 0.0, 0.6));
        assert!((pose.yaw_degrees() - 45.0).abs() < 1e-5);
        assert!((pose.pitch_degrees() - -30.0).abs() < 1e-5);
        assert!((pose.roll_degrees() - 15.0).abs() < 1e-5);

        assert!((pose.yaw - std::f32::consts::FRAC_PI_4).abs() < 1e-5);
    }

    #[test]
    fn test_is_facing_camera() {
        let centered = HeadPose6D::from_degrees(0.0, 0.0, 0.0, Point3D::new(0.0, 0.0, 0.5));
        assert!(centered.is_facing_camera_default());

        let slightly_turned = HeadPose6D::from_degrees(18.0, -10.0, 5.0, Point3D::new(0.0, 0.0, 0.5));
        assert!(slightly_turned.is_facing_camera_default());

        let turned_away = HeadPose6D::from_degrees(25.0, 0.0, 0.0, Point3D::new(0.0, 0.0, 0.5));
        assert!(!turned_away.is_facing_camera_default());
        assert!(turned_away.is_facing_camera(30.0, 20.0));

        let looking_down = HeadPose6D::from_degrees(0.0, -25.0, 0.0, Point3D::new(0.0, 0.0, 0.5));
        assert!(!looking_down.is_facing_camera_default());
    }

    #[test]
    fn test_forward_vector_frontal() {
        let frontal = HeadPose6D::from_degrees(0.0, 0.0, 0.0, Point3D::ORIGIN);
        let fwd = frontal.forward_vector();
        assert!((fwd.x).abs() < 1e-6);
        assert!((fwd.y).abs() < 1e-6);
        assert!((fwd.z - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_camera_intrinsics_projection_and_unprojection() {
        let cam = CameraIntrinsics::new(1000.0, 1000.0, 640.0, 360.0);
        let p3d = Point3D::new(0.1, 0.2, 1.0);

        let (u, v) = cam.project(&p3d).expect("Projection should succeed for z > 0");
        assert!((u - 740.0).abs() < 1e-5);
        assert!((v - 560.0).abs() < 1e-5);

        let unprojected = cam.unproject(u, v, 1.0).expect("Unprojection should succeed");
        assert!(unprojected.approx_eq(&p3d, 1e-5));

        // Negative depth should fail
        let behind = Point3D::new(0.1, 0.2, -1.0);
        assert!(cam.project(&behind).is_none());
    }
}
