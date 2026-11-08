//! Integration tests for Pillar 4: Preuve ZK de Respect Constitutionnel.
//!
//! Validates arithmetic / R1CS circuit satisfaction across all 8 fundamental rules,
//! non-interactive zero-knowledge proof generation, sub-5ms verification, and
//! cryptographic tamper resistance.

use hakille_constitution::zk::{
    ConstitutionalWitness, ConstitutionalZkEngine, DefaultConstitutionalZkEngine,
    MAX_ZK_PROOF_BYTES, canonical_constitution_hash, compute_action_digest,
    compute_token_commitment,
};
use hakille_core::capability::{CapabilityAction, CapabilityScope};

fn make_base_witness() -> ConstitutionalWitness {
    ConstitutionalWitness::new_valid(
        b"system.execute_task(id: 123)".to_vec(),
        CapabilityScope::Filesystem,
        CapabilityAction::Read,
        [42u8; 32], // Valid non-zero audit hash
    )
}

#[test]
fn test_integration_zk_valid_action_e2e_under_5ms() {
    let engine = DefaultConstitutionalZkEngine::new();
    let action_digest = compute_action_digest(
        "act-e2e-101",
        "did:key:z6MkuS",
        "tenant-enterprise",
        1731050000,
        "/vfs/secure/data.bin",
    );
    let constitution_hash = canonical_constitution_hash();
    let token_commitment = compute_token_commitment(b"biscuit_v5_sealed_token_bytes");
    let witness = make_base_witness();

    // Generate ZK proof
    let start_gen = std::time::Instant::now();
    let proof = engine
        .generate_proof(
            action_digest,
            constitution_hash,
            token_commitment,
            false,
            &witness,
        )
        .expect("Proof generation for valid witness must succeed");
    let gen_dur = start_gen.elapsed();
    println!("ZK Proof Generation Latency: {:?}", gen_dur);

    // Verify proof structure & byte size
    assert!(proof.proof_bytes.len() <= MAX_ZK_PROOF_BYTES);
    assert_eq!(proof.proof_bytes.len(), 196);
    assert!(proof.is_structurally_valid());

    // Verify ZK proof and measure latency (< 5ms)
    let start_verify = std::time::Instant::now();
    let verify_res = engine.verify_proof(&proof, false);
    let verify_dur = start_verify.elapsed();
    println!("ZK Proof Verification Latency: {:?}", verify_dur);

    assert!(verify_res.is_ok());
    assert!(
        verify_dur.as_millis() < 5,
        "Verification must complete under 5ms, took: {:?}",
        verify_dur
    );
}

#[test]
fn test_integration_zk_rule7_minor_wallet_and_fullsystem_denied() {
    let engine = DefaultConstitutionalZkEngine::new();
    let action_digest = [1u8; 32];
    let constitution_hash = canonical_constitution_hash();
    let token_commitment = [2u8; 32];

    // Case A: Minor attempting Wallet scope
    let mut witness_wallet = make_base_witness();
    witness_wallet.requested_scope = CapabilityScope::Wallet;
    let res_wallet = engine.generate_proof(
        action_digest,
        constitution_hash,
        token_commitment,
        true, // is_minor = true
        &witness_wallet,
    );
    assert!(res_wallet.is_err());
    let err_msg = format!("{:?}", res_wallet.unwrap_err());
    assert!(err_msg.contains("Rule 7 violation"));

    // Case B: Minor attempting FullSystem scope
    let mut witness_fullsys = make_base_witness();
    witness_fullsys.requested_scope = CapabilityScope::FullSystem;
    let res_fullsys = engine.generate_proof(
        action_digest,
        constitution_hash,
        token_commitment,
        true, // is_minor = true
        &witness_fullsys,
    );
    assert!(res_fullsys.is_err());
    let err_msg = format!("{:?}", res_fullsys.unwrap_err());
    assert!(err_msg.contains("Rule 7 violation"));

    // Case C: Non-minor allowed Wallet scope
    let res_adult = engine.generate_proof(
        action_digest,
        constitution_hash,
        token_commitment,
        false, // is_minor = false
        &witness_wallet,
    );
    assert!(res_adult.is_ok(), "Adult with high tau can access Wallet");
}

#[test]
fn test_integration_zk_rule1_untrusted_input_cannot_execute() {
    let engine = DefaultConstitutionalZkEngine::new();
    let action_digest = [1u8; 32];
    let constitution_hash = canonical_constitution_hash();
    let token_commitment = [2u8; 32];

    let mut witness = make_base_witness();
    witness.is_untrusted_input = true;
    witness.requested_action = CapabilityAction::Execute;

    let res = engine.generate_proof(
        action_digest,
        constitution_hash,
        token_commitment,
        false,
        &witness,
    );
    assert!(res.is_err());
    let err_msg = format!("{:?}", res.unwrap_err());
    assert!(err_msg.contains("Rule 1 violation"));
}

#[test]
fn test_integration_zk_all_other_rules_enforcement() {
    let engine = DefaultConstitutionalZkEngine::new();
    let action_digest = [1u8; 32];
    let constitution_hash = canonical_constitution_hash();
    let token_commitment = [2u8; 32];

    // Rule 2: Zero audit hash
    let mut w2 = make_base_witness();
    w2.audit_entry_hash = [0u8; 32];
    let r2 = engine.generate_proof(
        action_digest,
        constitution_hash,
        token_commitment,
        false,
        &w2,
    );
    assert!(r2.is_err());
    assert!(format!("{:?}", r2.unwrap_err()).contains("Rule 2 violation"));

    // Rule 3: Neural model weight modification
    let mut w3 = make_base_witness();
    w3.modifies_model_weights = true;
    let r3 = engine.generate_proof(
        action_digest,
        constitution_hash,
        token_commitment,
        false,
        &w3,
    );
    assert!(r3.is_err());
    assert!(format!("{:?}", r3.unwrap_err()).contains("Rule 3 violation"));

    // Rule 4: Delegation depth D = 5
    let mut w4 = make_base_witness();
    w4.delegation_depth = 5;
    let r4 = engine.generate_proof(
        action_digest,
        constitution_hash,
        token_commitment,
        false,
        &w4,
    );
    assert!(r4.is_err());
    assert!(format!("{:?}", r4.unwrap_err()).contains("Rule 4 violation"));

    // Rule 5: Crisis without helpline referral
    let mut w5 = make_base_witness();
    w5.is_crisis_situation = true;
    w5.routed_to_human_helpline = false;
    let r5 = engine.generate_proof(
        action_digest,
        constitution_hash,
        token_commitment,
        false,
        &w5,
    );
    assert!(r5.is_err());
    assert!(format!("{:?}", r5.unwrap_err()).contains("Rule 5 violation"));

    // Rule 6: Sensitive domain with low confidence (tau = 0.82 < 0.90)
    let mut w6 = make_base_witness();
    w6.is_sensitive_domain = true;
    w6.session_tau = 0.82;
    let r6 = engine.generate_proof(
        action_digest,
        constitution_hash,
        token_commitment,
        false,
        &w6,
    );
    assert!(r6.is_err());
    assert!(format!("{:?}", r6.unwrap_err()).contains("Rule 6 violation"));

    // Rule 8: Raw A/V stream exfiltration
    let mut w8 = make_base_witness();
    w8.raw_av_exfiltration = true;
    let r8 = engine.generate_proof(
        action_digest,
        constitution_hash,
        token_commitment,
        false,
        &w8,
    );
    assert!(r8.is_err());
    assert!(format!("{:?}", r8.unwrap_err()).contains("Rule 8 violation"));
}

#[test]
fn test_integration_zk_tamper_matrix() {
    let engine = DefaultConstitutionalZkEngine::new();
    let action_digest = [11u8; 32];
    let constitution_hash = canonical_constitution_hash();
    let token_commitment = [22u8; 32];
    let witness = make_base_witness();

    let valid_proof = engine
        .generate_proof(
            action_digest,
            constitution_hash,
            token_commitment,
            false,
            &witness,
        )
        .expect("Valid proof");

    // Attack 1: Modify 1 byte of public action digest
    let mut tampered_act = valid_proof.clone();
    tampered_act.public_action_digest[15] ^= 0x01;
    assert!(engine.verify_proof(&tampered_act, false).is_err());

    // Attack 2: Modify constitution hash
    let mut tampered_const = valid_proof.clone();
    tampered_const.constitution_hash[0] ^= 0x01;
    assert!(engine.verify_proof(&tampered_const, false).is_err());

    // Attack 3: Modify token commitment
    let mut tampered_tok = valid_proof.clone();
    tampered_tok.token_commitment[7] ^= 0x01;
    assert!(engine.verify_proof(&tampered_tok, false).is_err());

    // Attack 4: Modify timestamp
    let mut tampered_ts = valid_proof.clone();
    tampered_ts.created_at_ts += 1;
    assert!(engine.verify_proof(&tampered_ts, false).is_err());

    // Attack 5: Modify bit in proof bytes payload
    let mut tampered_bytes = valid_proof.clone();
    let len = tampered_bytes.proof_bytes.len();
    tampered_bytes.proof_bytes[len - 1] ^= 0xff;
    assert!(engine.verify_proof(&tampered_bytes, false).is_err());

    // Attack 6: Truncate proof bytes
    let mut truncated_bytes = valid_proof.clone();
    truncated_bytes.proof_bytes.pop();
    assert!(engine.verify_proof(&truncated_bytes, false).is_err());

    // Attack 7: Present non-minor proof under minor context
    assert!(engine.verify_proof(&valid_proof, true).is_err());
}
