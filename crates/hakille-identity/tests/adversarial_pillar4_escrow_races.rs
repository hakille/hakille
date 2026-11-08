//! Adversarial Stress Testing & Security Invariant Fuzzing: Parent Compute Escrow & Auction Concurrency
//!
//! Validates:
//! - Multi-threaded concurrent over-subscription prevention (budget conservation invariant)
//! - Concurrent bidding and settlement interleaved races (zero escrow leakage)
//! - Cryptographic bid signature tampering, spoofing, and cross-contract replay storms
//! - Swarm delegation depth cascade hard limit ($D \le 4$) under concurrent race conditions
//! - Concurrent double-settlement refund drain prevention

use ed25519_dalek::SigningKey;
use hakille_core::HakilleError;
use hakille_core::capability::{CapabilityAction, CapabilityScope};
use hakille_identity::agent::{AgentFactory, AgentPassport};
use hakille_identity::hierarchy::TenantId;
use hakille_identity::micro_market::{
    AgentBidOffer, AgentMicroMarketEngine, DefaultAgentMicroMarketEngine, TaskContractRequest,
};
use std::sync::{Arc, Mutex};
use std::thread;

fn make_parent_passport(depth: usize, fuel: u64, cents: u64) -> (SigningKey, AgentPassport) {
    let (key, did) = AgentFactory::spawn_agent();
    let (_pkey, pdid) = AgentFactory::spawn_agent();
    let passport =
        AgentFactory::issue_passport(did, pdid, TenantId::new(), 3600, fuel, cents, vec![], depth)
            .expect("Failed to issue parent test passport");
    (key, passport)
}

#[test]
fn test_adversarial_concurrent_escrow_auction_oversubscription() {
    // Parent has 100,000 fuel and 5,000 cents
    let (parent_key, parent_passport) = make_parent_passport(1, 100_000, 5_000);
    let parent_did = parent_passport.agent_did.clone();

    let market = Arc::new(Mutex::new(DefaultAgentMicroMarketEngine::new()));

    // Post 40 tasks to the micro-market
    for i in 0..40 {
        let task = TaskContractRequest {
            contract_id: format!("contract_task_{i:02}"),
            parent_passport: parent_passport.clone(),
            required_scopes: vec![CapabilityScope::Filesystem],
            required_actions: vec![CapabilityAction::Read],
            max_allocated_fuel: 5_000,
            max_allocated_cents: 250,
            deadline_seconds: 1800,
        };
        market.lock().unwrap().post_task(task).unwrap();
    }

    // 40 concurrent threads simultaneously bid 5,000 fuel & 250 cents each.
    // Total requested = 40 * 5,000 = 200,000 fuel (200% of parent budget).
    let mut handles = Vec::new();
    let parent_key_arc = Arc::new(parent_key);

    for i in 0..40 {
        let m = Arc::clone(&market);
        let pk = Arc::clone(&parent_key_arc);
        let contract_id = format!("contract_task_{i:02}");

        let handle = thread::spawn(move || {
            let (worker_key, worker_did) = AgentFactory::spawn_agent();
            let mut bid = AgentBidOffer {
                bidder_did: worker_did,
                requested_fuel: 5_000,
                requested_budget_cents: 250,
                estimated_duration_ms: 200,
                signature: vec![],
            };
            bid.sign(&contract_id, &worker_key);

            let mut guard = m.lock().unwrap();
            guard.award_contract(&contract_id, &[bid], &pk)
        });
        handles.push(handle);
    }

    let mut awarded_count = 0;
    let mut rejected_count = 0;

    for handle in handles {
        match handle.join().unwrap() {
            Ok(child_passport) => {
                assert_eq!(child_passport.max_execution_fuel, 5_000);
                assert_eq!(child_passport.budget_limit_cents, 250);
                awarded_count += 1;
            }
            Err(HakilleError::CapabilityDenied(msg)) => {
                assert!(msg.contains("Escrow over-allocation") || msg.contains("over-allocation"));
                rejected_count += 1;
            }
            Err(other) => panic!("Unexpected error during concurrent award: {:?}", other),
        }
    }

    // Strict invariant: Exactly 20 contracts awarded (20 * 5,000 = 100,000 fuel; 20 * 250 = 5,000 cents)
    // and exactly 20 rejected
    assert_eq!(
        awarded_count, 20,
        "Parent escrow must award exactly 20 contracts before full exhaustion"
    );
    assert_eq!(
        rejected_count, 20,
        "Parent escrow must reject exactly 20 over-subscribing contracts"
    );

    // Verify parent escrow available balance is exactly 0
    let guard = market.lock().unwrap();
    let budget = guard.get_parent_budget(&parent_did).unwrap().unwrap();
    assert_eq!(budget.total_fuel_points, 100_000);
    assert_eq!(budget.remaining_fuel(), 0, "No remaining fuel should exist");
    assert_eq!(
        budget.remaining_budget_cents(),
        0,
        "No remaining cents should exist"
    );
}

#[test]
fn test_adversarial_concurrent_bidding_and_settlement_reclaim_race() {
    let (parent_key, parent_passport) = make_parent_passport(1, 50_000, 2_500);
    let parent_did = parent_passport.agent_did.clone();

    let market = Arc::new(Mutex::new(DefaultAgentMicroMarketEngine::new()));

    // Award 5 contracts upfront (5 * 10,000 = 50,000 fuel -> 100% locked)
    for i in 0..5 {
        let cid = format!("batch1_task_{i}");
        market
            .lock()
            .unwrap()
            .post_task(TaskContractRequest {
                contract_id: cid.clone(),
                parent_passport: parent_passport.clone(),
                required_scopes: vec![CapabilityScope::Filesystem],
                required_actions: vec![CapabilityAction::Read],
                max_allocated_fuel: 10_000,
                max_allocated_cents: 500,
                deadline_seconds: 1800,
            })
            .unwrap();

        let (w_key, w_did) = AgentFactory::spawn_agent();
        let mut bid = AgentBidOffer {
            bidder_did: w_did,
            requested_fuel: 10_000,
            requested_budget_cents: 500,
            estimated_duration_ms: 100,
            signature: vec![],
        };
        bid.sign(&cid, &w_key);
        market
            .lock()
            .unwrap()
            .award_contract(&cid, &[bid], &parent_key)
            .unwrap();
    }

    // Now budget is 100% locked. Post 5 new tasks
    for i in 0..5 {
        let cid = format!("batch2_task_{i}");
        market
            .lock()
            .unwrap()
            .post_task(TaskContractRequest {
                contract_id: cid.clone(),
                parent_passport: parent_passport.clone(),
                required_scopes: vec![CapabilityScope::Filesystem],
                required_actions: vec![CapabilityAction::Read],
                max_allocated_fuel: 6_000,
                max_allocated_cents: 300,
                deadline_seconds: 1800,
            })
            .unwrap();
    }

    // Spawn 5 settlement threads (each settling with only 4,000 fuel used, refunding 6,000 fuel)
    // Concurrently with 5 bidder threads trying to grab the refunded 6,000 fuel for batch2 tasks
    let mut settle_handles = Vec::new();

    // 5 Settlement threads
    for i in 0..5 {
        let m = Arc::clone(&market);
        let cid = format!("batch1_task_{i}");
        settle_handles.push(thread::spawn(move || {
            let mut guard = m.lock().unwrap();
            guard.settle_contract(&cid, 4_000, 200)
        }));
    }

    // 5 Award threads for batch2
    let mut award_handles = Vec::new();
    let parent_key_arc = Arc::new(parent_key);
    for i in 0..5 {
        let m = Arc::clone(&market);
        let pk = Arc::clone(&parent_key_arc);
        let cid = format!("batch2_task_{i}");
        award_handles.push(thread::spawn(move || {
            let (w_key, w_did) = AgentFactory::spawn_agent();
            let mut bid = AgentBidOffer {
                bidder_did: w_did,
                requested_fuel: 6_000,
                requested_budget_cents: 300,
                estimated_duration_ms: 100,
                signature: vec![],
            };
            bid.sign(&cid, &w_key);

            // Retry loop to allow settlement thread to release funds
            for _ in 0..20 {
                let mut guard = m.lock().unwrap();
                match guard.award_contract(&cid, &[bid.clone()], &pk) {
                    Ok(pass) => return Ok(pass),
                    Err(_) => {
                        drop(guard);
                        thread::sleep(std::time::Duration::from_millis(5));
                    }
                }
            }
            let mut guard = m.lock().unwrap();
            guard.award_contract(&cid, &[bid], &pk)
        }));
    }

    for handle in settle_handles {
        let _ = handle.join().unwrap();
    }
    for handle in award_handles {
        let _ = handle.join().unwrap();
    }

    // Energy Conservation Invariant:
    // Total fuel settled + total fuel locked + available fuel == 50,000
    let guard = market.lock().unwrap();
    let budget = guard.get_parent_budget(&parent_did).unwrap().unwrap();
    assert_eq!(budget.total_fuel_points, 50_000);
    // Consumed + remaining must strictly equal 50,000
    assert_eq!(
        budget.consumed_fuel_points + budget.remaining_fuel(),
        50_000,
        "Total fuel must be strictly conserved under concurrent settle/bid"
    );
    assert_eq!(
        budget.consumed_budget_cents + budget.remaining_budget_cents(),
        2_500,
        "Total cents must be strictly conserved under concurrent settle/bid"
    );
}

#[test]
fn test_adversarial_bid_tampering_and_forgery_storm() {
    let (parent_key, parent_passport) = make_parent_passport(1, 100_000, 5_000);
    let mut market = DefaultAgentMicroMarketEngine::new();

    market
        .post_task(TaskContractRequest {
            contract_id: "tamper_task_01".to_string(),
            parent_passport,
            required_scopes: vec![CapabilityScope::Filesystem],
            required_actions: vec![CapabilityAction::Read],
            max_allocated_fuel: 10_000,
            max_allocated_cents: 500,
            deadline_seconds: 1800,
        })
        .unwrap();

    let (legit_key, legit_did) = AgentFactory::spawn_agent();
    let (_forger_key, forger_did) = AgentFactory::spawn_agent();

    // 1. Spoofed identity: Signed by legit_key, but claimed bidder_did is forger_did
    let mut spoofed_bid = AgentBidOffer {
        bidder_did: forger_did.clone(),
        requested_fuel: 5_000,
        requested_budget_cents: 250,
        estimated_duration_ms: 100,
        signature: vec![],
    };
    spoofed_bid.sign("tamper_task_01", &legit_key);
    assert!(
        !spoofed_bid.verify("tamper_task_01"),
        "Bid signed with key that does not match bidder_did must fail verification"
    );

    // 2. Tampered amount: Signed for 5_000 fuel, but changed to 8_000 after signing
    let mut tampered_bid = AgentBidOffer {
        bidder_did: legit_did.clone(),
        requested_fuel: 5_000,
        requested_budget_cents: 250,
        estimated_duration_ms: 100,
        signature: vec![],
    };
    tampered_bid.sign("tamper_task_01", &legit_key);
    tampered_bid.requested_fuel = 8_000;
    assert!(
        !tampered_bid.verify("tamper_task_01"),
        "Tampered fuel amount must fail signature verification"
    );

    // 3. Cross-contract replay: Valid bid signed for "other_contract", submitted for "tamper_task_01"
    let mut cross_contract_bid = AgentBidOffer {
        bidder_did: legit_did.clone(),
        requested_fuel: 5_000,
        requested_budget_cents: 250,
        estimated_duration_ms: 100,
        signature: vec![],
    };
    cross_contract_bid.sign("other_contract_99", &legit_key);
    assert!(
        !cross_contract_bid.verify("tamper_task_01"),
        "Bid bound to other contract ID must fail verification on tamper_task_01"
    );

    // 4. Corrupted signature bits
    let mut corrupted_sig_bid = AgentBidOffer {
        bidder_did: legit_did,
        requested_fuel: 5_000,
        requested_budget_cents: 250,
        estimated_duration_ms: 100,
        signature: vec![],
    };
    corrupted_sig_bid.sign("tamper_task_01", &legit_key);
    corrupted_sig_bid.signature[10] ^= 0x5a;
    assert!(
        !corrupted_sig_bid.verify("tamper_task_01"),
        "Flipped signature bits must fail verification"
    );

    // None of these forged bids can be awarded
    let award_res = market.award_contract(
        "tamper_task_01",
        &[
            spoofed_bid,
            tampered_bid,
            cross_contract_bid,
            corrupted_sig_bid,
        ],
        &parent_key,
    );
    assert!(
        award_res.is_err(),
        "All forged/tampered bids must be rejected"
    );
}

#[test]
fn test_adversarial_concurrent_double_settlement_refund_drain() {
    let (parent_key, parent_passport) = make_parent_passport(1, 50_000, 2_000);
    let parent_did = parent_passport.agent_did.clone();

    let mut market = DefaultAgentMicroMarketEngine::new();

    market
        .post_task(TaskContractRequest {
            contract_id: "double_settle_task".to_string(),
            parent_passport,
            required_scopes: vec![CapabilityScope::Filesystem],
            required_actions: vec![CapabilityAction::Read],
            max_allocated_fuel: 20_000,
            max_allocated_cents: 1_000,
            deadline_seconds: 1800,
        })
        .unwrap();

    let (w_key, w_did) = AgentFactory::spawn_agent();
    let mut bid = AgentBidOffer {
        bidder_did: w_did,
        requested_fuel: 20_000,
        requested_budget_cents: 1_000,
        estimated_duration_ms: 100,
        signature: vec![],
    };
    bid.sign("double_settle_task", &w_key);
    market
        .award_contract("double_settle_task", &[bid], &parent_key)
        .unwrap();

    let market_arc = Arc::new(Mutex::new(market));

    // 20 concurrent threads trying to settle the same contract
    let mut handles = Vec::new();
    for _ in 0..20 {
        let m = Arc::clone(&market_arc);
        handles.push(thread::spawn(move || {
            let mut guard = m.lock().unwrap();
            guard.settle_contract("double_settle_task", 10_000, 500)
        }));
    }

    let mut success_count = 0;
    let mut already_settled_count = 0;

    for handle in handles {
        match handle.join().unwrap() {
            Ok(_) => success_count += 1,
            Err(HakilleError::CapabilityDenied(msg)) => {
                assert!(msg.contains("already settled"));
                already_settled_count += 1;
            }
            Err(other) => panic!("Unexpected error: {:?}", other),
        }
    }

    assert_eq!(
        success_count, 1,
        "Exactly ONE thread must succeed in settling the contract"
    );
    assert_eq!(
        already_settled_count, 19,
        "Exactly 19 threads must receive already settled rejection"
    );

    // Verify parent escrow was refunded ONLY ONCE (20k awarded - 10k consumed = 10k refunded)
    let guard = market_arc.lock().unwrap();
    let budget = guard.get_parent_budget(&parent_did).unwrap().unwrap();
    assert_eq!(
        budget.consumed_fuel_points, 10_000,
        "Settled fuel must be exactly 10,000 (not duplicated 20 times!)"
    );
    assert_eq!(
        budget.remaining_fuel(),
        40_000,
        "Remaining fuel must be exactly 40,000 (50k - 10k)"
    );
}

#[test]
fn test_adversarial_concurrent_delegation_depth_overflow_at_d5() {
    let (_parent_key, parent_at_d4) = make_parent_passport(4, 50_000, 1_000);
    let market = Arc::new(Mutex::new(DefaultAgentMicroMarketEngine::new()));

    // 20 threads simultaneously trying to post tasks from a parent at D = 4
    let mut handles = Vec::new();
    for i in 0..20 {
        let m = Arc::clone(&market);
        let p = parent_at_d4.clone();
        handles.push(thread::spawn(move || {
            let task = TaskContractRequest {
                contract_id: format!("depth_overflow_task_{i}"),
                parent_passport: p,
                required_scopes: vec![CapabilityScope::ModelInference],
                required_actions: vec![CapabilityAction::Execute],
                max_allocated_fuel: 2_000,
                max_allocated_cents: 50,
                deadline_seconds: 1800,
            };
            let mut guard = m.lock().unwrap();
            guard.post_task(task)
        }));
    }

    for handle in handles {
        let res = handle.join().unwrap();
        assert!(
            res.is_err(),
            "All concurrent attempts to subcontract at D = 4 must fail validation"
        );
        match res.unwrap_err() {
            HakilleError::CapabilityDenied(msg) => {
                assert!(
                    msg.contains("cannot subcontract further")
                        || msg.contains("reaches or exceeds maximum")
                );
            }
            err => panic!("Unexpected error: {:?}", err),
        }
    }
}
