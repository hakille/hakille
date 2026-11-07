//! Empirical Boundary and Stress Tests for Pillars 3 & 4.
//!
//! Evaluates exact numerical and security boundaries:
//! - Pillar 3: Inactivity clause at 89.9 days vs 90.0 days, grace expiry at 103.9 days vs 104.0 days,
//!             heartbeat reset during grace, invalid / tampered trustee shares, legal vs intimate partition shredding.
//! - Pillar 4: Delegation depth boundary D = 4 vs D = 5, parent escrow exhaustion and refund on settlement.

use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use hakille_core::UserId;
use hakille_core::capability::{CapabilityAction, CapabilityScope};
use hakille_core::crypto::split_secret;
use hakille_identity::agent::{AgentFactory, AgentPassport};
use hakille_identity::hierarchy::TenantId;
use hakille_identity::inheritance::{
    BiometricHeartbeatAttestation, InheritanceVaultEngine, InheritanceVaultRecord,
    InheritanceVaultState,
};
use hakille_identity::micro_market::{
    AgentBidOffer, AgentComputeBudget, AgentMicroMarketEngine, DefaultAgentMicroMarketEngine,
    TaskContractRequest,
};
use hakille_identity::ssi::Did;
use hakille_identity::storage::RedbZkStore;
use rand::rngs::OsRng;
use uuid::Uuid;

// -----------------------------------------------------------------------------
// Test Helpers
// -----------------------------------------------------------------------------

fn make_test_vault() -> (InheritanceVaultRecord, [u8; 32], Vec<Did>, SigningKey) {
    let owner_id = UserId(Uuid::new_v4());
    let owner_did = Did::new("did:key:z6MkuAliceOwnerMock");
    let family_tenant = TenantId::new();

    let trustee_dids = vec![
        Did::new("did:key:z6MkuTrusteeBob1"),
        Did::new("did:key:z6MkuTrusteeCharlie2"),
        Did::new("did:key:z6MkuTrusteeDiana3"),
    ];

    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let signature = signing_key
        .sign(b"biometric_enclave_attestation_proof")
        .to_vec();

    let initial_heartbeat =
        BiometricHeartbeatAttestation::new(owner_id.clone(), Utc::now(), "FaceLiveness", signature);

    let legal_tokens = vec![[1u8; 32], [2u8; 32]];
    let intimate_tokens = vec![[3u8; 32], [4u8; 32]];

    let vault = InheritanceVaultEngine::create_vault(
        owner_id,
        owner_did,
        family_tenant,
        trustee_dids.clone(),
        2,  // threshold k = 2
        90, // threshold 90 days
        14, // grace period 14 days
        initial_heartbeat,
        legal_tokens,
        intimate_tokens,
    )
    .expect("Vault creation must succeed");

    let estate_key = [0x42u8; 32];
    (vault, estate_key, trustee_dids, signing_key)
}

fn create_agent(depth: usize, fuel: u64, cents: u64) -> (SigningKey, AgentPassport) {
    let (key, did) = AgentFactory::spawn_agent();
    let (_pkey, pdid) = AgentFactory::spawn_agent();
    let passport =
        AgentFactory::issue_passport(did, pdid, TenantId::new(), 3600, fuel, cents, vec![], depth)
            .expect("AgentFactory::issue_passport should succeed");
    (key, passport)
}

// -----------------------------------------------------------------------------
// Pillar 3: Inactivity Clause at 89.9 Days vs 90.0 Days
// -----------------------------------------------------------------------------

#[test]
fn test_pillar3_inactivity_clause_89_9_vs_90_0_days() {
    let (mut vault, _, _, _) = make_test_vault();
    let t0 = vault.inactivity_clause.last_biometric_attestation.timestamp;

    // 89.9 days: 89 days + 21 hours (89.875 days) -> Inactivity NOT met
    let t_89_9 = t0 + Duration::days(89) + Duration::hours(21);
    assert!(
        !vault.inactivity_clause.is_inactivity_met(t_89_9),
        "89.9 days must NOT meet the 90-day inactivity threshold"
    );

    let state_89_9 = InheritanceVaultEngine::evaluate_heartbeat(&mut vault, None, t_89_9)
        .expect("Evaluation must succeed");
    assert_eq!(
        state_89_9,
        InheritanceVaultState::Armed,
        "Vault must remain in Armed state at 89.9 days"
    );

    // 90.0 days: Exactly 90 days elapsed -> Inactivity MET, transitions to InactivityGracePeriod
    let t_90_0 = t0 + Duration::days(90);
    assert!(
        vault.inactivity_clause.is_inactivity_met(t_90_0),
        "90.0 days must satisfy the 90-day inactivity threshold"
    );

    let state_90_0 = InheritanceVaultEngine::evaluate_heartbeat(&mut vault, None, t_90_0)
        .expect("Evaluation must succeed");
    assert!(
        matches!(
            state_90_0,
            InheritanceVaultState::InactivityGracePeriod {
                days_inactive: 90,
                ..
            }
        ),
        "Vault must transition to InactivityGracePeriod at exactly 90.0 days"
    );
}

// -----------------------------------------------------------------------------
// Pillar 3: Grace Expiry at 103.9 Days vs 104.0 Days
// -----------------------------------------------------------------------------

#[test]
fn test_pillar3_grace_expiry_103_9_vs_104_0_days() {
    let (mut vault, _, _, _) = make_test_vault();
    let t0 = vault.inactivity_clause.last_biometric_attestation.timestamp;

    // Total required for grace expiry: 90 days inactivity + 14 days grace = 104 days

    // Fast-forward to InactivityGracePeriod at 90 days
    InheritanceVaultEngine::evaluate_heartbeat(&mut vault, None, t0 + Duration::days(90))
        .expect("Must transition to grace");
    assert!(matches!(
        vault.state,
        InheritanceVaultState::InactivityGracePeriod { .. }
    ));

    // 103.9 days: 103 days + 21 hours -> Grace NOT expired
    let t_103_9 = t0 + Duration::days(103) + Duration::hours(21);
    assert!(
        !vault.inactivity_clause.is_grace_expired(t_103_9),
        "103.9 days must NOT expire the grace period (104 days required)"
    );

    let state_103_9 = InheritanceVaultEngine::evaluate_heartbeat(&mut vault, None, t_103_9)
        .expect("Evaluation must succeed");
    assert!(
        matches!(
            state_103_9,
            InheritanceVaultState::InactivityGracePeriod { .. }
        ),
        "Vault must remain in InactivityGracePeriod at 103.9 days"
    );

    // 104.0 days: Exactly 104 days elapsed -> Grace EXPIRED, transitions to QuorumPending
    let t_104_0 = t0 + Duration::days(104);
    assert!(
        vault.inactivity_clause.is_grace_expired(t_104_0),
        "104.0 days must expire the grace period"
    );

    let state_104_0 = InheritanceVaultEngine::evaluate_heartbeat(&mut vault, None, t_104_0)
        .expect("Evaluation must succeed");
    assert!(
        matches!(
            state_104_0,
            InheritanceVaultState::QuorumPending {
                required_threshold: 2,
                total_trustees: 3,
                ..
            }
        ),
        "Vault must transition to QuorumPending at exactly 104.0 days"
    );
}

// -----------------------------------------------------------------------------
// Pillar 3: Heartbeat Reset During Grace
// -----------------------------------------------------------------------------

#[test]
fn test_pillar3_heartbeat_reset_during_grace() {
    let (mut vault, _, _, signing_key) = make_test_vault();
    let t0 = vault.inactivity_clause.last_biometric_attestation.timestamp;

    // Advance into grace period at 95 days
    let t_grace = t0 + Duration::days(95);
    InheritanceVaultEngine::evaluate_heartbeat(&mut vault, None, t_grace).unwrap();
    assert!(matches!(
        vault.state,
        InheritanceVaultState::InactivityGracePeriod { .. }
    ));

    // Owner comes back and provides fresh valid biometric heartbeat attestation
    let fresh_signature = signing_key.sign(b"fresh_liveness_proof").to_vec();
    let fresh_heartbeat = BiometricHeartbeatAttestation::new(
        vault.owner_user_id.clone(),
        t_grace,
        "ContinuousMultimodal",
        fresh_signature,
    );

    let state_reset = InheritanceVaultEngine::evaluate_heartbeat(
        &mut vault,
        Some(fresh_heartbeat.clone()),
        t_grace,
    )
    .expect("Heartbeat submission must succeed");

    assert_eq!(
        state_reset,
        InheritanceVaultState::Armed,
        "Valid heartbeat during grace period must reset vault to Armed state"
    );
    assert_eq!(
        vault
            .inactivity_clause
            .last_biometric_attestation
            .attestation_id,
        fresh_heartbeat.attestation_id
    );
}

// -----------------------------------------------------------------------------
// Pillar 3: Invalid / Tampered Trustee Shares & Quorum Unlocking
// -----------------------------------------------------------------------------

#[test]
fn test_pillar3_invalid_and_tampered_trustee_shares() {
    let (mut vault, estate_key, trustee_dids, _) = make_test_vault();
    let t0 = vault.inactivity_clause.last_biometric_attestation.timestamp;

    // Advance vault into QuorumPending (104 days)
    InheritanceVaultEngine::evaluate_heartbeat(&mut vault, None, t0 + Duration::days(90)).unwrap();
    InheritanceVaultEngine::evaluate_heartbeat(&mut vault, None, t0 + Duration::days(104)).unwrap();
    assert!(matches!(
        vault.state,
        InheritanceVaultState::QuorumPending { .. }
    ));

    // Generate Shamir 2-of-3 shares for master estate key
    let shares = split_secret(&estate_key, 2, 3).expect("Shamir split must succeed");

    // 1. Unauthorized trustee submitting a share -> REJECTED
    let fake_trustee = Did::new("did:key:z6MkuEveAttacker");
    let unauth_err =
        InheritanceVaultEngine::submit_trustee_share(&mut vault, &fake_trustee, shares[0].clone());
    assert!(
        unauth_err.is_err(),
        "Unauthorized trustee share must be rejected"
    );

    // 2. Tampered share (corrupted data byte or broken checksum) -> REJECTED
    let mut tampered_share = shares[0].clone();
    tampered_share.data[0] ^= 0xFF; // Bit flip invalidates checksum
    let tamper_err =
        InheritanceVaultEngine::submit_trustee_share(&mut vault, &trustee_dids[0], tampered_share);
    assert!(
        tamper_err.is_err(),
        "Tampered share checksum failure must be rejected"
    );

    // 3. First valid share from Trustee 0 -> accepted, but quorum (k=2) not reached yet (returns None)
    let res1 = InheritanceVaultEngine::submit_trustee_share(
        &mut vault,
        &trustee_dids[0],
        shares[0].clone(),
    )
    .expect("First valid share submission must succeed");
    assert_eq!(res1, None, "1 of 2 shares must not unlock the estate key");

    // 4. Duplicate submission of the same share -> deduplicated, does not prematurely satisfy quorum
    let res_dup = InheritanceVaultEngine::submit_trustee_share(
        &mut vault,
        &trustee_dids[0],
        shares[0].clone(),
    )
    .expect("Duplicate submission must not error");
    assert_eq!(
        res_dup, None,
        "Duplicate share must not unlock the estate key"
    );

    // 5. Second valid share from Trustee 1 -> Quorum reached, estate key reconstructed!
    let res2 = InheritanceVaultEngine::submit_trustee_share(
        &mut vault,
        &trustee_dids[1],
        shares[1].clone(),
    )
    .expect("Second valid share submission must succeed");

    assert!(
        res2.is_some(),
        "2 of 2 shares must reconstruct the estate key"
    );
    assert_eq!(
        res2.unwrap(),
        estate_key,
        "Reconstructed estate key must match original master key"
    );
    assert!(matches!(
        vault.state,
        InheritanceVaultState::Unlocked { .. }
    ));
}

// -----------------------------------------------------------------------------
// Pillar 3: Legal vs Intimate Partition Shredding
// -----------------------------------------------------------------------------

#[test]
fn test_pillar3_legal_vs_intimate_partition_shredding() {
    let (mut vault, _, _, _) = make_test_vault();
    let mut zk_store = RedbZkStore::new();

    // Insert vault into store
    zk_store.insert_inheritance_vault(vault.clone());

    assert_eq!(vault.legal_partition_blind_tokens.len(), 2);
    assert_eq!(vault.intimate_partition_blind_tokens.len(), 2);

    // 1. Shred intimate partition (destroys private memories while preserving legal documents)
    InheritanceVaultEngine::shred_intimate_partition(&mut vault, &mut zk_store)
        .expect("Shredding intimate partition must succeed");

    assert!(
        vault.intimate_partition_blind_tokens.is_empty(),
        "Intimate partition tokens must be completely wiped"
    );
    assert_eq!(
        vault.legal_partition_blind_tokens.len(),
        2,
        "Legal partition documents must remain intact after intimate shredding"
    );
    assert!(zk_store.is_vault_intimate_partition_shredded(&vault.vault_id));

    // 2. Permanent vault shredding (destroys everything and transitions state to CryptoShredded)
    InheritanceVaultEngine::permanently_shred_vault(&mut vault, &mut zk_store)
        .expect("Permanent shredding must succeed");

    assert!(vault.legal_partition_blind_tokens.is_empty());
    assert!(vault.intimate_partition_blind_tokens.is_empty());
    assert!(matches!(
        vault.state,
        InheritanceVaultState::CryptoShredded { .. }
    ));
}

// -----------------------------------------------------------------------------
// Pillar 4: Delegation Depth Boundary D = 4 vs D = 5
// -----------------------------------------------------------------------------

#[test]
fn test_pillar4_delegation_depth_boundary_d4_vs_d5() {
    let mut market = DefaultAgentMicroMarketEngine::new();
    let (parent_key_d3, parent_passport_d3) = create_agent(3, 1000, 500);

    // Case A: Parent at depth 3 -> Child receives depth 4 (D = 4 <= MAX_DELEGATION_DEPTH) -> ALLOWED
    let contract_req_d3 = TaskContractRequest {
        contract_id: "task_depth_3_to_4".to_string(),
        parent_passport: parent_passport_d3,
        required_scopes: vec![CapabilityScope::Filesystem],
        required_actions: vec![CapabilityAction::Read],
        max_allocated_fuel: 500,
        max_allocated_cents: 200,
        deadline_seconds: 3600,
    };

    let post_res_d3 = market.post_task(contract_req_d3);
    assert!(
        post_res_d3.is_ok(),
        "Posting task at parent depth 3 must succeed"
    );

    let (child_key, child_passport) = create_agent(0, 500, 200);
    let mut bid = AgentBidOffer {
        bidder_did: child_passport.agent_did.clone(),
        requested_fuel: 400,
        requested_budget_cents: 150,
        estimated_duration_ms: 1200,
        signature: vec![],
    };
    bid.sign("task_depth_3_to_4", &child_key);

    let award_res = market.award_contract("task_depth_3_to_4", &[bid], &parent_key_d3);
    assert!(
        award_res.is_ok(),
        "Awarding contract to child at depth 4 must succeed"
    );
    let child_awarded_passport = award_res.unwrap();
    assert_eq!(
        child_awarded_passport.delegation_depth, 4,
        "Child delegation depth must be exactly 4 (MAX_DELEGATION_DEPTH)"
    );

    // Case B: Parent at depth 4 -> Attempting to subcontract to depth 5 (D = 5 > 4) -> REJECTED
    let (_parent_key_d4, parent_passport_d4) = create_agent(4, 1000, 500);

    let contract_req_d4 = TaskContractRequest {
        contract_id: "task_depth_4_to_5_illegal".to_string(),
        parent_passport: parent_passport_d4,
        required_scopes: vec![CapabilityScope::Filesystem],
        required_actions: vec![CapabilityAction::Read],
        max_allocated_fuel: 300,
        max_allocated_cents: 100,
        deadline_seconds: 3600,
    };

    // Validation must fail-closed because parent_passport.delegation_depth (4) >= MAX_DELEGATION_DEPTH (4)
    let validate_res = contract_req_d4.validate();
    assert!(
        validate_res.is_err(),
        "TaskContractRequest with parent depth 4 must fail validation (cannot reach depth 5)"
    );

    let post_res_d4 = market.post_task(contract_req_d4);
    assert!(
        post_res_d4.is_err(),
        "Posting task that would spawn a depth 5 child must be rejected fail-closed"
    );
}

// -----------------------------------------------------------------------------
// Pillar 4: Parent Escrow Exhaustion and Refund on Settlement
// -----------------------------------------------------------------------------

#[test]
fn test_pillar4_parent_escrow_exhaustion_and_settlement_refund() {
    let mut market = DefaultAgentMicroMarketEngine::new();
    let (parent_key, parent_passport) = create_agent(1, 1000, 500);
    let parent_did = parent_passport.agent_did.clone();

    let initial_budget = AgentComputeBudget::new(1000, 500); // 1000 fuel points, 500 cents
    market
        .register_parent_budget(&parent_did, initial_budget)
        .unwrap();

    // 1. Post Contract A (allocates up to 800 fuel)
    let req_a = TaskContractRequest {
        contract_id: "contract_alpha".to_string(),
        parent_passport: parent_passport.clone(),
        required_scopes: vec![CapabilityScope::Filesystem],
        required_actions: vec![CapabilityAction::Read],
        max_allocated_fuel: 800,
        max_allocated_cents: 300,
        deadline_seconds: 3600,
    };
    market.post_task(req_a).unwrap();

    let (child_key_1, child_pass_1) = create_agent(0, 800, 300);
    let mut bid_a = AgentBidOffer {
        bidder_did: child_pass_1.agent_did,
        requested_fuel: 800,
        requested_budget_cents: 300,
        estimated_duration_ms: 1000,
        signature: vec![],
    };
    bid_a.sign("contract_alpha", &child_key_1);

    market
        .award_contract("contract_alpha", &[bid_a], &parent_key)
        .unwrap();

    // Check parent budget: 800 locked, 200 fuel available
    let b1 = market.get_parent_budget(&parent_did).unwrap().unwrap();
    assert_eq!(b1.remaining_fuel(), 200);

    // 2. Post Contract B requesting 300 fuel -> Exceeds remaining 200 fuel in escrow!
    let req_b = TaskContractRequest {
        contract_id: "contract_beta".to_string(),
        parent_passport: parent_passport.clone(),
        required_scopes: vec![CapabilityScope::Filesystem],
        required_actions: vec![CapabilityAction::Read],
        max_allocated_fuel: 300,
        max_allocated_cents: 100,
        deadline_seconds: 3600,
    };
    market.post_task(req_b).unwrap();

    let (child_key_2, child_pass_2) = create_agent(0, 300, 100);
    let mut bid_b = AgentBidOffer {
        bidder_did: child_pass_2.agent_did.clone(),
        requested_fuel: 300,
        requested_budget_cents: 100,
        estimated_duration_ms: 500,
        signature: vec![],
    };
    bid_b.sign("contract_beta", &child_key_2);

    // Awarding Contract B must fail with escrow exhaustion error
    let award_b_err = market.award_contract("contract_beta", &[bid_b.clone()], &parent_key);
    assert!(
        award_b_err.is_err(),
        "Awarding 300 fuel when only 200 fuel available in escrow must fail"
    );

    // 3. Settle Contract A early with only 600 fuel actually consumed (200 fuel refunded!)
    let settled_budget = market
        .settle_contract("contract_alpha", 600, 200)
        .expect("Settlement of contract alpha must succeed");
    assert_eq!(settled_budget.consumed_fuel_points, 600);

    // Parent budget now has 1000 - 600 = 400 fuel available!
    let b2 = market.get_parent_budget(&parent_did).unwrap().unwrap();
    assert_eq!(b2.remaining_fuel(), 400);

    // 4. Now awarding Contract B (300 fuel <= 400 available) succeeds!
    let award_b_success = market.award_contract("contract_beta", &[bid_b], &parent_key);
    assert!(
        award_b_success.is_ok(),
        "After settlement refund, contract beta award must succeed"
    );

    let b3 = market.get_parent_budget(&parent_did).unwrap().unwrap();
    assert_eq!(b3.remaining_fuel(), 100); // 400 - 300 = 100 remaining
}
