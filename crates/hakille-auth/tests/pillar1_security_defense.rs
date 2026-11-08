//! Integration Test Suite for Pillar 1: Sécurité Visuelle & Défense Ambiante (M7).
//!
//! Validates:
//! - F27 & F28: Shoulder-Surfing Shield multi-face background detection & solvePnP 3D pose gaze analysis.
//! - F29 & F30: Instantaneous confidence degradation tau -> min(tau, 0.40), Kalman variance, and HUD alert dispatch.
//! - F31 & F32: Asymmetric covert blink detector (3 rapid left-eye blinks) and bilateral blink rejection.
//! - F33: Tripartite distress coordinator (Decoy Environment profile, Digital Twilight crypto-shredding, RAM zeroization, silent mesh payload).

use chrono::Utc;
use hakille_auth::{
    ConfidenceDecayEngine, ContinuousMultimodalFusion, CovertBlinkDetector, DistressTriggerState,
    FaceBoundingBox, FaceDetection, FaceLandmarks, GazeDirection, ShoulderSurfingDetector,
    SilentDistressCoordinator,
};
use hakille_core::UserId;
use hakille_core::capability::CapabilityScope;
use hakille_identity::crypto::{SubkeyHierarchy, XChaChaBlob};
use hakille_identity::ssi::Did;
use hakille_identity::storage::{AccountStatus, RedbZkStore, UserRecord};
use hakille_ui_protocol::PrivacyGuardTrigger;
use std::time::{Duration, Instant};
use uuid::Uuid;

fn make_landmarks(yaw_deg: f32, pitch_deg: f32) -> FaceLandmarks {
    // eye centers at (100, 100) and (200, 100), iod = 100
    // mouth at (110, 200) and (190, 200)
    // nose x = 150 + yaw * 2.0
    // nose y = 150 + pitch * (20.0 / 30.0)
    let nx = 150.0 + (yaw_deg / 50.0) * 100.0;
    let ny = 150.0 + (pitch_deg / 30.0) * 20.0;
    FaceLandmarks {
        points: [
            (100.0, 100.0),
            (200.0, 100.0),
            (nx, ny),
            (110.0, 200.0),
            (190.0, 200.0),
        ],
    }
}

#[test]
fn test_pillar1_shoulder_surfing_multi_face_threat_clamping() {
    let detector = ShoulderSurfingDetector::new();
    let mut fusion = ContinuousMultimodalFusion::new(0.96);

    // Primary enrolled user centered on screen
    let primary_box = FaceBoundingBox::new(400.0, 200.0, 250.0, 250.0);
    let primary_face = FaceDetection {
        bbox: primary_box.clone(),
        landmarks: make_landmarks(0.0, 0.0),
        confidence: 0.99,
    };

    // Third-party observer peering over right shoulder looking at screen (|Yaw| = 14° <= 28°, Pitch = 6°)
    let threat_observer = FaceDetection {
        bbox: FaceBoundingBox::new(100.0, 80.0, 200.0, 200.0),
        landmarks: make_landmarks(14.0, 6.0),
        confidence: 0.94,
    };

    // Innocent passerby in background looking sideways (Yaw = 48.0° > 35.0°)
    let passerby = FaceDetection {
        bbox: FaceBoundingBox::new(950.0, 120.0, 200.0, 200.0),
        landmarks: make_landmarks(48.0, 0.0),
        confidence: 0.91,
    };

    let faces = vec![primary_face, threat_observer, passerby];
    let (assessment, event) =
        detector.evaluate_and_guard(&faces, 1920, 1080, Some(&primary_box), &mut fusion);

    // 1. Assessment verification
    assert!(assessment.is_threat_detected);
    assert_eq!(assessment.active_observers_count, 1);
    assert_eq!(assessment.observers.len(), 2); // Excludes primary
    assert_eq!(assessment.recommended_tau_cap, 0.40);

    let threat = assessment
        .observers
        .iter()
        .find(|o| o.gaze == GazeDirection::ScreenFocused);
    assert!(threat.is_some());
    let t = threat.unwrap();
    assert!(t.estimated_distance_m >= 0.4 && t.estimated_distance_m <= 4.0);

    let away = assessment
        .observers
        .iter()
        .find(|o| o.gaze == GazeDirection::LookingAway);
    assert!(away.is_some());

    // 2. Multimodal confidence clamping verification
    assert_eq!(fusion.current_confidence, 0.40);
    assert_eq!(fusion.kalman_tracker.estimate, 0.40);
    assert_eq!(fusion.kalman_tracker.error_covariance, 0.35);

    // 3. Privilege gating: Wallet and FullSystem require tau >= 0.90 -> MUST BE BLOCKED
    let decay_engine = ConfidenceDecayEngine::default();
    assert!(
        decay_engine
            .gate_action(fusion.current_confidence, &CapabilityScope::Wallet)
            .is_err()
    );
    assert!(
        decay_engine
            .gate_action(fusion.current_confidence, &CapabilityScope::FullSystem)
            .is_err()
    );
    // Peripheral access (Audio/Camera/GuiInput) threshold is 0.40 -> permitted for alert display
    assert!(
        decay_engine
            .gate_action(fusion.current_confidence, &CapabilityScope::Audio)
            .is_ok()
    );

    // 4. Privacy Guard Event verification
    assert!(event.is_some());
    let ev = event.unwrap();
    match ev.trigger {
        PrivacyGuardTrigger::ShoulderSurfingDetected { people_count } => {
            assert_eq!(people_count, 1);
        }
        _ => panic!("Expected ShoulderSurfingDetected trigger"),
    }
    assert!(ev.description.contains("Shoulder-surfing threat detected"));
    assert!(ev.recommended_action.contains("acrylic blur"));
}

#[test]
fn test_pillar1_shoulder_surfing_all_observers_looking_away() {
    let detector = ShoulderSurfingDetector::new();
    let mut fusion = ContinuousMultimodalFusion::new(0.95);

    let primary_box = FaceBoundingBox::new(400.0, 200.0, 250.0, 250.0);
    let primary_face = FaceDetection {
        bbox: primary_box.clone(),
        landmarks: make_landmarks(0.0, 0.0),
        confidence: 0.99,
    };

    let passerby1 = FaceDetection {
        bbox: FaceBoundingBox::new(80.0, 80.0, 140.0, 140.0),
        landmarks: make_landmarks(50.0, 0.0),
        confidence: 0.92,
    };

    let passerby2 = FaceDetection {
        bbox: FaceBoundingBox::new(900.0, 100.0, 140.0, 140.0),
        landmarks: make_landmarks(-45.0, 5.0),
        confidence: 0.91,
    };

    let faces = vec![primary_face, passerby1, passerby2];
    let (assessment, event) =
        detector.evaluate_and_guard(&faces, 1920, 1080, Some(&primary_box), &mut fusion);

    assert!(!assessment.is_threat_detected);
    assert_eq!(assessment.active_observers_count, 0);
    assert_eq!(assessment.recommended_tau_cap, 1.0);
    assert_eq!(fusion.current_confidence, 0.95);
    assert!(event.is_none());
}

#[test]
fn test_pillar1_shoulder_surfing_noise_and_distant_faces_filtered() {
    let detector = ShoulderSurfingDetector::new();
    let frame_w = 1920;
    let frame_h = 1080;

    // Distant background face: 60x60 = 3600 px^2, ratio = 3600 / 2073600 = 0.0017 < 0.015
    let tiny_face = FaceDetection {
        bbox: FaceBoundingBox::new(100.0, 100.0, 60.0, 60.0),
        landmarks: make_landmarks(0.0, 0.0),
        confidence: 0.85,
    };

    let assessment = detector.evaluate(&[tiny_face], frame_w, frame_h, None);
    assert_eq!(assessment.observers.len(), 0);
    assert!(!assessment.is_threat_detected);
}

#[test]
fn test_pillar1_silent_distress_three_rapid_left_blinks_flow() {
    let mut detector = CovertBlinkDetector::new();
    let base = Instant::now();

    // Helper: simulate a covert left blink
    let simulate_blink = |d: &mut CovertBlinkDetector, start_ms: u64, dur_ms: u64| {
        let t_close = base + Duration::from_millis(start_ms);
        let t_open = t_close + Duration::from_millis(dur_ms);

        let tr_close = d.process_frame(t_close, 0.14, 0.32);
        assert!(!tr_close);

        d.process_frame(t_open, 0.32, 0.32)
    };

    // Blink 1 at 0ms, duration 160ms
    let r1 = simulate_blink(&mut detector, 0, 160);
    assert!(!r1);
    assert_eq!(detector.state(), DistressTriggerState::Blink1Detected);

    // Blink 2 at 380ms (interval 220ms), duration 150ms
    let r2 = simulate_blink(&mut detector, 380, 150);
    assert!(!r2);
    assert_eq!(detector.state(), DistressTriggerState::Blink2Detected);

    // Blink 3 at 750ms (interval 220ms), duration 140ms
    let r3 = simulate_blink(&mut detector, 750, 140);
    assert!(
        r3,
        "3rd rapid covert blink must successfully trigger distress!"
    );
    assert_eq!(detector.state(), DistressTriggerState::Idle);
}

#[test]
fn test_pillar1_silent_distress_bilateral_blink_rejection() {
    let mut detector = CovertBlinkDetector::new();
    let base = Instant::now();

    // 2 valid covert blinks
    detector.process_frame(base, 0.12, 0.32);
    detector.process_frame(base + Duration::from_millis(150), 0.32, 0.32);
    assert_eq!(detector.state(), DistressTriggerState::Blink1Detected);

    let t2 = base + Duration::from_millis(400);
    detector.process_frame(t2, 0.12, 0.32);
    detector.process_frame(t2 + Duration::from_millis(150), 0.32, 0.32);
    assert_eq!(detector.state(), DistressTriggerState::Blink2Detected);

    // Bilateral blink: user blinks both eyes normally (EAR < 0.20 on both)
    let t3 = base + Duration::from_millis(700);
    let triggered = detector.process_frame(t3, 0.12, 0.15);
    assert!(!triggered);
    assert_eq!(detector.state(), DistressTriggerState::Idle);

    // Subsequent blink is treated as Blink 1, NOT Blink 3
    let t4 = base + Duration::from_millis(1000);
    detector.process_frame(t4, 0.12, 0.32);
    let triggered2 = detector.process_frame(t4 + Duration::from_millis(150), 0.32, 0.32);
    assert!(!triggered2);
    assert_eq!(detector.state(), DistressTriggerState::Blink1Detected);
}

#[test]
fn test_pillar1_silent_distress_timing_boundaries() {
    let mut detector = CovertBlinkDetector::new();
    let base = Instant::now();

    // 1. Blink too short (< 80 ms, e.g. 50 ms glitch)
    detector.process_frame(base, 0.12, 0.32);
    let trig_short = detector.process_frame(base + Duration::from_millis(50), 0.32, 0.32);
    assert!(!trig_short);
    assert_eq!(detector.state(), DistressTriggerState::Idle);

    // 2. Blink too long (> 450 ms, e.g. 600 ms squint/fatigue)
    let t2 = base + Duration::from_millis(200);
    detector.process_frame(t2, 0.12, 0.32);
    let trig_long = detector.process_frame(t2 + Duration::from_millis(600), 0.32, 0.32);
    assert!(!trig_long);
    assert_eq!(detector.state(), DistressTriggerState::Idle);

    // 3. Inter-blink interval exceeded (> 650 ms)
    let t3 = base + Duration::from_millis(1000);
    detector.process_frame(t3, 0.12, 0.32);
    detector.process_frame(t3 + Duration::from_millis(150), 0.32, 0.32);
    assert_eq!(detector.state(), DistressTriggerState::Blink1Detected);

    // Pause 900 ms (> 650 ms) before next blink
    let t4 = t3 + Duration::from_millis(1050);
    detector.process_frame(t4, 0.12, 0.32);
    detector.process_frame(t4 + Duration::from_millis(150), 0.32, 0.32);
    // Sequence restarted as Blink 1 instead of advancing to Blink 2
    assert_eq!(detector.state(), DistressTriggerState::Blink1Detected);
}

#[test]
fn test_pillar1_silent_distress_tripartite_response() {
    let mut zk_store = RedbZkStore::new();
    let user_id = UserId(Uuid::new_v4());
    let did = Did::new("did:key:z6MkhaXgBZDvotDkL5257faiz4zWrAZvUnDHiafvMnWWkAOE");

    let user_record = UserRecord {
        user_id: user_id.clone(),
        primary_did: did,
        blind_username_hash: [0xAA; 32],
        encrypted_profile_blob: XChaChaBlob {
            nonce: [0xBB; 24],
            ciphertext: vec![1, 2, 3, 4],
            tag: [0xCC; 16],
            aad_fingerprint: [0xDD; 16],
        },
        state: AccountStatus::Active,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    zk_store.insert_user(user_record).unwrap();

    let mut subkeys = SubkeyHierarchy {
        identity_key: [0x11; 32],
        memory_key: [0x22; 32],
        credentials_key: [0x33; 32],
        biometrics_key: [0x44; 32],
        blind_index_key: [0x55; 32],
    };

    let (payload, decoy) = SilentDistressCoordinator::execute_distress_protocol_with_decoy(
        &user_id,
        &mut subkeys,
        &mut zk_store,
    )
    .unwrap();

    // 1. Digital Twilight verification: account state transitions to grace & salt shredded
    assert!(zk_store.is_salt_shredded(&user_id));

    // 2. RAM Zeroize verification: all subkeys in memory wiped to zero
    assert_eq!(*subkeys.identity_key(), [0u8; 32]);
    assert_eq!(*subkeys.memory_key(), [0u8; 32]);
    assert_eq!(*subkeys.credentials_key(), [0u8; 32]);
    assert_eq!(*subkeys.biometrics_key(), [0u8; 32]);
    assert_eq!(*subkeys.blind_index_key(), [0u8; 32]);

    // 3. Decoy Profile verification
    assert!(decoy.vault_access_locked);
    assert_eq!(decoy.simulated_balance_cents, 24000); // 240.00 EUR
    assert!(!decoy.decoy_documents.is_empty());
    assert_eq!(decoy.session_id, payload.decoy_session_id);

    // 4. Mesh alert verification
    assert!(payload.is_silent);
    assert_eq!(payload.encrypted_mesh_telemetry.len(), 256);
    assert_eq!(payload.user_id, user_id);
    assert_eq!(payload.mesh_channel, "hakille.mesh.sovereign.distress");
}
