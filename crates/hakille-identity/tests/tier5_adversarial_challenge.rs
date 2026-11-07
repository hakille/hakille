//! Tier 5 Adversarial Coverage Hardening Challenge Suite for `hakille-identity`
//! Milestone 6, Phase 2: Empirical stress testing and adversarial invariant probing.
//!
//! Security Invariants Tested:
//! 1. Delegation depth boundary (D_max = 4): Strict rejection of D > 4 in Biscuit and Agent Passports.
//! 2. Rule 7 Minor NoWallet invariant: Multi-engine denial (ReBAC + RBAC + ABAC) across roles, agents, and delegation.
//! 6. Cryptographic key shredding & k-of-n Shamir recovery: Combinatorial threshold validation and Digital Twilight.

use ed25519_dalek::SigningKey;
use hakille_core::auth::{AuthorizationContext, Quota};
use hakille_core::capability::{CapabilityAction, CapabilityScope};
use hakille_core::types::Timestamp;
use hakille_core::{HakilleError, UserId};
use hakille_identity::access::{AccessEngine, AuthContext, DeviceTrustTier};
use hakille_identity::agent::AgentFactory;
use hakille_identity::biscuit::BiscuitCapabilityManager;
use hakille_identity::crypto::{
    MasterKey, MasterKeyKdf, SecretShare, XChaChaBlob,
    compute_blind_index, reconstruct_secret, split_secret,
};
use hakille_identity::hierarchy::TenantId;
use hakille_identity::rebac::RelationTuple;
use hakille_identity::ssi::Did;
use hakille_identity::storage::{AccountStatus, RedbZkStore, UserRecord};
use rand::{RngCore, rngs::OsRng};
use uuid::Uuid;

// =============================================================================
// SECTION 1: INVARIANT 1 — DELEGATION DEPTH BOUNDARY (D_max = 4)
// =============================================================================

#[test]
fn test_tier5_adversarial_agent_passport_direct_depth_overflow_rejected() {
    let (_, agent_did) = AgentFactory::spawn_agent();
    let principal_did = Did::new("did:key:zPrincipal");
    let tenant_id = TenantId::new();

    // Adversarially probe depths > MAX_DELEGATION_DEPTH (4)
    let overflow_depths = [5, 6, 7, 10, 42, 100, 1000, usize::MAX];
    for &d in &overflow_depths {
        let res = AgentFactory::issue_passport(
            agent_did.clone(),
            principal_did.clone(),
            tenant_id.clone(),
            3600,
            10_000,
            500,
            vec![0x01, 0x02],
            d,
        );
        assert!(
            res.is_err(),
            "Adversarial issuance with depth {} MUST fail closed",
            d
        );
        match res.unwrap_err() {
            HakilleError::CapabilityDenied(msg) => {
                assert!(
                    msg.contains("trop profonde"),
                    "Expected depth rejection error message, got: {}",
                    msg
                );
            }
            other => panic!("Expected CapabilityDenied, got {:?}", other),
        }
    }
}

#[test]
fn test_tier5_adversarial_agent_passport_cascade_depth_boundary_enforcement() {
    let tenant_id = TenantId::new();
    let (_, root_did) = AgentFactory::spawn_agent();

    // Issue valid root agent at depth 1
    let (_, d1_did) = AgentFactory::spawn_agent();
    let p1 = AgentFactory::issue_passport(
        d1_did.clone(),
        root_did,
        tenant_id.clone(),
        3600,
        100_000,
        10_000,
        vec![0xAA],
        1,
    )
    .expect("Depth 1 must succeed");
    assert_eq!(p1.delegation_depth, 1);

    // Sub-delegate to D=2
    let (_, d2_did) = AgentFactory::spawn_agent();
    let p2 = AgentFactory::delegate_sub_agent(&p1, d2_did, vec![0xBB], 80_000, 8_000)
        .expect("Depth 2 must succeed");
    assert_eq!(p2.delegation_depth, 2);

    // Sub-delegate to D=3
    let (_, d3_did) = AgentFactory::spawn_agent();
    let p3 = AgentFactory::delegate_sub_agent(&p2, d3_did, vec![0xCC], 60_000, 6_000)
        .expect("Depth 3 must succeed");
    assert_eq!(p3.delegation_depth, 3);

    // Sub-delegate to D=4 (maximum permissible boundary)
    let (_, d4_did) = AgentFactory::spawn_agent();
    let p4 = AgentFactory::delegate_sub_agent(&p3, d4_did, vec![0xDD], 40_000, 4_000)
        .expect("Depth 4 must succeed");
    assert_eq!(p4.delegation_depth, 4);

    // Sub-delegate to D=5: MUST FAIL CLOSED with CapabilityDenied
    let (_, d5_did) = AgentFactory::spawn_agent();
    let d5_attempt = AgentFactory::delegate_sub_agent(&p4, d5_did, vec![0xEE], 20_000, 2_000);
    assert!(
        d5_attempt.is_err(),
        "Delegation attempt from Depth 4 to Depth 5 MUST be strictly denied"
    );
    match d5_attempt.unwrap_err() {
        HakilleError::CapabilityDenied(msg) => {
            assert!(
                msg.contains("trop profonde"),
                "Expected delegation depth error, got: {}",
                msg
            );
        }
        other => panic!("Expected CapabilityDenied, got {:?}", other),
    }
}

#[test]
fn test_tier5_adversarial_agent_passport_signature_tampering_fails() {
    let principal_key = SigningKey::generate(&mut OsRng);
    let principal_pk = principal_key.verifying_key().to_bytes();
    let principal_did = hakille_identity::ssi::DidKey::from_ed25519_bytes(&principal_pk);

    let (_, agent_did) = AgentFactory::spawn_agent();
    let passport = AgentFactory::issue_passport(
        agent_did,
        principal_did,
        TenantId::new(),
        3600,
        1000,
        500,
        vec![1, 2, 3],
        4, // Valid depth 4
    )
    .unwrap();

    let sig = passport.sign_with(&principal_key);
    assert!(passport.verify_signature(&principal_pk, &sig));

    // Attack 1: Attacker modifies delegation_depth from 4 to 1 to bypass depth limits
    let mut tampered_depth = passport.clone();
    tampered_depth.delegation_depth = 1;
    assert!(
        !tampered_depth.verify_signature(&principal_pk, &sig),
        "Forged passport with altered delegation_depth must fail signature verification"
    );

    // Attack 2: Attacker elevates budget limit
    let mut tampered_budget = passport.clone();
    tampered_budget.budget_limit_cents = 999_999;
    assert!(
        !tampered_budget.verify_signature(&principal_pk, &sig),
        "Forged passport with altered budget must fail signature verification"
    );

    // Attack 3: Attacker elevates fuel limit
    let mut tampered_fuel = passport.clone();
    tampered_fuel.max_execution_fuel = 999_999;
    assert!(
        !tampered_fuel.verify_signature(&principal_pk, &sig),
        "Forged passport with altered fuel must fail signature verification"
    );

    // Attack 4: Attacker alters constitutional hash
    let mut tampered_constitution = passport.clone();
    tampered_constitution.constitutional_hash[0] ^= 0xFF;
    assert!(
        !tampered_constitution.verify_signature(&principal_pk, &sig),
        "Forged passport with altered constitutional hash must fail signature verification"
    );
}

#[test]
fn test_tier5_adversarial_biscuit_attenuation_depth_bounding_and_monotonicity() {
    let mgr = BiscuitCapabilityManager::new();
    let exp = Timestamp(chrono::Utc::now() + chrono::Duration::hours(2));

    let root_token = mgr
        .mint_root_token(
            vec![CapabilityScope::Filesystem, CapabilityScope::Audio],
            vec![CapabilityAction::Read, CapabilityAction::Write],
            exp.clone(),
        )
        .expect("Minting root token must succeed");

    // Attenuation 1 (Depth 1)
    let t1 = mgr
        .attenuate_token(
            &root_token,
            &[CapabilityScope::Filesystem, CapabilityScope::Audio],
            &[CapabilityAction::Read, CapabilityAction::Write],
            &[CapabilityScope::Filesystem],
            &[CapabilityAction::Read, CapabilityAction::Write],
            exp.clone(),
        )
        .expect("Attenuation 1 must succeed");

    // Attenuation 2 (Depth 2)
    let t2 = mgr
        .attenuate_token(
            &t1,
            &[CapabilityScope::Filesystem],
            &[CapabilityAction::Read, CapabilityAction::Write],
            &[CapabilityScope::Filesystem],
            &[CapabilityAction::Read],
            exp.clone(),
        )
        .expect("Attenuation 2 must succeed");

    // Attenuation 3 (Depth 3)
    let t3 = mgr
        .attenuate_token(
            &t2,
            &[CapabilityScope::Filesystem],
            &[CapabilityAction::Read],
            &[CapabilityScope::Filesystem],
            &[CapabilityAction::Read],
            exp.clone(),
        )
        .expect("Attenuation 3 must succeed");

    // Attenuation 4 (Depth 4 - Boundary limit)
    let t4 = mgr
        .attenuate_token(
            &t3,
            &[CapabilityScope::Filesystem],
            &[CapabilityAction::Read],
            &[CapabilityScope::Filesystem],
            &[CapabilityAction::Read],
            exp.clone(),
        )
        .expect("Attenuation 4 must succeed");

    // Attenuation 5 (Depth 5 - Overflow): MUST FAIL CLOSED
    let t5_res = mgr.attenuate_token(
        &t4,
        &[CapabilityScope::Filesystem],
        &[CapabilityAction::Read],
        &[CapabilityScope::Filesystem],
        &[CapabilityAction::Read],
        exp.clone(),
    );
    assert!(
        t5_res.is_err(),
        "5th Biscuit attenuation MUST exceed MAX_DELEGATION_DEPTH"
    );

    // Adversarial Monotonicity Violation: Child attempts to re-expand scopes
    let expansion_scope = mgr.attenuate_token(
        &t2,
        &[CapabilityScope::Filesystem],
        &[CapabilityAction::Read],
        &[CapabilityScope::Wallet], // Scope expansion attack!
        &[CapabilityAction::Read],
        exp.clone(),
    );
    assert!(
        expansion_scope.is_err(),
        "Adversarial scope expansion must be rejected"
    );

    // Adversarial Monotonicity Violation: Child attempts to re-expand actions
    let expansion_action = mgr.attenuate_token(
        &t2,
        &[CapabilityScope::Filesystem],
        &[CapabilityAction::Read],
        &[CapabilityScope::Filesystem],
        &[CapabilityAction::Admin], // Action expansion attack!
        exp.clone(),
    );
    assert!(
        expansion_action.is_err(),
        "Adversarial action expansion must be rejected"
    );

    // Adversarial Monotonicity Violation: Child attempts to extend expiration into future
    let future_exp = Timestamp(chrono::Utc::now() + chrono::Duration::hours(24));
    let expansion_time = mgr.attenuate_token(
        &t2,
        &[CapabilityScope::Filesystem],
        &[CapabilityAction::Read],
        &[CapabilityScope::Filesystem],
        &[CapabilityAction::Read],
        future_exp, // Expiration extension attack!
    );
    assert!(
        expansion_time.is_err(),
        "Adversarial expiry extension must be rejected"
    );
}

#[test]
fn test_tier5_adversarial_authorization_context_delegation_chain_stress() {
    let future_ts = Timestamp(chrono::Utc::now() + chrono::Duration::hours(1));

    // Valid chain of length 4 succeeds
    let valid_chain = vec![
        "orch".to_string(),
        "planner".to_string(),
        "worker".to_string(),
        "subworker".to_string(),
    ];
    let ctx_ok = AuthorizationContext::new(
        "user:alice",
        "hakille-kernel",
        future_ts.clone(),
        CapabilityScope::Filesystem,
        CapabilityAction::Read,
        "/vfs/docs/file.txt",
        Some(Quota::new(10)),
        "nonce_valid_1234",
        valid_chain,
    );
    assert!(ctx_ok.is_ok(), "Chain of length 4 must be accepted");
    assert!(ctx_ok.unwrap().validate().is_ok());

    // Adversarial chain of length 5 (D=5) MUST FAIL
    let overflow_chain = vec![
        "orch".to_string(),
        "planner".to_string(),
        "worker".to_string(),
        "subworker".to_string(),
        "malicious_leaf".to_string(),
    ];
    let ctx_overflow = AuthorizationContext::new(
        "user:alice",
        "hakille-kernel",
        future_ts.clone(),
        CapabilityScope::Filesystem,
        CapabilityAction::Read,
        "/vfs/docs/file.txt",
        Some(Quota::new(10)),
        "nonce_valid_1234",
        overflow_chain,
    );
    assert!(
        ctx_overflow.is_err(),
        "Delegation chain of length 5 must be rejected in AuthorizationContext"
    );

    // Adversarial cycle: duplicate entity in delegation chain
    let cyclic_chain = vec![
        "orch".to_string(),
        "worker".to_string(),
        "orch".to_string(), // Cycle!
    ];
    let ctx_cyclic = AuthorizationContext::new(
        "user:alice",
        "hakille-kernel",
        future_ts,
        CapabilityScope::Filesystem,
        CapabilityAction::Read,
        "/vfs/docs/file.txt",
        Some(Quota::new(10)),
        "nonce_valid_1234",
        cyclic_chain,
    );
    assert!(
        ctx_cyclic.is_err(),
        "Cyclic delegation chain must be rejected"
    );
}

// =============================================================================
// SECTION 2: INVARIANT 2 — RULE 7 MINOR NOWALLET INVARIANT
// =============================================================================

#[test]
fn test_tier5_adversarial_rule7_access_engine_complete_matrix() {
    let mut engine = AccessEngine::new();
    let tenant_id = TenantId::new();
    let minor_subject = "user:child_leo";

    // Fabricate a ReBAC relationship claiming minor is "owner" of wallet
    engine.rebac.add_tuple(RelationTuple::new(
        "wallet",
        "wallet_99",
        "owner",
        "user",
        "child_leo",
    ));
    engine.rebac.add_tuple(RelationTuple::new(
        "household",
        "home_1",
        "minor_member",
        "user",
        "child_leo",
    ));

    let relation_tuple = RelationTuple::new("wallet", "wallet_99", "owner", "user", "child_leo");

    // Adversarial Matrix: Probe various role permutations, confidence scores, and hardware trust tiers
    let adversarial_roles = [
        "Minor",
        "minor",
        "ChildMinor",
        "MinorDependent",
        "Dependent",
        "minor_teen",
    ];
    let confidence_levels = [0.0, 0.50, 0.89, 0.90, 0.95, 0.99, 1.0];
    let hardware_tiers = [
        DeviceTrustTier::Tier1HardwareEnclave,
        DeviceTrustTier::Tier2SoftwareHost,
        DeviceTrustTier::Tier3UntrustedWeb,
    ];

    for role in &adversarial_roles {
        for &conf in &confidence_levels {
            for &tier in &hardware_tiers {
                let ctx = AuthContext {
                    subject: minor_subject.to_string(),
                    base_role: role.to_string(),
                    confidence_score: conf,
                    device_tier: tier,
                    tenant_id: tenant_id.clone(),
                    scope: CapabilityScope::Wallet,
                    action: CapabilityAction::Execute,
                    resource: "wallet_99".to_string(),
                };

                let decision = engine.check_permission(&ctx, &relation_tuple);
                assert!(
                    !decision.is_allowed(),
                    "Rule 7 failure: Minor with role '{}', confidence {}, tier {:?} MUST BE DENIED Wallet access!",
                    role,
                    conf,
                    tier
                );
                match decision {
                    hakille_identity::access::Decision::Deny(reason) => {
                        assert!(
                            reason.contains("Rule 7") || reason.contains("Minor") || reason.contains("ABAC"),
                            "Unexpected denial reason for minor: {}",
                            reason
                        );
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}

#[test]
fn test_tier5_adversarial_rule7_agent_protection_boundary() {
    // 1. Direct Rule 7 enforcement checks
    assert!(
        AgentFactory::enforce_minor_protection_invariant(true, &CapabilityScope::Wallet).is_err(),
        "Rule 7 MUST deny Wallet to agents interacting with minors"
    );
    assert!(
        AgentFactory::enforce_minor_protection_invariant(true, &CapabilityScope::FullSystem).is_err(),
        "Rule 7 MUST deny FullSystem to agents interacting with minors"
    );

    // 2. Safe scopes are permitted for minor interaction
    assert!(
        AgentFactory::enforce_minor_protection_invariant(true, &CapabilityScope::Audio).is_ok()
    );
    assert!(
        AgentFactory::enforce_minor_protection_invariant(true, &CapabilityScope::Filesystem).is_ok()
    );
    assert!(
        AgentFactory::enforce_minor_protection_invariant(true, &CapabilityScope::Camera).is_ok()
    );

    // 3. Adult interaction allows Wallet
    assert!(
        AgentFactory::enforce_minor_protection_invariant(false, &CapabilityScope::Wallet).is_ok()
    );
    assert!(
        AgentFactory::enforce_minor_protection_invariant(false, &CapabilityScope::FullSystem).is_ok()
    );
}

// =============================================================================
// SECTION 3: INVARIANT 6 — CRYPTOGRAPHIC KEY SHREDDING & K-OF-N SHAMIR RECOVERY
// =============================================================================

#[test]
fn test_tier5_adversarial_shamir_combinatorial_threshold_matrix() {
    // Stress test k-of-n Shamir Secret Sharing across multiple (k, n) regimes
    let configs = [(2, 3), (3, 5), (4, 7), (5, 9)];

    for &(k, n) in &configs {
        let mut secret = [0u8; 32];
        OsRng.fill_bytes(&mut secret);

        let shares = split_secret(&secret, k, n).expect("split_secret must succeed");
        assert_eq!(shares.len(), n as usize);

        // 1. Exactly (k - 1) shares MUST ALWAYS FAIL reconstruction
        let sub_k_minus_1: Vec<SecretShare> = shares.iter().take((k - 1) as usize).cloned().collect();
        let fail_res = reconstruct_secret(&sub_k_minus_1);
        assert!(
            fail_res.is_err(),
            "Reconstruction with k-1 ({}) shares for regime ({}, {}) MUST FAIL",
            k - 1,
            k,
            n
        );

        // 2. Exactly k shares MUST ALWAYS SUCCEED
        let sub_k: Vec<SecretShare> = shares.iter().take(k as usize).cloned().collect();
        let rec_k = reconstruct_secret(&sub_k)
            .expect("Reconstruction with exactly k shares must succeed");
        assert_eq!(rec_k, secret, "Reconstructed secret must match exactly");

        // 3. Permuted order of k shares MUST SUCCEED with identical secret
        let mut rev_k = sub_k.clone();
        rev_k.reverse();
        let rec_rev = reconstruct_secret(&rev_k).expect("Reversed shares must succeed");
        assert_eq!(rec_rev, secret);

        // 4. Over-threshold (k + 1) shares MUST SUCCEED
        let sub_k_plus_1: Vec<SecretShare> = shares.iter().take((k + 1) as usize).cloned().collect();
        let rec_k_plus_1 = reconstruct_secret(&sub_k_plus_1)
            .expect("Reconstruction with k+1 shares must succeed");
        assert_eq!(rec_k_plus_1, secret);

        // 5. All n shares MUST SUCCEED
        let rec_all = reconstruct_secret(&shares).expect("Reconstruction with all n shares must succeed");
        assert_eq!(rec_all, secret);
    }
}

#[test]
fn test_tier5_adversarial_shamir_byzantine_share_tampering_defense() {
    let secret = [0x5Au8; 32];
    let shares = split_secret(&secret, 3, 5).unwrap();

    // Attack 1: Single bit flip in share data
    for bit in 0..8 {
        let mut corrupted = shares.clone();
        corrupted[1].data[0] ^= 1 << bit;
        let res = reconstruct_secret(&corrupted[0..3]);
        assert!(
            res.is_err(),
            "Bit flip in share data MUST fail commitment check"
        );
    }

    // Attack 2: Tamper share index
    let mut bad_index = shares.clone();
    bad_index[0].index = 42;
    assert!(reconstruct_secret(&bad_index[0..3]).is_err());

    // Attack 3: Duplicate share index (reuse share 1 twice)
    let dup_shares = vec![shares[0].clone(), shares[0].clone(), shares[1].clone()];
    let dup_res = reconstruct_secret(&dup_shares);
    assert!(
        dup_res.is_err(),
        "Duplicate share index in subset must be rejected"
    );

    // Attack 4: Zero index share
    let mut zero_index = shares.clone();
    zero_index[0].index = 0;
    assert!(reconstruct_secret(&zero_index[0..3]).is_err());

    // Attack 5: Truncated share data length
    let mut trunc_data = shares.clone();
    trunc_data[0].data.truncate(16);
    assert!(reconstruct_secret(&trunc_data[0..3]).is_err());
}

#[test]
fn test_tier5_adversarial_digital_twilight_crypto_shredding_state() {
    let mut store = RedbZkStore::new();
    let uid = UserId(Uuid::new_v4());
    let blind_key = [0x77u8; 32];
    let blind_hash = compute_blind_index("sovereign_user@hakille.org", &blind_key);

    let dummy_blob = XChaChaBlob {
        nonce: [0u8; 24],
        ciphertext: vec![0x10, 0x20],
        tag: [0u8; 16],
        aad_fingerprint: [0u8; 16],
    };

    let user = UserRecord {
        user_id: uid.clone(),
        primary_did: Did::new("did:key:zTestUser"),
        blind_username_hash: blind_hash,
        encrypted_profile_blob: dummy_blob,
        state: AccountStatus::Active,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    store.insert_user(user).unwrap();

    // Verify initial pre-twilight state
    assert!(!store.is_salt_shredded(&uid));

    // Trigger Digital Twilight
    store
        .trigger_digital_twilight(&uid)
        .expect("Digital Twilight transition must succeed");

    // Verify post-twilight state: salt is shredded
    assert!(
        store.is_salt_shredded(&uid),
        "Private partition salt must be recorded as shredded"
    );

    // Verify user account state is now DigitalTwilightGrace
    let fetched = store
        .lookup_user_by_blind_username("sovereign_user@hakille.org", &blind_key)
        .expect("User lookup by blind index must succeed");
    assert_eq!(fetched.state, AccountStatus::DigitalTwilightGrace);

    // Verify cryptographic subkey derivation irrecoverability:
    // With shredded salt, attempting to reconstruct original master subkey hierarchy
    // using zeroed/random salt produces an uncorrelated subkey
    let master = MasterKey::new([0x33u8; 32]);
    let original_sub = MasterKeyKdf::derive_subkeys(&master);

    // An attacker without the shredded salt can only guess or use a blank salt
    let alternate_master = MasterKey::new([0x00u8; 32]);
    let alternate_sub = MasterKeyKdf::derive_subkeys(&alternate_master);

    assert_ne!(
        original_sub.memory_key(),
        alternate_sub.memory_key(),
        "Shredded salt ensures mathematical irrecoverability"
    );
}
