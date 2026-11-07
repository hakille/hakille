//! Integration tests for Pillar 4: Agent Micro-Market & Monotonic Budget.

use ed25519_dalek::SigningKey;
use hakille_core::capability::{CapabilityAction, CapabilityScope};
use hakille_identity::agent::{AgentFactory, AgentPassport};
use hakille_identity::hierarchy::TenantId;
use hakille_identity::micro_market::{
    AgentBidOffer, AgentMicroMarketEngine, DefaultAgentMicroMarketEngine, TaskContractRequest,
};

fn create_agent(depth: usize, fuel: u64, cents: u64) -> (SigningKey, AgentPassport) {
    let (key, did) = AgentFactory::spawn_agent();
    let (_pkey, pdid) = AgentFactory::spawn_agent();
    let passport =
        AgentFactory::issue_passport(did, pdid, TenantId::new(), 3600, fuel, cents, vec![], depth)
            .expect("AgentFactory::issue_passport should succeed");
    (key, passport)
}

#[test]
fn test_integration_full_subcontracting_lifecycle_with_escrow_refund() {
    let mut market = DefaultAgentMicroMarketEngine::new();
    let (parent_key, parent_passport) = create_agent(1, 100_000, 5000);
    let parent_did = parent_passport.agent_did.clone();

    // 1. Post a task
    let task_id = "contract-ocr-pipeline".to_string();
    let task_req = TaskContractRequest {
        contract_id: task_id.clone(),
        parent_passport: parent_passport.clone(),
        required_scopes: vec![CapabilityScope::Filesystem, CapabilityScope::ModelInference],
        required_actions: vec![CapabilityAction::Read, CapabilityAction::Execute],
        max_allocated_fuel: 50_000,
        max_allocated_cents: 2500,
        deadline_seconds: 1200,
    };
    market
        .post_task(task_req)
        .expect("Task posting must succeed");

    // 2. Candidate 1 submits a bid that is expensive
    let (w1_key, w1_did) = AgentFactory::spawn_agent();
    let mut bid1 = AgentBidOffer {
        bidder_did: w1_did,
        requested_fuel: 48_000,
        requested_budget_cents: 2400,
        estimated_duration_ms: 1000,
        signature: vec![],
    };
    bid1.sign(&task_id, &w1_key);

    // Candidate 2 submits a competitive bid
    let (w2_key, w2_did) = AgentFactory::spawn_agent();
    let mut bid2 = AgentBidOffer {
        bidder_did: w2_did.clone(),
        requested_fuel: 30_000,
        requested_budget_cents: 1200,
        estimated_duration_ms: 600,
        signature: vec![],
    };
    bid2.sign(&task_id, &w2_key);

    // 3. Award contract: Engine must select the optimal valid bid (Candidate 2)
    let child_passport = market
        .award_contract(&task_id, &[bid1, bid2], &parent_key)
        .expect("Contract award must succeed");

    assert_eq!(child_passport.agent_did, w2_did);
    assert_eq!(child_passport.delegation_depth, 2);
    assert_eq!(child_passport.max_execution_fuel, 30_000);
    assert_eq!(child_passport.budget_limit_cents, 1200);

    // Verify parent escrow state: 30,000 fuel and 1200 cents locked
    let budget_during = market
        .get_parent_budget(&parent_did)
        .expect("Query must succeed")
        .expect("Budget must exist");
    assert_eq!(budget_during.remaining_fuel(), 70_000);
    assert_eq!(budget_during.remaining_budget_cents(), 3800);

    // 4. Settle contract with under-consumption
    let child_settlement = market
        .settle_contract(&task_id, 18_000, 800)
        .expect("Settlement must succeed");

    assert_eq!(child_settlement.total_fuel_points, 30_000);
    assert_eq!(child_settlement.consumed_fuel_points, 18_000);
    assert_eq!(child_settlement.total_budget_cents, 1200);
    assert_eq!(child_settlement.consumed_budget_cents, 800);

    // Verify parent escrow state after settlement: 12,000 fuel and 400 cents refunded!
    let budget_after = market
        .get_parent_budget(&parent_did)
        .expect("Query must succeed")
        .expect("Budget must exist");
    assert_eq!(budget_after.remaining_fuel(), 82_000); // 100k - 18k
    assert_eq!(budget_after.remaining_budget_cents(), 4200); // 5000 - 800
}

#[test]
fn test_integration_parent_escrow_multi_child_over_subscription_denied() {
    let mut market = DefaultAgentMicroMarketEngine::new();
    let (parent_key, parent_passport) = create_agent(1, 100_000, 1000);

    // Post 3 tasks
    for i in 1..=3 {
        let req = TaskContractRequest {
            contract_id: format!("parallel-task-{i}"),
            parent_passport: parent_passport.clone(),
            required_scopes: vec![CapabilityScope::Filesystem],
            required_actions: vec![CapabilityAction::Read],
            max_allocated_fuel: 40_000,
            max_allocated_cents: 400,
            deadline_seconds: 3600,
        };
        market.post_task(req).expect("Post task must succeed");
    }

    // Child 1 asks for 40,000 fuel & 400 cents (40%)
    let (k1, did1) = AgentFactory::spawn_agent();
    let mut b1 = AgentBidOffer {
        bidder_did: did1,
        requested_fuel: 40_000,
        requested_budget_cents: 400,
        estimated_duration_ms: 100,
        signature: vec![],
    };
    b1.sign("parallel-task-1", &k1);
    market
        .award_contract("parallel-task-1", &[b1], &parent_key)
        .expect("Award child 1 must succeed");

    // Child 2 asks for 40,000 fuel & 400 cents (40% + 40% = 80%)
    let (k2, did2) = AgentFactory::spawn_agent();
    let mut b2 = AgentBidOffer {
        bidder_did: did2,
        requested_fuel: 40_000,
        requested_budget_cents: 400,
        estimated_duration_ms: 100,
        signature: vec![],
    };
    b2.sign("parallel-task-2", &k2);
    market
        .award_contract("parallel-task-2", &[b2], &parent_key)
        .expect("Award child 2 must succeed");

    // Child 3 asks for 40,000 fuel & 400 cents (80% + 40% = 120% > 100%)
    let (k3, did3) = AgentFactory::spawn_agent();
    let mut b3 = AgentBidOffer {
        bidder_did: did3,
        requested_fuel: 40_000,
        requested_budget_cents: 400,
        estimated_duration_ms: 100,
        signature: vec![],
    };
    b3.sign("parallel-task-3", &k3);
    let award3 = market.award_contract("parallel-task-3", &[b3], &parent_key);

    assert!(
        award3.is_err(),
        "3rd allocation exceeding available parent escrow must be rejected"
    );
}

#[test]
fn test_integration_hard_rejection_delegation_depth_boundary_at_d5() {
    let mut market = DefaultAgentMicroMarketEngine::new();

    // Cascade: D=1 -> D=2 -> D=3 -> D=4 -> D=5(REJECTED)
    let (k1, p1) = create_agent(1, 100_000, 1000);

    // D=1 to D=2
    let req1 = TaskContractRequest {
        contract_id: "chain-d1-to-d2".to_string(),
        parent_passport: p1,
        required_scopes: vec![CapabilityScope::Filesystem],
        required_actions: vec![CapabilityAction::Read],
        max_allocated_fuel: 80_000,
        max_allocated_cents: 800,
        deadline_seconds: 3600,
    };
    market.post_task(req1).unwrap();
    let (k2, did2) = AgentFactory::spawn_agent();
    let mut b2 = AgentBidOffer {
        bidder_did: did2,
        requested_fuel: 80_000,
        requested_budget_cents: 800,
        estimated_duration_ms: 50,
        signature: vec![],
    };
    b2.sign("chain-d1-to-d2", &k2);
    let p2 = market
        .award_contract("chain-d1-to-d2", &[b2], &k1)
        .expect("Award D=2 must succeed");
    assert_eq!(p2.delegation_depth, 2);

    // D=2 to D=3
    let req2 = TaskContractRequest {
        contract_id: "chain-d2-to-d3".to_string(),
        parent_passport: p2,
        required_scopes: vec![CapabilityScope::Filesystem],
        required_actions: vec![CapabilityAction::Read],
        max_allocated_fuel: 60_000,
        max_allocated_cents: 600,
        deadline_seconds: 3600,
    };
    market.post_task(req2).unwrap();
    let (k3, did3) = AgentFactory::spawn_agent();
    let mut b3 = AgentBidOffer {
        bidder_did: did3,
        requested_fuel: 60_000,
        requested_budget_cents: 600,
        estimated_duration_ms: 50,
        signature: vec![],
    };
    b3.sign("chain-d2-to-d3", &k3);
    let p3 = market
        .award_contract("chain-d2-to-d3", &[b3], &k2)
        .expect("Award D=3 must succeed");
    assert_eq!(p3.delegation_depth, 3);

    // D=3 to D=4
    let req3 = TaskContractRequest {
        contract_id: "chain-d3-to-d4".to_string(),
        parent_passport: p3,
        required_scopes: vec![CapabilityScope::Filesystem],
        required_actions: vec![CapabilityAction::Read],
        max_allocated_fuel: 40_000,
        max_allocated_cents: 400,
        deadline_seconds: 3600,
    };
    market.post_task(req3).unwrap();
    let (k4, did4) = AgentFactory::spawn_agent();
    let mut b4 = AgentBidOffer {
        bidder_did: did4,
        requested_fuel: 40_000,
        requested_budget_cents: 400,
        estimated_duration_ms: 50,
        signature: vec![],
    };
    b4.sign("chain-d3-to-d4", &k4);
    let p4 = market
        .award_contract("chain-d3-to-d4", &[b4], &k3)
        .expect("Award D=4 must succeed");
    assert_eq!(p4.delegation_depth, 4);

    // D=4 to D=5: Must be HARD REJECTED
    let req4 = TaskContractRequest {
        contract_id: "chain-d4-to-d5".to_string(),
        parent_passport: p4,
        required_scopes: vec![CapabilityScope::Filesystem],
        required_actions: vec![CapabilityAction::Read],
        max_allocated_fuel: 20_000,
        max_allocated_cents: 200,
        deadline_seconds: 3600,
    };
    let post_d5 = market.post_task(req4);
    assert!(
        post_d5.is_err(),
        "Delegation attempt from parent at D=4 must be blocked (next depth D=5 exceeds max 4)"
    );
}

#[test]
fn test_integration_bid_tampering_and_double_settlement() {
    let mut market = DefaultAgentMicroMarketEngine::new();
    let (parent_key, parent_passport) = create_agent(1, 100_000, 1000);

    let task_id = "task-tampering-test".to_string();
    let req = TaskContractRequest {
        contract_id: task_id.clone(),
        parent_passport,
        required_scopes: vec![CapabilityScope::Filesystem],
        required_actions: vec![CapabilityAction::Read],
        max_allocated_fuel: 20_000,
        max_allocated_cents: 200,
        deadline_seconds: 1800,
    };
    market.post_task(req).unwrap();

    let (w_key, w_did) = AgentFactory::spawn_agent();
    let mut bid = AgentBidOffer {
        bidder_did: w_did,
        requested_fuel: 10_000,
        requested_budget_cents: 100,
        estimated_duration_ms: 100,
        signature: vec![],
    };
    bid.sign(&task_id, &w_key);

    // Attacker modifies requested fuel after signing
    let mut tampered_bid = bid.clone();
    tampered_bid.requested_fuel = 9999;
    assert!(!tampered_bid.verify(&task_id));

    // Awarding with tampered bid must fail
    let award_res = market.award_contract(&task_id, &[tampered_bid], &parent_key);
    assert!(award_res.is_err());

    // Award with original valid bid
    market
        .award_contract(&task_id, &[bid], &parent_key)
        .expect("Valid bid must succeed");

    // Settle contract
    market
        .settle_contract(&task_id, 8000, 80)
        .expect("First settlement must succeed");

    // Double settlement must be rejected
    let double_settle = market.settle_contract(&task_id, 8000, 80);
    assert!(
        double_settle.is_err(),
        "Double settlement of contract must be refused"
    );
}
