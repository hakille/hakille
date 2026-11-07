//! Rayon de regard 3D ([`GazeRay3D`]) et interaction avec la géométrie d'écran ([`ScreenBounds`]).

use crate::geometry::{Point3D, Vector3D};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Limites physiques d'un écran d'ordinateur dans le repère caméra (en mètres).
///
/// Par convention :
/// - L'écran est modélisé comme un rectangle centré sur `center` orthogonal à l'axe optique (plan Z constant).
/// - `width_m` : largeur horizontale en mètres (par exemple ~0.53 m pour un 24 pouces 16:9).
/// - `height_m` : hauteur verticale en mètres (par exemple ~0.30 m pour un 24 pouces 16:9).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScreenBounds {
    /// Centre de l'écran dans le repère caméra.
    pub center: Point3D,
    /// Largeur de la dalle en mètres.
    pub width_m: f32,
    /// Hauteur de la dalle en mètres.
    pub height_m: f32,
}

impl ScreenBounds {
    /// Crée de nouvelles limites d'écran.
    pub const fn new(center: Point3D, width_m: f32, height_m: f32) -> Self {
        Self {
            center,
            width_m,
            height_m,
        }
    }

    /// Configuration par défaut pour un écran de bureau standard 24" 16:9 situé juste sous la webcam intégrée (Z=0).
    pub fn default_desktop_24inch() -> Self {
        // Webcam fixée en haut au centre de l'écran : le centre de l'écran est 15 cm sous la caméra (Y = 0.15)
        Self {
            center: Point3D::new(0.0, 0.15, 0.0),
            width_m: 0.53,
            height_m: 0.30,
        }
    }

    /// Vérifie si un point d'impact 2D (x, y) se situe à l'intérieur des bordures de l'écran.
    pub fn contains_point_2d(&self, x: f32, y: f32) -> bool {
        let half_w = self.width_m * 0.5;
        let half_h = self.height_m * 0.5;
        let min_x = self.center.x - half_w;
        let max_x = self.center.x + half_w;
        let min_y = self.center.y - half_h;
        let max_y = self.center.y + half_h;

        x >= min_x && x <= max_x && y >= min_y && y <= max_y
    }
}

/// Rayon de regard dans l'espace 3D avec statut de convergence sur l'écran.
///
/// Conformément au contrat d'interface `PROJECT.md` (§ 1) :
/// - `origin` : point 3D d'origine du regard (par exemple le point médian inter-oculaire ou pupille).
/// - `direction` : composantes unitaires de la direction du regard stockées sous forme de [`Point3D`].
/// - `is_focused_on_screen` : indicateur booléen de fixation sur la surface utile de l'écran.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GazeRay3D {
    /// Point 3D d'origine du regard.
    pub origin: Point3D,
    /// Direction unitaire du regard (représentée en coordonnées cartésiennes Point3D).
    pub direction: Point3D,
    /// Indique si le regard intersecte l'écran actif.
    pub is_focused_on_screen: bool,
}

impl GazeRay3D {
    /// Crée un nouveau rayon de regard 3D avec direction sous forme de `Point3D`.
    pub const fn new(origin: Point3D, direction: Point3D, is_focused_on_screen: bool) -> Self {
        Self {
            origin,
            direction,
            is_focused_on_screen,
        }
    }

    /// Crée un nouveau rayon à partir d'un [`Vector3D`] de direction.
    /// La direction est automatiquement normalisée si sa norme est non nulle.
    pub fn from_vector(origin: Point3D, direction: Vector3D, is_focused_on_screen: bool) -> Self {
        let normalized = direction.normalize_or_zero();
        Self {
            origin,
            direction: normalized.to_point(),
            is_focused_on_screen,
        }
    }

    /// Retourne la direction sous la forme d'un [`Vector3D`] prêt pour les opérations vectorielles.
    pub fn direction_vector(&self) -> Vector3D {
        Vector3D::new(self.direction.x, self.direction.y, self.direction.z)
    }

    /// Calcule la position d'un point le long du rayon à la distance paramétrique $t$ : $P(t) = \text{origin} + t \cdot \vec{d}$.
    pub fn point_at_distance(&self, t: f32) -> Point3D {
        self.origin + (self.direction_vector() * t)
    }

    /// Calcule l'intersection du rayon avec un plan transversal $Z = \text{target\_z}$.
    ///
    /// Retourne `None` si le rayon est parallèle au plan ($|d_z| \le 10^{-6}$) ou si l'intersection
    /// se trouve derrière l'origine du regard dans le sens de propagation ($t < 0$).
    pub fn intersect_plane_z(&self, target_z: f32) -> Option<Point3D> {
        let dz = self.direction.z;
        if dz.abs() <= 1e-6 {
            return None;
        }

        // origin.z + t * dz = target_z => t = (target_z - origin.z) / dz
        let t = (target_z - self.origin.z) / dz;
        if t < 0.0 {
            // Le plan est dans la direction opposée au regard
            return None;
        }

        Some(self.point_at_distance(t))
    }

    /// Teste si le rayon intersecte l'écran spécifié et met à jour `self.is_focused_on_screen`.
    /// Retourne la valeur booléenne résultante.
    pub fn check_screen_intersection(&mut self, screen: &ScreenBounds) -> bool {
        let focused = self.is_looking_at_screen(screen);
        self.is_focused_on_screen = focused;
        focused
    }

    /// Teste si le rayon intersecte la surface de l'écran sans modifier l'état mutable.
    pub fn is_looking_at_screen(&self, screen: &ScreenBounds) -> bool {
        if let Some(intersection) = self.intersect_plane_z(screen.center.z) {
            screen.contains_point_2d(intersection.x, intersection.y)
        } else {
            false
        }
    }
}

impl Default for GazeRay3D {
    fn default() -> Self {
        Self {
            origin: Point3D::ORIGIN,
            direction: Point3D::new(0.0, 0.0, -1.0),
            is_focused_on_screen: false,
        }
    }
}

impl fmt::Display for GazeRay3D {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GazeRay3D(orig={}, dir={}, focused={})",
            self.origin, self.direction, self.is_focused_on_screen
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_bounds_containment() {
        let screen = ScreenBounds::new(Point3D::new(0.0, 0.15, 0.0), 0.50, 0.30);
        // Centre de l'écran
        assert!(screen.contains_point_2d(0.0, 0.15));
        // Coins
        assert!(screen.contains_point_2d(0.24, 0.29));
        assert!(screen.contains_point_2d(-0.24, 0.01));
        // Hors écran
        assert!(!screen.contains_point_2d(0.30, 0.15));
        assert!(!screen.contains_point_2d(0.0, 0.35));
    }

    #[test]
    fn test_gaze_ray_screen_focus_hit() {
        let screen = ScreenBounds::default_desktop_24inch();
        // Utilisateur situé à 60 cm (Z = 0.60 m) devant l'écran (Z = 0)
        // Yeux à (0.0, 0.10, 0.60) regardant vers le centre écran (0.0, 0.15, 0.0)
        let eye_pos = Point3D::new(0.0, 0.10, 0.60);
        let screen_target = Point3D::new(0.0, 0.15, 0.0);
        let dir_vec = (screen_target - eye_pos).normalize().expect("Valid dir");

        let mut gaze = GazeRay3D::from_vector(eye_pos, dir_vec, false);
        assert!(gaze.check_screen_intersection(&screen));
        assert!(gaze.is_focused_on_screen);

        let hit = gaze.intersect_plane_z(0.0).expect("Hit should exist");
        assert!((hit.x - screen_target.x).abs() < 1e-5);
        assert!((hit.y - screen_target.y).abs() < 1e-5);
        assert!((hit.z - screen_target.z).abs() < 1e-5);
    }

    #[test]
    fn test_gaze_ray_looking_away() {
        let screen = ScreenBounds::default_desktop_24inch();
        let eye_pos = Point3D::new(0.0, 0.10, 0.60);
        // Regard orienté vers la droite, très loin de l'écran (X = 1.0)
        let dir_vec = Vector3D::new(1.0, 0.0, -1.0).normalize().unwrap();

        let mut gaze = GazeRay3D::from_vector(eye_pos, dir_vec, true);
        assert!(!gaze.check_screen_intersection(&screen));
        assert!(!gaze.is_focused_on_screen);
    }

    #[test]
    fn test_gaze_ray_parallel_or_opposite() {
        let eye_pos = Point3D::new(0.0, 0.0, 0.60);
        // Regard parallèle au plan écran (dz = 0)
        let parallel = GazeRay3D::new(eye_pos, Point3D::new(1.0, 0.0, 0.0), false);
        assert!(parallel.intersect_plane_z(0.0).is_none());

        // Regard tournant le dos à l'écran (dz > 0, s'éloignant de Z=0)
        let opposite = GazeRay3D::new(eye_pos, Point3D::new(0.0, 0.0, 1.0), false);
        assert!(opposite.intersect_plane_z(0.0).is_none());
    }
}
