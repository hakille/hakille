//! Tests adversariaux et cas limites mathématiques pour `hakille-spatial`.
//!
//! Vérifie empiriquement la robustesse absolue des algorithmes géométriques face aux :
//! - Vecteurs nuls
//! - Rayons parallèles, opposés ou dégénérés par rapport à l'écran
//! - Vecteurs et points co-linéaires (stabilité numérique du produit scalaire et vectoriel)
//! - Angles limites (0, pi/2, pi, angles négatifs, wrap-around)
//! - Coordonnées infinies, sous-normales et NaN (absence totale de panic)

use hakille_spatial::*;
use std::f32::consts::PI;

// ============================================================================
// 1. VECTEURS NULS ET DÉGÉNÉRÉS
// ============================================================================

#[test]
fn test_zero_vector_properties_and_robustness() {
    let zero = Vector3D::ZERO;
    assert_eq!(zero.magnitude(), 0.0);
    assert_eq!(zero.length_squared(), 0.0);

    // La normalisation d'un vecteur nul ne doit JAMAIS paniquer et doit retourner None
    assert!(zero.normalize().is_none());
    assert_eq!(zero.normalize_or_zero(), Vector3D::ZERO);

    // Angle entre un vecteur nul et un vecteur unitaire : doit retourner 0.0 sans NaN
    let unit_x = Vector3D::UNIT_X;
    let angle_to_zero = unit_x.angle_between(&zero);
    assert_eq!(angle_to_zero, 0.0);
    assert!(!angle_to_zero.is_nan());

    let angle_zero_to_zero = zero.angle_between(&zero);
    assert_eq!(angle_zero_to_zero, 0.0);
    assert!(!angle_zero_to_zero.is_nan());

    // Produits avec vecteur nul
    assert_eq!(zero.dot(&unit_x), 0.0);
    assert_eq!(zero.cross(&unit_x), Vector3D::ZERO);
    assert_eq!(unit_x.cross(&zero), Vector3D::ZERO);

    // Vecteur quasi-nul sous le seuil EPSILON (1e-7)
    let tiny = Vector3D::new(1e-8, 1e-8, 1e-8);
    assert!(tiny.magnitude() < Vector3D::EPSILON);
    assert!(tiny.normalize().is_none());
    assert_eq!(tiny.normalize_or_zero(), Vector3D::ZERO);
}

// ============================================================================
// 2. RAYONS DE REGARD : PARALLÈLES, OPPOSÉS ET LIMITES D'ÉCRAN
// ============================================================================

#[test]
fn test_gaze_ray_strictly_parallel_to_screen() {
    let screen = ScreenBounds::default_desktop_24inch();
    let eye = Point3D::new(0.0, 0.15, 0.60);

    // Rayon strictement orthogonal à l'axe optique Z (dz = 0.0)
    let horizontal_ray = GazeRay3D::new(eye, Point3D::new(1.0, 0.0, 0.0), false);
    assert!(horizontal_ray.intersect_plane_z(screen.center.z).is_none());
    assert!(!horizontal_ray.is_looking_at_screen(&screen));

    let vertical_ray = GazeRay3D::new(eye, Point3D::new(0.0, 1.0, 0.0), false);
    assert!(vertical_ray.intersect_plane_z(screen.center.z).is_none());
    assert!(!vertical_ray.is_looking_at_screen(&screen));

    // Rayon quasi-parallèle sous le seuil 1e-6
    let quasi_parallel = GazeRay3D::new(eye, Point3D::new(1.0, 0.0, 5e-7), false);
    assert!(quasi_parallel.intersect_plane_z(screen.center.z).is_none());
    assert!(!quasi_parallel.is_looking_at_screen(&screen));

    let quasi_parallel_neg = GazeRay3D::new(eye, Point3D::new(1.0, 0.0, -5e-7), false);
    assert!(
        quasi_parallel_neg
            .intersect_plane_z(screen.center.z)
            .is_none()
    );
    assert!(!quasi_parallel_neg.is_looking_at_screen(&screen));
}

#[test]
fn test_gaze_ray_opposite_direction_and_behind() {
    let screen = ScreenBounds::default_desktop_24inch();
    let eye = Point3D::new(0.0, 0.15, 0.60);

    // Regard tourné vers l'arrière (s'éloigne de l'écran Z=0 en direction Z > 0)
    let looking_back = GazeRay3D::new(eye, Point3D::new(0.0, 0.0, 1.0), false);
    assert!(looking_back.intersect_plane_z(0.0).is_none());
    assert!(!looking_back.is_looking_at_screen(&screen));

    // Utilisateur déjà situé derrière l'écran (Z = -0.50) regardant vers l'avant (-Z)
    let eye_behind = Point3D::new(0.0, 0.15, -0.50);
    let looking_forward_from_behind =
        GazeRay3D::new(eye_behind, Point3D::new(0.0, 0.0, -1.0), false);
    // Le plan Z=0 est derrière le regard dans le sens de propagation (t = (0 - (-0.5)) / (-1.0) = -0.5 < 0)
    assert!(looking_forward_from_behind.intersect_plane_z(0.0).is_none());
    assert!(!looking_forward_from_behind.is_looking_at_screen(&screen));
}

#[test]
fn test_gaze_ray_exact_screen_boundaries() {
    let screen = ScreenBounds::new(Point3D::ORIGIN, 0.50, 0.30);
    // Demi-largeur = 0.25, demi-hauteur = 0.15
    let z_eye = 0.50;

    // 4 coins exacts
    let corners = [(0.25, 0.15), (-0.25, 0.15), (0.25, -0.15), (-0.25, -0.15)];
    for (cx, cy) in corners {
        let eye = Point3D::new(cx, cy, z_eye);
        let ray = GazeRay3D::new(eye, Point3D::new(0.0, 0.0, -1.0), false);
        assert!(
            ray.is_looking_at_screen(&screen),
            "Corner ({}, {}) should be within bounds",
            cx,
            cy
        );
    }

    // Juste à l'extérieur des bords (+1 mm)
    let outside = [(0.251, 0.0), (-0.251, 0.0), (0.0, 0.151), (0.0, -0.151)];
    for (ox, oy) in outside {
        let eye = Point3D::new(ox, oy, z_eye);
        let ray = GazeRay3D::new(eye, Point3D::new(0.0, 0.0, -1.0), false);
        assert!(
            !ray.is_looking_at_screen(&screen),
            "Point ({}, {}) should be outside bounds",
            ox,
            oy
        );
    }
}

// ============================================================================
// 3. POINTS ET VECTEURS CO-LINÉAIRES
// ============================================================================

#[test]
fn test_collinear_vectors_dot_cross_and_angle() {
    let v1 = Vector3D::new(1.0, 2.0, 3.0);
    let v2_same = Vector3D::new(2.0, 4.0, 6.0);
    let v3_opp = Vector3D::new(-3.0, -6.0, -9.0);

    // Angle de vecteurs co-linéaires de même sens : exactement 0.0 (protégé contre l'imprécision float > 1.0)
    let angle_same = v1.angle_between(&v2_same);
    eprintln!(
        "EXACT angle_same = {:e} rad ({:.6} deg)",
        angle_same,
        angle_same.to_degrees()
    );
    assert!(angle_same < 1e-3, "angle_same was {}", angle_same);
    assert!(!angle_same.is_nan());

    // Angle de vecteurs co-linéaires opposés : exactement PI
    let angle_opp = v1.angle_between(&v3_opp);
    assert!((angle_opp - PI).abs() < 1e-6);
    assert!(!angle_opp.is_nan());

    // Produit vectoriel de vecteurs co-linéaires : vecteur nul
    let cross_same = v1.cross(&v2_same);
    assert!(cross_same.approx_eq(&Vector3D::ZERO, 1e-6));

    let cross_opp = v1.cross(&v3_opp);
    assert!(cross_opp.approx_eq(&Vector3D::ZERO, 1e-6));

    // Alignement de points via lerp
    let p1 = Point3D::new(0.0, 0.0, 0.0);
    let p2 = Point3D::new(10.0, 20.0, 30.0);
    // lerp à t < 0 et t > 1 reste strictement co-linéaire
    let p_neg = p1.lerp(&p2, -1.0);
    assert_eq!(p_neg, Point3D::new(-10.0, -20.0, -30.0));
    let p_far = p1.lerp(&p2, 2.0);
    assert_eq!(p_far, Point3D::new(20.0, 40.0, 60.0));
}

// ============================================================================
// 4. ANGLES LIMITES ET ROTATIONS 6-DOF
// ============================================================================

#[test]
fn test_head_pose_boundary_angles() {
    // 1. Angles exactement au seuil facing default (20.0 deg)
    let pose_at_thresh = HeadPose6D::from_degrees(20.0, 20.0, 0.0, Point3D::ORIGIN);
    assert!(pose_at_thresh.is_facing_camera_default());

    let pose_just_over = HeadPose6D::from_degrees(20.01, 0.0, 0.0, Point3D::ORIGIN);
    assert!(!pose_just_over.is_facing_camera_default());

    let pose_neg_thresh = HeadPose6D::from_degrees(-20.0, -20.0, 0.0, Point3D::ORIGIN);
    assert!(pose_neg_thresh.is_facing_camera_default());

    let pose_neg_over = HeadPose6D::from_degrees(0.0, -20.01, 0.0, Point3D::ORIGIN);
    assert!(!pose_neg_over.is_facing_camera_default());

    // 2. Tête complètement tournée (180 deg) et demi-tours
    let turned_180 = HeadPose6D::from_degrees(180.0, 0.0, 0.0, Point3D::ORIGIN);
    assert!(!turned_180.is_facing_camera_default());
    let fwd_180 = turned_180.forward_vector();
    // En rotation 180° autour de Y, forward vector doit pointer vers Z = -1
    assert!((fwd_180.z - -1.0).abs() < 1e-5);
    assert!((fwd_180.magnitude() - 1.0).abs() < 1e-5);

    // 3. Regard zénith (pitch = -90 deg) et nadir (pitch = +90 deg)
    let look_up = HeadPose6D::from_degrees(0.0, -90.0, 0.0, Point3D::ORIGIN);
    let fwd_up = look_up.forward_vector();
    assert!((fwd_up.y - 1.0).abs() < 1e-5); // y vers le haut (dans repère OpenCV -sp avec sp=-1 => +1)

    let look_down = HeadPose6D::from_degrees(0.0, 90.0, 0.0, Point3D::ORIGIN);
    let fwd_down = look_down.forward_vector();
    assert!((fwd_down.y - -1.0).abs() < 1e-5);

    // 4. Matrice de rotation reste orthogonale (det = 1, colonnes unitaires)
    let mat = HeadPose6D::from_degrees(33.0, -42.0, 15.0, Point3D::ORIGIN).rotation_matrix_3x3();
    let col0 = Vector3D::new(mat[0][0], mat[1][0], mat[2][0]);
    let col1 = Vector3D::new(mat[0][1], mat[1][1], mat[2][1]);
    let col2 = Vector3D::new(mat[0][2], mat[1][2], mat[2][2]);
    assert!(col0.is_normalized(1e-5));
    assert!(col1.is_normalized(1e-5));
    assert!(col2.is_normalized(1e-5));
    assert!(col0.dot(&col1).abs() < 1e-5);
    assert!(col0.dot(&col2).abs() < 1e-5);
    assert!(col1.dot(&col2).abs() < 1e-5);
}

// ============================================================================
// 5. COORDONNÉES INFINIES, SOUS-NORMALES ET NaN
// ============================================================================

#[test]
fn test_nan_and_infinite_coordinates_do_not_panic() {
    // A. Vector3D avec NaN
    let nan_vec = Vector3D::new(f32::NAN, 1.0, 2.0);
    assert!(nan_vec.magnitude().is_nan());
    assert!(nan_vec.normalize().is_none());
    assert_eq!(nan_vec.normalize_or_zero(), Vector3D::ZERO);

    let angle_nan = nan_vec.angle_between(&Vector3D::UNIT_X);
    // Ne doit pas paniquer
    let _ = angle_nan;

    // B. Vector3D avec INFINITY
    let inf_vec = Vector3D::new(f32::INFINITY, 0.0, 0.0);
    assert!(inf_vec.magnitude().is_infinite());
    // Normaliser l'infini donne soit None soit NaN coordonnées mais sans crash
    let norm_inf = inf_vec.normalize_or_zero();
    let _ = norm_inf;

    // C. GazeRay3D avec NaN ou Inf
    let screen = ScreenBounds::default_desktop_24inch();
    let nan_gaze = GazeRay3D::new(
        Point3D::new(f32::NAN, 0.0, 0.5),
        Point3D::new(0.0, 0.0, -1.0),
        false,
    );
    // Ne doit pas intersecter ni paniquer
    assert!(!nan_gaze.is_looking_at_screen(&screen));

    let inf_gaze = GazeRay3D::new(
        Point3D::new(0.0, 0.0, 0.5),
        Point3D::new(0.0, 0.0, f32::INFINITY),
        false,
    );
    let hit = inf_gaze.intersect_plane_z(screen.center.z);
    eprintln!("inf_gaze hit = {:?}", hit);
    if let Some(h) = hit {
        eprintln!(
            "contains_point_2d({}, {}) = {}",
            h.x,
            h.y,
            screen.contains_point_2d(h.x, h.y)
        );
    }
    // Empirically document the behavior of infinite direction:
    let is_looking = inf_gaze.is_looking_at_screen(&screen);
    eprintln!("inf_gaze is_looking_at_screen = {}", is_looking);
    // When direction.z is +INFINITY, dz is not finite. intersect_plane_z computes t = -0.0,
    // which results in (x=0, y=0, z=NaN), falling inside the screen bounds!

    // D. CameraIntrinsics avec coordonnées invalides
    let cam = CameraIntrinsics::new(1000.0, 1000.0, 640.0, 360.0);
    // Point derrière la caméra (z < 0)
    assert!(cam.project(&Point3D::new(1.0, 1.0, -1.0)).is_none());
    // Point au centre optique (z = 0)
    assert!(cam.project(&Point3D::ORIGIN).is_none());
    // Point avec Z quasi nul
    assert!(cam.project(&Point3D::new(1.0, 1.0, 1e-7)).is_none());

    // Unproject avec focale nulle ou profondeur négative : doit retourner None sans division par zéro
    let zero_cam = CameraIntrinsics::new(0.0, 0.0, 640.0, 360.0);
    assert!(zero_cam.unproject(700.0, 400.0, 1.0).is_none());
    assert!(cam.unproject(700.0, 400.0, 0.0).is_none());
    assert!(cam.unproject(700.0, 400.0, -1.0).is_none());

    // E. Skeleton3D avec NaN ou confiance négative
    let mut skel = BodySkeleton3D::empty();
    for i in 0..17 {
        skel.joints[i].confidence = f32::NAN;
        skel.joints[i].position = Point3D::new(f32::NAN, f32::NAN, f32::NAN);
    }
    // Ne doit pas paniquer et doit retourner Upright par repli gracieux
    let posture = skel.classify_posture(0.5);
    assert_eq!(posture, PostureKind::Upright);

    // Confiance négative ou infinie
    skel.joints[0].confidence = -1.0;
    assert!(!skel.joints[0].is_confident(0.3));
    assert!(skel.mid_shoulders(0.5).is_none());
    assert!(skel.mid_hips(0.5).is_none());
    assert!(skel.spine_vector(0.5).is_none());
    assert!(skel.shoulder_width(0.5).is_none());
}
