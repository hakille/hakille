//! Empirical Boundary and Stress Tests for Pillar 4 ZK Proofs.
//!
//! Evaluates exact security boundaries:
//! - ZK proof tampering (mutated proof bytes, digest, constitution hash, token commitment, magic header, size).
//! - Minor profile attempting Wallet or FullSystem scope (Rule 7 enforcement).

use hakille_constitution::zk::{
    ConstitutionalWitness, ConstitutionalZkEngine, DefaultConstitutionalZkEngine,
    canonical_constitution_hash, compute_action_digest, compute_token_commitment,
};
use hakille_core::capability::{CapabilityAction, CapabilityScope};
use std::time::Instant;

fn make_valid_witness(scope: CapabilityScope, action: CapabilityAction) -> ConstitutionalWitness {
    let payload = b"benign_action_execution_payload".to_vec();
    ConstitutionalWitness::new_valid(payload, scope, action, [0x99u8; 32])
}

// -----------------------------------------------------------------------------
// Pillar 4: ZK Proof Tampering Matrix
// -----------------------------------------------------------------------------

#[test]
fn test_pillar4_zk_proof_tampering_matrix() {
    let engine = DefaultConstitutionalZkEngine::new();
    let action_digest = compute_action_digest(
        "action_001",
        "did:key:alice",
        "tenant_01",
        1700000000,
        "resource_fs",
    );
    let const_hash = canonical_constitution_hash();
    let token_commitment = compute_token_commitment(b"valid_biscuit_token_bytes");

    let witness = make_valid_witness(CapabilityScope::Filesystem, CapabilityAction::Read);

    // Generate valid reference proof
    let t_start = Instant::now();
    let valid_proof = engine
        .generate_proof(action_digest, const_hash, token_commitment, false, &witness)
        .expect("Valid proof generation must succeed");
    let gen_dur = t_start.elapsed();

    assert!(
        gen_dur.as_millis() < 10,
        "Proof generation must be fast (< 10ms)"
    );
    assert_eq!(valid_proof.proof_bytes.len(), 196);
    assert!(valid_proof.proof_bytes.len() <= 256);

    // Verify valid proof succeeds in < 5ms
    let t_ver = Instant::now();
    let verify_res = engine.verify_proof(&valid_proof, false);
    let ver_dur = t_ver.elapsed();

    assert!(verify_res.is_ok(), "Valid proof must verify cleanly");
    assert!(
        ver_dur.as_millis() < 5,
        "Verification must run in < 5ms (took {:?})",
        ver_dur
    );

    // Attack 1: Mutated byte in proof_bytes (witness commitment segment)
    let mut tampered_bytes = valid_proof.clone();
    tampered_bytes.proof_bytes[10] ^= 0x55;
    assert!(
        engine.verify_proof(&tampered_bytes, false).is_err(),
        "Mutating proof_bytes (witness commitment) must fail verification"
    );

    // Attack 2: Mutated byte in proof_bytes (challenge segment)
    let mut tampered_challenge = valid_proof.clone();
    tampered_challenge.proof_bytes[80] ^= 0xAA;
    assert!(
        engine.verify_proof(&tampered_challenge, false).is_err(),
        "Mutating proof_bytes (Fiat-Shamir challenge) must fail verification"
    );

    // Attack 3: Mutated byte in proof_bytes (algebraic tag segment)
    let mut tampered_tag = valid_proof.clone();
    tampered_tag.proof_bytes[180] ^= 0x0F;
    assert!(
        engine.verify_proof(&tampered_tag, false).is_err(),
        "Mutating proof_bytes (algebraic tag) must fail verification"
    );

    // Attack 4: Mutated public action digest
    let mut tampered_digest = valid_proof.clone();
    tampered_digest.public_action_digest[0] ^= 0x01;
    assert!(
        engine.verify_proof(&tampered_digest, false).is_err(),
        "Mutating public_action_digest must fail verification"
    );

    // Attack 5: Mutated constitution hash
    let mut tampered_const = valid_proof.clone();
    tampered_const.constitution_hash[5] ^= 0x80;
    assert!(
        engine.verify_proof(&tampered_const, false).is_err(),
        "Mutating constitution_hash must fail verification"
    );

    // Attack 6: Mutated token commitment
    let mut tampered_token = valid_proof.clone();
    tampered_token.token_commitment[31] ^= 0x01;
    assert!(
        engine.verify_proof(&tampered_token, false).is_err(),
        "Mutating token_commitment must fail verification"
    );

    // Attack 7: Discrepancy in is_minor context flag
    assert!(
        engine.verify_proof(&valid_proof, true).is_err(),
        "Verifying proof generated for adult with is_minor=true must fail verification"
    );

    // Attack 8: Corrupted magic header
    let mut tampered_magic = valid_proof.clone();
    tampered_magic.proof_bytes[0] = b'X';
    assert!(
        engine.verify_proof(&tampered_magic, false).is_err(),
        "Corrupted magic header must fail verification"
    );

    // Attack 9: Truncated proof bytes
    let mut truncated_proof = valid_proof.clone();
    truncated_proof.proof_bytes.truncate(150);
    assert!(
        engine.verify_proof(&truncated_proof, false).is_err(),
        "Truncated proof bytes must fail verification"
    );
}

// -----------------------------------------------------------------------------
// Pillar 4: Rule 7 Minor Attempting Wallet or FullSystem Scope
// -----------------------------------------------------------------------------

#[test]
fn test_pillar4_minor_attempting_wallet_or_fullsystem_scope() {
    let engine = DefaultConstitutionalZkEngine::new();
    let action_digest = compute_action_digest(
        "action_wallet_req",
        "did:key:leo_minor",
        "family_tenant",
        1700000000,
        "resource_wallet",
    );
    let const_hash = canonical_constitution_hash();
    let token_commitment = compute_token_commitment(b"token_wallet_attempt");

    // Case A: Adult attempting Wallet scope -> ALLOWED
    let adult_witness = make_valid_witness(CapabilityScope::Wallet, CapabilityAction::Write);
    let adult_proof = engine.generate_proof(
        action_digest,
        const_hash,
        token_commitment,
        false,
        &adult_witness,
    );
    assert!(
        adult_proof.is_ok(),
        "Adult attempting Wallet scope must be allowed by Rule 7"
    );
    assert!(engine.verify_proof(&adult_proof.unwrap(), false).is_ok());

    // Case B: Minor profile attempting Wallet scope -> STRICTLY PROHIBITED (Rule 7)
    let minor_wallet_witness = make_valid_witness(CapabilityScope::Wallet, CapabilityAction::Write);
    let minor_wallet_err = engine.generate_proof(
        action_digest,
        const_hash,
        token_commitment,
        true, // is_minor = true
        &minor_wallet_witness,
    );

    assert!(
        minor_wallet_err.is_err(),
        "Minor attempting Wallet scope must fail ZK circuit satisfaction"
    );
    let err_msg = format!("{:?}", minor_wallet_err.err().unwrap());
    assert!(
        err_msg.contains("Rule 7 violation")
            || err_msg.contains("Minor profiles are strictly prohibited from Wallet"),
        "Error message must explicitly cite Rule 7: {}",
        err_msg
    );

    // Case C: Minor profile attempting FullSystem scope -> STRICTLY PROHIBITED (Rule 7)
    let minor_fullsys_witness =
        make_valid_witness(CapabilityScope::FullSystem, CapabilityAction::Execute);
    let minor_fullsys_err = engine.generate_proof(
        action_digest,
        const_hash,
        token_commitment,
        true, // is_minor = true
        &minor_fullsys_witness,
    );

    assert!(
        minor_fullsys_err.is_err(),
        "Minor attempting FullSystem scope must fail ZK circuit satisfaction"
    );
    let err_msg2 = format!("{:?}", minor_fullsys_err.err().unwrap());
    assert!(
        err_msg2.contains("Rule 7 violation"),
        "Error message must cite Rule 7: {}",
        err_msg2
    );

    // Case D: Minor profile requesting standard educational/sensory scope (e.g. Sensor / GuiInput) -> ALLOWED
    let minor_safe_witness = make_valid_witness(CapabilityScope::Sensor, CapabilityAction::Read);
    let minor_safe_proof = engine.generate_proof(
        action_digest,
        const_hash,
        token_commitment,
        true, // is_minor = true
        &minor_safe_witness,
    );

    assert!(
        minor_safe_proof.is_ok(),
        "Minor requesting SensorData scope must be permitted"
    );
    assert!(
        engine
            .verify_proof(&minor_safe_proof.unwrap(), true)
            .is_ok()
    );
}
