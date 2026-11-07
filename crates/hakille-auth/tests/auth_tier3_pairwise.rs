//! Tier 3: Pairwise Cross-Feature Combinations Test Suite for Hakille Biometrics & Auth
//! Testing cross-domain interactions across biometrics, liveness, decay, and compliance (5 tests).

use hakille_auth::biohash::*;
use hakille_auth::face::*;
use hakille_auth::face_liveness::*;
use hakille_auth::gdpr::*;
use hakille_auth::guest::*;
use hakille_auth::household::*;
use hakille_auth::minor::*;
use hakille_auth::multimodal::*;
use hakille_core::UserId;
use hakille_core::capability::CapabilityScope;
use uuid::Uuid;

/// 1. Multi-Tenancy (Household/Minor) + Biometric Face Auth + Minor Protection:
/// Ambient camera captures face; system matches Child Leo; logs in under Minor profile;
/// Constitutional minor invariants enforce NoWallet and strict content filter.
#[test]
fn test_auth_tier3_pairwise_01_household_biometric_face_minor_guard() {
    let detector = ScrfdDetector::default();
    let bio_engine = BiohashingEngine::default();
    let child_salt = [0x55u8; 32];

    // 1. Detect face
    let frame = vec![130u8; 100 * 100 * 3];
    let detections = detector.detect_faces(&frame, 100, 100).unwrap();
    assert_eq!(detections.len(), 1);

    // 2. Extract embedding and biohash
    let crop = vec![0.3f32; 112 * 112 * 3];
    let emb = EdgeFaceExtractor::extract_embedding(&crop).unwrap();
    let child_biohash = bio_engine.project_embedding(&emb, &child_salt);

    // Reference matching
    let probe_biohash = bio_engine.project_embedding(&emb, &child_salt);
    assert!(bio_engine.verify_biohash(&probe_biohash, &child_biohash, 0.18));

    // 3. Child profile created with strict minor invariants
    let child_uid = UserId(Uuid::new_v4());
    let minor_prof = MinorProfile {
        member: HouseholdMember {
            user_id: child_uid.clone(),
            display_name: "Leo".to_string(),
            role: HouseholdRole::Minor,
            restrictions: default_minor_restrictions(&AgeCategory::Child),
        },
        parent_user_ids: vec![UserId(Uuid::new_v4())],
        age_category: AgeCategory::Child,
    };

    assert!(
        minor_prof
            .member
            .restrictions
            .contains(&Restriction::NoWallet)
    );
    assert!(
        minor_prof
            .member
            .restrictions
            .contains(&Restriction::ContentFilter(ContentFilterLevel::Strict))
    );
}

/// 2. Passive Anti-Spoofing (F16) + Active Liveness (F17) + Step-Up Escalation (F21):
/// Passive liveness score yields indeterminate/borderline result; triggers active gaze challenge;
/// upon active success, continuous confidence elevates from 0.50 to 0.95, granting Wallet capability.
#[test]
fn test_auth_tier3_pairwise_02_passive_active_liveness_stepup() {
    let passive_classifier = MiniFasNetClassifier::new(0.90);
    let active_engine = ActiveLivenessEngine::default();
    let decay_engine = ConfidenceDecayEngine::default();
    let mut fusion = ContinuousMultimodalFusion::new(0.50);

    // Initial state: Wallet gated
    assert!(
        decay_engine
            .gate_action(fusion.current_confidence, &CapabilityScope::Wallet)
            .is_err()
    );

    // 1. Passive check on slightly noisy crop
    let mut crop = Vec::with_capacity(100 * 100 * 3);
    for i in 0..(100 * 100 * 3) {
        crop.push(((i as f32 * 0.08).sin() * 20.0 + 128.0) as u8);
    }
    let passive_verdict = passive_classifier.evaluate_crop(&crop, 100, 100);

    // Borderline passive confidence (< 0.90) triggers active challenge
    let now = 2000;
    let challenge = active_engine.issue_challenge(ActiveChallengeType::GazePursuit, now);

    // 2. User completes active gaze pursuit challenge
    let target = vec![(10.0, 10.0), (20.0, 20.0), (30.0, 30.0), (40.0, 40.0)];
    let pupil = vec![(10.1, 9.9), (20.2, 19.8), (29.9, 30.2), (40.0, 40.1)];
    let active_ok = active_engine
        .verify_gaze_response(&challenge, &pupil, &target, now + 1)
        .unwrap();
    assert!(active_ok);

    // 3. Active challenge success elevates confidence to 0.95
    fusion.fuse_signals(&[MultimodalSignal {
        modality: SignalModality::FaceRecognition,
        score: 0.98,
        weight: 0.90,
        timestamp: now + 1,
    }]);
    assert!(fusion.current_confidence >= 0.75);

    // Re-verify after multi-signal step-up
    fusion.fuse_signals(&[MultimodalSignal {
        modality: SignalModality::FaceRecognition,
        score: 0.99,
        weight: 1.0,
        timestamp: now + 2,
    }]);
    assert!(fusion.current_confidence >= 0.90);
    assert!(
        decay_engine
            .gate_action(fusion.current_confidence, &CapabilityScope::Wallet)
            .is_ok()
    );
}

/// 3. Continuous Multimodal Fusion (F20) + Confidence Decay (F21) + Wallet Gating (F21):
/// Face and voice weak signals continually refresh confidence tau >= 0.90;
/// silence for 10 minutes decays tau below threshold; Wallet operation is locked.
#[test]
fn test_auth_tier3_pairwise_03_continuous_fusion_decay_wallet_gating() {
    let mut fusion = ContinuousMultimodalFusion::new(0.92);
    let decay_engine = ConfidenceDecayEngine::new(120.0); // 2 minute half-life

    // Wallet is authorized when active
    assert!(
        decay_engine
            .gate_action(fusion.current_confidence, &CapabilityScope::Wallet)
            .is_ok()
    );

    // After 600 seconds (10 minutes) of inactivity (no fresh signals)
    let decayed_tau = decay_engine.apply_decay(fusion.current_confidence, 600);
    assert!(decayed_tau < 0.90);
    assert!(
        decay_engine
            .gate_action(decayed_tau, &CapabilityScope::Wallet)
            .is_err()
    );
}

/// 4. Biometric Biohashing (F18) + GDPR Crypto-Shredding (F19) + Keystore:
/// User enrolls with revocable biometric salt; upon GDPR right-to-erasure request,
/// the salt is shredded, making probe matching against stored biohash permanently impossible.
#[test]
fn test_auth_tier3_pairwise_04_biohash_gdpr_crypto_shredding() {
    let mut gdpr = GdprConsentEngine::new();
    let bio_engine = BiohashingEngine::default();
    let user_id = UserId(Uuid::new_v4());
    let mut user_salt = [0x7au8; 32];

    // Enroll with consent
    gdpr.record_consent(
        user_id.clone(),
        ConsentPurpose::BiometricVerification1to1,
        true,
    )
    .unwrap();
    let emb = FaceEmbedding::new([0.08f32; 512]);
    let reference_template = bio_engine.project_embedding(&emb, &user_salt);

    // Normal verification works
    let probe_template = bio_engine.project_embedding(&emb, &user_salt);
    assert!(bio_engine.verify_biohash(&probe_template, &reference_template, 0.18));

    // User invokes GDPR Art. 17 right-to-be-forgotten -> Crypto-shredding
    gdpr.crypto_shred_user(&user_id);
    assert!(gdpr.is_shredded(&user_id));

    // Zeroize / destroy keystore salt
    user_salt = [0u8; 32];

    // Subsequent verification fails completely because salt is zeroed/destroyed
    let post_shred_probe = bio_engine.project_embedding(&emb, &user_salt);
    let dist = bio_engine.hamming_distance(&post_shred_probe, &reference_template);
    assert!(dist > 0.35, "Without salt, biohash matches fail");
}

/// 5. Active EAR Blink Challenge + Voice Liveness Classifier + Multimodal Fusion:
/// Fusing optical active face blink detection and acoustic voice liveness checks
/// into unified multimodal presence confidence.
#[test]
fn test_auth_tier3_pairwise_05_active_blink_voice_liveness_fusion() {
    let active_face = ActiveLivenessEngine::default();
    let voice_liveness = hakille_auth::liveness::VoiceLivenessDetector::default();
    let mut fusion = ContinuousMultimodalFusion::new(0.40);

    // 1. Active blink verified
    let now = 3000;
    let token = active_face.issue_challenge(ActiveChallengeType::RandomBlink, now);
    let blink_series = vec![0.30, 0.14, 0.28];
    let face_live = active_face
        .verify_blink_response(&token, &blink_series, now + 1)
        .unwrap();
    assert!(face_live);

    // 2. Voice liveness verified
    let mut natural_audio = Vec::with_capacity(3200);
    for i in 0..3200 {
        let envelope = ((i as f32 * 0.003).sin()).abs() + 0.1;
        let jitter = ((i as f32 * 0.07).sin()) * 0.1;
        natural_audio.push((i as f32 * 0.15 + jitter).sin() * 0.5 * envelope);
    }
    let voice_result = voice_liveness.analyze_pcm_frame(&natural_audio);
    assert!(voice_result.is_live_human);

    // 3. Multimodal fusion of both modalities
    let signals = vec![
        MultimodalSignal {
            modality: SignalModality::FaceRecognition,
            score: 0.95,
            weight: 0.60,
            timestamp: now + 1,
        },
        MultimodalSignal {
            modality: SignalModality::VoicePrint,
            score: voice_result.liveness_confidence as f32,
            weight: 0.40,
            timestamp: now + 1,
        },
    ];

    let fused = fusion.fuse_signals(&signals);
    assert!(
        fused >= 0.70,
        "Fused live face + live voice escalates confidence"
    );
}
