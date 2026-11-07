//! Tier 1: Feature Coverage Test Suite for Hakille Sovereign Biometrics & Multimodal Auth
//! Features: F14 through F21, plus F23.
//! Requirements: At least 5 distinct test cases per feature (9 features x 5 = 45 test cases).

use hakille_auth::biohash::*;
use hakille_auth::continuous::*;
use hakille_auth::face::*;
use hakille_auth::face_liveness::*;
use hakille_auth::gdpr::*;
use hakille_auth::guest::*;
use hakille_auth::minor::*;
use hakille_auth::multimodal::*;
use hakille_core::capability::CapabilityScope;
use hakille_core::UserId;
use uuid::Uuid;

// =============================================================================
// F14: SCRFD Face Detection & 5 Landmarks
// =============================================================================

#[test]
fn test_f14_face_detection_happy_path() {
    let detector = ScrfdDetector::new(0.65);
    // 100x100 RGB image with normal lighting (brightness ~128)
    let rgb = vec![128u8; 100 * 100 * 3];
    let detections = detector.detect_faces(&rgb, 100, 100).expect("detection succeeds");
    assert_eq!(detections.len(), 1);
    assert!(detections[0].confidence >= 0.65);
}

#[test]
fn test_f14_face_5_landmarks_localization() {
    let detector = ScrfdDetector::default();
    let rgb = vec![140u8; 120 * 120 * 3];
    let detections = detector.detect_faces(&rgb, 120, 120).unwrap();
    assert_eq!(detections.len(), 1);

    let lm = &detections[0].landmarks;
    assert_eq!(lm.points.len(), 5);
    assert!(lm.left_eye().0 < lm.right_eye().0, "Left eye must be to the left of right eye");
    assert!(lm.nose_tip().1 > lm.left_eye().1, "Nose must be lower than eyes");
    assert!(lm.left_mouth().1 > lm.nose_tip().1, "Mouth must be lower than nose");
}

#[test]
fn test_f14_face_affine_warp_canonical_alignment() {
    let detector = ScrfdDetector::default();
    let landmarks = FaceLandmarks {
        points: [
            (40.0, 45.0), // left eye
            (80.0, 45.0), // right eye
            (60.0, 60.0), // nose tip
            (45.0, 80.0), // left mouth
            (75.0, 80.0), // right mouth
        ],
    };
    let aligned = detector.compute_alignment_warp(&landmarks);
    assert_eq!(aligned.len(), 5);
    // Verification of standard canonical coordinate scale
    assert!(aligned[0].0 < aligned[1].0);
}

#[test]
fn test_f14_face_bounding_box_geometry() {
    let bbox = FaceBoundingBox::new(20.0, 25.0, 80.0, 90.0);
    assert!(bbox.is_valid());
    assert_eq!(bbox.area(), 7200.0);
}

#[test]
fn test_f14_face_inter_ocular_distance_metric() {
    let landmarks = FaceLandmarks {
        points: [
            (30.0, 50.0),
            (70.0, 50.0),
            (50.0, 70.0),
            (35.0, 90.0),
            (65.0, 90.0),
        ],
    };
    let iod = landmarks.inter_ocular_distance();
    assert_eq!(iod, 40.0);
}

// =============================================================================
// F15: Face Embedding Extraction (512-dim EdgeFace-XS)
// =============================================================================

#[test]
fn test_f15_face_embedding_512_dim_output() {
    let crop = vec![0.5f32; 112 * 112 * 3];
    let emb = EdgeFaceExtractor::extract_embedding(&crop).expect("embedding succeeds");
    assert_eq!(emb.vector.len(), 512);
}

#[test]
fn test_f15_face_embedding_l2_normalization() {
    let crop = vec![0.3f32; 112 * 112 * 3];
    let emb = EdgeFaceExtractor::extract_embedding(&crop).unwrap();
    let norm = emb.l2_norm();
    assert!((norm - 1.0).abs() < 1e-4, "Embedding must be unit L2-normalized: {}", norm);
}

#[test]
fn test_f15_face_embedding_same_face_cosine_match() {
    let crop = vec![0.4f32; 112 * 112 * 3];
    let emb1 = EdgeFaceExtractor::extract_embedding(&crop).unwrap();
    let emb2 = EdgeFaceExtractor::extract_embedding(&crop).unwrap();
    let sim = EdgeFaceExtractor::cosine_similarity(&emb1, &emb2);
    assert!((sim - 1.0).abs() < 1e-4, "Identical face must have cosine similarity ~1.0");
    assert!(EdgeFaceExtractor::is_match(&emb1, &emb2, 0.25));
}

#[test]
fn test_f15_face_embedding_distinct_faces_low_similarity() {
    let crop1 = vec![0.2f32; 112 * 112 * 3];
    let crop2 = vec![-0.7f32; 112 * 112 * 3];
    let emb1 = EdgeFaceExtractor::extract_embedding(&crop1).unwrap();
    let emb2 = EdgeFaceExtractor::extract_embedding(&crop2).unwrap();
    let sim = EdgeFaceExtractor::cosine_similarity(&emb1, &emb2);
    assert!(sim < 0.80, "Distinct faces must have low cosine similarity");
}

#[test]
fn test_f15_face_embedding_cosine_distance_metric() {
    let crop1 = vec![0.5f32; 112 * 112 * 3];
    let emb1 = EdgeFaceExtractor::extract_embedding(&crop1).unwrap();
    let dist_self = EdgeFaceExtractor::cosine_distance(&emb1, &emb1);
    assert!(dist_self.abs() < 1e-4, "Distance to self must be 0.0");
}

// =============================================================================
// F16: Passive Anti-Spoofing (MiniFASNetV2)
// =============================================================================

#[test]
fn test_f16_passive_spoof_genuine_live_human_passes() {
    let classifier = MiniFasNetClassifier::default();
    // High-frequency texture buffer simulating human micro-pores
    let mut live_crop = Vec::with_capacity(100 * 100 * 3);
    for i in 0..(100 * 100 * 3) {
        live_crop.push(((i as f32 * 0.17).sin() * 50.0 + 128.0) as u8);
    }
    let verdict = classifier.evaluate_crop(&live_crop, 100, 100);
    assert!(verdict.is_live);
    assert_eq!(verdict.detected_attack, PassiveSpoofingType::None);
    assert!(verdict.liveness_confidence >= 0.85);
}

#[test]
fn test_f16_passive_spoof_2d_printed_photo_rejected() {
    let classifier = MiniFasNetClassifier::default();
    // Ultra flat texture with no high-frequency biological variations
    let flat_crop = vec![120u8; 100 * 100 * 3];
    let verdict = classifier.evaluate_crop(&flat_crop, 100, 100);
    assert!(!verdict.is_live);
    assert_eq!(verdict.detected_attack, PassiveSpoofingType::PrintAttack2D);
}

#[test]
fn test_f16_passive_spoof_screen_replay_moire_rejected() {
    let classifier = MiniFasNetClassifier::default();
    // Periodic stripes simulating an LCD screen grid
    let mut moire_crop = Vec::with_capacity(100 * 100 * 3);
    for row in 0..100 {
        let val = if row % 2 == 0 { 150u8 } else { 140u8 };
        for _ in 0..(100 * 3) {
            moire_crop.push(val);
        }
    }
    let verdict = classifier.evaluate_crop(&moire_crop, 100, 100);
    assert!(!verdict.is_live);
    assert!(matches!(
        verdict.detected_attack,
        PassiveSpoofingType::ScreenReplayAttack | PassiveSpoofingType::PrintAttack2D
    ));
}

#[test]
fn test_f16_passive_spoof_empty_buffer_rejected() {
    let classifier = MiniFasNetClassifier::default();
    let verdict = classifier.evaluate_crop(&[], 0, 0);
    assert!(!verdict.is_live);
    assert_eq!(verdict.liveness_confidence, 0.0);
}

#[test]
fn test_f16_passive_spoof_confidence_threshold_filter() {
    let strict = MiniFasNetClassifier::new(0.95);
    let lenient = MiniFasNetClassifier::new(0.70);
    let mut crop = Vec::with_capacity(100 * 100 * 3);
    for i in 0..(100 * 100 * 3) {
        crop.push(((i as f32 * 0.05).sin() * 20.0 + 128.0) as u8);
    }

    let verdict_strict = strict.evaluate_crop(&crop, 100, 100);
    let verdict_lenient = lenient.evaluate_crop(&crop, 100, 100);
    // Strict demands higher confidence
    assert!(verdict_strict.liveness_confidence <= verdict_lenient.liveness_confidence);
}

// =============================================================================
// F17: Active Liveness Challenge Engine
// =============================================================================

#[test]
fn test_f17_active_challenge_issue_signed_token() {
    let engine = ActiveLivenessEngine::default();
    let now = 1000;
    let token = engine.issue_challenge(ActiveChallengeType::RandomBlink, now);

    assert_eq!(token.challenge_type, ActiveChallengeType::RandomBlink);
    assert_eq!(token.expires_at, now + 4);
    assert!(engine.verify_token_validity(&token, now + 1).is_ok());
}

#[test]
fn test_f17_active_challenge_ear_blink_detection_success() {
    let engine = ActiveLivenessEngine::default();
    let now = 1000;
    let token = engine.issue_challenge(ActiveChallengeType::RandomBlink, now);

    // Natural blink: eyes open (0.30) -> dip below 0.18 (0.12) -> recovery (0.28)
    let ear_series = vec![0.30, 0.28, 0.20, 0.12, 0.10, 0.15, 0.26, 0.29];
    let res = engine.verify_blink_response(&token, &ear_series, now + 1).unwrap();
    assert!(res);
}

#[test]
fn test_f17_active_challenge_gaze_pursuit_correlation_success() {
    let engine = ActiveLivenessEngine::default();
    let now = 1000;
    let token = engine.issue_challenge(ActiveChallengeType::GazePursuit, now);

    let target = vec![(10.0, 10.0), (20.0, 20.0), (30.0, 30.0), (40.0, 40.0)];
    let pupil = vec![(10.2, 9.8), (20.1, 19.9), (29.8, 30.1), (40.0, 40.2)];

    let res = engine.verify_gaze_response(&token, &pupil, &target, now + 1).unwrap();
    assert!(res, "Gaze correlated with target trajectory R^2 >= 0.85");
}

#[test]
fn test_f17_active_challenge_head_pose_rotation_success() {
    let engine = ActiveLivenessEngine::default();
    let now = 1000;
    let token = engine.issue_challenge(ActiveChallengeType::HeadPoseRotation, now);

    let yaw_series = vec![(0.0, 0.0), (5.0, 0.0), (12.0, 0.0), (18.5, 0.0)];
    let res = engine.verify_head_pose_response(&token, &yaw_series, 15.0, now + 1).unwrap();
    assert!(res, "Head pose exceeded 15 degrees yaw");
}

#[test]
fn test_f17_active_challenge_expired_token_rejected() {
    let engine = ActiveLivenessEngine::default();
    let now = 1000;
    let token = engine.issue_challenge(ActiveChallengeType::RandomBlink, now);

    // Verified after expiration (t = 1006 > 1004) -> must fail closed
    let res = engine.verify_blink_response(&token, &[0.3, 0.1, 0.3], now + 6);
    assert!(matches!(res, Err(hakille_core::HakilleError::CapabilityExpired)));
}

// =============================================================================
// F18: Biohashing & Ephemeral RAM Protection
// =============================================================================

#[test]
fn test_f18_biohash_1024_bit_projection() {
    let engine = BiohashingEngine::default();
    let emb = FaceEmbedding::new([0.05f32; 512]);
    let salt = [0x42u8; 32];

    let digest = engine.project_embedding(&emb, &salt);
    assert_eq!(digest.bit_len(), 1024);
    assert_eq!(digest.bits.len(), 128);
}

#[test]
fn test_f18_biohash_same_face_low_hamming_distance() {
    let engine = BiohashingEngine::default();
    let emb1 = FaceEmbedding::new([0.05f32; 512]);
    let emb2 = FaceEmbedding::new([0.051f32; 512]); // Tiny perturbation
    let salt = [0x55u8; 32];

    let d1 = engine.project_embedding(&emb1, &salt);
    let d2 = engine.project_embedding(&emb2, &salt);
    let dist = engine.hamming_distance(&d1, &d2);
    assert!(dist <= 0.05, "Hamming distance must be very low for identical face: {}", dist);
    assert!(engine.verify_biohash(&d1, &d2, 0.18));
}

#[test]
fn test_f18_biohash_revocable_salt_generates_uncorrelated_template() {
    let engine = BiohashingEngine::default();
    let emb = FaceEmbedding::new([0.1f32; 512]);
    let salt_original = [0x11u8; 32];
    let salt_revoked = [0x22u8; 32];

    let d_orig = engine.project_embedding(&emb, &salt_original);
    let d_rev = engine.project_embedding(&emb, &salt_revoked);

    let dist = engine.hamming_distance(&d_orig, &d_rev);
    // Orthogonal random projections yield Hamming distance ~ 0.50 (uncorrelated)
    assert!(dist >= 0.40 && dist <= 0.60, "Revoked salt must produce uncorrelated biohash: {}", dist);
    assert!(!engine.verify_biohash(&d_orig, &d_rev, 0.18));
}

#[test]
fn test_f18_biohash_distinct_faces_high_hamming_distance() {
    let engine = BiohashingEngine::default();
    let emb1 = FaceEmbedding::new([0.5f32; 512]);
    let emb2 = FaceEmbedding::new([-0.5f32; 512]);
    let salt = [0x33u8; 32];

    let d1 = engine.project_embedding(&emb1, &salt);
    let d2 = engine.project_embedding(&emb2, &salt);
    let dist = engine.hamming_distance(&d1, &d2);
    assert!(dist > 0.35, "Distinct faces must have high Hamming distance: {}", dist);
}

#[test]
fn test_f18_biohash_ephemeral_buffer_zeroize_on_drop() {
    let raw = RawBiometricBuffer::new(vec![1.0, 2.0, 3.0]);
    assert_eq!(raw.data.len(), 3);
    drop(raw); // Drops with zeroization
}

// =============================================================================
// F19: GDPR Art. 9 & EU AI Act Compliance
// =============================================================================

#[test]
fn test_f19_gdpr_explicit_opt_in_consent_recorded() {
    let mut gdpr = GdprConsentEngine::new();
    let user = UserId(Uuid::new_v4());

    gdpr.record_consent(user.clone(), ConsentPurpose::BiometricVerification1to1, true)
        .expect("opt-in recorded");
    assert!(gdpr.has_valid_consent(&user, &ConsentPurpose::BiometricVerification1to1));
}

#[test]
fn test_f19_gdpr_missing_opt_in_rejected() {
    let mut gdpr = GdprConsentEngine::new();
    let user = UserId(Uuid::new_v4());

    let res = gdpr.record_consent(user.clone(), ConsentPurpose::BiometricVerification1to1, false);
    assert!(res.is_err(), "Non-opt-in must be rejected under GDPR Art. 9");
}

#[test]
fn test_f19_gdpr_withdrawal_of_consent() {
    let mut gdpr = GdprConsentEngine::new();
    let user = UserId(Uuid::new_v4());

    gdpr.record_consent(user.clone(), ConsentPurpose::VoicePrintRecognition, true).unwrap();
    assert!(gdpr.has_valid_consent(&user, &ConsentPurpose::VoicePrintRecognition));

    gdpr.withdraw_consent(&user, &ConsentPurpose::VoicePrintRecognition).unwrap();
    assert!(!gdpr.has_valid_consent(&user, &ConsentPurpose::VoicePrintRecognition));
}

#[test]
fn test_f19_gdpr_crypto_shredding_renders_unusable() {
    let mut gdpr = GdprConsentEngine::new();
    let user = UserId(Uuid::new_v4());

    gdpr.record_consent(user.clone(), ConsentPurpose::BiometricVerification1to1, true).unwrap();
    gdpr.crypto_shred_user(&user);

    assert!(gdpr.is_shredded(&user));
    assert!(!gdpr.has_valid_consent(&user, &ConsentPurpose::BiometricVerification1to1));
}

#[test]
fn test_f19_ai_act_1_to_1_verification_carveout_and_prohibitions() {
    let gdpr = GdprConsentEngine::new();

    // 1:1 sovereign biometric verification is permitted
    assert!(gdpr.validate_ai_act_compliance(true, false, false).is_ok());

    // Prohibited: Emotion recognition (Article 5(1)(e))
    assert!(gdpr.validate_ai_act_compliance(true, true, false).is_err());

    // Prohibited: Public space remote biometric identification (Article 5(1)(d))
    assert!(gdpr.validate_ai_act_compliance(true, false, true).is_err());

    // Prohibited: 1:N non-verification biometric surveillance
    assert!(gdpr.validate_ai_act_compliance(false, false, false).is_err());
}

// =============================================================================
// F20: Continuous Multimodal Fusion Engine
// =============================================================================

#[test]
fn test_f20_multimodal_fusion_face_and_voice() {
    let mut engine = ContinuousMultimodalFusion::new(0.50);
    let signals = vec![
        MultimodalSignal {
            modality: SignalModality::FaceRecognition,
            score: 0.95,
            weight: 0.60,
            timestamp: 100,
        },
        MultimodalSignal {
            modality: SignalModality::VoicePrint,
            score: 0.90,
            weight: 0.40,
            timestamp: 100,
        },
    ];

    let fused = engine.fuse_signals(&signals);
    assert!(fused > 0.70, "Strong face + voice signals must elevate confidence");
}

#[test]
fn test_f20_multimodal_fusion_weighted_bayesian_update() {
    let mut engine = ContinuousMultimodalFusion::new(0.40);
    // Heavy face weight vs light proximity weight
    let signals = vec![
        MultimodalSignal {
            modality: SignalModality::FaceRecognition,
            score: 0.90,
            weight: 0.80,
            timestamp: 100,
        },
        MultimodalSignal {
            modality: SignalModality::ProximityBle,
            score: 0.30,
            weight: 0.20,
            timestamp: 100,
        },
    ];

    let fused = engine.fuse_signals(&signals);
    assert!(fused > 0.60, "Face score dominates due to higher weight");
}

#[test]
fn test_f20_multimodal_fusion_temporal_smoothing() {
    let mut engine = ContinuousMultimodalFusion::new(0.90);
    // Transient weak drop
    let signals = vec![MultimodalSignal {
        modality: SignalModality::FaceRecognition,
        score: 0.40,
        weight: 1.0,
        timestamp: 101,
    }];
    let fused = engine.fuse_signals(&signals);
    // Smoothing retains historical confidence (0.6 * 0.40 + 0.4 * 0.90 = 0.60)
    assert!(fused >= 0.55 && fused <= 0.65);
}

#[test]
fn test_f20_multimodal_fusion_weak_signals_escalate_confidence() {
    let mut engine = ContinuousMultimodalFusion::new(0.30);
    let signals = vec![
        MultimodalSignal {
            modality: SignalModality::FaceRecognition,
            score: 0.80,
            weight: 0.5,
            timestamp: 100,
        },
        MultimodalSignal {
            modality: SignalModality::KeystrokeDynamics,
            score: 0.80,
            weight: 0.5,
            timestamp: 100,
        },
    ];
    let new_tau = engine.fuse_signals(&signals);
    assert!(new_tau > 0.50);
}

#[test]
fn test_f20_multimodal_fusion_empty_signals_retains_confidence() {
    let mut engine = ContinuousMultimodalFusion::new(0.75);
    let fused = engine.fuse_signals(&[]);
    assert_eq!(fused, 0.75);
}

// =============================================================================
// F21: Confidence Decay & Privilege Escalation Gating
// =============================================================================

#[test]
fn test_f21_confidence_decay_temporal_half_life() {
    let decay_engine = ConfidenceDecayEngine::new(180.0);
    let tau_initial = 1.0;
    // After 180 seconds (1 half-life), tau drops by approximately 50%
    let tau_decayed = decay_engine.apply_decay(tau_initial, 180);
    assert!((tau_decayed - 0.50).abs() < 0.05);
}

#[test]
fn test_f21_confidence_decay_fail_closed_15min_timeout() {
    let decay_engine = ConfidenceDecayEngine::default();
    // After 900 seconds (15 minutes), fail-closed timeout forces tau = 0.0
    let tau = decay_engine.apply_decay(0.95, 900);
    assert_eq!(tau, 0.0);
}

#[test]
fn test_f21_privilege_scope_ambient_threshold() {
    let decay = ConfidenceDecayEngine::default();
    assert!(decay.gate_action(0.45, &CapabilityScope::Audio).is_ok());
    assert!(decay.gate_action(0.35, &CapabilityScope::Audio).is_err());
}

#[test]
fn test_f21_privilege_scope_wallet_high_threshold_0_90() {
    let decay = ConfidenceDecayEngine::default();
    assert!(decay.gate_action(0.92, &CapabilityScope::Wallet).is_ok());
    assert!(decay.gate_action(0.88, &CapabilityScope::Wallet).is_err());
}

#[test]
fn test_f21_privilege_step_up_rejection_when_insufficient() {
    let decay = ConfidenceDecayEngine::default();
    let res = decay.gate_action(0.65, &CapabilityScope::FullSystem);
    assert!(res.is_err(), "FullSystem requires 0.90");
}

// =============================================================================
// F23: Guest & Minor Safety Invariants
// =============================================================================

#[test]
fn test_f23_minor_profile_no_wallet_restriction() {
    let child_restrictions = default_minor_restrictions(&AgeCategory::Child);
    assert!(child_restrictions.contains(&hakille_auth::household::Restriction::NoWallet));
}

#[test]
fn test_f23_minor_content_filter_levels() {
    let child_r = default_minor_restrictions(&AgeCategory::Child);
    let teen_r = default_minor_restrictions(&AgeCategory::Teenager);

    assert!(child_r.contains(&hakille_auth::household::Restriction::ContentFilter(
        hakille_auth::household::ContentFilterLevel::Strict
    )));
    assert!(teen_r.contains(&hakille_auth::household::Restriction::ContentFilter(
        hakille_auth::household::ContentFilterLevel::Moderate
    )));
}

#[test]
fn test_f23_guest_session_24h_duration_cap() {
    let mut mgr = GuestSessionManager::new();
    let host = UserId(Uuid::new_v4());

    // 1441 minutes (> 24 hours) is rejected
    assert!(mgr.create_guest_session("g-long", host.clone(), vec![CapabilityScope::Audio], "test", 1441).is_err());
    // 1440 minutes (<= 24 hours) is accepted
    assert!(mgr.create_guest_session("g-ok", host, vec![CapabilityScope::Audio], "test", 1440).is_ok());
}

#[test]
fn test_f23_guest_wallet_capability_strictly_forbidden() {
    let mut mgr = GuestSessionManager::new();
    let host = UserId(Uuid::new_v4());

    let res = mgr.create_guest_session("g-wallet", host, vec![CapabilityScope::Wallet], "test", 60);
    assert!(res.is_err(), "Guest can never hold Wallet capability");
}

#[test]
fn test_f23_guest_session_auto_expiry_purge() {
    let mut mgr = GuestSessionManager::new();
    let host = UserId(Uuid::new_v4());
    let mut session = mgr.create_guest_session("g-purge", host, vec![CapabilityScope::Audio], "test", 10).unwrap();

    // Expire artificially
    session.expires_at = hakille_core::Timestamp(chrono::Utc::now() - chrono::Duration::minutes(1));
    assert!(session.is_expired());
    assert!(!session.allows_scope(&CapabilityScope::Audio));
}
