//! Primitives cartésiennes 3D : [`Point3D`] et [`Vector3D`].

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

/// Point dans l'espace euclidien 3D.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point3D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Point3D {
    /// Origine du repère cartésien (0, 0, 0).
    pub const ORIGIN: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// Crée un nouveau point à partir des coordonnées x, y, z.
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Retourne l'origine du repère (0, 0, 0).
    pub fn origin() -> Self {
        Self::ORIGIN
    }

    /// Calcule la distance euclidienne entre deux points : $\sqrt{(x_2-x_1)^2 + (y_2-y_1)^2 + (z_2-z_1)^2}$.
    pub fn distance_to(&self, other: &Point3D) -> f32 {
        self.distance_squared(other).sqrt()
    }

    /// Calcule le carré de la distance euclidienne (évite l'appel coûteux à sqrt).
    pub fn distance_squared(&self, other: &Point3D) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        dx * dx + dy * dy + dz * dz
    }

    /// Calcule le point médian entre deux points.
    pub fn midpoint(&self, other: &Point3D) -> Point3D {
        Point3D {
            x: (self.x + other.x) * 0.5,
            y: (self.y + other.y) * 0.5,
            z: (self.z + other.z) * 0.5,
        }
    }

    /// Interpolation linéaire entre deux points avec paramètre t in [0.0, 1.0].
    pub fn lerp(&self, other: &Point3D, t: f32) -> Point3D {
        Point3D {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
            z: self.z + (other.z - self.z) * t,
        }
    }

    /// Convertit ce point en vecteur depuis l'origine.
    pub fn to_vector(&self) -> Vector3D {
        Vector3D::new(self.x, self.y, self.z)
    }

    /// Teste l'égalité approximative avec un seuil epsilon.
    pub fn approx_eq(&self, other: &Point3D, epsilon: f32) -> bool {
        (self.x - other.x).abs() <= epsilon
            && (self.y - other.y).abs() <= epsilon
            && (self.z - other.z).abs() <= epsilon
    }
}

impl Default for Point3D {
    fn default() -> Self {
        Self::ORIGIN
    }
}

impl fmt::Display for Point3D {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Point3D({:.3}, {:.3}, {:.3})", self.x, self.y, self.z)
    }
}

impl From<(f32, f32, f32)> for Point3D {
    fn from((x, y, z): (f32, f32, f32)) -> Self {
        Self::new(x, y, z)
    }
}

impl From<[f32; 3]> for Point3D {
    fn from(arr: [f32; 3]) -> Self {
        Self::new(arr[0], arr[1], arr[2])
    }
}

impl From<Point3D> for (f32, f32, f32) {
    fn from(p: Point3D) -> Self {
        (p.x, p.y, p.z)
    }
}

impl From<Point3D> for [f32; 3] {
    fn from(p: Point3D) -> Self {
        [p.x, p.y, p.z]
    }
}

impl Sub<Point3D> for Point3D {
    type Output = Vector3D;

    fn sub(self, rhs: Point3D) -> Self::Output {
        Vector3D::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Add<Vector3D> for Point3D {
    type Output = Point3D;

    fn add(self, rhs: Vector3D) -> Self::Output {
        Point3D::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub<Vector3D> for Point3D {
    type Output = Point3D;

    fn sub(self, rhs: Vector3D) -> Self::Output {
        Point3D::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl AddAssign<Vector3D> for Point3D {
    fn add_assign(&mut self, rhs: Vector3D) {
        self.x += rhs.x;
        self.y += rhs.y;
        self.z += rhs.z;
    }
}

impl SubAssign<Vector3D> for Point3D {
    fn sub_assign(&mut self, rhs: Vector3D) {
        self.x -= rhs.x;
        self.y -= rhs.y;
        self.z -= rhs.z;
    }
}

/// Vecteur dans l'espace euclidien 3D.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vector3D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vector3D {
    /// Vecteur nul (0, 0, 0).
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    /// Vecteur unitaire axe X (1, 0, 0).
    pub const UNIT_X: Self = Self {
        x: 1.0,
        y: 0.0,
        z: 0.0,
    };
    /// Vecteur unitaire axe Y (0, 1, 0).
    pub const UNIT_Y: Self = Self {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };
    /// Vecteur unitaire axe Z (0, 0, 1).
    pub const UNIT_Z: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };

    /// Seuil numérique minimal pour éviter les divisions par zéro lors de la normalisation.
    pub const EPSILON: f32 = 1e-7;

    /// Crée un nouveau vecteur à partir de ses composantes.
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Retourne le vecteur nul.
    pub fn zero() -> Self {
        Self::ZERO
    }

    /// Calcule la norme euclidienne (longueur) du vecteur : $\sqrt{x^2 + y^2 + z^2}$.
    pub fn magnitude(&self) -> f32 {
        self.length_squared().sqrt()
    }

    /// Calcule le carré de la norme euclidienne : $x^2 + y^2 + z^2$.
    pub fn length_squared(&self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// Calcule le produit scalaire (dot product) : $x_1 x_2 + y_1 y_2 + z_1 z_2$.
    pub fn dot(&self, other: &Vector3D) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// Calcule le produit vectoriel (cross product) : $\vec{u} \times \vec{v}$.
    pub fn cross(&self, other: &Vector3D) -> Vector3D {
        Vector3D {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    /// Normalise le vecteur pour obtenir un vecteur unitaire (longueur = 1).
    /// Retourne `None` si le vecteur est trop proche de zéro (< EPSILON).
    pub fn normalize(&self) -> Option<Vector3D> {
        let mag = self.magnitude();
        if mag > Self::EPSILON {
            let inv = 1.0 / mag;
            Some(Vector3D::new(self.x * inv, self.y * inv, self.z * inv))
        } else {
            None
        }
    }

    /// Normalise le vecteur ou retourne le vecteur nul si dégénéré.
    pub fn normalize_or_zero(&self) -> Vector3D {
        self.normalize().unwrap_or(Self::ZERO)
    }

    /// Vérifie si le vecteur est unitaire (norme proche de 1.0 avec tolérance epsilon).
    pub fn is_normalized(&self, epsilon: f32) -> bool {
        (self.length_squared() - 1.0).abs() <= epsilon
    }

    /// Calcule l'angle non orienté en radians entre deux vecteurs $\in [0, \pi]$.
    pub fn angle_between(&self, other: &Vector3D) -> f32 {
        let denom = self.magnitude() * other.magnitude();
        if denom <= Self::EPSILON {
            return 0.0;
        }
        let cos_theta = (self.dot(other) / denom).clamp(-1.0, 1.0);
        cos_theta.acos()
    }

    /// Convertit ce vecteur en point cartésien.
    pub fn to_point(&self) -> Point3D {
        Point3D::new(self.x, self.y, self.z)
    }

    /// Teste l'égalité approximative avec un seuil epsilon.
    pub fn approx_eq(&self, other: &Vector3D, epsilon: f32) -> bool {
        (self.x - other.x).abs() <= epsilon
            && (self.y - other.y).abs() <= epsilon
            && (self.z - other.z).abs() <= epsilon
    }
}

impl Default for Vector3D {
    fn default() -> Self {
        Self::ZERO
    }
}

impl fmt::Display for Vector3D {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Vector3D({:.3}, {:.3}, {:.3})", self.x, self.y, self.z)
    }
}

impl From<(f32, f32, f32)> for Vector3D {
    fn from((x, y, z): (f32, f32, f32)) -> Self {
        Self::new(x, y, z)
    }
}

impl From<[f32; 3]> for Vector3D {
    fn from(arr: [f32; 3]) -> Self {
        Self::new(arr[0], arr[1], arr[2])
    }
}

impl From<Point3D> for Vector3D {
    fn from(p: Point3D) -> Self {
        Vector3D::new(p.x, p.y, p.z)
    }
}

impl From<Vector3D> for Point3D {
    fn from(v: Vector3D) -> Self {
        Point3D::new(v.x, v.y, v.z)
    }
}

impl Add<Vector3D> for Vector3D {
    type Output = Vector3D;

    fn add(self, rhs: Vector3D) -> Self::Output {
        Vector3D::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub<Vector3D> for Vector3D {
    type Output = Vector3D;

    fn sub(self, rhs: Vector3D) -> Self::Output {
        Vector3D::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Neg for Vector3D {
    type Output = Vector3D;

    fn neg(self) -> Self::Output {
        Vector3D::new(-self.x, -self.y, -self.z)
    }
}

impl Mul<f32> for Vector3D {
    type Output = Vector3D;

    fn mul(self, rhs: f32) -> Self::Output {
        Vector3D::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl Mul<Vector3D> for f32 {
    type Output = Vector3D;

    fn mul(self, rhs: Vector3D) -> Self::Output {
        Vector3D::new(rhs.x * self, rhs.y * self, rhs.z * self)
    }
}

impl Div<f32> for Vector3D {
    type Output = Vector3D;

    fn div(self, rhs: f32) -> Self::Output {
        let inv = 1.0 / rhs;
        Vector3D::new(self.x * inv, self.y * inv, self.z * inv)
    }
}

impl AddAssign<Vector3D> for Vector3D {
    fn add_assign(&mut self, rhs: Vector3D) {
        self.x += rhs.x;
        self.y += rhs.y;
        self.z += rhs.z;
    }
}

impl SubAssign<Vector3D> for Vector3D {
    fn sub_assign(&mut self, rhs: Vector3D) {
        self.x -= rhs.x;
        self.y -= rhs.y;
        self.z -= rhs.z;
    }
}

impl MulAssign<f32> for Vector3D {
    fn mul_assign(&mut self, rhs: f32) {
        self.x *= rhs;
        self.y *= rhs;
        self.z *= rhs;
    }
}

impl DivAssign<f32> for Vector3D {
    fn div_assign(&mut self, rhs: f32) {
        let inv = 1.0 / rhs;
        self.x *= inv;
        self.y *= inv;
        self.z *= inv;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_distance_2d_and_3d() {
        let p1 = Point3D::new(0.0, 0.0, 0.0);
        let p2 = Point3D::new(3.0, 4.0, 0.0);
        assert!((p1.distance_to(&p2) - 5.0).abs() < 1e-6);
        assert!((p1.distance_squared(&p2) - 25.0).abs() < 1e-6);

        let p3 = Point3D::new(1.0, 2.0, 3.0);
        let p4 = Point3D::new(4.0, 6.0, 3.0);
        assert!((p3.distance_to(&p4) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_point_midpoint_and_lerp() {
        let p1 = Point3D::new(0.0, 10.0, 20.0);
        let p2 = Point3D::new(10.0, 20.0, 40.0);
        let mid = p1.midpoint(&p2);
        assert_eq!(mid, Point3D::new(5.0, 15.0, 30.0));

        let lerp25 = p1.lerp(&p2, 0.25);
        assert_eq!(lerp25, Point3D::new(2.5, 12.5, 25.0));
    }

    #[test]
    fn test_point_vector_arithmetic() {
        let p = Point3D::new(1.0, 2.0, 3.0);
        let v = Vector3D::new(4.0, 5.0, 6.0);
        let p_plus_v = p + v;
        assert_eq!(p_plus_v, Point3D::new(5.0, 7.0, 9.0));

        let p_minus_p = p_plus_v - p;
        assert_eq!(p_minus_p, v);

        let mut p_mut = p;
        p_mut += v;
        assert_eq!(p_mut, p_plus_v);
    }

    #[test]
    fn test_vector_magnitude_and_normalization() {
        let v = Vector3D::new(3.0, 4.0, 12.0);
        assert!((v.magnitude() - 13.0).abs() < 1e-6);
        assert!((v.length_squared() - 169.0).abs() < 1e-6);

        let norm = v.normalize().expect("Normalization should succeed");
        assert!((norm.magnitude() - 1.0).abs() < 1e-6);
        assert!(norm.is_normalized(1e-5));
        assert!((norm.x - 3.0 / 13.0).abs() < 1e-6);
        assert!((norm.y - 4.0 / 13.0).abs() < 1e-6);
        assert!((norm.z - 12.0 / 13.0).abs() < 1e-6);

        let zero = Vector3D::ZERO;
        assert!(zero.normalize().is_none());
        assert_eq!(zero.normalize_or_zero(), Vector3D::ZERO);
    }

    #[test]
    fn test_vector_dot_and_cross_product() {
        let x = Vector3D::UNIT_X;
        let y = Vector3D::UNIT_Y;
        let z = Vector3D::UNIT_Z;

        assert_eq!(x.dot(&y), 0.0);
        assert_eq!(x.dot(&x), 1.0);

        let cross_xy = x.cross(&y);
        assert_eq!(cross_xy, z);

        let cross_yz = y.cross(&z);
        assert_eq!(cross_yz, x);

        let cross_zx = z.cross(&x);
        assert_eq!(cross_zx, y);
    }

    #[test]
    fn test_vector_angle_between() {
        let x = Vector3D::UNIT_X;
        let y = Vector3D::UNIT_Y;
        let angle = x.angle_between(&y);
        assert!((angle - std::f32::consts::FRAC_PI_2).abs() < 1e-6);

        let opp = -x;
        let angle_opp = x.angle_between(&opp);
        assert!((angle_opp - std::f32::consts::PI).abs() < 1e-6);
    }

    #[test]
    fn test_vector_arithmetic() {
        let v1 = Vector3D::new(1.0, 2.0, 3.0);
        let v2 = Vector3D::new(4.0, 5.0, 6.0);
        assert_eq!(v1 + v2, Vector3D::new(5.0, 7.0, 9.0));
        assert_eq!(v2 - v1, Vector3D::new(3.0, 3.0, 3.0));
        assert_eq!(-v1, Vector3D::new(-1.0, -2.0, -3.0));
        assert_eq!(v1 * 2.0, Vector3D::new(2.0, 4.0, 6.0));
        assert_eq!(2.0 * v1, Vector3D::new(2.0, 4.0, 6.0));
        assert_eq!(v1 / 2.0, Vector3D::new(0.5, 1.0, 1.5));
    }
}
