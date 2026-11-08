//! Empirical Boundary and Stress Tests for Pillars 1 & 2.
//!
//! Evaluates exact numerical and timing boundaries:
//! - Pillar 1: |Yaw| = 28.0° vs 28.1°, extreme distances, rapid blink 79ms vs 80ms, 451ms vs 450ms,
//!             inter-blink interval 651ms vs 650ms, bilateral blink injection.
//! - Pillar 2: 499ms dwell vs 500ms dwell, micro-saccade jitter exceeding 100ms,
//!             micro-nod pitch excursions <3.0° and >12.0°, torso slouching Delta Y = 10.1mm vs 9.9mm,
//!             gesture validity expiration at 1201ms.

use hakille_inference::ModelStatus;
use hakille_inference::models::facemesh::{CanonicalPnPPoints, FaceMeshOutput};
use hakille_perception::cognition::covert_blink::{CovertBlinkDetector, DistressTriggerState};
use hakille_perception::cognition::gaze_intent::{
    ContactlessGestureAuthorizer, GazeInteractableTarget, GestureAuthState,
};
use hakille_perception::cognition::gaze_tracker::GazeTrackingResult;
use hakille_perception::vision::shoulder_surfing::{GazeDirection, ShoulderSurfingDetector};
use hakille_spatial::{
    BodySkeleton3D, CocoJointId, GazeRay3D, HeadPose6D, Point3D, ScreenBounds, SkeletonJoint3D,
    Vector3D,
};
use std::time::{Duration, Instant};

// -----------------------------------------------------------------------------
// Test Helpers
// -----------------------------------------------------------------------------

fn make_dummy_facemesh() -> FaceMeshOutput {
    FaceMeshOutput {
        landmarks: vec![Point3D::ORIGIN; 478],
        left_iris: Point3D::new(-0.03, 0.0, 0.5),
        right_iris: Point3D::new(0.03, 0.0, 0.5),
        ipd_distance: 0.063,
        left_ear: 0.30,
        right_ear: 0.30,
        mar: 0.15,
        pnp_points: CanonicalPnPPoints {
            nose_tip: Point3D::ORIGIN,
            chin: Point3D::new(0.0, -0.08, 0.0),
            left_eye_corner: Point3D::new(-0.03, 0.03, 0.0),
            right_eye_corner: Point3D::new(0.03, 0.03, 0.0),
            left_mouth_corner: Point3D::new(-0.02, -0.05, 0.0),
            right_mouth_corner: Point3D::new(0.02, -0.05, 0.0),
        },
        confidence: 0.95,
        status: ModelStatus::Simulated,
    }
}

fn make_skeleton(mid_shoulder_y: f32) -> BodySkeleton3D {
    let mut skel = BodySkeleton3D::empty();
    skel.joints[CocoJointId::LeftShoulder as usize] = SkeletonJoint3D::new(
        CocoJointId::LeftShoulder as u8,
        Point3D::new(-0.20, mid_shoulder_y, 0.60),
        0.95,
    );
    skel.joints[CocoJointId::RightShoulder as usize] = SkeletonJoint3D::new(
        CocoJointId::RightShoulder as u8,
        Point3D::new(0.20, mid_shoulder_y, 0.60),
        0.95,
    );
    skel
}

fn make_gaze_result(
    impact_x: f32,
    impact_y: f32,
    pitch_deg: f32,
    yaw_deg: f32,
) -> GazeTrackingResult {
    GazeTrackingResult {
        is_screen_focused: true,
        gaze_angles: (pitch_deg.to_radians(), yaw_deg.to_radians()),
        gaze_ray: GazeRay3D::from_vector(
            Point3D::new(0.0, 0.0, 0.65),
            Vector3D::new(impact_x, impact_y, -0.65),
            true,
        ),
        screen_intersection: Some(Point3D::new(impact_x, impact_y, 0.0)),
        head_pose: HeadPose6D::from_degrees(yaw_deg, pitch_deg, 0.0, Point3D::new(0.0, 0.0, 0.65)),
        confidence: 0.95,
    }
}

// -----------------------------------------------------------------------------
// Pillar 1: Shoulder-Surfing Boundary Angle Tests (|Yaw| = 28.0° vs 28.1°)
// -----------------------------------------------------------------------------

#[test]
fn test_pillar1_boundary_angles_yaw_28_0_vs_28_1() {
    let detector = ShoulderSurfingDetector::default();

    // Exactly at threshold 28.0°: ScreenFocused
    assert_eq!(
        detector.classify_gaze(28.0, 0.0),
        GazeDirection::ScreenFocused,
        "Yaw = +28.0° must be ScreenFocused"
    );
    assert_eq!(
        detector.classify_gaze(-28.0, 0.0),
        GazeDirection::ScreenFocused,
        "Yaw = -28.0° must be ScreenFocused"
    );

    // Just past threshold 28.1°: NOT ScreenFocused
    assert_ne!(
        detector.classify_gaze(28.1, 0.0),
        GazeDirection::ScreenFocused,
        "Yaw = +28.1° must NOT be ScreenFocused"
    );
    assert_ne!(
        detector.classify_gaze(-28.1, 0.0),
        GazeDirection::ScreenFocused,
        "Yaw = -28.1° must NOT be ScreenFocused"
    );

    // Evaluation through evaluate_candidates
    let face_at_28_0 = [((200.0, 200.0, 300.0, 300.0), 28.0, 0.0, 0.95)];
    let assessment_28_0 = detector.evaluate_candidates(&face_at_28_0, 1920, 1080, None);
    assert!(
        assessment_28_0.is_threat_detected,
        "Threat must be detected at |Yaw| = 28.0°"
    );
    assert_eq!(assessment_28_0.active_observers_count, 1);
    assert_eq!(assessment_28_0.recommended_tau_cap, 0.40);

    let face_at_28_1 = [((200.0, 200.0, 300.0, 300.0), 28.1, 0.0, 0.95)];
    let assessment_28_1 = detector.evaluate_candidates(&face_at_28_1, 1920, 1080, None);
    assert!(
        !assessment_28_1.is_threat_detected,
        "Threat must NOT be detected at |Yaw| = 28.1°"
    );
    assert_eq!(assessment_28_1.active_observers_count, 0);
    assert_eq!(assessment_28_1.recommended_tau_cap, 1.0);
}

// -----------------------------------------------------------------------------
// Pillar 1: Extreme Distances & Dimensions
// -----------------------------------------------------------------------------

#[test]
fn test_pillar1_extreme_distances_and_dimensions() {
    let detector = ShoulderSurfingDetector::default();

    // Degenerate inputs return safe default 2.0m
    assert_eq!(detector.estimate_distance(0.0, 1920.0 * 1080.0), 2.0);
    assert_eq!(detector.estimate_distance(500.0, 0.0), 2.0);
    assert_eq!(detector.estimate_distance(-100.0, 1920.0 * 1080.0), 2.0);

    // Minimum distance clamp (face covers entire frame) -> 0.4m
    let d_min = detector.estimate_distance(1920.0 * 1080.0, 1920.0 * 1080.0);
    assert_eq!(d_min, 0.4, "Full-frame face must clamp to 0.4m min");

    // Maximum distance clamp (tiny pixel face in 4K / 8K frame) -> 5.0m
    let d_max = detector.estimate_distance(1.0, 7680.0 * 4320.0);
    assert_eq!(d_max, 5.0, "Tiny pixel face must clamp to 5.0m max");

    // Extreme giant frame area
    let d_extreme = detector.estimate_distance(100.0, 100_000.0 * 100_000.0);
    assert!(d_extreme >= 0.4 && d_extreme <= 5.0);
}

// -----------------------------------------------------------------------------
// Pillar 1: Rapid Blinking Timing Boundaries (79ms vs 80ms, 451ms vs 450ms)
// -----------------------------------------------------------------------------

#[test]
fn test_pillar1_rapid_blinking_timing_79ms_vs_80ms() {
    let mut detector = CovertBlinkDetector::new();
    let base = Instant::now();

    // 79ms: Below min_blink_duration (80ms) -> REJECTED
    detector.process_frame(base, 0.12, 0.32);
    let trig_79 = detector.process_frame(base + Duration::from_millis(79), 0.32, 0.32);
    assert!(!trig_79);
    assert_eq!(
        detector.state(),
        DistressTriggerState::Idle,
        "79ms blink must be rejected and stay Idle"
    );

    // 80ms: Exactly at min_blink_duration (80ms) -> ACCEPTED
    let t_start = base + Duration::from_millis(200);
    detector.process_frame(t_start, 0.12, 0.32);
    let trig_80 = detector.process_frame(t_start + Duration::from_millis(80), 0.32, 0.32);
    assert!(!trig_80);
    assert_eq!(
        detector.state(),
        DistressTriggerState::Blink1Detected,
        "80ms blink must be accepted as Blink 1"
    );
}

#[test]
fn test_pillar1_rapid_blinking_timing_451ms_vs_450ms() {
    let mut detector = CovertBlinkDetector::new();
    let base = Instant::now();

    // 450ms: Exactly at max_blink_duration (450ms) -> ACCEPTED
    detector.process_frame(base, 0.12, 0.32);
    let trig_450 = detector.process_frame(base + Duration::from_millis(450), 0.32, 0.32);
    assert!(!trig_450);
    assert_eq!(
        detector.state(),
        DistressTriggerState::Blink1Detected,
        "450ms blink must be accepted as Blink 1"
    );

    // Reset for 451ms test
    detector.reset();

    // 451ms: Exceeds max_blink_duration (450ms) -> REJECTED
    let t2 = base + Duration::from_millis(1000);
    detector.process_frame(t2, 0.12, 0.32);
    let trig_451 = detector.process_frame(t2 + Duration::from_millis(451), 0.32, 0.32);
    assert!(!trig_451);
    assert_eq!(
        detector.state(),
        DistressTriggerState::Idle,
        "451ms blink must be rejected (too sluggish) and reset to Idle"
    );
}

#[test]
fn test_pillar1_inter_blink_interval_651ms_vs_650ms() {
    let mut detector = CovertBlinkDetector::new();
    let base = Instant::now();

    // Complete Blink 1 (0ms to 100ms)
    detector.process_frame(base, 0.12, 0.32);
    detector.process_frame(base + Duration::from_millis(100), 0.32, 0.32);
    assert_eq!(detector.state(), DistressTriggerState::Blink1Detected);

    // Pause exactly 650ms: Blink 2 finishes at 100 + 650 = 750ms -> ACCEPTED as Blink 2
    let t2_close = base + Duration::from_millis(650);
    detector.process_frame(t2_close, 0.12, 0.32);
    let t2_open = base + Duration::from_millis(750); // duration 100ms, elapsed 650ms from Blink 1 finish
    detector.process_frame(t2_open, 0.32, 0.32);
    assert_eq!(
        detector.state(),
        DistressTriggerState::Blink2Detected,
        "Pause of 650ms must successfully advance to Blink 2"
    );

    // Pause 651ms test: Reset and perform Blink 1 again
    detector.reset();
    let t_b1 = base + Duration::from_millis(1500);
    detector.process_frame(t_b1, 0.12, 0.32);
    let t_b1_end = t_b1 + Duration::from_millis(100);
    detector.process_frame(t_b1_end, 0.32, 0.32);
    assert_eq!(detector.state(), DistressTriggerState::Blink1Detected);

    // Pause 651ms after Blink 1 end: t_b1_end + 651ms = t_b2_end
    let t_b2_close = t_b1_end + Duration::from_millis(551);
    detector.process_frame(t_b2_close, 0.12, 0.32);
    let t_b2_end = t_b1_end + Duration::from_millis(651);
    detector.process_frame(t_b2_end, 0.32, 0.32);
    assert_ne!(
        detector.state(),
        DistressTriggerState::Blink2Detected,
        "Pause exceeding 650ms (651ms) must NOT advance to Blink 2"
    );
    assert_eq!(
        detector.state(),
        DistressTriggerState::Blink1Detected,
        "Pause of 651ms must restart sequence as Blink 1"
    );
}

#[test]
fn test_pillar1_bilateral_blink_injection() {
    let mut detector = CovertBlinkDetector::new();
    let base = Instant::now();

    // Valid Blink 1
    detector.process_frame(base, 0.12, 0.32);
    detector.process_frame(base + Duration::from_millis(100), 0.32, 0.32);
    assert_eq!(detector.state(), DistressTriggerState::Blink1Detected);

    // Valid Blink 2
    let t2 = base + Duration::from_millis(300);
    detector.process_frame(t2, 0.12, 0.32);
    detector.process_frame(t2 + Duration::from_millis(100), 0.32, 0.32);
    assert_eq!(detector.state(), DistressTriggerState::Blink2Detected);

    // Injected bilateral blink (both eyes closed: left < 0.20 and right < 0.20)
    let t_bilat = base + Duration::from_millis(550);
    let trig = detector.process_frame(t_bilat, 0.12, 0.12);
    assert!(!trig);
    assert_eq!(
        detector.state(),
        DistressTriggerState::Idle,
        "Bilateral blink injection must reset state machine directly to Idle"
    );

    // Next blink does not trigger distress; it is treated as fresh Blink 1
    let t3 = base + Duration::from_millis(750);
    detector.process_frame(t3, 0.12, 0.32);
    let trig3 = detector.process_frame(t3 + Duration::from_millis(100), 0.32, 0.32);
    assert!(!trig3);
    assert_eq!(detector.state(), DistressTriggerState::Blink1Detected);
}

// -----------------------------------------------------------------------------
// Pillar 2: Gaze-Intent Dwell (499ms vs 500ms)
// -----------------------------------------------------------------------------

#[test]
fn test_pillar2_gaze_dwell_499ms_vs_500ms() {
    let mut authorizer = ContactlessGestureAuthorizer::new();
    let target = GazeInteractableTarget::new(
        "target_tx",
        ScreenBounds::new(Point3D::new(0.0, 0.15, 0.0), 0.10, 0.10),
        "transfer",
    );
    let targets = [target];
    let facemesh = make_dummy_facemesh();
    let skeleton = make_skeleton(0.15);
    let gaze = make_gaze_result(0.0, 0.15, 0.0, 0.0);

    let t0 = Instant::now();

    // Frame at t=0ms: starts DwellFixation
    authorizer.update(t0, &gaze, &facemesh, &skeleton, &targets);
    assert!(matches!(
        authorizer.state(),
        GestureAuthState::DwellFixation { .. }
    ));

    // Frame at t=499ms: Dwell duration NOT reached
    let t_499 = t0 + Duration::from_millis(499);
    let dec_499 = authorizer.update(t_499, &gaze, &facemesh, &skeleton, &targets);
    assert!(dec_499.is_none());
    assert!(
        matches!(authorizer.state(), GestureAuthState::DwellFixation { .. }),
        "499ms dwell must remain in DwellFixation"
    );

    // Frame at t=500ms: Exact 500ms dwell threshold reached -> AwaitingMicroGesture
    let t_500 = t0 + Duration::from_millis(500);
    let dec_500 = authorizer.update(t_500, &gaze, &facemesh, &skeleton, &targets);
    assert!(dec_500.is_none());
    assert!(
        matches!(
            authorizer.state(),
            GestureAuthState::AwaitingMicroGesture { .. }
        ),
        "500ms dwell must transition to AwaitingMicroGesture"
    );
}

// -----------------------------------------------------------------------------
// Pillar 2: Micro-Saccade Jitter Tolerance Exceeding 100ms
// -----------------------------------------------------------------------------

#[test]
fn test_pillar2_micro_saccade_jitter_tolerance_exceeding_100ms() {
    let mut authorizer = ContactlessGestureAuthorizer::new();
    let target = GazeInteractableTarget::new(
        "target_tx",
        ScreenBounds::new(Point3D::new(0.0, 0.15, 0.0), 0.10, 0.10),
        "transfer",
    );
    let targets = [target];
    let facemesh = make_dummy_facemesh();
    let skeleton = make_skeleton(0.15);

    let gaze_in = make_gaze_result(0.0, 0.15, 0.0, 0.0);
    let gaze_out = make_gaze_result(0.50, 0.50, 0.0, 0.0);

    let t0 = Instant::now();
    authorizer.update(t0, &gaze_in, &facemesh, &skeleton, &targets);

    // Gaze leaves target at t = 200ms
    let t_leave = t0 + Duration::from_millis(200);
    authorizer.update(t_leave, &gaze_in, &facemesh, &skeleton, &targets);

    // 101ms after last in bounds (t = 200 + 101 = 301ms): Jitter tolerance exceeded (> 100ms)
    let t_jitter_fail = t_leave + Duration::from_millis(101);
    authorizer.update(t_jitter_fail, &gaze_out, &facemesh, &skeleton, &targets);
    assert_eq!(
        *authorizer.state(),
        GestureAuthState::Idle,
        "Gaze leaving target for 101ms (>100ms jitter window) must reset to Idle"
    );
}

// -----------------------------------------------------------------------------
// Pillar 2: Micro-Nod Pitch Excursions (<3.0° and >12.0°)
// -----------------------------------------------------------------------------

#[test]
fn test_pillar2_micro_nod_pitch_excursions_below_3_and_above_12() {
    let target = GazeInteractableTarget::new(
        "target_tx",
        ScreenBounds::new(Point3D::new(0.0, 0.15, 0.0), 0.10, 0.10),
        "transfer",
    );
    let targets = [target];
    let facemesh = make_dummy_facemesh();
    let skeleton = make_skeleton(0.15);

    // Sub-case A: Pitch excursion 2.9° (< 3.0° minimum)
    {
        let mut authorizer = ContactlessGestureAuthorizer::new();
        let t0 = Instant::now();
        let gaze_dwell = make_gaze_result(0.0, 0.15, 0.0, 0.0);
        authorizer.update(t0, &gaze_dwell, &facemesh, &skeleton, &targets);
        authorizer.update(
            t0 + Duration::from_millis(500),
            &gaze_dwell,
            &facemesh,
            &skeleton,
            &targets,
        );
        assert!(matches!(
            authorizer.state(),
            GestureAuthState::AwaitingMicroGesture { .. }
        ));

        // Start nod
        let t_start = t0 + Duration::from_millis(550);
        authorizer.update(
            t_start,
            &make_gaze_result(0.0, 0.15, 1.5, 0.0),
            &facemesh,
            &skeleton,
            &targets,
        );

        // Peak at 2.9°
        let t_peak = t_start + Duration::from_millis(100);
        authorizer.update(
            t_peak,
            &make_gaze_result(0.0, 0.15, 2.9, 0.0),
            &facemesh,
            &skeleton,
            &targets,
        );

        // Rebound to baseline
        let t_rebound = t_start + Duration::from_millis(200);
        let dec = authorizer.update(
            t_rebound,
            &make_gaze_result(0.0, 0.15, 0.2, 0.0),
            &facemesh,
            &skeleton,
            &targets,
        );
        assert!(
            dec.is_none(),
            "Micro-nod with 2.9° peak (< 3.0°) must NOT authorize"
        );
    }

    // Sub-case B: Pitch excursion exactly 3.0° (>= 3.0° and <= 12.0°) -> SUCCESS
    {
        let mut authorizer = ContactlessGestureAuthorizer::new();
        let t0 = Instant::now();
        let gaze_dwell = make_gaze_result(0.0, 0.15, 0.0, 0.0);
        authorizer.update(t0, &gaze_dwell, &facemesh, &skeleton, &targets);
        authorizer.update(
            t0 + Duration::from_millis(500),
            &gaze_dwell,
            &facemesh,
            &skeleton,
            &targets,
        );

        let t_start = t0 + Duration::from_millis(550);
        authorizer.update(
            t_start,
            &make_gaze_result(0.0, 0.15, 1.5, 0.0),
            &facemesh,
            &skeleton,
            &targets,
        );

        let t_peak = t_start + Duration::from_millis(100);
        authorizer.update(
            t_peak,
            &make_gaze_result(0.0, 0.15, 3.0, 0.0),
            &facemesh,
            &skeleton,
            &targets,
        );

        let t_rebound = t_start + Duration::from_millis(200);
        let dec = authorizer.update(
            t_rebound,
            &make_gaze_result(0.0, 0.15, 0.2, 0.0),
            &facemesh,
            &skeleton,
            &targets,
        );
        assert!(
            dec.is_some(),
            "Micro-nod with exactly 3.0° peak must authorize"
        );
        assert!(dec.unwrap().is_authorized);
    }

    // Sub-case C: Pitch excursion 12.1° (> 12.0° maximum)
    {
        let mut authorizer = ContactlessGestureAuthorizer::new();
        let t0 = Instant::now();
        let gaze_dwell = make_gaze_result(0.0, 0.15, 0.0, 0.0);
        authorizer.update(t0, &gaze_dwell, &facemesh, &skeleton, &targets);
        authorizer.update(
            t0 + Duration::from_millis(500),
            &gaze_dwell,
            &facemesh,
            &skeleton,
            &targets,
        );

        let t_start = t0 + Duration::from_millis(550);
        authorizer.update(
            t_start,
            &make_gaze_result(0.0, 0.15, 3.0, 0.0),
            &facemesh,
            &skeleton,
            &targets,
        );

        let t_peak = t_start + Duration::from_millis(100);
        authorizer.update(
            t_peak,
            &make_gaze_result(0.0, 0.15, 12.1, 0.0),
            &facemesh,
            &skeleton,
            &targets,
        );

        let t_rebound = t_start + Duration::from_millis(200);
        let dec = authorizer.update(
            t_rebound,
            &make_gaze_result(0.0, 0.15, 0.5, 0.0),
            &facemesh,
            &skeleton,
            &targets,
        );
        assert!(
            dec.is_none(),
            "Micro-nod with 12.1° peak (> 12.0°) must NOT authorize"
        );
    }
}

// -----------------------------------------------------------------------------
// Pillar 2: Torso Slouching Shoulder Displacement (Delta Y = 10.1mm vs 9.9mm)
// -----------------------------------------------------------------------------

#[test]
fn test_pillar2_torso_slouching_shoulder_delta_y_10_1mm_vs_9_9mm() {
    let target = GazeInteractableTarget::new(
        "target_tx",
        ScreenBounds::new(Point3D::new(0.0, 0.15, 0.0), 0.10, 0.10),
        "transfer",
    );
    let targets = [target];
    let facemesh = make_dummy_facemesh();

    // Sub-case A: Shoulder Delta Y = 9.9mm (0.0099m <= 0.010m) -> AUTHORIZED
    {
        let mut authorizer = ContactlessGestureAuthorizer::new();
        let base_skeleton = make_skeleton(0.150);
        let t0 = Instant::now();

        let gaze_dwell = make_gaze_result(0.0, 0.15, 0.0, 0.0);
        authorizer.update(t0, &gaze_dwell, &facemesh, &base_skeleton, &targets);
        authorizer.update(
            t0 + Duration::from_millis(500),
            &gaze_dwell,
            &facemesh,
            &base_skeleton,
            &targets,
        );

        let t_start = t0 + Duration::from_millis(550);
        authorizer.update(
            t_start,
            &make_gaze_result(0.0, 0.15, 2.0, 0.0),
            &facemesh,
            &base_skeleton,
            &targets,
        );

        let t_peak = t_start + Duration::from_millis(100);
        authorizer.update(
            t_peak,
            &make_gaze_result(0.0, 0.15, 6.0, 0.0),
            &facemesh,
            &base_skeleton,
            &targets,
        );

        // Shoulder displaced by 9.9mm (0.150 + 0.0099 = 0.1599m)
        let skeleton_9_9mm = make_skeleton(0.1599);
        let t_rebound = t_start + Duration::from_millis(200);
        let dec = authorizer.update(
            t_rebound,
            &make_gaze_result(0.0, 0.15, 0.5, 0.0),
            &facemesh,
            &skeleton_9_9mm,
            &targets,
        );
        assert!(
            dec.is_some(),
            "Delta Y = 9.9mm (<= 10.0mm) must succeed and authorize"
        );
        assert!(dec.unwrap().is_authorized);
    }

    // Sub-case B: Shoulder Delta Y = 10.1mm (0.0101m > 0.010m) -> REJECTED & RESET TO IDLE
    {
        let mut authorizer = ContactlessGestureAuthorizer::new();
        let base_skeleton = make_skeleton(0.150);
        let t0 = Instant::now();

        let gaze_dwell = make_gaze_result(0.0, 0.15, 0.0, 0.0);
        authorizer.update(t0, &gaze_dwell, &facemesh, &base_skeleton, &targets);
        authorizer.update(
            t0 + Duration::from_millis(500),
            &gaze_dwell,
            &facemesh,
            &base_skeleton,
            &targets,
        );

        let t_start = t0 + Duration::from_millis(550);
        authorizer.update(
            t_start,
            &make_gaze_result(0.0, 0.15, 2.0, 0.0),
            &facemesh,
            &base_skeleton,
            &targets,
        );

        // Shoulder displaced by 10.1mm (0.150 + 0.0101 = 0.1601m)
        let skeleton_10_1mm = make_skeleton(0.1601);
        let t_slouch = t_start + Duration::from_millis(100);
        let dec = authorizer.update(
            t_slouch,
            &make_gaze_result(0.0, 0.15, 6.0, 0.0),
            &facemesh,
            &skeleton_10_1mm,
            &targets,
        );
        assert!(
            dec.is_none(),
            "Delta Y = 10.1mm (> 10.0mm) must NOT authorize"
        );
        assert_eq!(
            *authorizer.state(),
            GestureAuthState::Idle,
            "Delta Y = 10.1mm must reset authorizer state to Idle"
        );
    }
}

// -----------------------------------------------------------------------------
// Pillar 2: Token / Gesture Validity Window Expiration at 1201ms
// -----------------------------------------------------------------------------

#[test]
fn test_pillar2_gesture_validity_window_expiration_at_1201ms() {
    let mut authorizer = ContactlessGestureAuthorizer::new();
    let target = GazeInteractableTarget::new(
        "target_tx",
        ScreenBounds::new(Point3D::new(0.0, 0.15, 0.0), 0.10, 0.10),
        "transfer",
    );
    let targets = [target];
    let facemesh = make_dummy_facemesh();
    let skeleton = make_skeleton(0.15);

    let t0 = Instant::now();
    let gaze_dwell = make_gaze_result(0.0, 0.15, 0.0, 0.0);
    authorizer.update(t0, &gaze_dwell, &facemesh, &skeleton, &targets);

    // Target locked at t = t0 + 500ms
    let t_lock = t0 + Duration::from_millis(500);
    authorizer.update(t_lock, &gaze_dwell, &facemesh, &skeleton, &targets);
    assert!(matches!(
        authorizer.state(),
        GestureAuthState::AwaitingMicroGesture { .. }
    ));

    // Update at exactly 1201ms after lock (t = 500 + 1201 = 1701ms)
    // Validity window is 1200ms -> expired!
    let t_expired = t_lock + Duration::from_millis(1201);
    let dec = authorizer.update(t_expired, &gaze_dwell, &facemesh, &skeleton, &targets);
    assert!(dec.is_none());
    assert_eq!(
        *authorizer.state(),
        GestureAuthState::Idle,
        "Attempting nod at 1201ms (> 1200ms window) must reset to Idle"
    );
}
