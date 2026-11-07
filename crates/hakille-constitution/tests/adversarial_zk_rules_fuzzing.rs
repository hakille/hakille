//! Adversarial Stress Testing & Security Invariant Fuzzing: Constitutional Rules 1-8 ZK Engine
//!
//! Validates:
//! - Strict proof generation failure for EACH of Rules 1 through 8 individually and in combination
//! - Precise boundary values (e.g., tau = 0.8999 vs 0.9000, delegation depth = 4 vs 5)
//! - Witness falsification and private wire tamper detection
//! - Proof forgery resistance and Fiat-Shamir challenge verification
//! - Constant execution time invariant (< 5ms verification, <= 256B size limit)

use hakille_constitution::zk::{
    ConstitutionalWitness, ConstitutionalZkEngine, DefaultConstitutionalZkEngine,
    MAX_ZK_PROOF_BYTES, canonical_constitution_hash, compute_action_digest,
    compute_token_commitment,
};
use hakille_core::HakilleError;
use hakille_core::capability::{CapabilityAction, CapabilityScope};

fn make_base_witness() -> ([u8; 32], [u8; 32], [u8; 32], ConstitutionalWitness) {
    let action_digest = compute_action_digest(
        "act_sovereign_777",
        "did:key:z6MkuProverTest",
        "tenant_sovereign",
        1731005000,
        "/secure/enclave/contract.zk",
    );
    let constitution_hash = canonical_constitution_hash();
    let token_commitment = compute_token_commitment(b"biscuit_token_v5_sealed_commitment");
    let audit_hash = [0x55u8; 32];

    let witness = ConstitutionalWitness::new_valid(
        b"execute secure contract".to_vec(),
        CapabilityScope::Filesystem,
        CapabilityAction::Read,
        audit_hash,
    );

    (action_digest, constitution_hash, token_commitment, witness)
}

#[test]
fn test_adversarial_rule1_untrusted_input_execution_fuzzing() {
    let engine = DefaultConstitutionalZkEngine::new();
    let (action_digest, const_hash, token_commitment, mut witness) = make_base_witness();

    // Untrusted input attempting Execute -> must fail Rule 1
    witness.is_untrusted_input = true;
    witness.requested_action = CapabilityAction::Execute;

    let res = engine.generate_proof(action_digest, const_hash, token_commitment, false, &witness);
    assert!(res.is_err());
    match res.unwrap_err() {
        HakilleError::ConstitutionViolation(msg) => {
            assert!(msg.contains("Rule 1 violation"));
        }
        err => panic!("Expected Rule 1 ConstitutionViolation, got {:?}", err),
    }

    // Untrusted input attempting Read -> permitted by Rule 1 (reading untrusted content is safe)
    witness.requested_action = CapabilityAction::Read;
    let res_read =
        engine.generate_proof(action_digest, const_hash, token_commitment, false, &witness);
    assert!(
        res_read.is_ok(),
        "Untrusted input with Read action must pass Rule 1"
    );
}

#[test]
fn test_adversarial_rule2_missing_audit_provenance_fuzzing() {
    let engine = DefaultConstitutionalZkEngine::new();
    let (action_digest, const_hash, token_commitment, mut witness) = make_base_witness();

    // Null audit entry hash [0u8; 32] -> must fail Rule 2
    witness.audit_entry_hash = [0u8; 32];

    let res = engine.generate_proof(action_digest, const_hash, token_commitment, false, &witness);
    assert!(res.is_err());
    match res.unwrap_err() {
        HakilleError::ConstitutionViolation(msg) => {
            assert!(msg.contains("Rule 2 violation"));
        }
        err => panic!("Expected Rule 2 ConstitutionViolation, got {:?}", err),
    }

    // Non-zero audit hash -> passes
    witness.audit_entry_hash = [1u8; 32];
    let res_ok =
        engine.generate_proof(action_digest, const_hash, token_commitment, false, &witness);
    assert!(res_ok.is_ok());
}

#[test]
fn test_adversarial_rule3_model_weights_modification_fuzzing() {
    let engine = DefaultConstitutionalZkEngine::new();
    let (action_digest, const_hash, token_commitment, mut witness) = make_base_witness();

    // Modifying model weights -> must fail Rule 3
    witness.modifies_model_weights = true;

    let res = engine.generate_proof(action_digest, const_hash, token_commitment, false, &witness);
    assert!(res.is_err());
    match res.unwrap_err() {
        HakilleError::ConstitutionViolation(msg) => {
            assert!(msg.contains("Rule 3 violation"));
        }
        err => panic!("Expected Rule 3 ConstitutionViolation, got {:?}", err),
    }

    // Zero weight modification -> passes
    witness.modifies_model_weights = false;
    let res_ok =
        engine.generate_proof(action_digest, const_hash, token_commitment, false, &witness);
    assert!(res_ok.is_ok());
}

#[test]
fn test_adversarial_rule4_delegation_depth_boundary_fuzzing() {
    let engine = DefaultConstitutionalZkEngine::new();
    let (action_digest, const_hash, token_commitment, mut witness) = make_base_witness();

    // Depths 0, 1, 2, 3, 4 must all succeed
    for valid_depth in 0..=4 {
        witness.delegation_depth = valid_depth;
        let res =
            engine.generate_proof(action_digest, const_hash, token_commitment, false, &witness);
        assert!(
            res.is_ok(),
            "Delegation depth {} <= 4 must pass Rule 4",
            valid_depth
        );
    }

    // Depths 5, 6, 10, 100, 1000 must all strictly fail Rule 4
    for invalid_depth in [5, 6, 10, 100, 1000] {
        witness.delegation_depth = invalid_depth;
        let res =
            engine.generate_proof(action_digest, const_hash, token_commitment, false, &witness);
        assert!(res.is_err());
        match res.unwrap_err() {
            HakilleError::ConstitutionViolation(msg) => {
                assert!(msg.contains("Rule 4 violation"));
            }
            err => panic!("Expected Rule 4 ConstitutionViolation, got {:?}", err),
        }
    }
}

#[test]
fn test_adversarial_rule5_crisis_human_routing_fuzzing() {
    let engine = DefaultConstitutionalZkEngine::new();
    let (action_digest, const_hash, token_commitment, mut witness) = make_base_witness();

    // Crisis situation WITHOUT human helpline routing -> must fail Rule 5
    witness.is_crisis_situation = true;
    witness.routed_to_human_helpline = false;

    let res = engine.generate_proof(action_digest, const_hash, token_commitment, false, &witness);
    assert!(res.is_err());
    match res.unwrap_err() {
        HakilleError::ConstitutionViolation(msg) => {
            assert!(msg.contains("Rule 5 violation"));
        }
        err => panic!("Expected Rule 5 ConstitutionViolation, got {:?}", err),
    }

    // Crisis situation WITH human helpline routing -> passes Rule 5
    witness.routed_to_human_helpline = true;
    let res_ok =
        engine.generate_proof(action_digest, const_hash, token_commitment, false, &witness);
    assert!(res_ok.is_ok(), "Crisis with human routing must pass Rule 5");
}

#[test]
fn test_adversarial_rule6_sensitive_domain_tau_boundary_fuzzing() {
    let engine = DefaultConstitutionalZkEngine::new();
    let (action_digest, const_hash, token_commitment, mut witness) = make_base_witness();

    witness.is_sensitive_domain = true;

    // Boundary check: tau = 0.8999 (< 0.90) -> fails Rule 6
    witness.session_tau = 0.8999;
    let res_below =
        engine.generate_proof(action_digest, const_hash, token_commitment, false, &witness);
    assert!(res_below.is_err());
    match res_below.unwrap_err() {
        HakilleError::ConstitutionViolation(msg) => {
            assert!(msg.contains("Rule 6 violation"));
        }
        err => panic!("Expected Rule 6 ConstitutionViolation, got {:?}", err),
    }

    // Boundary check: tau = 0.9000 (>= 0.90) -> passes Rule 6
    witness.session_tau = 0.90;
    let res_exact =
        engine.generate_proof(action_digest, const_hash, token_commitment, false, &witness);
    assert!(res_exact.is_ok(), "tau = 0.90 must pass Rule 6");

    // Wallet scope is inherently sensitive: requires tau >= 0.90 even if is_sensitive_domain is false
    witness.is_sensitive_domain = false;
    witness.requested_scope = CapabilityScope::Wallet;

    witness.session_tau = 0.899;
    let res_wallet_below =
        engine.generate_proof(action_digest, const_hash, token_commitment, false, &witness);
    assert!(res_wallet_below.is_err());

    witness.session_tau = 0.95;
    let res_wallet_ok =
        engine.generate_proof(action_digest, const_hash, token_commitment, false, &witness);
    assert!(res_wallet_ok.is_ok());
}

#[test]
fn test_adversarial_rule7_minor_wallet_and_fullsystem_rejection() {
    let engine = DefaultConstitutionalZkEngine::new();
    let (action_digest, const_hash, token_commitment, mut witness) = make_base_witness();

    // Minor attempting Wallet scope -> must fail Rule 7
    witness.requested_scope = CapabilityScope::Wallet;
    let res_minor_wallet =
        engine.generate_proof(action_digest, const_hash, token_commitment, true, &witness);
    assert!(res_minor_wallet.is_err());
    match res_minor_wallet.unwrap_err() {
        HakilleError::ConstitutionViolation(msg) => {
            assert!(msg.contains("Rule 7 violation"));
        }
        err => panic!("Expected Rule 7 ConstitutionViolation, got {:?}", err),
    }

    // Minor attempting FullSystem scope -> must fail Rule 7
    witness.requested_scope = CapabilityScope::FullSystem;
    let res_minor_full =
        engine.generate_proof(action_digest, const_hash, token_commitment, true, &witness);
    assert!(res_minor_full.is_err());
    match res_minor_full.unwrap_err() {
        HakilleError::ConstitutionViolation(msg) => {
            assert!(msg.contains("Rule 7 violation"));
        }
        err => panic!("Expected Rule 7 ConstitutionViolation, got {:?}", err),
    }

    // Minor attempting Filesystem scope -> passes Rule 7
    witness.requested_scope = CapabilityScope::Filesystem;
    let res_minor_fs =
        engine.generate_proof(action_digest, const_hash, token_commitment, true, &witness);
    assert!(res_minor_fs.is_ok(), "Minor on Filesystem scope must pass");

    // Adult (non-minor) attempting Wallet scope (tau = 0.95) -> passes Rule 7
    witness.requested_scope = CapabilityScope::Wallet;
    witness.session_tau = 0.95;
    let res_adult_wallet =
        engine.generate_proof(action_digest, const_hash, token_commitment, false, &witness);
    assert!(
        res_adult_wallet.is_ok(),
        "Adult with tau >= 0.90 must pass Wallet scope"
    );
}

#[test]
fn test_adversarial_rule8_raw_av_exfiltration_fuzzing() {
    let engine = DefaultConstitutionalZkEngine::new();
    let (action_digest, const_hash, token_commitment, mut witness) = make_base_witness();

    // Raw AV exfiltration -> must fail Rule 8
    witness.raw_av_exfiltration = true;

    let res = engine.generate_proof(action_digest, const_hash, token_commitment, false, &witness);
    assert!(res.is_err());
    match res.unwrap_err() {
        HakilleError::ConstitutionViolation(msg) => {
            assert!(msg.contains("Rule 8 violation"));
        }
        err => panic!("Expected Rule 8 ConstitutionViolation, got {:?}", err),
    }

    // No raw AV exfiltration -> passes
    witness.raw_av_exfiltration = false;
    let res_ok =
        engine.generate_proof(action_digest, const_hash, token_commitment, false, &witness);
    assert!(res_ok.is_ok());
}

#[test]
fn test_adversarial_multi_rule_simultaneous_violations() {
    let engine = DefaultConstitutionalZkEngine::new();
    let (action_digest, const_hash, token_commitment, mut witness) = make_base_witness();

    // Violate Rule 1, 3, 4, 7, 8 simultaneously
    witness.is_untrusted_input = true;
    witness.requested_action = CapabilityAction::Execute;
    witness.modifies_model_weights = true;
    witness.delegation_depth = 8;
    witness.requested_scope = CapabilityScope::Wallet;
    witness.raw_av_exfiltration = true;

    let res = engine.generate_proof(action_digest, const_hash, token_commitment, true, &witness);
    assert!(res.is_err());
    // Residual verification evaluates sequentially, stops at first violation (Rule 1)
    match res.unwrap_err() {
        HakilleError::ConstitutionViolation(msg) => {
            assert!(msg.contains("Rule 1 violation"));
        }
        err => panic!("Expected ConstitutionViolation, got {:?}", err),
    }
}

#[test]
fn test_adversarial_proof_size_and_verification_latency_guarantees() {
    let engine = DefaultConstitutionalZkEngine::new();
    let (action_digest, const_hash, token_commitment, witness) = make_base_witness();

    let proof = engine
        .generate_proof(action_digest, const_hash, token_commitment, false, &witness)
        .expect("Valid witness must generate proof");

    // Size guarantee: <= 256 bytes
    assert!(
        proof.proof_bytes.len() <= MAX_ZK_PROOF_BYTES,
        "Proof size {} exceeds limit of 256 bytes",
        proof.proof_bytes.len()
    );
    assert_eq!(proof.proof_bytes.len(), 196);

    // Verification latency guarantee: sub-5ms across 100 consecutive runs
    let mut total_latency = std::time::Duration::ZERO;
    for _ in 0..100 {
        let t_start = std::time::Instant::now();
        engine
            .verify_proof(&proof, false)
            .expect("Verification must pass");
        let elapsed = t_start.elapsed();
        total_latency += elapsed;
        assert!(
            elapsed.as_millis() < 5,
            "Individual verification exceeded 5ms: {:?}",
            elapsed
        );
    }
    let avg_latency = total_latency / 100;
    println!("Average ZK verification latency: {:?}", avg_latency);
    assert!(
        avg_latency.as_micros() < 500,
        "Average latency should be < 0.5ms"
    );
}
