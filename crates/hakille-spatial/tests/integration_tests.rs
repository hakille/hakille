//! Tests d'intégration et de sérialisation JSON pour `hakille-spatial`.
//!
//! Valide la robustesse des calculs 3D sans aucune dépendance externe,
//! ainsi que la compatibilité avec le protocole IPC de Hakille via `serde_json`.

#[cfg(test)]
mod tests {
    use hakille_spatial::*;

    #[test]
    fn test_point_vector_roundtrip_serde() {
        let p = Point3D::new(1.23, -4.56, 7.89);
        let serialized = serde_json::to_string(&p).expect("Serialize Point3D");
        let deserialized: Point3D = serde_json::from_str(&serialized).expect("Deserialize Point3D");
        assert_eq!(p, deserialized);

        let v = Vector3D::new(-0.12, 3.45, -6.78);
        let v_ser = serde_json::to_string(&v).expect("Serialize Vector3D");
        let v_de: Vector3D = serde_json::from_str(&v_ser).expect("Deserialize Vector3D");
        assert_eq!(v, v_de);
    }

    #[test]
    fn test_head_pose_serde_and_facing() {
        let pose = HeadPose6D::from_degrees(12.0, -8.0, 3.0, Point3D::new(0.05, -0.02, 0.65));
        let ser = serde_json::to_string(&pose).expect("Serialize HeadPose6D");
        let de: HeadPose6D = serde_json::from_str(&ser).expect("Deserialize HeadPose6D");

        assert_eq!(pose, de);
        assert!(de.is_facing_camera_default());
        assert_eq!(de.translation, Point3D::new(0.05, -0.02, 0.65));
    }

    #[test]
    fn test_skeleton_serde_and_posture_kinds() {
        let mut skel = BodySkeleton3D::empty();
        skel.posture = PostureKind::Slouched;
        skel.joint_mut(CocoJointId::Nose).position = Point3D::new(0.0, 0.2, 0.5);
        skel.joint_mut(CocoJointId::Nose).confidence = 0.95;

        let ser = serde_json::to_string(&skel).expect("Serialize BodySkeleton3D");
        let de: BodySkeleton3D = serde_json::from_str(&ser).expect("Deserialize BodySkeleton3D");

        assert_eq!(skel, de);
        assert_eq!(de.posture, PostureKind::Slouched);
        assert_eq!(de.joint(CocoJointId::Nose).confidence, 0.95);

        // Test de sérialisation de chaque variante de PostureKind
        for posture in [
            PostureKind::Upright,
            PostureKind::Slouched,
            PostureKind::LeaningForward,
            PostureKind::Reclined,
            PostureKind::Standing,
            PostureKind::Seated,
        ] {
            let p_ser = serde_json::to_string(&posture).unwrap();
            let p_de: PostureKind = serde_json::from_str(&p_ser).unwrap();
            assert_eq!(posture, p_de);
        }
    }

    #[test]
    fn test_gaze_ray_serde_and_screen_intersection() {
        let screen = ScreenBounds::default_desktop_24inch();
        let ray = GazeRay3D::new(
            Point3D::new(0.0, 0.0, 0.6),
            Point3D::new(0.0, 0.25, -1.0),
            true,
        );

        let ser = serde_json::to_string(&ray).expect("Serialize GazeRay3D");
        let de: GazeRay3D = serde_json::from_str(&ser).expect("Deserialize GazeRay3D");

        assert_eq!(ray, de);
        assert!(de.is_focused_on_screen);

        let s_ser = serde_json::to_string(&screen).expect("Serialize ScreenBounds");
        let s_de: ScreenBounds = serde_json::from_str(&s_ser).expect("Deserialize ScreenBounds");
        assert_eq!(screen, s_de);
    }

    #[test]
    fn test_end_to_end_spatial_reasoning() {
        // Scénario complet : utilisateur assis devant son poste
        // 1. Détection de pose de tête
        let head_pose = HeadPose6D::from_degrees(5.0, -10.0, 0.0, Point3D::new(0.0, -0.05, 0.58));
        assert!(head_pose.is_facing_camera_default());

        // 2. Détection de squelette COCO
        let mut skeleton = BodySkeleton3D::empty();
        skeleton.joint_mut(CocoJointId::LeftShoulder).position = Point3D::new(-0.20, 0.25, 0.58);
        skeleton.joint_mut(CocoJointId::LeftShoulder).confidence = 0.95;
        skeleton.joint_mut(CocoJointId::RightShoulder).position = Point3D::new(0.20, 0.25, 0.58);
        skeleton.joint_mut(CocoJointId::RightShoulder).confidence = 0.95;
        skeleton.joint_mut(CocoJointId::LeftHip).position = Point3D::new(-0.15, 0.75, 0.58);
        skeleton.joint_mut(CocoJointId::LeftHip).confidence = 0.92;
        skeleton.joint_mut(CocoJointId::RightHip).position = Point3D::new(0.15, 0.75, 0.58);
        skeleton.joint_mut(CocoJointId::RightHip).confidence = 0.92;
        skeleton.joint_mut(CocoJointId::Nose).position = Point3D::new(0.0, 0.0, 0.55);
        skeleton.joint_mut(CocoJointId::Nose).confidence = 0.98;

        let posture = skeleton.classify_posture(0.5);
        assert_eq!(posture, PostureKind::Upright);

        // 3. Rayon de regard
        let screen = ScreenBounds::default_desktop_24inch();
        let eye_origin = Point3D::new(0.0, 0.0, 0.55);
        let screen_target = Point3D::new(0.05, 0.15, 0.0);
        let gaze_dir = (screen_target - eye_origin).normalize().unwrap();

        let mut gaze_ray = GazeRay3D::from_vector(eye_origin, gaze_dir, false);
        let focused = gaze_ray.check_screen_intersection(&screen);
        assert!(focused);
        assert!(gaze_ray.is_focused_on_screen);
    }
}
