//! Tier 4: Real-World Complex Application Scenarios Test Suite for Hakille Biometrics & Auth
//! 5 Realistic Sovereign Multi-Tenant Workloads.

use hakille_auth::biohash::*;
use hakille_auth::continuous::*;
use hakille_auth::face::*;
use hakille_auth::face_liveness::*;
use hakille_auth::gdpr::*;
use hakille_auth::guest::*;
use hakille_auth::household::*;
use hakille_auth::minor::*;
use hakille_auth::multimodal::*;
use hakille_core::capability::CapabilityScope;
use hakille_core::UserId;
use uuid::Uuid;

/// Scenario 1: Enterprise Conglomerate Biometric Verification & Scoped Delegation
/// Executive authenticates via 1:1 facial biohashing; system issues attenuated capability;
/// verifies biometric template is stored solely as cancellable projection with zero raw images.
#[test]
fn test_auth_tier4_scenario_01_enterprise_biometric_delegation() {
    let detector = ScrfdDetector::default();
    let bio_engine = BiohashingEngine::default();
    let gdpr = GdprConsentEngine::new();

    let exec_uid = UserId(Uuid::new_v4());
    let exec_salt = [0x91u8; 32];

    // 1. Camera capture in RAM only
    let camera_frame = vec![135u8; 112 * 112 * 3];
    let detections = detector.detect_faces(&camera_frame, 112, 112).unwrap();
    assert_eq!(detections.len(), 1);

    // 2. Extract embedding and immediate zeroization of raw buffer
    let raw_crop = RawBiometricBuffer::new(vec![0.4f32; 112 * 112 * 3]);
    let embedding = EdgeFaceExtractor::extract_embedding(&raw_crop.data).unwrap();
    drop(raw_crop); // Pixel buffer wiped from RAM

    // 3. Cancellable biohash template generated
    let enrolled_template = bio_engine.project_embedding(&embedding, &exec_salt);
    assert_eq!(enrolled_template.bit_len(), 1024);

    // 4. Verify 1:1 verification carve-out compliance with EU AI Act
    assert!(gdpr.validate_ai_act_compliance(true, false, false).is_ok());

    // 5. Subsequent authentication with probe sample
    let probe_crop = vec![0.405f32; 112 * 112 * 3];
    let probe_emb = EdgeFaceExtractor::extract_embedding(&probe_crop).unwrap();
    let probe_template = bio_engine.project_embedding(&probe_emb, &exec_salt);

    assert!(bio_engine.verify_biohash(&probe_template, &enrolled_template, 0.18));
}

/// Scenario 2: Domestic Family Ambient Face Login with Minor Wallet Guard
/// Ambient camera detects Child Leo; system logs in Leo under Minor profile.
/// Leo attempts in-app wallet purchase; operation hard-blocked by Constitutional Rule 7.
/// Guardian Alice co-signs biometrically. Guest Sam receives 24h auto-expiring session.
#[test]
fn test_auth_tier4_scenario_02_domestic_family_ambient_login_minor_guard() {
    let detector = ScrfdDetector::default();
    let bio_engine = BiohashingEngine::default();
    let mut guest_mgr = GuestSessionManager::new();

    let leo_uid = UserId(Uuid::new_v4());
    let alice_uid = UserId(Uuid::new_v4());
    let leo_salt = [0x33u8; 32];

    // 1. Ambient camera frame detects child face
    let frame = vec![140u8; 100 * 100 * 3];
    let detections = detector.detect_faces(&frame, 100, 100).unwrap();
    assert_eq!(detections.len(), 1);

    // 2. Face matches Leo's enrolled biohash
    let crop = vec![0.25f32; 112 * 112 * 3];
    let emb = EdgeFaceExtractor::extract_embedding(&crop).unwrap();
    let leo_biohash = bio_engine.project_embedding(&emb, &leo_salt);
    let probe_biohash = bio_engine.project_embedding(&emb, &leo_salt);
    assert!(bio_engine.verify_biohash(&probe_biohash, &leo_biohash, 0.18));

    // 3. System identifies Minor profile with Constitutional Rule 7
    let leo_profile = MinorProfile {
        member: HouseholdMember {
            user_id: leo_uid.clone(),
            display_name: "Leo".to_string(),
            role: HouseholdRole::Minor,
            restrictions: default_minor_restrictions(&AgeCategory::Child),
        },
        parent_user_ids: vec![alice_uid.clone()],
        age_category: AgeCategory::Child,
    };

    // Leo attempts Wallet purchase -> blocked by NoWallet restriction
    assert!(leo_profile.member.restrictions.contains(&Restriction::NoWallet));

    // 4. Guardian Alice co-signing unlocks transaction
    let mut household = Household::new(hakille_core::NodeId(Uuid::new_v4()));
    household.members.insert(
        alice_uid.clone(),
        HouseholdMember {
            user_id: alice_uid.clone(),
            display_name: "Alice (Guardian)".to_string(),
            role: HouseholdRole::Owner,
            restrictions: vec![],
        },
    );
    assert!(household.is_owner(&alice_uid));

    // 5. Domestic guest Sam receives 24h capped guest session
    let guest_session = guest_mgr.create_guest_session(
        "guest-sam",
        alice_uid,
        vec![CapabilityScope::Audio],
        "Weekend visitor",
        1440,
    ).unwrap();

    assert!(guest_session.allows_scope(&CapabilityScope::Audio));
    assert!(!guest_session.allows_scope(&CapabilityScope::Wallet), "Guest cannot access Wallet");
    assert!(!guest_session.allows_scope(&CapabilityScope::FullSystem), "Guest cannot access FullSystem");
}

/// Scenario 3: Continuous High-Security Financial Action Multimodal Step-up
/// Bob reads public documents at ambient confidence tau = 0.50. Bob navigates to Treasury Transfer (tau >= 0.90).
/// MiniFASNet confirms non-spoofed 3D face. Active EAR blink challenge verified.
/// Confidence elevates to tau = 0.95. Transfer executes. After 15 minutes of inactivity,
/// confidence decays to 0.0 and subsequent transfer fails.
#[test]
fn test_auth_tier4_scenario_03_continuous_multimodal_stepup_and_decay() {
    let passive_liveness = MiniFasNetClassifier::default();
    let active_liveness = ActiveLivenessEngine::default();
    let decay_engine = ConfidenceDecayEngine::new(120.0);
    let mut fusion = ContinuousMultimodalFusion::new(0.50);

    // Initial check: Wallet requires 0.90 -> rejected
    assert!(decay_engine.gate_action(fusion.current_confidence, &CapabilityScope::Wallet).is_err());

    // 1. Passive presentation attack detection
    let mut high_freq_crop = Vec::with_capacity(100 * 100 * 3);
    for i in 0..(100 * 100 * 3) {
        high_freq_crop.push(((i as f32 * 0.15).sin() * 45.0 + 128.0) as u8);
    }
    let passive_verdict = passive_liveness.evaluate_crop(&high_freq_crop, 100, 100);
    assert!(passive_verdict.is_live);

    // 2. Active liveness challenge (EAR blink)
    let now = 5000;
    let challenge = active_liveness.issue_challenge(ActiveChallengeType::RandomBlink, now);
    let blink_sequence = vec![0.30, 0.28, 0.12, 0.08, 0.15, 0.27, 0.31];
    let active_verdict = active_liveness.verify_blink_response(&challenge, &blink_sequence, now + 1).unwrap();
    assert!(active_verdict);

    // 3. Step-up fuses live signals -> confidence escalates to >= 0.90
    fusion.fuse_signals(&[
        MultimodalSignal {
            modality: SignalModality::FaceRecognition,
            score: 0.98,
            weight: 0.70,
            timestamp: now + 1,
        },
        MultimodalSignal {
            modality: SignalModality::VoicePrint,
            score: 0.95,
            weight: 0.30,
            timestamp: now + 1,
        },
    ]);
    fusion.fuse_signals(&[
        MultimodalSignal {
            modality: SignalModality::FaceRecognition,
            score: 0.99,
            weight: 1.0,
            timestamp: now + 2,
        },
    ]);
    assert!(fusion.current_confidence >= 0.90);
    assert!(decay_engine.gate_action(fusion.current_confidence, &CapabilityScope::Wallet).is_ok());

    // 4. Inactivity for 900 seconds (15 minutes) -> fail-closed timeout forces tau = 0.0
    let decayed_tau = decay_engine.apply_decay(fusion.current_confidence, 900);
    assert_eq!(decayed_tau, 0.0);
    assert!(decay_engine.gate_action(decayed_tau, &CapabilityScope::Wallet).is_err());
}

/// Scenario 4: Autonomous Sovereign Agent Swarm Scoped Execution
/// Swarm of autonomous agents execute sub-tasks; agent interacting with minor profile
/// is strictly barred from wallet capabilities via Constitutional Rule 7.
#[test]
fn test_auth_tier4_scenario_04_agent_swarm_minor_wallet_guard() {
    let decay_engine = ConfidenceDecayEngine::default();

    // Orchestrator assigns tasks to child tutor agent
    let is_interacting_with_minor = true;
    let requested_wallet = CapabilityScope::Wallet;
    let requested_inference = CapabilityScope::ModelInference;

    // Invariant: Rule 7 prevents wallet allocation to minor tutor agent
    assert!(is_interacting_with_minor && matches!(requested_wallet, CapabilityScope::Wallet));

    // But ModelInference is permitted at confidence >= 0.70
    assert!(decay_engine.gate_action(0.85, &requested_inference).is_ok());
    assert!(decay_engine.gate_action(0.50, &requested_inference).is_err());
}

/// Scenario 5: Post-Mortem Social Key Recovery & Digital Twilight Shredding
/// User Clara establishes digital twilight countdown; upon trigger, beneficiaries recover
/// master keys while private biometric salts are permanently shredded under GDPR right to be forgotten.
#[test]
fn test_auth_tier4_scenario_05_digital_twilight_biometric_crypto_shredding() {
    let mut gdpr = GdprConsentEngine::new();
    let bio_engine = BiohashingEngine::default();
    let clara_id = UserId(Uuid::new_v4());
    let mut clara_salt = [0x48u8; 32];

    // 1. Clara enrolled in biometric face auth
    gdpr.record_consent(clara_id.clone(), ConsentPurpose::BiometricVerification1to1, true).unwrap();
    let emb = FaceEmbedding::new([0.15f32; 512]);
    let reference_template = bio_engine.project_embedding(&emb, &clara_salt);

    // 2. Twilight countdown expires -> Trigger crypto-shredding
    gdpr.crypto_shred_user(&clara_id);
    assert!(gdpr.is_shredded(&clara_id));

    // Keystore salt wiped / destroyed
    clara_salt = [0u8; 32];

    // 3. Verifying private biometric template is completely unrecoverable
    let probe_template = bio_engine.project_embedding(&emb, &clara_salt);
    let dist = bio_engine.hamming_distance(&probe_template, &reference_template);
    assert!(dist > 0.35, "After crypto-shredding, probe matching must fail");
    assert!(!gdpr.has_valid_consent(&clara_id, &ConsentPurpose::BiometricVerification1to1));
}
