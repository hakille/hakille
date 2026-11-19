//! Tests adversariaux et d'intégration pour le Jalon M3 (Inférence Cognitive Multimodale)
//!
//! Vérifie :
//! 1. La sécurité multi-thread sans interblocage lors des inférences concurrentes (FaceMesh, HSEmotion, SSR-Net, Pose).
//! 2. La robustesse aux images dégénérées (image noire, saturation blanche, bruit, ratios extrêmes).
//! 3. L'application stricte et inviolable de la Règle Constitutionnelle 7 (protection des mineurs).
//! 4. La cohérence des métriques biométriques (EAR, MAR, IPD, angle du cou, posture).

use hakille_inference::{
    ModelStatus,
    facemesh_onnx::{
        FACEMESH_POINTS_COUNT, FaceMeshIrisSession, compute_facial_metrics,
        generate_canonical_landmarks,
    },
    hsemotion_onnx::HSEmotionSession,
    pose_onnx::{PoseSession, calculate_neck_angle, generate_canonical_seated_skeleton},
    ssrnet_onnx::{SSRNetSession, build_age_result},
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

            // Sans artefact ONNX, les 4 sessions doivent refuser honnêtement (zéro simulation silencieuse)
            // et sans aucun deadlock ni panique sous concurrence.
            assert!(fm.infer(&dummy_frame, 320, 240).is_err());
            assert!(em.infer(&dummy_frame, 320, 240).is_err());
            assert!(sn.infer(&dummy_frame, 320, 240).is_err());
            assert!(ps.infer(&dummy_frame, 320, 240).is_err());
        });

        handles.push(handle);
    }

    for h in handles {
        h.join()
            .expect("thread completed without panic or deadlock");
    }
}

#[test]
fn test_m3_adversarial_degraded_inputs() {
    let facemesh = FaceMeshIrisSession::open_or_simulated(None);
    let hsemotion = HSEmotionSession::open_or_simulated(None);
    let ssrnet = SSRNetSession::open_or_simulated(None);
    let pose = PoseSession::open_or_simulated(None);

    // Sans artefact ONNX, toutes les entrées (y compris noires/blanches/extrêmes/vides)
    // doivent retourner Err proprement sans jamais paniquer ni fabriquer d'estimation.
    for (w, h, fill) in [
        (192, 192, 0u8),
        (224, 224, 255u8),
        (1000, 20, 128u8),
        (20, 1000, 128u8),
    ] {
        let frame = vec![fill; w * h * 3];
        assert!(facemesh.infer(&frame, w, h).is_err());
        assert!(hsemotion.infer(&frame, w, h).is_err());
        assert!(ssrnet.infer(&frame, w, h).is_err());
        assert!(pose.infer(&frame, w, h).is_err());
    }

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
        (18.0, false), // 18.0 ans = majorité légale
        (18.5, false), // Au-delà de la marge
        (25.0, false),
        (40.0, false),
        (75.0, false),
    ];

    for (age, expected_minor) in test_ages {
        let res = build_age_result(age, 0.90, ModelStatus::Real);
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
    let landmarks = generate_canonical_landmarks(640.0, 480.0);
    let fm_out = compute_facial_metrics(landmarks, ModelStatus::Real);
    let skeleton = generate_canonical_seated_skeleton(640.0, 480.0);
    let neck_angle_deg = calculate_neck_angle(&skeleton);

    // 1. EAR dans les bornes physiologiques humaines [0.05, 0.50]
    assert!(fm_out.landmarks.len() == FACEMESH_POINTS_COUNT);
    assert!(fm_out.left_ear >= 0.05 && fm_out.left_ear <= 0.50);
    assert!(fm_out.right_ear >= 0.05 && fm_out.right_ear <= 0.50);

    // 2. MAR au repos dans les bornes [0.05, 0.40]
    assert!(fm_out.mar >= 0.05 && fm_out.mar <= 0.40);

    // 3. Distance inter-pupillaire positive et raisonnable
    assert!(fm_out.ipd_distance > 10.0 && fm_out.ipd_distance < 640.0);

    // 4. Squelette COCO 17 points valide
    assert_eq!(skeleton.joints.len(), 17);
    assert_eq!(skeleton.posture, PostureKind::Seated);
    assert!((0.0..=90.0).contains(&neck_angle_deg));
}
