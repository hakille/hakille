//! Tests adversariaux de robustesse, cas limites et contraintes constitutionnelles
//! pour crates/hakille-inference/src/models/

use hakille_inference::models::{
    AgeGroup, FaceMeshEngine, HsEmotionEngine, NormalizationMode, RtmPoseEngine, SsrNetEngine,
    decode_simcc_joints, letterbox_rgb_nchw, resize_bilinear_rgb_nchw,
    ssrnet::calculate_minor_flag, synthetic_blank_rgb, synthetic_checkerboard_rgb,
    synthetic_face_crop, synthetic_gradient_rgb,
};

#[test]
fn test_adversarial_image_dimensions_and_ratios() {
    let facemesh = FaceMeshEngine::mock_synthetic();
    let hsemotion = HsEmotionEngine::mock_synthetic();
    let ssrnet = SsrNetEngine::mock_synthetic();
    let rtmpose = RtmPoseEngine::mock_synthetic();

    // 1. Sans artefact ONNX, toutes les entrées doivent retourner Err proprement (jamais de panic ni de fabrication).
    for (w, h, data) in [
        (0usize, 0usize, vec![]),
        (0, 0, vec![0, 0, 0]),
        (1, 1, vec![128, 64, 200]),
        (10000, 1, vec![128; 10000 * 3]),
        (1, 10000, vec![128; 10000 * 3]),
    ] {
        assert!(facemesh.detect_landmarks(&data, w, h).is_err());
        assert!(hsemotion.infer_emotion(&data, w, h).is_err());
        assert!(ssrnet.estimate_age(&data, w, h).is_err());
        assert!(rtmpose.extract_pose(&data, w, h).is_err());
    }

    // 2. Le prétraitement bilinéaire pur Rust doit supporter 1x1, 10000x1 et 1x10000 sans panic ni NaN.
    let mut dst = vec![0.0f32; 3 * 64 * 64];
    for (w, h, data) in [
        (1usize, 1usize, vec![128u8, 64, 200]),
        (10000, 1, vec![128; 10000 * 3]),
        (1, 10000, vec![128; 10000 * 3]),
    ] {
        resize_bilinear_rgb_nchw(&data, w, h, &mut dst, 64, 64, NormalizationMode::Centered);
        assert!(dst.iter().all(|v| v.is_finite()));
    }
}

#[test]
fn test_adversarial_uniform_images_and_noise() {
    let facemesh = FaceMeshEngine::mock_synthetic();
    let hsemotion = HsEmotionEngine::mock_synthetic();
    let ssrnet = SsrNetEngine::mock_synthetic();
    let rtmpose = RtmPoseEngine::mock_synthetic();

    let (w, h) = (128, 128);
    let black = synthetic_blank_rgb(w, h, 0, 0, 0);
    let white = synthetic_blank_rgb(w, h, 255, 255, 255);
    let checker = synthetic_checkerboard_rgb(w, h, 2);
    let grad = synthetic_gradient_rgb(w, h);
    let mut dst = vec![0.0f32; 3 * 192 * 192];

    for buf in [&black, &white, &checker, &grad] {
        assert!(facemesh.detect_landmarks(buf, w, h).is_err());
        assert!(hsemotion.infer_emotion(buf, w, h).is_err());
        assert!(ssrnet.estimate_age(buf, w, h).is_err());
        assert!(rtmpose.extract_pose(buf, w, h).is_err());
        resize_bilinear_rgb_nchw(buf, w, h, &mut dst, 192, 192, NormalizationMode::ImageNet);
        assert!(dst.iter().all(|v| v.is_finite()));
    }
}

#[test]
fn test_adversarial_normalization_modes_all_u8() {
    for mode in [
        NormalizationMode::ImageNet,
        NormalizationMode::ZeroOne,
        NormalizationMode::Centered,
        NormalizationMode::Raw,
    ] {
        for val in 0..=255u8 {
            let (r, g, b) = mode.normalize(val, val, val);
            assert!(r.is_finite(), "mode {:?} val {} r={}", mode, val, r);
            assert!(g.is_finite(), "mode {:?} val {} g={}", mode, val, g);
            assert!(b.is_finite(), "mode {:?} val {} b={}", mode, val, b);

            match mode {
                NormalizationMode::ZeroOne => {
                    assert!((0.0..=1.0).contains(&r));
                }
                NormalizationMode::Centered => {
                    assert!((-1.0..=1.0).contains(&r));
                }
                NormalizationMode::Raw => {
                    assert_eq!(r, val as f32);
                }
                NormalizationMode::ImageNet => {
                    assert!((-3.0..=3.0).contains(&r));
                }
            }
        }
    }
}

#[test]
fn test_adversarial_simcc_decoding_edge_cases() {
    let bins = 64;
    let count = 17;

    let flat_x = vec![0.0f32; count * bins];
    let flat_y = vec![0.0f32; count * bins];
    let joints_flat = decode_simcc_joints(&flat_x, &flat_y, 2.0).expect("flat simcc decode");
    for j in &joints_flat {
        assert!(j.position.x.is_finite());
        assert!(j.position.y.is_finite());
        assert!(j.confidence.is_finite() && (0.0..=1.0).contains(&j.confidence));
    }

    let mut edge_x = vec![0.0f32; count * bins];
    let mut edge_y = vec![0.0f32; count * bins];
    for i in 0..count {
        edge_x[i * bins] = 100.0;
        edge_y[i * bins + (bins - 1)] = 100.0;
    }
    let joints_edge = decode_simcc_joints(&edge_x, &edge_y, 2.0).expect("edge simcc decode");
    for j in &joints_edge {
        assert!(j.position.x.is_finite() && (0.0..=1.0).contains(&j.position.x));
        assert!(j.position.y.is_finite() && (0.0..=1.0).contains(&j.position.y));
    }

    let neg_x = vec![-50.0f32; count * bins];
    let neg_y = vec![-50.0f32; count * bins];
    let joints_neg = decode_simcc_joints(&neg_x, &neg_y, 2.0).expect("neg simcc decode");
    for j in &joints_neg {
        assert!((0.0..=1.0).contains(&j.confidence));
    }

    let joints_sr0 =
        decode_simcc_joints(&flat_x, &flat_y, 0.0).expect("sr 0 should be guarded to max(1.0)");
    for j in &joints_sr0 {
        assert!(j.position.x.is_finite());
    }

    assert!(decode_simcc_joints(&[], &[], 2.0).is_err());
    assert!(decode_simcc_joints(&[1.0; 16], &[1.0; 16], 2.0).is_err());
}

#[test]
fn test_adversarial_letterbox_extreme_aspect_ratios() {
    let src_w = 10000;
    let src_h = 1;
    let src = vec![128u8; src_w * src_h * 3];
    let dst_w = 192;
    let dst_h = 192;
    let mut dst = vec![0.0f32; 3 * dst_w * dst_h];

    let t = letterbox_rgb_nchw(
        &src,
        src_w,
        src_h,
        &mut dst,
        dst_w,
        dst_h,
        NormalizationMode::ZeroOne,
    );

    assert!(dst.iter().all(|v| v.is_finite()));

    let (mx, my) = t.original_to_model(5000.0, 0.5);
    assert!(mx.is_finite() && my.is_finite());

    let (ox, oy) = t.model_to_original(mx, my);
    assert!(ox.is_finite() && oy.is_finite());
}

#[test]
fn test_adversarial_rule_7_constitutional_minor_protection_probe() {
    let test_minor_ages = [
        0.5f32, 2.0, 5.0, 8.0, 10.0, 12.0, 14.0, 15.0, 16.0, 17.0, 17.5, 17.9,
    ];

    let ssrnet = SsrNetEngine::mock_synthetic();

    for &age in &test_minor_ages {
        let group = AgeGroup::from_age(age);
        if age < 12.0 {
            assert_eq!(group, AgeGroup::Child, "Âge {} doit être Child", age);
        } else {
            assert_eq!(
                group,
                AgeGroup::Adolescent,
                "Âge {} doit être Adolescent",
                age
            );
        }
        let (is_minor, prob) = calculate_minor_flag(age);
        assert!(is_minor, "Âge {age} doit être protégé (Règle 7)");
        assert!(prob.is_finite());
    }

    // Sans modèle ONNX, SsrNetEngine refuse honnêtement plutôt que de simuler un adulte.
    let crop = synthetic_face_crop(64, 64);
    assert!(ssrnet.estimate_age(&crop, 64, 64).is_err());
}

#[test]
fn test_adversarial_repro_resize_0x0_underflow() {
    let mut dst = vec![0.0f32; 3 * 16 * 16];
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        resize_bilinear_rgb_nchw(&[], 0, 0, &mut dst, 16, 16, NormalizationMode::ZeroOne);
    }));
    assert!(
        res.is_ok(),
        "resize_bilinear_rgb_nchw ne doit plus paniquer sur 0x0"
    );
}

#[test]
fn test_adversarial_repro_simcc_inf_nan_leak() {
    let bins = 64;
    let count = 17;
    let mut simcc_x = vec![0.0f32; count * bins];
    let mut simcc_y = vec![0.0f32; count * bins];
    for i in 0..count {
        simcc_x[i * bins + 10] = f32::INFINITY;
        simcc_x[i * bins + 11] = f32::INFINITY;
        simcc_y[i * bins + 10] = 5.0;
    }

    let joints = decode_simcc_joints(&simcc_x, &simcc_y, 2.0).expect("decode");
    for (i, j) in joints.iter().enumerate() {
        assert!(
            j.position.x.is_finite() && j.position.y.is_finite(),
            "Joint {} ne doit pas contenir de NaN/Inf : ({}, {})",
            i,
            j.position.x,
            j.position.y
        );
    }
}

#[test]
fn test_adversarial_repro_rule_7_gap_ssrnet() {
    // Règle 7 corrigée : 16.0, 17.0 et 17.9 ans DOIVENT être qualifiés de mineurs.
    for age in [16.0f32, 17.0, 17.9] {
        let (is_minor, _p) = calculate_minor_flag(age);
        assert!(
            is_minor,
            "Adolescent de {age} ans doit impérativement être protégé (is_minor == true)"
        );
    }
    let (is_minor_nan, _) = calculate_minor_flag(f32::NAN);
    assert!(is_minor_nan, "NaN doit basculer en protection mineur");
}
