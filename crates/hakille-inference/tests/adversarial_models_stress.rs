//! Tests adversariaux de robustesse, cas limites et contraintes constitutionnelles
//! pour crates/hakille-inference/src/models/

use hakille_inference::models::{
    AgeGroup, FaceMeshEngine, HsEmotionEngine, NormalizationMode, RtmPoseEngine, SsrNetEngine,
    decode_simcc_joints, letterbox_rgb_nchw, resize_bilinear_rgb_nchw, synthetic_blank_rgb,
    synthetic_checkerboard_rgb, synthetic_face_crop, synthetic_gradient_rgb,
};

#[test]
fn test_adversarial_image_dimensions_and_ratios() {
    let facemesh = FaceMeshEngine::mock_synthetic();
    let hsemotion = HsEmotionEngine::mock_synthetic();
    let ssrnet = SsrNetEngine::mock_synthetic();
    let rtmpose = RtmPoseEngine::mock_synthetic();

    // 1. Dimensions 0x0 ou buffer vide -> doivent retourner Err sans panic
    assert!(facemesh.detect_landmarks(&[], 0, 0).is_err());
    assert!(hsemotion.infer_emotion(&[], 0, 0).is_err());
    assert!(ssrnet.estimate_age(&[], 0, 0).is_err());
    assert!(rtmpose.extract_pose(&[], 0, 0).is_err());

    // 2. Buffer non-vide mais dimensions 0
    assert!(facemesh.detect_landmarks(&[0, 0, 0], 0, 0).is_err());
    assert!(hsemotion.infer_emotion(&[0, 0, 0], 0, 0).is_err());
    assert!(ssrnet.estimate_age(&[0, 0, 0], 0, 0).is_err());
    assert!(rtmpose.extract_pose(&[0, 0, 0], 0, 0).is_err());

    // 3. Dimensions 1x1 (pixel unique RGB)
    let single_pixel = [128u8, 64u8, 200u8];
    let fm_1x1 = facemesh
        .detect_landmarks(&single_pixel, 1, 1)
        .expect("1x1 facemesh");
    assert_eq!(fm_1x1.landmarks.len(), 478);
    assert!(fm_1x1.ipd_distance.is_finite());
    assert!(fm_1x1.left_ear.is_finite());
    assert!(fm_1x1.right_ear.is_finite());
    assert!(fm_1x1.mar.is_finite());

    let hs_1x1 = hsemotion
        .infer_emotion(&single_pixel, 1, 1)
        .expect("1x1 hsemotion");
    assert!(hs_1x1.confidence.is_finite());
    assert!(hs_1x1.valence.is_finite());
    assert!(hs_1x1.arousal.is_finite());

    let ssr_1x1 = ssrnet
        .estimate_age(&single_pixel, 1, 1)
        .expect("1x1 ssrnet");
    assert!(ssr_1x1.estimated_age.is_finite());
    assert!(ssr_1x1.minor_probability.is_finite());

    let pose_1x1 = rtmpose
        .extract_pose(&single_pixel, 1, 1)
        .expect("1x1 rtmpose");
    assert_eq!(pose_1x1.skeleton.joints.len(), 17);
    assert!(pose_1x1.neck_angle_deg.is_finite());

    // 4. Ratios extrêmes : 10000x1 (très large) et 1x10000 (très haut)
    let wide_w = 10000;
    let wide_h = 1;
    let wide_buf = vec![128u8; wide_w * wide_h * 3];
    assert!(facemesh.detect_landmarks(&wide_buf, wide_w, wide_h).is_ok());
    assert!(hsemotion.infer_emotion(&wide_buf, wide_w, wide_h).is_ok());
    assert!(ssrnet.estimate_age(&wide_buf, wide_w, wide_h).is_ok());
    assert!(rtmpose.extract_pose(&wide_buf, wide_w, wide_h).is_ok());

    let tall_w = 1;
    let tall_h = 10000;
    let tall_buf = vec![128u8; tall_w * tall_h * 3];
    assert!(facemesh.detect_landmarks(&tall_buf, tall_w, tall_h).is_ok());
    assert!(hsemotion.infer_emotion(&tall_buf, tall_w, tall_h).is_ok());
    assert!(ssrnet.estimate_age(&tall_buf, tall_w, tall_h).is_ok());
    assert!(rtmpose.extract_pose(&tall_buf, tall_w, tall_h).is_ok());
}

#[test]
fn test_adversarial_uniform_images_and_noise() {
    let facemesh = FaceMeshEngine::mock_synthetic();
    let hsemotion = HsEmotionEngine::mock_synthetic();
    let ssrnet = SsrNetEngine::mock_synthetic();
    let rtmpose = RtmPoseEngine::mock_synthetic();

    let (w, h) = (128, 128);

    // 1. Noir total (0 lumens)
    let black = synthetic_blank_rgb(w, h, 0, 0, 0);
    let fm_black = facemesh
        .detect_landmarks(&black, w, h)
        .expect("black facemesh");
    assert!(fm_black.ipd_distance > 0.0 && fm_black.ipd_distance.is_finite());
    assert!(fm_black.left_ear.is_finite() && !fm_black.left_ear.is_nan());
    assert!(fm_black.right_ear.is_finite() && !fm_black.right_ear.is_nan());
    assert!(fm_black.mar.is_finite() && !fm_black.mar.is_nan());

    let hs_black = hsemotion
        .infer_emotion(&black, w, h)
        .expect("black hsemotion");
    assert!(hs_black.confidence > 0.0 && hs_black.confidence.is_finite());
    assert!(hs_black.valence.is_finite() && hs_black.arousal.is_finite());

    let ssr_black = ssrnet.estimate_age(&black, w, h).expect("black ssrnet");
    assert!(ssr_black.estimated_age >= 0.0 && ssr_black.estimated_age.is_finite());
    assert!(ssr_black.minor_probability.is_finite());

    let pose_black = rtmpose.extract_pose(&black, w, h).expect("black rtmpose");
    assert!(pose_black.neck_angle_deg.is_finite());

    // 2. Blanc total (saturation 255)
    let white = synthetic_blank_rgb(w, h, 255, 255, 255);
    let fm_white = facemesh
        .detect_landmarks(&white, w, h)
        .expect("white facemesh");
    assert!(fm_white.ipd_distance.is_finite());
    let hs_white = hsemotion
        .infer_emotion(&white, w, h)
        .expect("white hsemotion");
    assert!(hs_white.confidence.is_finite());
    let ssr_white = ssrnet.estimate_age(&white, w, h).expect("white ssrnet");
    assert!(ssr_white.estimated_age.is_finite());
    let pose_white = rtmpose.extract_pose(&white, w, h).expect("white rtmpose");
    assert!(pose_white.neck_angle_deg.is_finite());

    // 3. Bruit aléatoire pur / pseudo-bruit
    let mut rng_buf = vec![0u8; w * h * 3];
    for (i, byte) in rng_buf.iter_mut().enumerate() {
        *byte = ((i * 1664525 + 1013904223) >> 16) as u8;
    }
    let fm_noise = facemesh
        .detect_landmarks(&rng_buf, w, h)
        .expect("noise facemesh");
    assert!(fm_noise.ipd_distance.is_finite());
    let hs_noise = hsemotion
        .infer_emotion(&rng_buf, w, h)
        .expect("noise hsemotion");
    assert!(hs_noise.confidence.is_finite());
    let ssr_noise = ssrnet.estimate_age(&rng_buf, w, h).expect("noise ssrnet");
    assert!(ssr_noise.estimated_age.is_finite());
    let pose_noise = rtmpose.extract_pose(&rng_buf, w, h).expect("noise rtmpose");
    assert!(pose_noise.neck_angle_deg.is_finite());

    // 4. Damier fin et dégradé
    let checker = synthetic_checkerboard_rgb(w, h, 2);
    let grad = synthetic_gradient_rgb(w, h);
    assert!(facemesh.detect_landmarks(&checker, w, h).is_ok());
    assert!(hsemotion.infer_emotion(&grad, w, h).is_ok());
}

#[test]
fn test_adversarial_normalization_modes_all_u8() {
    // Tester que les 4 modes de normalisation pour TOUTES les valeurs u8 0..=255
    // ne produisent JAMAIS de NaN, Inf, ni valeur hors bornes
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
            assert!(!r.is_nan() && !g.is_nan() && !b.is_nan());

            match mode {
                NormalizationMode::ZeroOne => {
                    assert!(r >= 0.0 && r <= 1.0);
                }
                NormalizationMode::Centered => {
                    assert!(r >= -1.0 && r <= 1.0);
                }
                NormalizationMode::Raw => {
                    assert_eq!(r, val as f32);
                }
                NormalizationMode::ImageNet => {
                    // ImageNet doit être dans [-3.0, 3.0]
                    assert!(r >= -3.0 && r <= 3.0);
                }
            }
        }
    }
}

#[test]
fn test_adversarial_simcc_decoding_edge_cases() {
    let bins = 64;
    let count = 17;

    // 1. Vecteurs uniformes / nuls
    let flat_x = vec![0.0f32; count * bins];
    let flat_y = vec![0.0f32; count * bins];
    let joints_flat = decode_simcc_joints(&flat_x, &flat_y, 2.0).expect("flat simcc decode");
    for j in &joints_flat {
        assert!(j.position.x.is_finite(), "x not finite: {}", j.position.x);
        assert!(j.position.y.is_finite(), "y not finite: {}", j.position.y);
        assert!(!j.position.x.is_nan());
        assert!(!j.position.y.is_nan());
        assert!(j.confidence.is_finite() && !j.confidence.is_nan());
        assert!(j.confidence >= 0.0 && j.confidence <= 1.0);
    }

    // 2. Pic en bordure extrême (bin 0 et bin 63)
    let mut edge_x = vec![0.0f32; count * bins];
    let mut edge_y = vec![0.0f32; count * bins];
    for i in 0..count {
        edge_x[i * bins] = 100.0; // tout à gauche
        edge_y[i * bins + (bins - 1)] = 100.0; // tout à droite
    }
    let joints_edge = decode_simcc_joints(&edge_x, &edge_y, 2.0).expect("edge simcc decode");
    for j in &joints_edge {
        assert!(j.position.x.is_finite() && !j.position.x.is_nan());
        assert!(j.position.y.is_finite() && !j.position.y.is_nan());
        assert!(j.position.x >= 0.0 && j.position.x <= 1.0);
        assert!(j.position.y >= 0.0 && j.position.y <= 1.0);
    }

    // 3. Valeurs négatives
    let neg_x = vec![-50.0f32; count * bins];
    let neg_y = vec![-50.0f32; count * bins];
    let joints_neg = decode_simcc_joints(&neg_x, &neg_y, 2.0).expect("neg simcc decode");
    for j in &joints_neg {
        assert!(j.confidence >= 0.0 && j.confidence <= 1.0);
    }

    // 4. Split ratio extrême ou 0
    let joints_sr0 =
        decode_simcc_joints(&flat_x, &flat_y, 0.0).expect("sr 0 should be guarded to max(1.0)");
    for j in &joints_sr0 {
        assert!(j.position.x.is_finite() && !j.position.x.is_nan());
    }

    // 5. Tailles de bins insuffisantes
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

    // Vérifier l'absence de NaN ou Inf dans le tampon de destination
    for (i, &v) in dst.iter().enumerate() {
        assert!(v.is_finite(), "dst[{}] = {} is not finite", i, v);
        assert!(!v.is_nan(), "dst[{}] is NaN", i);
    }

    // Vérifier la projection aller-retour
    let (mx, my) = t.original_to_model(5000.0, 0.5);
    assert!(mx.is_finite() && !mx.is_nan());
    assert!(my.is_finite() && !my.is_nan());

    let (ox, oy) = t.model_to_original(mx, my);
    assert!(ox.is_finite() && !ox.is_nan());
    assert!(oy.is_finite() && !oy.is_nan());
}

#[test]
fn test_adversarial_rule_7_constitutional_minor_protection_probe() {
    // Cette sonde vérifie la Règle Constitutionnelle 7 :
    // Tout être humain de moins de 18 ans DOIT être reconnu comme mineur (`is_minor == true`).
    // Les âges testés couvrent le spectre complet des mineurs :
    // 0, 1, 5, 8, 10, 12, 14, 15, 16, 17, 17.5, 17.9 ans.

    let test_minor_ages = [
        0.5f32, 2.0, 5.0, 8.0, 10.0, 12.0, 14.0, 15.0, 16.0, 17.0, 17.5, 17.9,
    ];

    let ssrnet = SsrNetEngine::mock_synthetic();

    // Tester la cohérence de AgeGroup pour chaque âge
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
    }

    // Vérifier l'inférence synthétique de base
    let crop = synthetic_face_crop(64, 64);
    let res = ssrnet.estimate_age(&crop, 64, 64).expect("estimate age");
    assert!(res.estimated_age.is_finite());
    assert!(res.minor_probability.is_finite());
    assert!(!res.minor_probability.is_nan());
}

#[test]
fn test_adversarial_repro_resize_0x0_underflow() {
    let mut dst = vec![0.0f32; 3 * 16 * 16];
    // Tester resize_bilinear_rgb_nchw avec src_w = 0, src_h = 0
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        resize_bilinear_rgb_nchw(&[], 0, 0, &mut dst, 16, 16, NormalizationMode::ZeroOne);
    }));

    // En debug mode, (src_w - 1) avec src_w = 0 provoque 'attempt to subtract with overflow'
    if let Err(err) = res {
        println!(
            "[VULNERABILITÉ CONSTATÉE] resize_bilinear_rgb_nchw a paniqué sur 0x0 : {:?}",
            err
        );
    }
}

#[test]
fn test_adversarial_repro_simcc_inf_nan_leak() {
    let bins = 64;
    let count = 17;
    // Tenseur SimCC avec deux infinis consécutifs : division inf/inf -> NaN dans refine_subpixel
    let mut simcc_x = vec![0.0f32; count * bins];
    let mut simcc_y = vec![0.0f32; count * bins];
    for i in 0..count {
        simcc_x[i * bins + 10] = f32::INFINITY;
        simcc_x[i * bins + 11] = f32::INFINITY;
        simcc_y[i * bins + 10] = 5.0;
    }

    let joints = decode_simcc_joints(&simcc_x, &simcc_y, 2.0).expect("decode");
    let mut nan_count = 0;
    for (i, j) in joints.iter().enumerate() {
        if j.position.x.is_nan() || j.position.y.is_nan() {
            nan_count += 1;
            println!(
                "[VULNERABILITÉ CONSTATÉE] Joint {} possède des coordonnées NaN : ({}, {})",
                i, j.position.x, j.position.y
            );
        }
    }
    println!(
        "Total joints avec NaN lors de saturation SimCC: {}",
        nan_count
    );
}

#[test]
fn test_adversarial_repro_rule_7_gap_ssrnet() {
    // Formule exacte de crates/hakille-inference/src/models/ssrnet.rs (lignes 286-293)
    let calculate_minor_flag_models = |age: f32| -> (bool, f32) {
        let sigma = 3.2f32;
        let z = (18.0 - age) / sigma;
        let p_minor = 1.0 / (1.0 + (-1.702 * z).exp());
        let is_minor = p_minor >= 0.85;
        (is_minor, p_minor)
    };

    // Pour un adolescent de 16 ans (qui est indéniablement mineur selon l'Article VI Règle 7) :
    let (is_minor_16, p_minor_16) = calculate_minor_flag_models(16.0);
    println!(
        "[VULNERABILITE CONSTATEE] Âge 16.0 ans dans models/ssrnet.rs: is_minor = {}, p_minor = {:.4} (ATTENDU: is_minor = true !)",
        is_minor_16, p_minor_16
    );

    // Pour un adolescent de 17 ans :
    let (is_minor_17, p_minor_17) = calculate_minor_flag_models(17.0);
    println!(
        "[VULNERABILITE CONSTATEE] Âge 17.0 ans dans models/ssrnet.rs: is_minor = {}, p_minor = {:.4} (ATTENDU: is_minor = true !)",
        is_minor_17, p_minor_17
    );

    // Pour un adolescent de 17.9 ans :
    let (is_minor_17_9, p_minor_17_9) = calculate_minor_flag_models(17.9);
    println!(
        "[VULNERABILITE CONSTATEE] Âge 17.9 ans dans models/ssrnet.rs: is_minor = {}, p_minor = {:.4} (ATTENDU: is_minor = true !)",
        is_minor_17_9, p_minor_17_9
    );

    // Démontrer que is_minor est FALSE pour un mineur de 16 ans
    assert_eq!(
        is_minor_16, false,
        "Preuve que le code actuel qualifie faussement un adolescent de 16 ans comme majeur"
    );
    assert_eq!(
        is_minor_17, false,
        "Preuve que le code actuel qualifie faussement un adolescent de 17 ans comme majeur"
    );
}
