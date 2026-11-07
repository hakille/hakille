//! Tests adversariaux et d'intégration pour le Jalon M3 (Inférence Cognitive Multimodale)
//!
//! Vérifie :
//! 1. La sécurité multi-thread sans interblocage lors des inférences concurrentes (FaceMesh, HSEmotion, SSR-Net, Pose).
//! 2. La robustesse aux images dégénérées (image noire, saturation blanche, bruit, ratios extrêmes).
//! 3. L'application stricte et inviolable de la Règle Constitutionnelle 7 (protection des mineurs).
//! 4. La cohérence des métriques biométriques (EAR, MAR, IPD, angle du cou, posture).

use hakille_inference::{
    facemesh_onnx::{FaceMeshIrisSession, FACEMESH_POINTS_COUNT},
    hsemotion_onnx::HSEmotionSession,
    pose_onnx::PoseSession,
    ssrnet_onnx::{build_age_result, SSRNetSession},
    ModelStatus,
};
use hakille_spatial::PostureKind;
use std::sync::Arc;
use std::thread;

#[test]
fn test_m3_concurrent_inference_deadlock_free() {
    let facemesh = Arc::new(FaceMeshIrisSession::open_or_simulated(None));
    let hsemotion = Arc::new(HSEmotionSession::open_or_simulated(None));
    let ssrnet = Arc::new(SSRNetSession::open_or_simulated(None));
    let pose = Arc::new(PoseSession::open_or_simulated(None));

    let mut handles = Vec::new();

    for t in 0..8 {
        let fm = Arc::clone(&facemesh);
        let em = Arc::clone(&hsemotion);
        let sn = Arc::clone(&ssrnet);
        let ps = Arc::clone(&pose);

        let handle = thread::spawn(move || {
            let dummy_frame = vec![((t * 30) % 255) as u8; 320 * 240 * 3];

            // 1. FaceMesh
            let fm_res = fm.infer(&dummy_frame, 320, 240).expect("facemesh infer ok");
            assert_eq!(fm_res.landmarks.len(), FACEMESH_POINTS_COUNT);

            // 2. HSEmotion
            let em_res = em.infer(&dummy_frame, 320, 240).expect("hsemotion infer ok");
            assert!(em_res.confidence > 0.0);

            // 3. SSR-Net
            let sn_res = sn.infer(&dummy_frame, 320, 240).expect("ssrnet infer ok");
            assert!(sn_res.confidence > 0.0);

            // 4. Pose
            let ps_res = ps.infer(&dummy_frame, 320, 240).expect("pose infer ok");
            assert_eq!(ps_res.skeleton.joints.len(), 17);
        });

        handles.push(handle);
    }

    for h in handles {
        h.join().expect("thread completed without panic or deadlock");
    }
}

#[test]
fn test_m3_adversarial_degraded_inputs() {
    let facemesh = FaceMeshIrisSession::open_or_simulated(None);
    let hsemotion = HSEmotionSession::open_or_simulated(None);
    let ssrnet = SSRNetSession::open_or_simulated(None);
    let pose = PoseSession::open_or_simulated(None);

    // Image totalement noire (0 lumens)
    let black_frame = vec![0u8; 192 * 192 * 3];
    assert!(facemesh.infer(&black_frame, 192, 192).is_ok());
    assert!(hsemotion.infer(&black_frame, 192, 192).is_ok());
    assert!(ssrnet.infer(&black_frame, 192, 192).is_ok());
    assert!(pose.infer(&black_frame, 192, 192).is_ok());

    // Image totalement blanche (255 saturation)
    let white_frame = vec![255u8; 224 * 224 * 3];
    assert!(facemesh.infer(&white_frame, 224, 224).is_ok());
    assert!(hsemotion.infer(&white_frame, 224, 224).is_ok());
    assert!(ssrnet.infer(&white_frame, 224, 224).is_ok());
    assert!(pose.infer(&white_frame, 224, 224).is_ok());

    // Ratios d'aspect extrêmes (bandeau horizontal 1000 x 20)
    let wide_frame = vec![128u8; 1000 * 20 * 3];
    assert!(facemesh.infer(&wide_frame, 1000, 20).is_ok());
    assert!(hsemotion.infer(&wide_frame, 1000, 20).is_ok());
    assert!(ssrnet.infer(&wide_frame, 1000, 20).is_ok());
    assert!(pose.infer(&wide_frame, 1000, 20).is_ok());

    // Ratios d'aspect extrêmes (colonne verticale 20 x 1000)
    let tall_frame = vec![128u8; 20 * 1000 * 3];
    assert!(facemesh.infer(&tall_frame, 20, 1000).is_ok());
    assert!(hsemotion.infer(&tall_frame, 20, 1000).is_ok());
    assert!(ssrnet.infer(&tall_frame, 20, 1000).is_ok());
    assert!(pose.infer(&tall_frame, 20, 1000).is_ok());

    // Entrées invalides -> erreur explicite, pas de panic
    assert!(facemesh.infer(&[], 0, 0).is_err());
    assert!(hsemotion.infer(&[], 0, 0).is_err());
    assert!(ssrnet.infer(&[], 0, 0).is_err());
    assert!(pose.infer(&[], 0, 0).is_err());
}

#[test]
fn test_m3_rule_7_constitutional_minor_protection_spectrum() {
    // Spectre complet d'âge de 5 ans à 80 ans
    let test_ages = [
        (5.0, true),
        (10.0, true),
        (14.0, true),
        (16.5, true),
        (17.9, true),
        (18.0, false),  // 18.0 ans = majorité légale
        (18.5, false),  // Au-delà de la marge
        (25.0, false),
        (40.0, false),
        (75.0, false),
    ];

    for (age, expected_minor) in test_ages {
        let res = build_age_result(age, 0.90, ModelStatus::Simulated);
        assert_eq!(
            res.is_minor, expected_minor,
            "Âge {} ans attendu mineur={} mais obtenu={}",
            age, expected_minor, res.is_minor
        );

        if age <= 16.0 {
            assert!(res.minor_probability > 0.70);
        } else if age >= 25.0 {
            assert!(res.minor_probability < 0.05);
        }
    }
}

#[test]
fn test_m3_biometric_facial_and_posture_consistency() {
    let facemesh = FaceMeshIrisSession::open_or_simulated(None);
    let pose = PoseSession::open_or_simulated(None);

    let dummy_img = vec![128u8; 640 * 480 * 3];
    let fm_out = facemesh.infer(&dummy_img, 640, 480).expect("facemesh infer");
    let pose_out = pose.infer(&dummy_img, 640, 480).expect("pose infer");

    // 1. EAR dans les bornes physiologiques humaines [0.05, 0.50]
    assert!(fm_out.left_ear >= 0.05 && fm_out.left_ear <= 0.50);
    assert!(fm_out.right_ear >= 0.05 && fm_out.right_ear <= 0.50);

    // 2. MAR au repos dans les bornes [0.05, 0.40]
    assert!(fm_out.mar >= 0.05 && fm_out.mar <= 0.40);

    // 3. Distance inter-pupillaire positive et raisonnable
    assert!(fm_out.ipd_distance > 10.0 && fm_out.ipd_distance < 640.0);

    // 4. Squelette COCO 17 points valide
    assert_eq!(pose_out.skeleton.joints.len(), 17);
    assert_eq!(pose_out.posture, PostureKind::Seated);
    assert!(pose_out.neck_angle_deg >= 0.0 && pose_out.neck_angle_deg <= 90.0);
}
