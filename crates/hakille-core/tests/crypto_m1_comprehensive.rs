use hakille_core::crypto::{
    AadBinding, Argon2Params, CryptoError, DidJwk, DidKey, FuzzyExtractor, KeyPair, MasterKey,
    MasterKeyKdf, SubkeyDomain, SubkeyHierarchy, VerifiableCredential, XChaChaCipher,
    compute_blind_index, compute_blind_prefix_index, decrypt_at_rest, derive_subkey,
    derive_subkeys_for_tenant, encrypt_at_rest, format_did_jwk, format_did_key, parse_did_jwk,
    parse_did_key, reconstruct_secret, split_secret,
};
use rand::rngs::OsRng;
use zeroize::Zeroize;

// =============================================================================
// F01: Argon2id KDF Tests
// =============================================================================

#[test]
fn test_f01_argon2id_derivation_and_zeroize() {
    let passphrase = b"hakille-sovereign-master-passphrase-2026";
    let salt = [0x5au8; 32];
    let params = Argon2Params::test_params();

    let key1 = MasterKeyKdf::derive_master_key_with_params(passphrase, &salt, &params)
        .expect("Argon2id derivation should succeed");
    let key2 = MasterKeyKdf::derive_master_key_with_params(passphrase, &salt, &params)
        .expect("Argon2id derivation should be deterministic");

    assert_eq!(key1.as_bytes(), key2.as_bytes());
    assert_ne!(key1.as_bytes(), &[0u8; 32]);

    // Different salt produces completely orthogonal key
    let salt_diff = [0x5bu8; 32];
    let key_diff = MasterKeyKdf::derive_master_key_with_params(passphrase, &salt_diff, &params)
        .expect("Derivation should succeed");
    assert_ne!(key1.as_bytes(), key_diff.as_bytes());

    // Zeroize test
    let mut key_to_zeroize = MasterKey::new(*key1.as_bytes());
    assert_eq!(key_to_zeroize.as_bytes(), key1.as_bytes());
    key_to_zeroize.zeroize();
    assert_eq!(key_to_zeroize.as_bytes(), &[0u8; 32]);
}

#[test]
fn test_f01_argon2id_default_production_params() {
    let params = Argon2Params::default();
    assert_eq!(params.memory_kib, 65536); // 64 MiB
    assert_eq!(params.passes, 3); // t = 3
    assert_eq!(params.parallelism, 4); // p = 4
    assert_eq!(params.output_len, 32); // 256 bits
}

#[test]
fn test_f01_argon2id_production_derivation_roundtrip() {
    let passphrase = b"hakille-production-master-key-benchmark-2026";
    let salt = [0x99u8; 32];
    // Derives key with genuine 64MB memory, 3 passes, 4 threads (Argon2Params::default())
    let master = MasterKeyKdf::derive_master_key(passphrase, &salt)
        .expect("Production Argon2id derivation must succeed");
    assert_ne!(master.as_bytes(), &[0u8; 32]);

    let hierarchy = MasterKeyKdf::derive_subkeys(&master);
    assert_ne!(hierarchy.identity_key(), &[0u8; 32]);
    assert_ne!(hierarchy.blind_index_key(), &[0u8; 32]);
}

// =============================================================================
// F02: HKDF-SHA256 Subkey Tree Tests
// =============================================================================

#[test]
fn test_f02_hkdf_domain_separation_and_tenant_isolation() {
    let master = MasterKey::new([0x3c; 32]);

    // Domain separation within same tenant
    let k_id = derive_subkey(&master, &SubkeyDomain::Identity, Some("tenant-corp")).unwrap();
    let k_cred = derive_subkey(&master, &SubkeyDomain::Credentials, Some("tenant-corp")).unwrap();
    let k_mem = derive_subkey(
        &master,
        &SubkeyDomain::MemoryPartition("user-vault".to_string()),
        Some("tenant-corp"),
    )
    .unwrap();
    let k_bio = derive_subkey(&master, &SubkeyDomain::Biometrics, Some("tenant-corp")).unwrap();
    let k_bidx = derive_subkey(&master, &SubkeyDomain::BlindIndex, Some("tenant-corp")).unwrap();

    let all_keys = [k_id, k_cred, k_mem, k_bio, k_bidx];
    for i in 0..all_keys.len() {
        for j in (i + 1)..all_keys.len() {
            assert_ne!(
                all_keys[i], all_keys[j],
                "Domain keys at {i} and {j} must be distinct"
            );
        }
    }

    // Tenant isolation: same domain, different tenants
    let k_id_tenant_a =
        derive_subkey(&master, &SubkeyDomain::Identity, Some("tenant-alpha")).unwrap();
    let k_id_tenant_b =
        derive_subkey(&master, &SubkeyDomain::Identity, Some("tenant-beta")).unwrap();
    assert_ne!(k_id_tenant_a, k_id_tenant_b);

    // Full hierarchy derivation
    let hierarchy: SubkeyHierarchy = derive_subkeys_for_tenant(&master, "tenant-alpha").unwrap();
    assert_eq!(hierarchy.identity_key(), &k_id_tenant_a);
}

// =============================================================================
// F03: XChaCha20-Poly1305 AEAD Tests
// =============================================================================

#[test]
fn test_f03_xchacha20_encryption_roundtrip_and_aad_binding() {
    let key = [0x66; 32];
    let plaintext = b"Hakille Sovereign Operating System Zero-Knowledge Confidential Memory";
    let binding = AadBinding::new("tenant-omega", "user-42", "memory_triplets", "rec-007", 1);
    let aad = binding.to_bytes();

    let encrypted = XChaChaCipher::encrypt(plaintext, &key, &aad).unwrap();
    assert_eq!(encrypted.nonce.len(), 24);
    assert_ne!(encrypted.ciphertext, plaintext);
    assert_ne!(encrypted.tag, [0u8; 16]);
    assert_ne!(encrypted.aad_fingerprint, [0u8; 32]);

    let decrypted = XChaChaCipher::decrypt(&encrypted, &key, &aad).unwrap();
    assert_eq!(decrypted, plaintext);

    // Splicing attack: tamper with tenant in AAD
    let forged_binding = AadBinding::new("tenant-EVIL", "user-42", "memory_triplets", "rec-007", 1);
    let forged_aad = forged_binding.to_bytes();
    let err = XChaChaCipher::decrypt(&encrypted, &key, &forged_aad).unwrap_err();
    assert_eq!(err, CryptoError::AadMismatch);

    // Splicing attack: tamper with table ID in AAD
    let forged_table = AadBinding::new("tenant-omega", "user-42", "credentials", "rec-007", 1);
    let forged_table_aad = forged_table.to_bytes();
    let err_table = XChaChaCipher::decrypt(&encrypted, &key, &forged_table_aad).unwrap_err();
    assert_eq!(err_table, CryptoError::AadMismatch);
}

// =============================================================================
// F04: HMAC-SHA256 Blind Indexing Tests
// =============================================================================

#[test]
fn test_f04_blind_indexing_exact_and_prefix() {
    let key = [0x88; 32];

    // Normalization invariance: spaces and cases
    let t1 = compute_blind_index("  Dr. Beverly Crusher  ", &key);
    let t2 = compute_blind_index("dr. beverly crusher", &key);
    let t3 = compute_blind_index("DR.   BEVERLY   CRUSHER", &key);
    assert_eq!(t1, t2);
    assert_eq!(t2, t3);

    // Prefix index safe handling of UTF-8 characters
    let p1 = compute_blind_prefix_index("crusher", 4, &key);
    let p2 = compute_blind_prefix_index("crushing blow", 4, &key);
    let p3 = compute_blind_prefix_index("crane", 4, &key);
    assert_eq!(p1, p2); // First 4 chars ("crus") match
    assert_ne!(p1, p3); // "cran" != "crus"

    // Multi-byte Unicode prefix safety
    let p_unicode = compute_blind_prefix_index("Équipe Sécurisée 🚀", 7, &key);
    assert_ne!(p_unicode, [0u8; 32]);
}

// =============================================================================
// F05: Shamir's Secret Sharing (k-of-n over GF(2^8)) Tests
// =============================================================================

#[test]
fn test_f05_shamir_secret_sharing_reconstruction_and_verification() {
    let secret = [
        0xde, 0xad, 0xbe, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba,
        0x98, 0x76, 0x54, 0x32, 0x10, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99,
        0xaa, 0xbb,
    ];

    // 3-of-5 threshold scheme
    let shares = split_secret(&secret, 3, 5).unwrap();
    assert_eq!(shares.len(), 5);

    // Any 3 shares must reconstruct the secret
    let subset_a = vec![shares[0].clone(), shares[1].clone(), shares[2].clone()];
    assert_eq!(reconstruct_secret(&subset_a).unwrap(), secret);

    let subset_b = vec![shares[1].clone(), shares[3].clone(), shares[4].clone()];
    assert_eq!(reconstruct_secret(&subset_b).unwrap(), secret);

    let subset_c = vec![shares[0].clone(), shares[2].clone(), shares[4].clone()];
    assert_eq!(reconstruct_secret(&subset_c).unwrap(), secret);

    // Reconstructing with 4 or 5 shares also succeeds
    let subset_all = shares.clone();
    assert_eq!(reconstruct_secret(&subset_all).unwrap(), secret);

    // Tampered share data fails checksum
    let mut corrupted_shares = subset_a.clone();
    corrupted_shares[0].data[5] ^= 0xff;
    let err = reconstruct_secret(&corrupted_shares).unwrap_err();
    assert!(matches!(err, CryptoError::CorruptedShare(_)));

    // Duplicate share index is rejected
    let duplicate_shares = vec![shares[0].clone(), shares[0].clone(), shares[1].clone()];
    let err_dup = reconstruct_secret(&duplicate_shares).unwrap_err();
    assert!(matches!(err_dup, CryptoError::DuplicateShareIndex(1)));
}

#[test]
fn test_f05_shamir_2_of_3_threshold() {
    let secret = [0x42; 32];
    let shares = split_secret(&secret, 2, 3).unwrap();
    assert_eq!(shares.len(), 3);

    assert_eq!(
        reconstruct_secret(&[shares[0].clone(), shares[1].clone()]).unwrap(),
        secret
    );
    assert_eq!(
        reconstruct_secret(&[shares[1].clone(), shares[2].clone()]).unwrap(),
        secret
    );
    assert_eq!(
        reconstruct_secret(&[shares[0].clone(), shares[2].clone()]).unwrap(),
        secret
    );
}

// =============================================================================
// F06: Fuzzy Extractor Biometric Commitments Tests
// =============================================================================

#[test]
fn test_f06_fuzzy_extractor_noise_resilience_and_rejection() {
    let extractor = FuzzyExtractor::default();
    let embedding: Vec<f32> = (0..512).map(|i| ((i as f32) * 0.01).sin()).collect();
    let salt = [0x77u8; 32];

    let (master_key, record) = extractor
        .generate_commitment(&embedding, &salt)
        .expect("Biometric commitment generation must succeed");

    // Exact reproduction
    let recovered_exact = extractor
        .reproduce_key(&embedding, &record)
        .expect("Exact reproduction must succeed");
    assert_eq!(master_key.as_bytes(), recovered_exact.as_bytes());

    // Reproduction with noise <= 20%
    let mut quantized_probe = FuzzyExtractor::quantize(&embedding);
    let total_bits = hakille_core::crypto::fuzzy::CODEWORD_BITS;
    let num_flips_18_percent = (total_bits as f32 * 0.18) as usize;

    for i in 0..num_flips_18_percent {
        let bit_idx = (i * 5) % total_bits;
        quantized_probe[bit_idx / 8] ^= 1 << (bit_idx % 8);
    }

    let recovered_noisy = extractor
        .reproduce_from_bits(&quantized_probe, &record)
        .expect("Reproduction under 18% noise must succeed");
    assert_eq!(master_key.as_bytes(), recovered_noisy.as_bytes());

    // Excessive noise (> 35%) must be rejected
    let mut noisy_probe_excess = FuzzyExtractor::quantize(&embedding);
    let num_flips_38_percent = (total_bits as f32 * 0.38) as usize;
    for i in 0..num_flips_38_percent {
        let bit_idx = (i * 2) % total_bits;
        noisy_probe_excess[bit_idx / 8] ^= 1 << (bit_idx % 8);
    }

    let result_excess = extractor.reproduce_from_bits(&noisy_probe_excess, &record);
    assert_eq!(
        result_excess.unwrap_err(),
        CryptoError::BiometricVerificationFailed
    );
}

// =============================================================================
// F07: SSI Primitives (did:key, did:jwk, W3C VCs) Tests
// =============================================================================

#[test]
fn test_f07_did_key_and_did_jwk_roundtrips() {
    let mut csprng = OsRng;
    let ed_key = ed25519_dalek::SigningKey::generate(&mut csprng);
    let vk = ed_key.verifying_key();
    let pub_bytes = vk.as_bytes();

    // did:key:z6Mk...
    let did_key = format_did_key(pub_bytes);
    assert!(did_key.starts_with("did:key:z6Mk"));
    let parsed_key = parse_did_key(&did_key).unwrap();
    assert_eq!(&parsed_key, pub_bytes);

    // did:jwk:...
    let did_jwk = format_did_jwk(pub_bytes);
    assert!(did_jwk.starts_with("did:jwk:"));
    let parsed_jwk = parse_did_jwk(&did_jwk).unwrap();
    assert_eq!(&parsed_jwk, pub_bytes);

    // Helper wrapper structs
    assert_eq!(DidKey::to_bytes(&did_key).unwrap(), *pub_bytes);
    assert_eq!(DidJwk::to_bytes(&did_jwk).unwrap(), *pub_bytes);
}

#[test]
fn test_f07_w3c_verifiable_credential_lifecycle() {
    let mut csprng = OsRng;
    let issuer_signing_key = ed25519_dalek::SigningKey::generate(&mut csprng);

    let mut vc = VerifiableCredential {
        context: vec![
            "https://www.w3.org/ns/credentials/v2".to_string(),
            "https://schema.hakille.org/identity/v1".to_string(),
        ],
        id: "urn:uuid:e0a58572-1c2f-48d6-a2bc-330ec5fa1011".to_string(),
        credential_type: vec![
            "VerifiableCredential".to_string(),
            "EnterpriseAgentAttestation".to_string(),
        ],
        issuer: String::new(), // Populated during issuance
        valid_from: "2026-01-01T00:00:00Z".to_string(),
        valid_until: Some("2028-01-01T00:00:00Z".to_string()),
        credential_subject: serde_json::json!({
            "agent_id": "agent:supervisor-01",
            "tier": "enterprise",
            "delegation_depth": 2,
            "allowed_scopes": ["read:triplets", "write:triplets"]
        }),
        proof: None,
    };

    // Issue credential
    vc.issue(&issuer_signing_key)
        .expect("VC issuance must succeed");
    assert!(vc.issuer.starts_with("did:key:z6Mk"));
    assert!(vc.proof.is_some());

    // Verify valid credential
    assert!(vc.verify().expect("Valid VC verification should succeed"));

    // Verify tampered credential fails
    let mut tampered_vc = vc.clone();
    tampered_vc.credential_subject = serde_json::json!({
        "agent_id": "agent:supervisor-01",
        "tier": "enterprise",
        "delegation_depth": 99, // Unauthorized privilege escalation!
        "allowed_scopes": ["fullsystem"]
    });
    assert!(tampered_vc.verify().is_err());
}

// =============================================================================
// Backward Compatibility Tests
// =============================================================================

#[test]
fn test_backward_compatibility_legacy_primitives() {
    let kp = KeyPair::generate();
    let data = b"Legacy attestation signature verification";
    let sig = kp.sign(data);
    assert!(kp.verify(data, &sig));

    let key = [0x12u8; 32];
    let plain = b"Legacy encrypted memory payload";
    let blob = encrypt_at_rest(plain, &key).expect("Legacy encryption failed");
    let decrypted = decrypt_at_rest(&blob, &key).expect("Legacy decryption failed");
    assert_eq!(decrypted, plain);
}
