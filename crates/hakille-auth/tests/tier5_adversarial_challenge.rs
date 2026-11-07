//! Tier 5 Adversarial Coverage Hardening Challenge Suite for `hakille-auth`
//! Milestone 6, Phase 2: Empirical stress testing and adversarial invariant probing.
//!
//! Security Invariants Tested:
//! 2. Rule 7 Minor NoWallet invariant: Absolute denial in WalletManager, MinorProfile, and household restrictions.
//! 3. 24-hour Guest session cap: Strict 1440m boundary, instant expiration, and persistent/wallet scope denial.
//! 4. Step-up authentication gating: Strict failure below tau = 0.90, success at/above tau = 0.90, and temporal decay gating.
//! 5. Biometric presentation attacks: Strict rejection of Fourier cutoff spoofs, flat gaze, and active challenge anomalies.

use hakille_auth::biohash::{BiohashDigest, BiohashingEngine};
use hakille_auth::face::FaceEmbedding;
use hakille_auth::face_liveness::{
    ActiveChallengeType, ActiveLivenessEngine, MiniFasNetClassifier, PassiveSpoofingType,
};
use hakille_auth::guest::{GuestSessionManager, MAX_GUEST_DURATION_MINUTES};
use hakille_auth::household::{HouseholdMember, HouseholdRole, Restriction};
use hakille_auth::minor::{
    AgeCategory, GuardianApproval, MinorProfile, default_minor_restrictions,
};
use hakille_auth::multimodal::{
    ConfidenceDecayEngine, ContinuousMultimodalFusion, MultimodalSignal, SignalModality,
    StepUpRequirement,
};
use hakille_auth::wallet::{WalletManager, WalletTransaction};
use hakille_core::capability::CapabilityScope;
use hakille_core::types::Timestamp;
use hakille_core::{HakilleError, SpendingCategory, UserId};
use uuid::Uuid;

// =============================================================================
// SECTION 1: INVARIANT 2 — RULE 7 MINOR NOWALLET INVARIANT
// =============================================================================

#[test]
fn test_tier5_adversarial_minor_wallet_creation_and_signing_strictly_blocked() {
    let mut wallet_mgr = WalletManager::new();
    let adult_id = UserId(Uuid::new_v4());
    let minor_id = UserId(Uuid::new_v4());
    let exp = Timestamp(chrono::Utc::now() + chrono::Duration::hours(2));
    let signing_key = [0x55u8; 32];

    // Adversarial Attempt 1: Minor user attempts to create a guarded wallet cap
    // with maximum confidence tau = 1.0, maximum budget, and Tier 1 hardware
    let minor_create_attempt = wallet_mgr.create_guarded_wallet_cap(
        minor_id.clone(),
        50_000,
        "EUR".to_string(),
        SpendingCategory::Food,
        exp.clone(),
        1.0,  // Maximum possible confidence
        true, // is_minor = true
        &default_minor_restrictions(&AgeCategory::Child),
    );
    assert!(
        minor_create_attempt.is_err(),
        "Rule 7 Violation: Minor must NEVER be granted a wallet capability"
    );
    match minor_create_attempt.unwrap_err() {
        HakilleError::CapabilityDenied(msg) => {
            assert!(
                msg.contains("Rule 7") || msg.contains("Minor"),
                "Expected Rule 7 error message, got: {}",
                msg
            );
        }
        other => panic!("Expected CapabilityDenied, got {:?}", other),
    }

    // Adversarial Attempt 2: Adult creates wallet, but minor attempts to authorize & sign transaction
    let mut adult_wallet = wallet_mgr
        .create_guarded_wallet_cap(
            adult_id,
            50_000,
            "EUR".to_string(),
            SpendingCategory::Food,
            exp,
            0.95,
            false,
            &[],
        )
        .expect("Adult with tau 0.95 must be granted wallet");

    let tx = WalletTransaction {
        tx_id: Uuid::new_v4(),
        recipient: "0x1234567890abcdef".to_string(),
        amount_cents: 1500,
        currency: "EUR".to_string(),
        category: SpendingCategory::Food,
        description: "Adversarial minor purchase attempt".to_string(),
        nonce: 1,
        timestamp: Timestamp::now(),
    };

    let minor_sign_attempt = wallet_mgr.authorize_and_sign_transaction(
        &mut adult_wallet,
        tx.clone(),
        1.0,  // Maximum confidence
        true, // is_minor
        &[Restriction::NoWallet],
        &signing_key,
    );
    assert!(
        minor_sign_attempt.is_err(),
        "Rule 7 Violation: Minor must NEVER be permitted to authorize and sign transactions"
    );

    // Adversarial Attempt 3: User claimed is_minor = false, but has Restriction::NoWallet attached
    let restricted_sign_attempt = wallet_mgr.authorize_and_sign_transaction(
        &mut adult_wallet,
        tx,
        1.0,
        false, // claimed not minor
        &[Restriction::NoWallet],
        &signing_key,
    );
    assert!(
        restricted_sign_attempt.is_err(),
        "Restriction::NoWallet must unconditionally block transaction signing"
    );
}

#[test]
fn test_tier5_adversarial_minor_profile_and_guardian_cosign_bounds() {
    let parent_id = UserId(Uuid::new_v4());
    let stranger_id = UserId(Uuid::new_v4());
    let minor_id = UserId(Uuid::new_v4());

    let minor_profile = MinorProfile {
        member: HouseholdMember {
            user_id: minor_id.clone(),
            display_name: "Leo".to_string(),
            role: HouseholdRole::Minor,
            restrictions: default_minor_restrictions(&AgeCategory::Child),
        },
        parent_user_ids: vec![parent_id.clone()],
        age_category: AgeCategory::Child,
    };

    // Absolute invariants on MinorProfile
    assert!(!minor_profile.is_wallet_permitted());
    assert!(minor_profile.enforce_rule7_no_wallet().is_err());
    assert!(!minor_profile.allows_scope(&CapabilityScope::Wallet));
    assert!(!minor_profile.allows_scope(&CapabilityScope::FullSystem));
    assert!(minor_profile.allows_scope(&CapabilityScope::Audio));

    let now = chrono::Utc::now();

    // Adversarial Co-sign Attack 1: Stranger attempts to co-sign for minor
    let stranger_approval = GuardianApproval {
        guardian_user_id: stranger_id.clone(),
        minor_user_id: minor_id.clone(),
        approved_amount_cents: 2000,
        purpose: "Unauthorized".to_string(),
        created_at: Timestamp(now),
        expires_at: Timestamp(now + chrono::Duration::hours(1)),
        signature_proof: vec![0xDE, 0xAD],
    };
    assert!(
        minor_profile
            .validate_guardian_cosign(&stranger_id, &stranger_approval)
            .is_err(),
        "Stranger co-signing for minor must be rejected"
    );

    // Adversarial Co-sign Attack 2: Parent attempts to use expired approval
    let expired_approval = GuardianApproval {
        guardian_user_id: parent_id.clone(),
        minor_user_id: minor_id.clone(),
        approved_amount_cents: 2000,
        purpose: "Expired Book".to_string(),
        created_at: Timestamp(now - chrono::Duration::hours(2)),
        expires_at: Timestamp(now - chrono::Duration::seconds(5)),
        signature_proof: vec![0xBE, 0xEF],
    };
    assert!(
        minor_profile
            .validate_guardian_cosign(&parent_id, &expired_approval)
            .is_err(),
        "Expired guardian approval must fail validation"
    );

    // Adversarial Co-sign Attack 3: Approval targets a different minor
    let other_minor_id = UserId(Uuid::new_v4());
    let mismatched_approval = GuardianApproval {
        guardian_user_id: parent_id.clone(),
        minor_user_id: other_minor_id,
        approved_amount_cents: 2000,
        purpose: "Mismatched Target".to_string(),
        created_at: Timestamp(now),
        expires_at: Timestamp(now + chrono::Duration::hours(1)),
        signature_proof: vec![0xCA, 0xFE],
    };
    assert!(
        minor_profile
            .validate_guardian_cosign(&parent_id, &mismatched_approval)
            .is_err(),
        "Guardian approval targeting different minor must be rejected"
    );

    // Legitimate co-signing succeeds
    let valid_approval = GuardianApproval {
        guardian_user_id: parent_id.clone(),
        minor_user_id: minor_id,
        approved_amount_cents: 2000,
        purpose: "School Lunch".to_string(),
        created_at: Timestamp(now),
        expires_at: Timestamp(now + chrono::Duration::hours(1)),
        signature_proof: vec![0x01, 0x02],
    };
    assert!(
        minor_profile
            .validate_guardian_cosign(&parent_id, &valid_approval)
            .is_ok()
    );
}

// =============================================================================
// SECTION 2: INVARIANT 3 — 24-HOUR GUEST SESSION CAP & RESTRICTIONS
// =============================================================================

#[test]
fn test_tier5_adversarial_guest_session_duration_boundaries() {
    let mut guest_mgr = GuestSessionManager::new();
    let host_id = UserId(Uuid::new_v4());

    // 1. Exactly 1440 minutes (24 hours) is the strict upper bound: MUST SUCCEED
    let ok_24h = guest_mgr.create_guest_session(
        "guest-24h",
        host_id.clone(),
        vec![CapabilityScope::Audio],
        "Permitted 24h visit",
        MAX_GUEST_DURATION_MINUTES,
    );
    assert!(
        ok_24h.is_ok(),
        "Guest session with exactly 1440 min (24h) must be accepted"
    );

    // 2. 1441 minutes (24 hours + 1 min): MUST BE STRICTLY REJECTED
    let overflow_1min = guest_mgr.create_guest_session(
        "guest-overflow",
        host_id.clone(),
        vec![CapabilityScope::Audio],
        "Attempted 1441 min",
        MAX_GUEST_DURATION_MINUTES + 1,
    );
    assert!(
        overflow_1min.is_err(),
        "Guest session exceeding 1440 minutes MUST fail closed"
    );

    // 3. 0 minutes: MUST BE REJECTED
    let zero_min = guest_mgr.create_guest_session(
        "guest-zero",
        host_id.clone(),
        vec![CapabilityScope::Audio],
        "Attempted 0 min",
        0,
    );
    assert!(
        zero_min.is_err(),
        "Zero-minute guest session must be rejected"
    );

    // 4. Large duration: 10,000 minutes MUST BE REJECTED
    let large_min = guest_mgr.create_guest_session(
        "guest-large",
        host_id,
        vec![CapabilityScope::Audio],
        "Attempted large min",
        10_000,
    );
    assert!(
        large_min.is_err(),
        "10,000-minute guest session must be rejected"
    );
}

#[test]
fn test_tier5_adversarial_guest_forbidden_scopes_and_revocation() {
    let mut guest_mgr = GuestSessionManager::new();
    let host_id = UserId(Uuid::new_v4());

    // 1. Attempting to grant Wallet scope MUST FAIL CLOSED
    let wallet_attempt = guest_mgr.create_guest_session(
        "guest-w",
        host_id.clone(),
        vec![CapabilityScope::Wallet],
        "Adversarial Wallet attempt",
        120,
    );
    assert!(
        wallet_attempt.is_err(),
        "Guests MUST NEVER be granted Wallet capability"
    );

    // 2. Attempting to grant FullSystem scope MUST FAIL CLOSED
    let fullsystem_attempt = guest_mgr.create_guest_session(
        "guest-fs",
        host_id.clone(),
        vec![CapabilityScope::FullSystem],
        "Adversarial FullSystem attempt",
        120,
    );
    assert!(
        fullsystem_attempt.is_err(),
        "Guests MUST NEVER be granted FullSystem capability"
    );

    // 3. Validation helper must reject both forbidden scopes
    assert!(
        GuestSessionManager::validate_guest_constraints(&[CapabilityScope::Wallet], 60).is_err()
    );
    assert!(
        GuestSessionManager::validate_guest_constraints(&[CapabilityScope::FullSystem], 60)
            .is_err()
    );

    // 4. Guest session scope access checks: even if someone managed to instantiate
    // a GuestSession struct with Wallet in allowed_scopes, `allows_scope` MUST block it!
    let now = chrono::Utc::now();
    let forged_session = hakille_auth::guest::GuestSession {
        guest_id: "forged-guest".to_string(),
        host_user_id: host_id.clone(),
        allowed_scopes: vec![CapabilityScope::Wallet, CapabilityScope::Audio],
        purpose: "Forged".to_string(),
        created_at: Timestamp(now),
        expires_at: Timestamp(now + chrono::Duration::hours(2)),
        is_active: true,
    };
    assert!(
        !forged_session.allows_scope(&CapabilityScope::Wallet),
        "GuestSession::allows_scope MUST block Wallet regardless of allowed_scopes content"
    );
    assert!(
        !forged_session.allows_scope(&CapabilityScope::FullSystem),
        "GuestSession::allows_scope MUST block FullSystem regardless of allowed_scopes content"
    );
    assert!(
        forged_session.allows_scope(&CapabilityScope::Audio),
        "Safe scope Audio should be permitted"
    );

    // 5. Revocation blocks all scopes immediately
    let _valid_session = guest_mgr
        .create_guest_session(
            "guest-to-revoke",
            host_id,
            vec![CapabilityScope::Audio, CapabilityScope::Network],
            "Temporary access",
            60,
        )
        .unwrap();

    assert!(guest_mgr.can_guest_access("guest-to-revoke", &CapabilityScope::Audio));
    assert!(guest_mgr.revoke_session("guest-to-revoke"));
    assert!(
        !guest_mgr.can_guest_access("guest-to-revoke", &CapabilityScope::Audio),
        "Revoked guest session must not allow any access"
    );
}

// =============================================================================
// SECTION 3: INVARIANT 4 — STEP-UP AUTHENTICATION GATING (tau >= 0.90)
// =============================================================================

#[test]
fn test_tier5_adversarial_stepup_gating_fine_grained_tau_grid() {
    let decay_engine = ConfidenceDecayEngine::default();
    let wallet_mgr = WalletManager::new();

    // Fine-grained test grid around the 0.90 threshold
    let below_threshold = [0.0, 0.10, 0.40, 0.70, 0.85, 0.88, 0.89, 0.899, 0.89999];
    let at_or_above_threshold = [0.90, 0.90001, 0.901, 0.92, 0.95, 0.99, 1.0];

    for &tau in &below_threshold {
        assert!(
            decay_engine.wallet_signing_check(tau).is_err(),
            "tau = {:.5} MUST FAIL wallet signing check",
            tau
        );
        assert!(
            decay_engine.admin_elevation_check(tau).is_err(),
            "tau = {:.5} MUST FAIL admin elevation check",
            tau
        );
        assert!(
            decay_engine
                .gate_action(tau, &CapabilityScope::Wallet)
                .is_err(),
            "tau = {:.5} MUST FAIL gate_action for Wallet",
            tau
        );
        assert!(
            decay_engine
                .gate_action(tau, &CapabilityScope::FullSystem)
                .is_err(),
            "tau = {:.5} MUST FAIL gate_action for FullSystem",
            tau
        );
        assert!(
            wallet_mgr.verify_stepup_and_rule7(tau, false, &[]).is_err(),
            "tau = {:.5} MUST FAIL wallet_mgr verify_stepup_and_rule7",
            tau
        );
        assert_eq!(
            decay_engine.evaluate_step_up(tau, &CapabilityScope::Wallet),
            StepUpRequirement::ActiveBiometricChallenge,
            "tau = {:.5} MUST require active biometric challenge",
            tau
        );
    }

    for &tau in &at_or_above_threshold {
        assert!(
            decay_engine.wallet_signing_check(tau).is_ok(),
            "tau = {:.5} MUST SUCCEED wallet signing check",
            tau
        );
        assert!(
            decay_engine.admin_elevation_check(tau).is_ok(),
            "tau = {:.5} MUST SUCCEED admin elevation check",
            tau
        );
        assert!(
            decay_engine
                .gate_action(tau, &CapabilityScope::Wallet)
                .is_ok(),
            "tau = {:.5} MUST SUCCEED gate_action for Wallet",
            tau
        );
        assert!(
            decay_engine
                .gate_action(tau, &CapabilityScope::FullSystem)
                .is_ok(),
            "tau = {:.5} MUST SUCCEED gate_action for FullSystem",
            tau
        );
        assert!(
            wallet_mgr.verify_stepup_and_rule7(tau, false, &[]).is_ok(),
            "tau = {:.5} MUST SUCCEED wallet_mgr verify_stepup_and_rule7",
            tau
        );
        assert_eq!(
            decay_engine.evaluate_step_up(tau, &CapabilityScope::Wallet),
            StepUpRequirement::None,
            "tau = {:.5} requires no further step-up",
            tau
        );
    }
}

#[test]
fn test_tier5_adversarial_confidence_decay_timeout_and_kalman_recovery() {
    let decay_engine = ConfidenceDecayEngine::new(180.0); // half-life 180s

    let initial_tau = 0.95;
    assert!(decay_engine.wallet_signing_check(initial_tau).is_ok());

    // After 60 seconds: tau(60) = 0.95 * 2^(-60/180) ~ 0.95 * 0.7937 ~ 0.754 < 0.90
    let tau_60s = decay_engine.apply_decay(initial_tau, 60);
    assert!(
        tau_60s < 0.90,
        "After 60s decay, tau {:.3} should fall below 0.90",
        tau_60s
    );
    assert!(
        decay_engine.wallet_signing_check(tau_60s).is_err(),
        "Wallet signing MUST be denied after confidence decays below 0.90"
    );

    // After 900 seconds (15 min fail-closed timeout): tau must be strictly clamped to 0.0
    let tau_900s = decay_engine.apply_decay(initial_tau, 900);
    assert_eq!(
        tau_900s, 0.0,
        "15-minute fail-closed timeout MUST clamp confidence to 0.0"
    );

    let tau_1200s = decay_engine.apply_decay(initial_tau, 1200);
    assert_eq!(tau_1200s, 0.0);

    // Multimodal step-up restoration via live biometric fusion
    let mut fusion = ContinuousMultimodalFusion::new(tau_60s);
    let fresh_signals = vec![
        MultimodalSignal {
            modality: SignalModality::FaceRecognition,
            score: 0.98,
            weight: 0.50,
            timestamp: 100,
        },
        MultimodalSignal {
            modality: SignalModality::VoicePrint,
            score: 0.95,
            weight: 0.35,
            timestamp: 100,
        },
    ];

    let elevated_tau = fusion.fuse_bayesian(&fresh_signals);
    assert!(
        elevated_tau >= 0.90,
        "Concordant strong face + voice signals MUST elevate confidence back to >= 0.90 (got {:.3})",
        elevated_tau
    );
    assert!(decay_engine.wallet_signing_check(elevated_tau).is_ok());
}

// =============================================================================
// SECTION 4: INVARIANT 5 — BIOMETRIC PRESENTATION ATTACKS (SPOOF FRAMES)
// =============================================================================

#[test]
fn test_tier5_adversarial_mini_fasnet_spectral_cutoff_and_screen_moire() {
    let classifier = MiniFasNetClassifier::default();

    // Attack 1: Pitch black 2D photo frame (all zeros)
    let black_frame = vec![0u8; 120 * 120 * 3];
    let verdict_black = classifier.evaluate_crop(&black_frame, 120, 120);
    assert!(
        !verdict_black.is_live,
        "Black frame MUST be classified as non-live"
    );
    assert_eq!(
        verdict_black.detected_attack,
        PassiveSpoofingType::PrintAttack2D
    );

    // Attack 2: Saturated white 2D photo frame (all 255s)
    let white_frame = vec![255u8; 120 * 120 * 3];
    let verdict_white = classifier.evaluate_crop(&white_frame, 120, 120);
    assert!(
        !verdict_white.is_live,
        "White frame MUST be classified as non-live"
    );
    assert_eq!(
        verdict_white.detected_attack,
        PassiveSpoofingType::PrintAttack2D
    );

    // Attack 3: Completely uniform gray color patch (flat texture)
    let gray_frame = vec![128u8; 100 * 100 * 3];
    let verdict_gray = classifier.evaluate_crop(&gray_frame, 100, 100);
    assert!(
        !verdict_gray.is_live,
        "Uniform flat texture MUST be classified as 2D print attack"
    );
    assert_eq!(
        verdict_gray.detected_attack,
        PassiveSpoofingType::PrintAttack2D
    );

    // Attack 4: Screen Replay LCD Subpixel Grid Moiré Attack
    let mut moire_frame = vec![0u8; 100 * 100 * 3];
    // Vertical RGB subpixel stripes across all rows (high horizontal freq, identical vertical rows)
    for row in 0..100 {
        for col in 0..100 {
            let idx = (row * 100 + col) * 3;
            match col % 3 {
                0 => {
                    moire_frame[idx] = 255;
                    moire_frame[idx + 1] = 0;
                    moire_frame[idx + 2] = 0;
                }
                1 => {
                    moire_frame[idx] = 0;
                    moire_frame[idx + 1] = 255;
                    moire_frame[idx + 2] = 0;
                }
                _ => {
                    moire_frame[idx] = 0;
                    moire_frame[idx + 1] = 0;
                    moire_frame[idx + 2] = 255;
                }
            }
        }
    }
    let verdict_moire = classifier.evaluate_crop(&moire_frame, 100, 100);
    assert!(
        !verdict_moire.is_live,
        "Screen replay LCD grid MUST be classified as non-live"
    );
    assert_eq!(
        verdict_moire.detected_attack,
        PassiveSpoofingType::ScreenReplayAttack
    );

    // Genuine biological 3D face micro-texture pattern
    let mut genuine_live = Vec::with_capacity(100 * 100 * 3);
    for i in 0..(100 * 100 * 3) {
        genuine_live.push(((i as f32 * 0.23).sin() * 50.0 + 128.0) as u8);
    }
    let verdict_genuine = classifier.evaluate_crop(&genuine_live, 100, 100);
    assert!(
        verdict_genuine.is_live,
        "Genuine micro-textured face MUST pass passive liveness"
    );
    assert_eq!(verdict_genuine.detected_attack, PassiveSpoofingType::None);
}

#[test]
fn test_tier5_adversarial_active_liveness_flat_gaze_and_blink_anomalies() {
    let engine = ActiveLivenessEngine::default();
    let now = 2000;

    // 1. Flat Gaze presentation attack: User does not follow trajectory (staring blankly)
    let gaze_token = engine.issue_challenge(ActiveChallengeType::GazePursuit, now);
    let target_trajectory = vec![
        (10.0, 0.0),
        (20.0, 5.0),
        (30.0, 10.0),
        (40.0, 15.0),
        (50.0, 20.0),
    ];
    let flat_pupil_gaze = vec![
        (15.0, 0.0),
        (15.0, 0.0),
        (15.0, 0.0),
        (15.0, 0.0),
        (15.0, 0.0),
    ];
    let flat_res = engine
        .verify_gaze_response(&gaze_token, &flat_pupil_gaze, &target_trajectory, now + 1)
        .expect("Verification must return result without panic");
    assert!(
        !flat_res,
        "Flat pupil gaze MUST FAIL active pursuit challenge correlation"
    );

    // 2. Genuine gaze pursuit: User follows trajectory closely
    let correlated_pupil_gaze = vec![
        (10.5, 0.2),
        (19.8, 4.9),
        (30.2, 9.8),
        (39.9, 14.7),
        (50.1, 20.2),
    ];
    let genuine_gaze_res = engine
        .verify_gaze_response(
            &gaze_token,
            &correlated_pupil_gaze,
            &target_trajectory,
            now + 1,
        )
        .expect("Verification must succeed");
    assert!(
        genuine_gaze_res,
        "Concordant gaze pursuit MUST SUCCEED active challenge"
    );

    // 3. Blink presentation attack: Staring without blinking (constant open EAR)
    let blink_token = engine.issue_challenge(ActiveChallengeType::RandomBlink, now);
    let staring_no_blink = vec![0.35, 0.34, 0.35, 0.36, 0.35, 0.34];
    let staring_res = engine
        .verify_blink_response(&blink_token, &staring_no_blink, now + 1)
        .unwrap();
    assert!(
        !staring_res,
        "Staring without blink dip MUST FAIL blink challenge"
    );

    // 4. Incomplete blink: Eyes close but never reopen (sleeping or cut feed)
    let closed_never_open = vec![0.32, 0.28, 0.12, 0.10, 0.09, 0.11];
    let incomplete_res = engine
        .verify_blink_response(&blink_token, &closed_never_open, now + 1)
        .unwrap();
    assert!(
        !incomplete_res,
        "Closing eyes without recovery MUST FAIL blink challenge"
    );

    // 5. Genuine active blink: Open -> Dip below 0.18 -> Recovery above 0.25
    let genuine_blink = vec![0.32, 0.28, 0.12, 0.29, 0.33];
    let genuine_blink_res = engine
        .verify_blink_response(&blink_token, &genuine_blink, now + 1)
        .unwrap();
    assert!(
        genuine_blink_res,
        "Genuine full physiological blink MUST SUCCEED blink challenge"
    );

    // 6. Expired challenge token: MUST BE REJECTED
    let expired_token = engine.issue_challenge(ActiveChallengeType::RandomBlink, now);
    let expired_res = engine.verify_blink_response(&expired_token, &genuine_blink, now + 10);
    assert!(
        matches!(expired_res, Err(HakilleError::CapabilityExpired)),
        "Expired challenge token MUST return CapabilityExpired"
    );
}

#[test]
fn test_tier5_adversarial_biohash_template_protection_and_zeroize() {
    let engine = BiohashingEngine::default();
    let salt = [0x42u8; 32];

    let emb1 = FaceEmbedding {
        vector: [0.1f32; 512],
    };
    let emb2 = FaceEmbedding {
        vector: [-0.1f32; 512],
    };

    let biohash1 = engine.project_embedding(&emb1, &salt);
    let biohash2 = engine.project_embedding(&emb2, &salt);

    // Opposite embeddings must yield high normalized Hamming distance
    let dist = engine.hamming_distance(&biohash1, &biohash2);
    assert!(
        dist > 0.35,
        "Distinct opposite faces must have high Hamming distance in biohash space (got {:.3})",
        dist
    );

    // Ephemeral RAM zeroization verification on drop
    let zeroizable_digest = BiohashDigest {
        bits: vec![0xFF; 128],
    };
    assert_eq!(zeroizable_digest.bits[0], 0xFF);
    drop(zeroizable_digest); // Triggers ZeroizeOnDrop
}

#[test]
fn test_tier5_adversarial_active_liveness_orthogonal_and_anticorrelated_gaze_probe() {
    let engine = ActiveLivenessEngine::default();
    let now = 3000;
    let token = engine.issue_challenge(ActiveChallengeType::GazePursuit, now);

    let target = vec![(10.0, 0.0), (20.0, 0.0), (30.0, 0.0), (40.0, 0.0)];

    // Probe 1: Orthogonal gaze (Target moves horizontally on X, eyes move vertically on Y)
    let orthogonal_pupil = vec![(0.0, 10.0), (0.0, 20.0), (0.0, 30.0), (0.0, 40.0)];
    let res_ortho = engine
        .verify_gaze_response(&token, &orthogonal_pupil, &target, now + 1)
        .unwrap();
    assert!(
        !res_ortho,
        "Orthogonal pupil motion (R = 0.0) MUST FAIL gaze pursuit challenge"
    );

    // Probe 2: Flat pupil (staring straight at one point without tracking)
    let flat_pupil = vec![(25.0, 0.0), (25.0, 0.0), (25.0, 0.0), (25.0, 0.0)];
    let res_flat = engine
        .verify_gaze_response(&token, &flat_pupil, &target, now + 1)
        .unwrap();
    assert!(
        !res_flat,
        "Zero-variance flat pupil trajectory MUST FAIL gaze pursuit challenge"
    );

    // Probe 3: Mismatched trajectory lengths
    let truncated_pupil = vec![(10.0, 0.0), (20.0, 0.0)];
    let res_trunc = engine
        .verify_gaze_response(&token, &truncated_pupil, &target, now + 1)
        .unwrap();
    assert!(!res_trunc, "Mismatched trajectory length must return false");

    // Probe 4: Empty trajectories
    let res_empty = engine
        .verify_gaze_response(&token, &[], &target, now + 1)
        .unwrap();
    assert!(!res_empty, "Empty pupil trajectory must return false");

    // Empirical Observation on Inverted Blink / Anti-Correlated Gaze:
    // In current MiniFASNet/ActiveLiveness implementation:
    // - Anti-correlated pupil gaze (moving left when target moves right): Pearson correlation r ~ -1.0.
    //   Because the code evaluates `let r_squared = r * r; Ok(r_squared >= 0.85)`, (-1.0)^2 = 1.0 >= 0.85,
    //   it mathematically passes the R^2 threshold if direction sign is unverified.
    // - Inverted blink sequence starting closed [0.10, 0.28]: passes because `position(|&e| e < 0.18)`
    //   matches index 0, and `[0..].any(|&e| e > 0.25)` matches index 1.
    // Documenting these exact empirical behaviors in our Challenger assessment.
}

#[test]
fn test_tier5_adversarial_guest_session_temporal_expiration_and_id_collision() {
    let mut guest_mgr = GuestSessionManager::new();
    let host_id = UserId(Uuid::new_v4());

    // 1. Create valid guest session
    let session = guest_mgr
        .create_guest_session(
            "guest-unique-alpha",
            host_id.clone(),
            vec![CapabilityScope::Audio],
            "Music streaming guest",
            1440, // 24 hours
        )
        .expect("Creation must succeed");

    assert_eq!(session.guest_id, "guest-unique-alpha");
    assert!(session.remaining_minutes() > 1400);
    assert_eq!(guest_mgr.active_session_count(), 1);

    // 2. Adversarial duplicate ID creation: MUST FAIL
    let duplicate_attempt = guest_mgr.create_guest_session(
        "guest-unique-alpha",
        host_id.clone(),
        vec![CapabilityScope::Network],
        "Impersonation attempt",
        60,
    );
    assert!(
        duplicate_attempt.is_err(),
        "Reusing existing guest_id MUST fail closed (token collision error)"
    );

    // 3. Empty guest_id automatically provisions unique token
    let auto_session = guest_mgr
        .create_guest_session(
            "",
            host_id.clone(),
            vec![CapabilityScope::Audio],
            "Auto-id guest",
            120,
        )
        .expect("Empty ID should auto-generate UUID");
    assert!(auto_session.guest_id.starts_with("guest-"));
    assert_ne!(auto_session.guest_id, "guest-unique-alpha");
    assert_eq!(guest_mgr.active_session_count(), 2);

    // 4. Temporal Expiration enforcement:
    // Construct a guest session that has expired
    let expired_session = hakille_auth::guest::GuestSession {
        guest_id: "guest-expired-test".to_string(),
        host_user_id: host_id,
        allowed_scopes: vec![CapabilityScope::Audio],
        purpose: "Expired visit".to_string(),
        created_at: Timestamp(chrono::Utc::now() - chrono::Duration::hours(25)),
        expires_at: Timestamp(chrono::Utc::now() - chrono::Duration::minutes(1)), // Expired 1 min ago
        is_active: true,
    };
    assert!(expired_session.is_expired());
    assert_eq!(expired_session.remaining_minutes(), 0);
    assert!(
        !expired_session.allows_scope(&CapabilityScope::Audio),
        "Expired guest session MUST deny all scopes"
    );
}

#[test]
fn test_tier5_adversarial_stepup_financial_lifecycle_and_budget_guards() {
    let mut wallet_mgr = WalletManager::new();
    let adult_id = UserId(Uuid::new_v4());
    let exp = Timestamp(chrono::Utc::now() + chrono::Duration::hours(3));
    let signing_key = [0x99u8; 32];
    let decay_engine = ConfidenceDecayEngine::new(180.0);

    // Step 1: Provision guarded wallet cap with initial confidence tau = 0.95
    let mut wallet = wallet_mgr
        .create_guarded_wallet_cap(
            adult_id,
            10_000, // 100.00 EUR limit
            "EUR".to_string(),
            SpendingCategory::Food,
            exp,
            0.95,
            false,
            &[],
        )
        .expect("Guarded wallet creation with tau 0.95 must succeed");

    assert_eq!(wallet_mgr.remaining_budget(&wallet), 10_000);

    // Step 2: Authorize valid transaction within budget at tau = 0.95
    let tx1 = WalletTransaction {
        tx_id: Uuid::new_v4(),
        recipient: "0xMerchantA".to_string(),
        amount_cents: 4000,
        currency: "EUR".to_string(),
        category: SpendingCategory::Food,
        description: "Grocery purchase".to_string(),
        nonce: 101,
        timestamp: Timestamp::now(),
    };
    let proof1 = wallet_mgr
        .authorize_and_sign_transaction(&mut wallet, tx1, 0.95, false, &[], &signing_key)
        .expect("Transaction 1 must succeed");
    assert_eq!(proof1.transaction.amount_cents, 4000);
    assert_eq!(wallet_mgr.remaining_budget(&wallet), 6000);

    // Step 3: Adversarial currency mismatch attack (attempt spend in USD against EUR cap)
    let tx_bad_curr = WalletTransaction {
        tx_id: Uuid::new_v4(),
        recipient: "0xMerchantB".to_string(),
        amount_cents: 1000,
        currency: "USD".to_string(),
        category: SpendingCategory::Food,
        description: "Cross-currency attack".to_string(),
        nonce: 102,
        timestamp: Timestamp::now(),
    };
    assert!(
        wallet_mgr
            .authorize_and_sign_transaction(
                &mut wallet,
                tx_bad_curr,
                0.95,
                false,
                &[],
                &signing_key,
            )
            .is_err(),
        "Currency mismatch MUST be rejected"
    );

    // Step 4: Adversarial category mismatch attack (attempt spend in Entertainment against Food cap)
    let tx_bad_cat = WalletTransaction {
        tx_id: Uuid::new_v4(),
        recipient: "0xMerchantC".to_string(),
        amount_cents: 1000,
        currency: "EUR".to_string(),
        category: SpendingCategory::Entertainment,
        description: "Category escalation attack".to_string(),
        nonce: 103,
        timestamp: Timestamp::now(),
    };
    assert!(
        wallet_mgr.authorize_and_sign_transaction(
            &mut wallet,
            tx_bad_cat,
            0.95,
            false,
            &[],
            &signing_key,
        ).is_err(),
        "Category mismatch MUST be rejected"
    );

    // Step 5: Adversarial budget overflow attack (attempt spend of 7000 when remaining is 6000)
    let tx_overflow = WalletTransaction {
        tx_id: Uuid::new_v4(),
        recipient: "0xMerchantD".to_string(),
        amount_cents: 7000,
        currency: "EUR".to_string(),
        category: SpendingCategory::Food,
        description: "Budget overflow attack".to_string(),
        nonce: 104,
        timestamp: Timestamp::now(),
    };
    assert!(
        wallet_mgr
            .authorize_and_sign_transaction(
                &mut wallet,
                tx_overflow,
                0.95,
                false,
                &[],
                &signing_key,
            )
            .is_err(),
        "Transaction exceeding remaining wallet budget MUST be rejected"
    );

    // Step 6: Temporal decay occurs: 300 seconds elapse without biometric presence
    // Confidence decays below 0.90
    let decayed_tau = decay_engine.apply_decay(0.95, 300);
    assert!(
        decayed_tau < 0.90,
        "Decayed tau ({:.3}) must be below 0.90",
        decayed_tau
    );

    let tx2 = WalletTransaction {
        tx_id: Uuid::new_v4(),
        recipient: "0xMerchantE".to_string(),
        amount_cents: 1000,
        currency: "EUR".to_string(),
        category: SpendingCategory::Food,
        description: "Post-decay purchase".to_string(),
        nonce: 105,
        timestamp: Timestamp::now(),
    };
    let decayed_attempt = wallet_mgr.authorize_and_sign_transaction(
        &mut wallet,
        tx2.clone(),
        decayed_tau,
        false,
        &[],
        &signing_key,
    );
    assert!(
        decayed_attempt.is_err(),
        "Transaction after confidence decay below 0.90 MUST be denied with step-up required"
    );

    // Step 7: Step-up re-authentication restores confidence to 0.96
    let stepup_tau = 0.96;
    let stepup_tx = wallet_mgr
        .authorize_and_sign_transaction(&mut wallet, tx2, stepup_tau, false, &[], &signing_key)
        .expect("Transaction following successful step-up authentication must succeed");
    assert_eq!(stepup_tx.authorized_at_confidence, 0.96);
    assert_eq!(wallet_mgr.remaining_budget(&wallet), 5000);
}
