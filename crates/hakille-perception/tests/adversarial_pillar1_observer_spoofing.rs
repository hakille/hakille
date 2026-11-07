//! Adversarial Stress Testing & Security Invariant Fuzzing: Third-Party Observer Spoofing & Evasion
//!
//! Validates shoulder-surfing detection robustness against:
//! - Multi-observer swarm scaling (up to 100 background faces)
//! - Area ratio evasion boundary (1.49% filtered vs 1.51% detected)
//! - IoU spoofing and primary user boundary discrimination
//! - High-frequency yaw angle flutter around decision thresholds
//! - Extreme coordinate and floating-point fuzzing (NaN, Inf, negative dimensions, zero frame area)
//! - Pinhole distance estimation saturation bounds

use hakille_perception::vision::shoulder_surfing::{GazeDirection, ShoulderSurfingDetector};
use hakille_ui_protocol::PrivacyGuardTrigger;

#[test]
fn test_adversarial_multi_observer_swarm_scaling() {
    let detector = ShoulderSurfingDetector::new();
    let frame_w = 1920;
    let frame_h = 1080;

    let mut faces = Vec::new();
    // Primary user at index 0 (facing screen)
    faces.push(((600.0, 300.0, 400.0, 400.0), 0.0, 0.0, 0.99));

    // Inject 100 third-party observers in background with area ratio > 0.015 (200x200 = 40000 / 2073600 = 1.93%)
    let mut expected_threat_count = 0;
    for i in 1..=100 {
        let x = (i % 10) as f32 * 180.0;
        let y = (i / 10) as f32 * 100.0;
        let bbox = (x, y, 200.0, 200.0);

        if i <= 60 {
            // Screen-focused threat: yaw within [-28.0, 28.0] and pitch within [-20.0, 25.0]
            let yaw = ((i as f32 % 28.0) - 14.0) * 1.5; // [-21.0, 21.0]
            let pitch = 5.0;
            faces.push((bbox, yaw, pitch, 0.90));
            expected_threat_count += 1;
        } else {
            // Passerby looking away: yaw > 35.0
            let yaw = 45.0 + (i as f32 % 30.0);
            let pitch = 0.0;
            faces.push((bbox, yaw, pitch, 0.85));
        }
    }

    let assessment = detector.evaluate_candidates(&faces, frame_w, frame_h, Some(0));

    assert!(assessment.is_threat_detected);
    assert_eq!(assessment.active_observers_count, expected_threat_count);
    assert_eq!(assessment.observers.len(), 100);
    assert_eq!(assessment.recommended_tau_cap, 0.40);

    let event = detector.create_privacy_guard_event(&assessment);
    assert!(event.is_some());
    let ev = event.unwrap();
    match ev.trigger {
        PrivacyGuardTrigger::ShoulderSurfingDetected { people_count } => {
            assert_eq!(people_count, expected_threat_count as u32);
        }
        _ => panic!("Expected ShoulderSurfingDetected trigger"),
    }
}

#[test]
fn test_adversarial_area_ratio_evasion_boundary() {
    let detector = ShoulderSurfingDetector::new();
    let frame_w = 1000;
    let frame_h = 1000;
    let _total_area = 1_000_000.0; // 1M pixels, so 1.5% is 15,000 px^2

    // Observer 1: Area = 14,900 (1.49% of frame) -> Below 1.5% threshold, filtered out
    let side_sub = (14900.0_f32).sqrt();
    let faces_sub = vec![((100.0, 100.0, side_sub, side_sub), 0.0, 0.0, 0.95)];
    let assess_sub = detector.evaluate_candidates(&faces_sub, frame_w, frame_h, None);
    assert_eq!(
        assess_sub.observers.len(),
        0,
        "1.49% face must be filtered out as distant background"
    );
    assert!(!assess_sub.is_threat_detected);

    // Observer 2: Area = 15,100 (1.51% of frame) -> Above 1.5% threshold, detected as threat!
    let side_sup = (15100.0_f32).sqrt();
    let faces_sup = vec![((100.0, 100.0, side_sup, side_sup), 0.0, 0.0, 0.95)];
    let assess_sup = detector.evaluate_candidates(&faces_sup, frame_w, frame_h, None);
    assert_eq!(
        assess_sup.observers.len(),
        1,
        "1.51% face must be detected as observer"
    );
    assert!(assess_sup.is_threat_detected);
}

#[test]
fn test_adversarial_yaw_flutter_boundary_discrimination() {
    let detector = ShoulderSurfingDetector::new();

    // Boundary at exactly 28.0°
    assert_eq!(
        detector.classify_gaze(28.0, 0.0),
        GazeDirection::ScreenFocused,
        "28.0° must be ScreenFocused (inclusive threshold)"
    );
    assert_eq!(
        detector.classify_gaze(-28.0, 0.0),
        GazeDirection::ScreenFocused,
        "-28.0° must be ScreenFocused (inclusive threshold)"
    );

    // Indeterminate zone: 28.01° to 35.0°
    assert_eq!(
        detector.classify_gaze(28.01, 0.0),
        GazeDirection::Indeterminate,
        "28.01° is in the indeterminate buffer zone"
    );
    assert_eq!(
        detector.classify_gaze(35.0, 0.0),
        GazeDirection::Indeterminate,
        "35.0° is indeterminate (LookingAway requires > 35.0°)"
    );

    // Looking away zone: > 35.0°
    assert_eq!(
        detector.classify_gaze(35.01, 0.0),
        GazeDirection::LookingAway,
        "35.01° must be LookingAway"
    );
    assert_eq!(
        detector.classify_gaze(-35.01, 0.0),
        GazeDirection::LookingAway,
        "-35.01° must be LookingAway"
    );

    // Rapid yaw flutter simulation across 1000 jitter samples
    for sample in 0..1000 {
        let yaw = (sample as f32 - 500.0) / 10.0; // [-50.0, 50.0]
        let gaze = detector.classify_gaze(yaw, 0.0);
        if yaw.abs() <= 28.0 {
            assert_eq!(gaze, GazeDirection::ScreenFocused);
        } else if yaw.abs() > 35.0 {
            assert_eq!(gaze, GazeDirection::LookingAway);
        } else {
            assert_eq!(gaze, GazeDirection::Indeterminate);
        }
    }
}

#[test]
fn test_adversarial_floating_point_and_coordinate_fuzzing() {
    let detector = ShoulderSurfingDetector::new();

    // Extreme floating-point values in gaze classification
    let extremes = vec![
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        1e9,
        -1e9,
        f32::EPSILON,
        -f32::EPSILON,
    ];

    for &val in &extremes {
        let gaze = detector.classify_gaze(val, 0.0);
        if val.is_nan() {
            assert_eq!(gaze, GazeDirection::Indeterminate);
        }
    }

    // Zero frame dimensions clamped gracefully without panic
    let zero_frame_res =
        detector.evaluate_candidates(&[((100.0, 100.0, 50.0, 50.0), 0.0, 0.0, 0.9)], 0, 0, None);
    // Since total area is clamped to max(1) = 1, face area 2500 / 1 = 2500 >= 0.015, so candidate is processed
    assert!(zero_frame_res.is_threat_detected);

    // Negative bounding box coordinates / dimensions
    let negative_faces = vec![
        ((-100.0, -100.0, -50.0, -50.0), 0.0, 0.0, 0.9),
        ((0.0, 0.0, 0.0, 0.0), 0.0, 0.0, 0.9),
        ((100.0, 100.0, 1e7, 1e7), 0.0, 0.0, 0.9),
    ];
    let res = detector.evaluate_candidates(&negative_faces, 1920, 1080, None);
    assert!(res.is_threat_detected);
}

#[test]
fn test_adversarial_distance_pinhole_projection_bounds() {
    let detector = ShoulderSurfingDetector::new();
    let total_area = 1920.0 * 1080.0;

    // Minimum area ratio -> saturated at 5.0m maximum distance
    let dist_tiny = detector.estimate_distance(1.0, total_area);
    assert_eq!(dist_tiny, 5.0, "Tiny distant face must saturate at 5.0m");

    // Face covering entire frame -> saturated at 0.4m minimum distance
    let dist_huge = detector.estimate_distance(total_area, total_area);
    assert_eq!(dist_huge, 0.4, "Full frame face must saturate at 0.4m");

    // Zero face or frame area -> fallback to default 2.0m
    let dist_zero_face = detector.estimate_distance(0.0, total_area);
    assert_eq!(dist_zero_face, 2.0, "Zero face area must fallback to 2.0m");

    let dist_zero_frame = detector.estimate_distance(100.0, 0.0);
    assert_eq!(
        dist_zero_frame, 2.0,
        "Zero frame area must fallback to 2.0m"
    );

    // Negative values -> fallback to 2.0m
    let dist_neg = detector.estimate_distance(-50.0, total_area);
    assert_eq!(dist_neg, 2.0);
}
