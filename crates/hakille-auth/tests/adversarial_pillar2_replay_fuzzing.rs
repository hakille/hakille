//! Adversarial Stress Testing & Invariant Fuzzing: Contactless Gesture Nonce & Replay Attacks
//!
//! Validates anti-replay defenses, single-use consumption invariants, concurrent race condition
//! resistance, target spoofing rejection, and temporal validity boundaries.

use chrono::Utc;
use hakille_auth::gesture_auth::{CONTACTLESS_AUTH_WINDOW, ContactlessGestureAuthManager};
use hakille_core::HakilleError;
use hakille_core::capability::{
    CapabilityAction, CapabilityAuthority, CapabilityScope, CapabilityToken,
};
use hakille_core::types::Timestamp;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

fn make_valid_token(duration_secs: i64) -> CapabilityToken {
    let authority = CapabilityAuthority::new();
    let expires_at = Timestamp(Utc::now() + chrono::Duration::seconds(duration_secs));
    authority
        .create_root_token(
            vec![CapabilityScope::Wallet, CapabilityScope::Filesystem],
            vec![CapabilityAction::Execute, CapabilityAction::Read],
            expires_at,
        )
        .expect("Test token creation must succeed")
}

#[test]
fn test_adversarial_replay_attack_same_challenge_immediate() {
    let mut manager = ContactlessGestureAuthManager::new();
    let token = make_valid_token(3600);
    let t0 = Instant::now();

    let challenge = manager.register_challenge(
        "critical_tx_transfer_001",
        token,
        CapabilityScope::Wallet,
        CapabilityAction::Execute,
        t0,
    );

    // 1st validation (legitimate)
    let verdict1 = manager.validate_authorization(
        challenge.challenge_id,
        "critical_tx_transfer_001",
        true,
        t0 + Duration::from_millis(300),
    );
    assert!(verdict1.is_ok(), "Legitimate first validation must succeed");
    let v1 = verdict1.unwrap();
    assert_eq!(v1.action_nonce, challenge.action_nonce);

    // 2nd validation (immediate replay attempt with identical parameters)
    let verdict2 = manager.validate_authorization(
        challenge.challenge_id,
        "critical_tx_transfer_001",
        true,
        t0 + Duration::from_millis(305),
    );
    assert!(
        verdict2.is_err(),
        "Immediate replay of consumed challenge must fail"
    );
    match verdict2.unwrap_err() {
        HakilleError::CapabilityDenied(msg) => {
            assert!(
                msg.contains("inconnu ou déjà consommé"),
                "Expected consumed challenge rejection, got: {msg}"
            );
        }
        err => panic!("Unexpected error type: {:?}", err),
    }

    // 3rd validation (delayed replay attempt)
    let verdict3 = manager.validate_authorization(
        challenge.challenge_id,
        "critical_tx_transfer_001",
        true,
        t0 + Duration::from_millis(900),
    );
    assert!(verdict3.is_err(), "Delayed replay must also fail");
}

#[test]
fn test_adversarial_concurrent_race_condition_validation() {
    // 50 threads racing to validate the SAME challenge simultaneously
    let manager = Arc::new(Mutex::new(ContactlessGestureAuthManager::new()));
    let token = make_valid_token(3600);
    let t0 = Instant::now();

    let challenge = manager.lock().unwrap().register_challenge(
        "concurrent_race_target",
        token,
        CapabilityScope::Wallet,
        CapabilityAction::Execute,
        t0,
    );
    let challenge_id = challenge.challenge_id;

    let mut handles = Vec::new();
    for thread_idx in 0..50 {
        let mgr = Arc::clone(&manager);
        let handle = thread::spawn(move || {
            let decision_time = t0 + Duration::from_millis(100 + (thread_idx % 10) * 10);
            let mut guard = mgr.lock().unwrap();
            guard.validate_authorization(
                challenge_id,
                "concurrent_race_target",
                true,
                decision_time,
            )
        });
        handles.push(handle);
    }

    let mut success_count = 0;
    let mut failure_count = 0;

    for handle in handles {
        match handle.join().unwrap() {
            Ok(verdict) => {
                assert!(verdict.is_valid);
                assert_eq!(verdict.challenge_id, challenge_id);
                success_count += 1;
            }
            Err(HakilleError::CapabilityDenied(_)) => {
                failure_count += 1;
            }
            Err(other) => panic!("Unexpected error during concurrent race: {:?}", other),
        }
    }

    assert_eq!(
        success_count, 1,
        "Exactly ONE concurrent thread must win the authorization challenge"
    );
    assert_eq!(
        failure_count, 49,
        "Exactly 49 concurrent racing threads must be denied"
    );
    assert_eq!(
        manager.lock().unwrap().pending_challenges_count(),
        0,
        "Challenge must be completely consumed from manager"
    );
}

#[test]
fn test_adversarial_timing_boundary_fuzzing() {
    let mut manager = ContactlessGestureAuthManager::new();
    let token = make_valid_token(3600);
    let t0 = Instant::now();

    // Challenge 1: Just inside boundary (1199 ms)
    let c1 = manager.register_challenge(
        "target_boundary",
        token.clone(),
        CapabilityScope::Wallet,
        CapabilityAction::Execute,
        t0,
    );
    let res1 = manager.validate_authorization(
        c1.challenge_id,
        "target_boundary",
        true,
        t0 + Duration::from_millis(1199),
    );
    assert!(
        res1.is_ok(),
        "1199ms is within the 1200ms window and must be accepted"
    );

    // Challenge 2: Exactly at boundary (1200 ms)
    let c2 = manager.register_challenge(
        "target_boundary",
        token.clone(),
        CapabilityScope::Wallet,
        CapabilityAction::Execute,
        t0,
    );
    let res2 = manager.validate_authorization(
        c2.challenge_id,
        "target_boundary",
        true,
        t0 + CONTACTLESS_AUTH_WINDOW,
    );
    assert!(
        res2.is_ok(),
        "1200ms is inclusive boundary and must be accepted"
    );

    // Challenge 3: Just past boundary (1201 ms)
    let c3 = manager.register_challenge(
        "target_boundary",
        token.clone(),
        CapabilityScope::Wallet,
        CapabilityAction::Execute,
        t0,
    );
    let res3 = manager.validate_authorization(
        c3.challenge_id,
        "target_boundary",
        true,
        t0 + Duration::from_millis(1201),
    );
    assert!(
        res3.is_err(),
        "1201ms exceeds 1200ms window and must be denied"
    );

    // Challenge 4: Far past boundary (3000 ms)
    let c4 = manager.register_challenge(
        "target_boundary",
        token,
        CapabilityScope::Wallet,
        CapabilityAction::Execute,
        t0,
    );
    let res4 = manager.validate_authorization(
        c4.challenge_id,
        "target_boundary",
        true,
        t0 + Duration::from_millis(3000),
    );
    assert!(res4.is_err(), "3000ms must be denied");
}

#[test]
fn test_adversarial_target_spoofing_matrix() {
    let mut manager = ContactlessGestureAuthManager::new();
    let token = make_valid_token(3600);
    let t0 = Instant::now();

    let spoof_candidates = vec![
        "bank_transfer_tx_123\0",
        "bank_transfer_tx_124",
        "",
        "BANK_TRANSFER_TX_123",
        "bank_transfer_tx_123 ",
        " bank_transfer_tx_123",
        "../bank_transfer_tx_123",
        "bank_transfer_tx_123\n",
    ];

    for candidate in spoof_candidates {
        let challenge = manager.register_challenge(
            "bank_transfer_tx_123",
            token.clone(),
            CapabilityScope::Wallet,
            CapabilityAction::Execute,
            t0,
        );

        let res = manager.validate_authorization(
            challenge.challenge_id,
            candidate,
            true,
            t0 + Duration::from_millis(250),
        );

        assert!(
            res.is_err(),
            "Spoofed target '{}' must be rejected",
            candidate
        );

        // Fail-closed invariant: even if the user now supplies the true target with the consumed challenge, it must fail
        let retry_res = manager.validate_authorization(
            challenge.challenge_id,
            "bank_transfer_tx_123",
            true,
            t0 + Duration::from_millis(260),
        );
        assert!(
            retry_res.is_err(),
            "Challenge consumed on spoofed attempt must remain permanently consumed"
        );
    }
}

#[test]
fn test_adversarial_pruning_and_resurrection_defense() {
    let mut manager = ContactlessGestureAuthManager::new();
    let token = make_valid_token(3600);
    let t0 = Instant::now();

    let mut challenge_ids = Vec::new();
    for i in 0..100 {
        let target = format!("batch_target_{i}");
        let c = manager.register_challenge(
            &target,
            token.clone(),
            CapabilityScope::Filesystem,
            CapabilityAction::Read,
            t0,
        );
        challenge_ids.push((c.challenge_id, target));
    }

    assert_eq!(manager.pending_challenges_count(), 100);

    // Advance time past the 1200ms expiration window
    let t_expired = t0 + Duration::from_millis(1500);
    let pruned = manager.prune_expired_challenges(t_expired);
    assert_eq!(pruned, 100, "All 100 challenges must be pruned");
    assert_eq!(manager.pending_challenges_count(), 0);

    // Attempt resurrection of all 100 pruned challenges
    for (id, target) in challenge_ids {
        let res = manager.validate_authorization(id, &target, true, t_expired);
        assert!(
            res.is_err(),
            "Pruned challenge '{}' must not be resurrectable",
            id
        );
    }
}

#[test]
fn test_adversarial_expired_token_rejected_even_in_rapid_gesture() {
    let mut manager = ContactlessGestureAuthManager::new();
    // Token that expired 1 second before challenge creation
    let expired_token = make_valid_token(-1);
    let t0 = Instant::now();

    let challenge = manager.register_challenge(
        "immediate_action",
        expired_token,
        CapabilityScope::Sensor,
        CapabilityAction::Read,
        t0,
    );

    // Gesture arrives ultra-fast in 50ms
    let res = manager.validate_authorization(
        challenge.challenge_id,
        "immediate_action",
        true,
        t0 + Duration::from_millis(50),
    );

    assert!(res.is_err());
    assert!(matches!(res.unwrap_err(), HakilleError::CapabilityExpired));
}

#[test]
fn test_adversarial_unauthorized_gesture_verdict_rejection() {
    let mut manager = ContactlessGestureAuthManager::new();
    let token = make_valid_token(3600);
    let t0 = Instant::now();

    let challenge = manager.register_challenge(
        "unauthorized_nod",
        token,
        CapabilityScope::Audio,
        CapabilityAction::Write,
        t0,
    );

    // Adversary attempts validation with is_gesture_authorized = false
    let res = manager.validate_authorization(
        challenge.challenge_id,
        "unauthorized_nod",
        false,
        t0 + Duration::from_millis(200),
    );

    assert!(res.is_err());
    match res.unwrap_err() {
        HakilleError::CapabilityDenied(msg) => {
            assert!(msg.contains("n'a pas été validé"));
        }
        err => panic!("Expected CapabilityDenied, got {:?}", err),
    }
}
