//! Adversarial Empirical Challenge Suite for Milestone 1
//! Cryptographic Core & Zero-Knowledge Storage
//! Targets: crypto.rs, storage.rs, ssi.rs

use hakille_core::HakilleError;
use hakille_core::UserId;
use hakille_identity::*;
use ed25519_dalek::SigningKey;
use rand::{RngCore, rngs::OsRng};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use uuid::Uuid;

// =============================================================================
// SUITE 1: Shamir Secret Sharing Empirical Challenge
// =============================================================================

#[test]
fn test_adversarial_shamir_corrupted_share_data_fails() {
    let secret = [0x55u8; 32];
    let mut shares = split_secret(&secret, 3, 5).expect("split succeeds");

    // Adversarially mutate 1 byte in share 0
    shares[0].data[15] ^= 0xaa;
    let res = reconstruct_secret(&shares[0..3]);
    assert!(res.is_err(), "Corrupted share data must fail commitment verification");
    match res.unwrap_err() {
        HakilleError::CryptoError(msg) => {
            assert!(msg.contains("failed commitment verification"), "Expected commitment error, got: {msg}");
        }
        other => panic!("Unexpected error type: {:?}", other),
    }
}

#[test]
fn test_adversarial_shamir_corrupted_share_index_fails() {
    let secret = [0x77u8; 32];
    let mut shares = split_secret(&secret, 3, 5).unwrap();

    // Adversarially change share index from 1 to 9 (data untouched)
    shares[0].index = 9;
    let res = reconstruct_secret(&shares[0..3]);
    assert!(res.is_err(), "Tampered share index must fail commitment verification");
}

#[test]
fn test_adversarial_shamir_corrupted_share_commitment_fails() {
    let secret = [0x88u8; 32];
    let mut shares = split_secret(&secret, 3, 5).unwrap();

    // Corrupt the commitment hash directly
    shares[1].commitment[0] ^= 0x01;
    let res = reconstruct_secret(&shares[0..3]);
    assert!(res.is_err(), "Mismatched commitment must be rejected");
}

#[test]
fn test_adversarial_shamir_insufficient_shares_cannot_recover_secret() {
    let secret = [0x33u8; 32];
    let shares = split_secret(&secret, 3, 5).unwrap();

    // 0 shares -> explicit error
    assert!(reconstruct_secret(&[]).is_err());

    // 1 share (< k=3): must return Err (insufficient shares)
    let res_1 = reconstruct_secret(&shares[0..1]);
    assert!(res_1.is_err(), "1 share must fail reconstruction with InsufficientShares");

    // 2 shares (< k=3): must return Err (insufficient shares)
    let res_2 = reconstruct_secret(&shares[0..2]);
    assert!(res_2.is_err(), "2 shares must fail reconstruction with InsufficientShares");

    // Statistical check: 50 trials with random secrets and k-1 shares
    for _trial in 0..50 {
        let mut sec = [0u8; 32];
        OsRng.fill_bytes(&mut sec);
        let sh = split_secret(&sec, 4, 7).unwrap();
        // provide 3 shares (k=4 required)
        let res = reconstruct_secret(&sh[0..3]);
        assert!(res.is_err(), "k-1 shares must fail reconstruction with InsufficientShares");
    }
}

#[test]
fn test_adversarial_shamir_out_of_order_permutations_recover_exact_secret() {
    let secret = [0xfe_u8; 32];
    let shares = split_secret(&secret, 4, 7).unwrap();

    let p1 = vec![shares[0].clone(), shares[1].clone(), shares[2].clone(), shares[3].clone()];
    let p2 = vec![shares[3].clone(), shares[2].clone(), shares[1].clone(), shares[0].clone()];
    let p3 = vec![shares[1].clone(), shares[3].clone(), shares[0].clone(), shares[2].clone()];
    let p4 = vec![shares[2].clone(), shares[0].clone(), shares[3].clone(), shares[1].clone()];

    assert_eq!(reconstruct_secret(&p1).unwrap(), secret);
    assert_eq!(reconstruct_secret(&p2).unwrap(), secret);
    assert_eq!(reconstruct_secret(&p3).unwrap(), secret);
    assert_eq!(reconstruct_secret(&p4).unwrap(), secret);
}

#[test]
fn test_adversarial_shamir_duplicate_share_indices_rejected() {
    let secret = [0x42u8; 32];
    let shares = split_secret(&secret, 3, 5).unwrap();

    // Identical duplicate
    let dup1 = vec![shares[0].clone(), shares[0].clone(), shares[1].clone()];
    assert!(reconstruct_secret(&dup1).is_err());

    // Duplicate index with different data
    let mut share_diff_data = shares[0].clone();
    share_diff_data.data[0] ^= 0x01;
    // recompute commitment so it passes commitment check
    let mut h = Sha256::new();
    h.update(&[share_diff_data.index]);
    h.update(&share_diff_data.data);
    share_diff_data.commitment.copy_from_slice(&h.finalize());

    let dup2 = vec![shares[0].clone(), share_diff_data, shares[1].clone()];
    let res = reconstruct_secret(&dup2);
    assert!(res.is_err());
    match res.unwrap_err() {
        HakilleError::CryptoError(msg) => {
            assert!(msg.contains("Duplicate share index"), "Expected duplicate share error, got: {msg}");
        }
        other => panic!("Unexpected error: {:?}", other),
    }
}

#[test]
fn test_adversarial_shamir_zero_share_index_rejected() {
    let fake_share = SecretShare {
        index: 0,
        data: vec![0u8; 32],
        commitment: [0u8; 32],
    };
    let res = reconstruct_secret(&[fake_share]);
    assert!(res.is_err());
}

#[test]
fn test_adversarial_shamir_invalid_share_data_length_rejected() {
    let secret = [0x11u8; 32];
    let mut shares = split_secret(&secret, 2, 3).unwrap();
    shares[0].data.pop(); // 31 bytes
    let res = reconstruct_secret(&shares[0..2]);
    assert!(res.is_err());
}

#[test]
fn test_adversarial_shamir_maximum_field_size_255_shares() {
    let secret = [0x99u8; 32];
    // Split with total = 255 shares, threshold = 10
    let shares = split_secret(&secret, 10, 255).expect("255 shares split succeeds");
    assert_eq!(shares.len(), 255);

    // Pick 10 arbitrary distinct shares across the entire GF(2^8) spectrum
    let indices = [0, 24, 49, 74, 99, 124, 149, 174, 199, 254];
    let subset: Vec<SecretShare> = indices.iter().map(|&i| shares[i].clone()).collect();
    let recovered = reconstruct_secret(&subset).expect("reconstruct 10-of-255 succeeds");
    assert_eq!(recovered, secret);
}

#[test]
fn test_adversarial_shamir_forged_share_local_commitment_bypasses_share_integrity() {
    let secret = [0xaau8; 32];
    let shares = split_secret(&secret, 3, 5).unwrap();

    let mut forged_share = shares[0].clone();
    forged_share.data = vec![0x66u8; 32]; // Forged data
    let mut h = Sha256::new();
    h.update(&[forged_share.index]);
    h.update(&forged_share.data);
    forged_share.commitment.copy_from_slice(&h.finalize());

    let mixed = vec![forged_share, shares[1].clone(), shares[2].clone()];
    let res = reconstruct_secret(&mixed);
    assert!(res.is_err(), "Byzantine / forged share must be rejected during verification");
}

// =============================================================================
// SUITE 2: XChaCha20-Poly1305 AEAD Empirical Challenge
// =============================================================================

#[test]
fn test_adversarial_xchacha_bit_flip_at_all_offsets() {
    let key = [0x44u8; 32];
    let plaintext = b"Hakille Sovereign Operating System High Security Cryptographic Core 2026";
    let aad = b"tenant:core|table:journal";

    let blob = XChaChaCipher::encrypt(plaintext, &key, aad).unwrap();

    // Adversarially flip every single byte in the ciphertext one by one
    for offset in 0..blob.ciphertext.len() {
        let mut tampered = blob.clone();
        tampered.ciphertext[offset] ^= 0x01;
        let res = XChaChaCipher::decrypt(&tampered, &key, aad);
        assert!(res.is_err(), "Bit flip at ciphertext offset {offset} must fail AEAD verification");
    }
}

#[test]
fn test_adversarial_xchacha_tampered_aad_fails_fingerprint() {
    let key = [0x55u8; 32];
    let plaintext = b"Sensitive Financial Wire Authorization";
    let original_aad = b"tenant:family|account:12345";
    let tampered_aad = b"tenant:enterprise|account:12345";

    let blob = XChaChaCipher::encrypt(plaintext, &key, original_aad).unwrap();
    let res = XChaChaCipher::decrypt(&blob, &key, tampered_aad);
    assert!(res.is_err(), "Tampered AAD must fail");
    match res.unwrap_err() {
        HakilleError::CryptoError(msg) => {
            assert!(msg.contains("AAD fingerprint mismatch"), "Expected fingerprint mismatch error, got: {msg}");
        }
        other => panic!("Unexpected error: {:?}", other),
    }
}

#[test]
fn test_adversarial_xchacha_tampered_aad_and_tampered_fingerprint_fails_poly1305() {
    // Attack scenario: Attacker intercepts blob and tampered AAD, and rewrites the
    // aad_fingerprint field on the blob to match the tampered AAD.
    // The fingerprint check will pass, but the underlying Poly1305 MAC MUST reject it.
    let key = [0x66u8; 32];
    let plaintext = b"Sovereign Kernel Delegation Token";
    let original_aad = b"tenant:alpha";
    let tampered_aad = b"tenant:bravo";

    let mut blob = XChaChaCipher::encrypt(plaintext, &key, original_aad).unwrap();

    // Update fingerprint to match tampered AAD
    let tampered_hash = Sha256::digest(tampered_aad);
    blob.aad_fingerprint.copy_from_slice(&tampered_hash[..16]);

    let res = XChaChaCipher::decrypt(&blob, &key, tampered_aad);
    assert!(res.is_err(), "Poly1305 MAC must reject tampered AAD even when fingerprint is updated");
    match res.unwrap_err() {
        HakilleError::CryptoError(msg) => {
            assert!(msg.contains("XChaCha decryption failed"), "Expected AEAD decryption failure, got: {msg}");
        }
        other => panic!("Unexpected error: {:?}", other),
    }
}

#[test]
fn test_adversarial_xchacha_truncated_ciphertext_fails() {
    let key = [0x77u8; 32];
    let plaintext = b"Payload for Truncation Testing";
    let aad = b"aad";

    let blob = XChaChaCipher::encrypt(plaintext, &key, aad).unwrap();

    // Truncate ciphertext to 0 bytes (when plaintext was 30 bytes)
    let mut truncated_0 = blob.clone();
    truncated_0.ciphertext.clear();
    assert!(XChaChaCipher::decrypt(&truncated_0, &key, aad).is_err());

    // Truncate by 1 byte
    let mut truncated_1 = blob.clone();
    truncated_1.ciphertext.pop();
    assert!(XChaChaCipher::decrypt(&truncated_1, &key, aad).is_err());
}

#[test]
fn test_adversarial_xchacha_trailing_garbage_fails() {
    let key = [0x88u8; 32];
    let plaintext = b"Exact Payload";
    let aad = b"aad";

    let mut blob = XChaChaCipher::encrypt(plaintext, &key, aad).unwrap();
    blob.ciphertext.push(0x00); // 1 byte garbage appended
    assert!(XChaChaCipher::decrypt(&blob, &key, aad).is_err());

    blob.ciphertext.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
    assert!(XChaChaCipher::decrypt(&blob, &key, aad).is_err());
}

#[test]
fn test_adversarial_xchacha_corrupted_tag_all_bytes() {
    let key = [0x99u8; 32];
    let plaintext = b"Payload for Tag Corruption";
    let aad = b"aad";

    let blob = XChaChaCipher::encrypt(plaintext, &key, aad).unwrap();

    for tag_idx in 0..16 {
        let mut tampered = blob.clone();
        tampered.tag[tag_idx] ^= 0x80;
        assert!(XChaChaCipher::decrypt(&tampered, &key, aad).is_err());
    }
}

#[test]
fn test_adversarial_xchacha_corrupted_nonce_all_bytes() {
    let key = [0xaau8; 32];
    let plaintext = b"Payload for Nonce Corruption";
    let aad = b"aad";

    let blob = XChaChaCipher::encrypt(plaintext, &key, aad).unwrap();

    for nonce_idx in 0..24 {
        let mut tampered = blob.clone();
        tampered.nonce[nonce_idx] ^= 0x01;
        assert!(XChaChaCipher::decrypt(&tampered, &key, aad).is_err());
    }
}

#[test]
fn test_adversarial_xchacha_wrong_key_1_bit_difference() {
    let key = [0xbbu8; 32];
    let plaintext = b"Secret under key A";
    let aad = b"aad";

    let blob = XChaChaCipher::encrypt(plaintext, &key, aad).unwrap();

    let mut wrong_key = key;
    wrong_key[31] ^= 0x01; // 1 bit flip in key
    assert!(XChaChaCipher::decrypt(&blob, &wrong_key, aad).is_err());
}

// =============================================================================
// SUITE 3: HMAC Blind Indexing Empirical Challenge
// =============================================================================

#[test]
fn test_adversarial_blind_index_10k_collision_resistance() {
    let blind_key = [0x37u8; 32];
    let count = 10_000;
    let mut exact_set = HashSet::with_capacity(count);
    let mut prefix_set = HashSet::with_capacity(count);

    for i in 0..count {
        let term = format!("user_{:08x}_node", i);
        let exact = compute_blind_index(&term, &blind_key);
        assert!(exact_set.insert(exact), "Collision detected in exact blind index at iteration {i}!");

        let prefix_term = format!("{:04x}", i); // 10,000 unique 4-character prefixes
        let prefix = compute_prefix_blind_index(&prefix_term, 4, &blind_key);
        assert!(prefix_set.insert(prefix), "Collision detected in prefix blind index at iteration {i}!");
    }

    assert_eq!(exact_set.len(), count);
    assert_eq!(prefix_set.len(), count);
}

#[test]
fn test_adversarial_blind_index_normalization_stability() {
    let blind_key = [0x48u8; 32];

    let variants = [
        "  User.Alice@Hakille.org  ",
        "user.alice@hakille.org",
        "USER.ALICE@HAKILLE.ORG",
        "User.Alice@Hakille.org\n",
        "\tuser.alice@hakille.org\t",
    ];

    let base_exact = compute_blind_index(variants[0], &blind_key);
    let base_prefix = compute_prefix_blind_index(variants[0], 4, &blind_key);

    for &v in &variants[1..] {
        assert_eq!(compute_blind_index(v, &blind_key), base_exact, "Exact index unstable for variant: {v}");
        assert_eq!(compute_prefix_blind_index(v, 4, &blind_key), base_prefix, "Prefix index unstable for variant: {v}");
    }
}

#[test]
fn test_adversarial_blind_index_short_terms() {
    let blind_key = [0x59u8; 32];

    // Terms with length < 4
    let t1 = "a";
    let t2 = "ab";
    let t3 = "abc";

    let p1 = compute_prefix_blind_index(t1, 4, &blind_key);
    let p2 = compute_prefix_blind_index(t2, 4, &blind_key);
    let p3 = compute_prefix_blind_index(t3, 4, &blind_key);

    assert_ne!(p1, p2);
    assert_ne!(p2, p3);
    assert_ne!(p1, p3);
}

#[test]
fn test_adversarial_blind_index_unicode_case_folding() {
    let blind_key = [0x6au8; 32];
    // French accented characters
    let t_upper = "ÉLÉPHANT";
    let t_lower = "éléphant";
    assert_eq!(compute_blind_index(t_upper, &blind_key), compute_blind_index(t_lower, &blind_key));
}

#[test]
fn test_adversarial_blind_index_key_independence() {
    let k1 = [0x11u8; 32];
    let k2 = [0x22u8; 32];
    let term = "sovereign_identity";

    let t1 = compute_blind_index(term, &k1);
    let t2 = compute_blind_index(term, &k2);
    assert_ne!(t1, t2);

    let p1 = compute_prefix_blind_index(term, 4, &k1);
    let p2 = compute_prefix_blind_index(term, 4, &k2);
    assert_ne!(p1, p2);
}

// =============================================================================
// SUITE 4: Fuzzy Extractor Empirical Challenge
// =============================================================================

#[test]
fn test_adversarial_fuzzy_extractor_noise_tolerance_bounds() {
    let fe = FuzzyExtractor::new(16);
    let base_embedding = vec![0.5f32; 256];
    let (key, helper, commit) = fe.generate(&base_embedding).unwrap();

    // 0 noise
    let r0 = fe.reproduce(&base_embedding, &helper, &commit).unwrap();
    assert_eq!(r0, key);

    // 1 bit flip (Hamming distance 1)
    let mut probe_1 = base_embedding.clone();
    probe_1[0] = -0.5;
    assert_eq!(fe.reproduce(&probe_1, &helper, &commit).unwrap(), key);

    // 2 bit flips (Hamming distance 2)
    let mut probe_2 = base_embedding.clone();
    probe_2[0] = -0.5;
    probe_2[1] = -0.5;
    assert_eq!(fe.reproduce(&probe_2, &helper, &commit).unwrap(), key);

    // 3 bit flips (Hamming distance 3) - guaranteed worst-case bound
    let mut probe_3 = base_embedding.clone();
    probe_3[0] = -0.5;
    probe_3[1] = -0.5;
    probe_3[2] = -0.5;
    assert_eq!(fe.reproduce(&probe_3, &helper, &commit).unwrap(), key);

    // 16 distributed bit flips (1 error per block across 16 different seed bits)
    // Bit indices 0, 1, 2, ..., 15 in the quantized vector belong to different seed bits
    let mut probe_16_dist = base_embedding.clone();
    for i in 0..16 {
        probe_16_dist[i] = -0.5;
    }
    assert_eq!(fe.reproduce(&probe_16_dist, &helper, &commit).unwrap(), key, "Distributed noise within error-correction capacity must succeed");
}

#[test]
fn test_adversarial_fuzzy_extractor_failure_beyond_bound_concentrated() {
    // EMPIRICAL MATHEMATICAL FINDING (ASYMMETRIC TIE-BREAKING IN 8-FOLD REPETITION):
    // In an 8-fold repetition code with tie-breaking rule (ones >= 5 => 1, else 0):
    // - If true seed bit was 1: 4 bit flips produce 4 ones, decoding to 0 (failure).
    // - If true seed bit was 0: 4 bit flips produce 4 ones, decoding to 0 (false success!).
    // Therefore, 4 bit flips fail with 50% probability depending on the random seed bit.
    // To deterministically exceed the majority voting bound for BOTH 0 and 1,
    // exactly 5 bit flips are required (5 errors on 0 produces 5 ones => decodes to 1;
    // 5 errors on 1 produces 3 ones => decodes to 0).
    let fe = FuzzyExtractor::new(16);
    let base_embedding = vec![0.5f32; 256];
    let (_key, helper, commit) = fe.generate(&base_embedding).unwrap();

    let mut probe_concentrated = base_embedding.clone();
    // Flip 5 copies of seed bit 0 (at reps 0, 1, 2, 3, 4)
    probe_concentrated[0] = -0.5;   // bit 0 (rep 0)
    probe_concentrated[32] = -0.5;  // bit 32 (rep 1)
    probe_concentrated[64] = -0.5;  // bit 64 (rep 2)
    probe_concentrated[96] = -0.5;  // bit 96 (rep 3)
    probe_concentrated[128] = -0.5; // bit 128 (rep 4)

    let res = fe.reproduce(&probe_concentrated, &helper, &commit);
    assert!(res.is_err(), "5 concentrated errors on a single seed bit MUST cause decoding failure!");
    match res.unwrap_err() {
        HakilleError::AuthenticationFailed => {}
        other => panic!("Expected AuthenticationFailed, got: {:?}", other),
    }
}

#[test]
fn test_adversarial_fuzzy_extractor_failure_random_noise_large_delta() {
    let fe = FuzzyExtractor::new(16);
    let base_embedding = vec![0.7f32; 256];
    let (_key, helper, commit) = fe.generate(&base_embedding).unwrap();

    // Random noise flipping 128 of 256 bits (Hamming distance 128)
    let mut probe_noisy = base_embedding.clone();
    for i in (0..256).step_by(2) {
        probe_noisy[i] = -0.7;
    }
    assert!(fe.reproduce(&probe_noisy, &helper, &commit).is_err());

    // Complete inversion (Hamming distance 256)
    let probe_inverted = vec![-0.7f32; 256];
    assert!(fe.reproduce(&probe_inverted, &helper, &commit).is_err());
}

#[test]
fn test_adversarial_fuzzy_extractor_tampered_helper_data() {
    let fe = FuzzyExtractor::new(16);
    let sample = vec![0.3f32; 256];
    let (_key, helper, commit) = fe.generate(&sample).unwrap();

    // Tampered syndrome across 5 repetitions (exceeds error-correction capacity)
    let mut helper_corrupt_syn = helper.clone();
    for rep in 0..5 {
        helper_corrupt_syn.syndrome[rep * 4] ^= 0x01;
    }
    assert!(fe.reproduce(&sample, &helper_corrupt_syn, &commit).is_err());

    // Tampered salt
    let mut helper_corrupt_salt = helper.clone();
    helper_corrupt_salt.salt[0] ^= 0x01;
    assert!(fe.reproduce(&sample, &helper_corrupt_salt, &commit).is_err());

    // Truncated syndrome
    let mut helper_truncated = helper.clone();
    helper_truncated.syndrome.truncate(16);
    assert!(fe.reproduce(&sample, &helper_truncated, &commit).is_err());
}

#[test]
fn test_adversarial_fuzzy_extractor_delta_threshold_dead_code_behavior() {
    let fe = FuzzyExtractor::new(2); // threshold set to 2
    let base = vec![0.5f32; 256];
    let (_key, helper, commit) = fe.generate(&base).unwrap();

    let mut probe_12_errors = base.clone();
    for i in 0..12 {
        probe_12_errors[i] = -0.5; // flips 12 bits across 12 distinct seed positions
    }

    let dist = FuzzyExtractor::hamming_distance(
        &FuzzyExtractor::quantize(&base),
        &FuzzyExtractor::quantize(&probe_12_errors),
    );
    assert_eq!(dist, 12);
    assert!(dist > fe.delta_threshold, "Distance 12 is greater than configured delta_threshold 2");

    let res = fe.reproduce(&probe_12_errors, &helper, &commit);
    assert!(res.is_err(), "Reproduction must fail when Hamming distance exceeds delta_threshold");
}

// =============================================================================
// SUITE 5: ZK Storage & SSI Verifiable Credentials Empirical Challenge
// =============================================================================

#[test]
fn test_adversarial_vc_claims_hash_concatenation_collision() {
    let issuer_key = SigningKey::generate(&mut OsRng);
    let now = chrono::Utc::now().timestamp();
    let claims = serde_json::json!({ "role": "Member" });

    let vc1 = VerifiableCredential::issue(
        "urn:id:1",
        vec!["Admin".into(), "User".into()],
        &issuer_key,
        now,
        now + 1000,
        claims.clone(),
    );

    let vc2 = VerifiableCredential::issue(
        "urn:id:1",
        vec!["AdminU".into(), "ser".into()],
        &issuer_key,
        now,
        now + 1000,
        claims,
    );

    let h1 = vc1.compute_claims_hash();
    let h2 = vc2.compute_claims_hash();
    assert_ne!(h1, h2, "Field framing prevents concatenation collisions in vc_type");
}

#[test]
fn test_adversarial_storage_duplicate_blind_user_fails() {
    let mut store = RedbZkStore::new();
    let uid1 = UserId(Uuid::new_v4());
    let uid2 = UserId(Uuid::new_v4());
    let blind_key = [0x42u8; 32];
    let blind_hash = compute_blind_index("alice@hakille.org", &blind_key);

    let dummy_blob = XChaChaBlob {
        nonce: [0u8; 24],
        ciphertext: vec![],
        tag: [0u8; 16],
        aad_fingerprint: [0u8; 16],
    };

    let user1 = UserRecord {
        user_id: uid1,
        primary_did: Did::new("did:key:z1"),
        blind_username_hash: blind_hash,
        encrypted_profile_blob: dummy_blob.clone(),
        state: AccountStatus::Active,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let user2 = UserRecord {
        user_id: uid2,
        primary_did: Did::new("did:key:z2"),
        blind_username_hash: blind_hash,
        encrypted_profile_blob: dummy_blob,
        state: AccountStatus::Active,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    assert!(store.insert_user(user1).is_ok());
    let res = store.insert_user(user2);
    assert!(res.is_err(), "Duplicate blind user insertion must fail");
}

#[test]
fn test_adversarial_storage_digital_twilight_lifecycle() {
    let mut store = RedbZkStore::new();
    let uid = UserId(Uuid::new_v4());
    let blind_key = [0x42u8; 32];
    let blind_hash = compute_blind_index("bob@hakille.org", &blind_key);

    let dummy_blob = XChaChaBlob {
        nonce: [0u8; 24],
        ciphertext: vec![],
        tag: [0u8; 16],
        aad_fingerprint: [0u8; 16],
    };

    let user = UserRecord {
        user_id: uid.clone(),
        primary_did: Did::new("did:key:zBob"),
        blind_username_hash: blind_hash,
        encrypted_profile_blob: dummy_blob,
        state: AccountStatus::Active,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    store.insert_user(user).unwrap();
    assert!(!store.is_salt_shredded(&uid));

    // Trigger twilight
    store.trigger_digital_twilight(&uid).unwrap();
    assert!(store.is_salt_shredded(&uid));

    // Verify user state is DigitalTwilightGrace
    let rec = store.lookup_user_by_blind_username("bob@hakille.org", &blind_key).unwrap();
    assert_eq!(rec.state, AccountStatus::DigitalTwilightGrace);
}
