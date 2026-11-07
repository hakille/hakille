use hakille_core::crypto::{
    AadBinding, Argon2Params, CryptoError, MasterKey, MasterKeyKdf, SubkeyDomain,
    XChaChaCipher, derive_subkey,
};
use sha2::{Digest, Sha256};

// =============================================================================
// Helper Functions for Cryptographic Metric Evaluation
// =============================================================================

fn hamming_distance(a: &[u8], b: &[u8]) -> u32 {
    assert_eq!(a.len(), b.len(), "Slices must have equal length for Hamming distance");
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x ^ y).count_ones())
        .sum()
}

/// Bit-level Pearson correlation over 256 individual bits (N = 256 samples)
fn bit_correlation(a: &[u8; 32], b: &[u8; 32]) -> f64 {
    let mut bits_a = [0.0f64; 256];
    let mut bits_b = [0.0f64; 256];
    for byte_idx in 0..32 {
        for bit_idx in 0..8 {
            let idx = byte_idx * 8 + bit_idx;
            bits_a[idx] = ((a[byte_idx] >> bit_idx) & 1) as f64;
            bits_b[idx] = ((b[byte_idx] >> bit_idx) & 1) as f64;
        }
    }

    let mean_a: f64 = bits_a.iter().sum::<f64>() / 256.0;
    let mean_b: f64 = bits_b.iter().sum::<f64>() / 256.0;

    let mut cov = 0.0;
    let mut var_a = 0.0;
    let mut var_b = 0.0;

    for i in 0..256 {
        let da = bits_a[i] - mean_a;
        let db = bits_b[i] - mean_b;
        cov += da * db;
        var_a += da * da;
        var_b += db * db;
    }

    if var_a == 0.0 || var_b == 0.0 {
        return 0.0;
    }
    cov / (var_a.sqrt() * var_b.sqrt())
}

// =============================================================================
// CATEGORY 1: F01 Argon2id Boundary, Stress & Edge-Case Probes
// =============================================================================

#[test]
fn test_adversarial_argon2id_empty_passphrase() {
    let salt = [0x42u8; 32];
    let params = Argon2Params::test_params();

    // RFC 9106 allows 0-byte password input
    let key_empty1 = MasterKeyKdf::derive_master_key_with_params(b"", &salt, &params)
        .expect("Argon2id derivation must succeed for empty passphrase");
    let key_empty2 = MasterKeyKdf::derive_master_key_with_params(b"", &salt, &params)
        .expect("Argon2id must be deterministic for empty passphrase");

    assert_eq!(key_empty1.as_bytes(), key_empty2.as_bytes());
    assert_ne!(key_empty1.as_bytes(), &[0u8; 32], "Empty passphrase must not output zero key");

    // Must be strictly distinct from non-empty password
    let key_nonempty = MasterKeyKdf::derive_master_key_with_params(b" ", &salt, &params)
        .expect("Derivation for whitespace password should succeed");
    assert_ne!(key_empty1.as_bytes(), key_nonempty.as_bytes());
}

#[test]
fn test_adversarial_argon2id_boundary_zero_parameters() {
    let passphrase = b"boundary-params-passphrase";
    let salt = [0x11u8; 32];

    // memory_kib = 0
    let mut params = Argon2Params::test_params();
    params.memory_kib = 0;
    let res = MasterKeyKdf::derive_master_key_with_params(passphrase, &salt, &params);
    assert!(matches!(res, Err(CryptoError::KdfError(_))));

    // passes = 0
    let mut params = Argon2Params::test_params();
    params.passes = 0;
    let res = MasterKeyKdf::derive_master_key_with_params(passphrase, &salt, &params);
    assert!(matches!(res, Err(CryptoError::KdfError(_))));

    // parallelism = 0
    let mut params = Argon2Params::test_params();
    params.parallelism = 0;
    let res = MasterKeyKdf::derive_master_key_with_params(passphrase, &salt, &params);
    assert!(matches!(res, Err(CryptoError::KdfError(_))));
}

#[test]
fn test_adversarial_argon2id_invalid_memory_for_parallelism() {
    let passphrase = b"invalid-memory-ratio";
    let salt = [0x22u8; 32];

    // Argon2 requires memory_kib >= 8 * parallelism.
    // If memory_kib = 8 and parallelism = 4, memory is too low.
    let mut params = Argon2Params::test_params();
    params.memory_kib = 8;
    params.parallelism = 4;
    let res = MasterKeyKdf::derive_master_key_with_params(passphrase, &salt, &params);
    // Must fail closed with KdfError, not panic
    assert!(matches!(res, Err(CryptoError::KdfError(_))));
}

#[test]
fn test_adversarial_argon2id_output_len_mismatch() {
    let passphrase = b"mismatched-output-len";
    let salt = [0x33u8; 32];

    // Params with output_len != 32 (e.g. 64 or 16)
    let mut params = Argon2Params::test_params();
    params.output_len = 64;
    let res = MasterKeyKdf::derive_master_key_with_params(passphrase, &salt, &params);
    assert!(matches!(res, Err(CryptoError::KdfError(_))));

    params.output_len = 16;
    let res = MasterKeyKdf::derive_master_key_with_params(passphrase, &salt, &params);
    assert!(matches!(res, Err(CryptoError::KdfError(_))));
}

#[test]
fn test_adversarial_argon2id_extreme_salts_and_diffusion() {
    let passphrase = b"extreme-salts-verification";
    let params = Argon2Params::test_params();

    // All zero salt
    let salt_zero = [0x00u8; 32];
    let key_zero = MasterKeyKdf::derive_master_key_with_params(passphrase, &salt_zero, &params)
        .expect("Zero salt should succeed");
    assert_ne!(key_zero.as_bytes(), &[0u8; 32]);

    // All 0xFF salt
    let salt_ff = [0xffu8; 32];
    let key_ff = MasterKeyKdf::derive_master_key_with_params(passphrase, &salt_ff, &params)
        .expect("0xFF salt should succeed");
    assert_ne!(key_ff.as_bytes(), &[0u8; 32]);
    assert_ne!(key_zero.as_bytes(), key_ff.as_bytes());

    // Single-bit salt flip avalanche
    let mut salt_bitflip = salt_zero;
    salt_bitflip[0] ^= 0x01; // flip 1 bit
    let key_bitflip = MasterKeyKdf::derive_master_key_with_params(passphrase, &salt_bitflip, &params)
        .expect("Bitflip salt should succeed");

    let dist = hamming_distance(key_zero.as_bytes(), key_bitflip.as_bytes());
    // In a 256-bit output, optimal avalanche is 128 bits. We assert 90 <= dist <= 165
    assert!(
        (90..=165).contains(&dist),
        "Salt avalanche effect failed: Hamming distance was {}/256",
        dist
    );
}

#[test]
fn test_adversarial_argon2id_large_passphrase() {
    let large_passphrase = vec![0x61u8; 65536]; // 64 KiB password
    let salt = [0x77u8; 32];
    let params = Argon2Params::test_params();

    let key = MasterKeyKdf::derive_master_key_with_params(&large_passphrase, &salt, &params)
        .expect("Derivation with 64 KiB password must succeed without stack overflow");
    assert_ne!(key.as_bytes(), &[0u8; 32]);
}

// =============================================================================
// CATEGORY 2: F02 HKDF Subkey Independence & Bit Leakage Probes
// =============================================================================

#[test]
fn test_adversarial_hkdf_cross_domain_bitwise_independence() {
    let master = MasterKey::new([0xa5; 32]);

    let domains = [
        SubkeyDomain::Identity,
        SubkeyDomain::Credentials,
        SubkeyDomain::Biometrics,
        SubkeyDomain::BlindIndex,
        SubkeyDomain::MemoryPartition("scratchpad".to_string()),
        SubkeyDomain::MemoryPartition("vault_primary".to_string()),
        SubkeyDomain::MemoryPartition("vault_secondary".to_string()),
    ];

    let mut derived_keys = Vec::new();
    for domain in &domains {
        let k = derive_subkey(&master, domain, Some("tenant-corp-enterprise-01"))
            .expect("Subkey derivation should succeed");
        derived_keys.push((domain.as_str(), k));
    }

    // Pairwise statistical check: ensure no bit leakage, overlap, or linear correlation
    for i in 0..derived_keys.len() {
        for j in (i + 1)..derived_keys.len() {
            let (name_a, key_a) = &derived_keys[i];
            let (name_b, key_b) = &derived_keys[j];

            let dist = hamming_distance(key_a, key_b);
            // 256 bits total: expect between 90 and 166 bits flipped (within normal bounds)
            assert!(
                (90..=166).contains(&dist),
                "Subkey domains {} and {} have poor separation: Hamming distance {}/256",
                name_a,
                name_b,
                dist
            );

            // Bit-level Pearson correlation over N = 256 bits
            let corr = bit_correlation(key_a, key_b);
            assert!(
                corr.abs() < 0.25,
                "Subkey domains {} and {} show bit correlation: {}",
                name_a,
                name_b,
                corr
            );
        }
    }
}

#[test]
fn test_adversarial_hkdf_avalanche_diffusion_on_master_key_bitflip() {
    let master1 = MasterKey::new([0x3c; 32]);
    let mut m2_bytes = [0x3c; 32];
    m2_bytes[15] ^= 0x80; // Flip 1 bit in the middle of master key
    let master2 = MasterKey::new(m2_bytes);

    let domains = [
        SubkeyDomain::Identity,
        SubkeyDomain::Credentials,
        SubkeyDomain::Biometrics,
        SubkeyDomain::BlindIndex,
        SubkeyDomain::MemoryPartition("default".to_string()),
    ];

    for domain in &domains {
        let k1 = derive_subkey(&master1, domain, Some("tenant-fixed")).unwrap();
        let k2 = derive_subkey(&master2, domain, Some("tenant-fixed")).unwrap();

        let dist = hamming_distance(&k1, &k2);
        assert!(
            (90..=166).contains(&dist),
            "Domain {} failed avalanche on master key bitflip: Hamming distance {}/256",
            domain.as_str(),
            dist
        );
    }
}

#[test]
fn test_adversarial_hkdf_tenant_isolation_boundary() {
    let master = MasterKey::new([0x5a; 32]);

    // Subkeys for distinct tenants must never collide or correlate
    let k_org1 = derive_subkey(&master, &SubkeyDomain::Identity, Some("org_alpha")).unwrap();
    let k_org2 = derive_subkey(&master, &SubkeyDomain::Identity, Some("org_beta")).unwrap();
    let k_org1_sub = derive_subkey(&master, &SubkeyDomain::Identity, Some("org_alpha_sub")).unwrap();

    assert_ne!(k_org1, k_org2);
    assert_ne!(k_org1, k_org1_sub);

    let dist = hamming_distance(&k_org1, &k_org2);
    assert!((90..=166).contains(&dist));
}

#[test]
fn test_adversarial_hkdf_delimiter_collision_boundary_analysis() {
    let master = MasterKey::new([0xee; 32]);

    // Probe: What happens if partition name and tenant_id contain ':'?
    // MemoryPartition("a:b") + tenant "c" vs MemoryPartition("a") + tenant "b:c"
    let k1 = derive_subkey(&master, &SubkeyDomain::MemoryPartition("a:b".to_string()), Some("c")).unwrap();
    let k2 = derive_subkey(&master, &SubkeyDomain::MemoryPartition("a".to_string()), Some("b:c")).unwrap();

    // Notice that without delimiter sanitization, info string is identical ("hakille:v1:memory-partition:a:b:c").
    // We document this boundary condition explicitly in our Challenger findings:
    assert_eq!(
        k1, k2,
        "Confirmed boundary vulnerability: unescaped colon delimiter in partition/tenant ID causes collision"
    );
}

// =============================================================================
// CATEGORY 3: F03 XChaCha20-Poly1305 Adversarial Cipher & Splicing Probes
// =============================================================================

#[test]
fn test_adversarial_xchacha20_empty_plaintext() {
    let key = [0x88; 32];
    let binding = AadBinding::new("tenant-zero", "user-0", "table-0", "rec-0", 1);
    let aad = binding.to_bytes();

    // Encrypt empty message
    let blob = XChaChaCipher::encrypt(b"", &key, &aad)
        .expect("Encrypting empty plaintext must succeed");

    assert_eq!(blob.ciphertext.len(), 0, "Ciphertext for empty plaintext should be 0 bytes");
    assert_eq!(blob.tag.len(), 16, "Tag must still be 16 bytes");

    let decrypted = XChaChaCipher::decrypt(&blob, &key, &aad)
        .expect("Decrypting empty plaintext must succeed");
    assert_eq!(decrypted, Vec::<u8>::new());
}

#[test]
fn test_adversarial_xchacha20_ciphertext_exhaustive_single_bit_flip() {
    let key = [0x55; 32];
    let plaintext = b"Hakille Sovereign OS Confidential Memory Triplet 2026";
    let binding = AadBinding::new("tenant-a", "user-1", "tbl_triplets", "rec-42", 1);
    let aad = binding.to_bytes();

    let original_blob = XChaChaCipher::encrypt(plaintext, &key, &aad).unwrap();

    // Flip EVERY bit of the ciphertext payload (54 bytes * 8 bits = 432 bit flips)
    let total_bytes = original_blob.ciphertext.len();
    for byte_idx in 0..total_bytes {
        for bit_idx in 0..8 {
            let mut tampered_blob = original_blob.clone();
            tampered_blob.ciphertext[byte_idx] ^= 1 << bit_idx;

            let result = XChaChaCipher::decrypt(&tampered_blob, &key, &aad);
            assert!(
                matches!(result, Err(CryptoError::DecryptionFailed(_))),
                "Bitflip at byte {} bit {} failed to trigger DecryptionFailed! Result: {:?}",
                byte_idx,
                bit_idx,
                result
            );
        }
    }
}

#[test]
fn test_adversarial_xchacha20_tag_exhaustive_single_bit_flip() {
    let key = [0x66; 32];
    let plaintext = b"Poly1305 MAC Integrity Stress Test";
    let binding = AadBinding::new("tenant-b", "user-2", "tbl_auth", "rec-88", 1);
    let aad = binding.to_bytes();

    let original_blob = XChaChaCipher::encrypt(plaintext, &key, &aad).unwrap();

    // Flip EVERY bit of the 16-byte authentication tag (16 * 8 = 128 bit flips)
    for byte_idx in 0..16 {
        for bit_idx in 0..8 {
            let mut tampered_blob = original_blob.clone();
            tampered_blob.tag[byte_idx] ^= 1 << bit_idx;

            let result = XChaChaCipher::decrypt(&tampered_blob, &key, &aad);
            assert!(
                matches!(result, Err(CryptoError::DecryptionFailed(_))),
                "Tag bitflip at byte {} bit {} failed to trigger DecryptionFailed!",
                byte_idx,
                bit_idx
            );
        }
    }
}

#[test]
fn test_adversarial_xchacha20_nonce_tampering() {
    let key = [0x77; 32];
    let plaintext = b"Nonce Integrity Check";
    let aad = b"aad:context";

    let original_blob = XChaChaCipher::encrypt(plaintext, &key, aad).unwrap();

    for byte_idx in 0..24 {
        let mut tampered_blob = original_blob.clone();
        tampered_blob.nonce[byte_idx] ^= 0x01;

        let result = XChaChaCipher::decrypt(&tampered_blob, &key, aad);
        assert!(
            matches!(result, Err(CryptoError::DecryptionFailed(_))),
            "Nonce tampering at byte {} did not fail closed!",
            byte_idx
        );
    }
}

#[test]
fn test_adversarial_xchacha20_payload_truncation_and_extension() {
    let key = [0x99; 32];
    let plaintext = b"Payload length boundary tampering testing vector Hakille OS";
    let aad = b"aad:boundary";

    let original_blob = XChaChaCipher::encrypt(plaintext, &key, aad).unwrap();

    // Truncate ciphertext by 1 byte
    let mut truncated_1 = original_blob.clone();
    truncated_1.ciphertext.pop();
    assert!(matches!(
        XChaChaCipher::decrypt(&truncated_1, &key, aad),
        Err(CryptoError::DecryptionFailed(_))
    ));

    // Truncate ciphertext by half
    let mut truncated_half = original_blob.clone();
    truncated_half.ciphertext.truncate(plaintext.len() / 2);
    assert!(matches!(
        XChaChaCipher::decrypt(&truncated_half, &key, aad),
        Err(CryptoError::DecryptionFailed(_))
    ));

    // Truncate to 0 bytes
    let mut truncated_zero = original_blob.clone();
    truncated_zero.ciphertext.clear();
    assert!(matches!(
        XChaChaCipher::decrypt(&truncated_zero, &key, aad),
        Err(CryptoError::DecryptionFailed(_))
    ));

    // Extend ciphertext by appending bytes
    let mut extended = original_blob.clone();
    extended.ciphertext.extend_from_slice(b"extra garbage");
    assert!(matches!(
        XChaChaCipher::decrypt(&extended, &key, aad),
        Err(CryptoError::DecryptionFailed(_))
    ));

    // Prepend garbage to ciphertext
    let mut prepended = original_blob.clone();
    prepended.ciphertext.insert(0, 0xff);
    assert!(matches!(
        XChaChaCipher::decrypt(&prepended, &key, aad),
        Err(CryptoError::DecryptionFailed(_))
    ));
}

#[test]
fn test_adversarial_xchacha20_wrong_and_bitflipped_key() {
    let key = [0x42; 32];
    let plaintext = b"Sensitive key sensitivity payload";
    let aad = b"aad:key-test";

    let blob = XChaChaCipher::encrypt(plaintext, &key, aad).unwrap();

    // Completely distinct key
    let wrong_key = [0x99; 32];
    assert!(matches!(
        XChaChaCipher::decrypt(&blob, &wrong_key, aad),
        Err(CryptoError::DecryptionFailed(_))
    ));

    // Single bit flipped in key
    for byte_idx in 0..32 {
        let mut flipped_key = key;
        flipped_key[byte_idx] ^= 0x01;
        assert!(matches!(
            XChaChaCipher::decrypt(&blob, &flipped_key, aad),
            Err(CryptoError::DecryptionFailed(_))
        ));
    }
}

#[test]
fn test_adversarial_xchacha20_cross_tenant_aad_splicing_comprehensive() {
    let key = [0x33; 32];
    let plaintext = b"Ultra-secure enterprise executive board resolution";

    let valid_binding = AadBinding::new("enterprise-conglomerate", "user-ceo", "board_resolutions", "res-2026-001", 1);
    let valid_aad = valid_binding.to_bytes();

    let blob = XChaChaCipher::encrypt(plaintext, &key, &valid_aad).unwrap();

    // Attack Vector 1: Cross-tenant splicing
    let spliced_tenant = AadBinding::new("enterprise-subsidiary", "user-ceo", "board_resolutions", "res-2026-001", 1).to_bytes();
    assert_eq!(
        XChaChaCipher::decrypt(&blob, &key, &spliced_tenant).unwrap_err(),
        CryptoError::AadMismatch
    );

    // Attack Vector 2: Cross-user splicing
    let spliced_user = AadBinding::new("enterprise-conglomerate", "user-intern", "board_resolutions", "res-2026-001", 1).to_bytes();
    assert_eq!(
        XChaChaCipher::decrypt(&blob, &key, &spliced_user).unwrap_err(),
        CryptoError::AadMismatch
    );

    // Attack Vector 3: Cross-table splicing
    let spliced_table = AadBinding::new("enterprise-conglomerate", "user-ceo", "public_bulletin", "res-2026-001", 1).to_bytes();
    assert_eq!(
        XChaChaCipher::decrypt(&blob, &key, &spliced_table).unwrap_err(),
        CryptoError::AadMismatch
    );

    // Attack Vector 4: Cross-record UUID splicing
    let spliced_record = AadBinding::new("enterprise-conglomerate", "user-ceo", "board_resolutions", "res-2026-999", 1).to_bytes();
    assert_eq!(
        XChaChaCipher::decrypt(&blob, &key, &spliced_record).unwrap_err(),
        CryptoError::AadMismatch
    );

    // Attack Vector 5: Schema version rollback / upgrade splicing
    let spliced_schema = AadBinding::new("enterprise-conglomerate", "user-ceo", "board_resolutions", "res-2026-001", 2).to_bytes();
    assert_eq!(
        XChaChaCipher::decrypt(&blob, &key, &spliced_schema).unwrap_err(),
        CryptoError::AadMismatch
    );
}

#[test]
fn test_adversarial_xchacha20_forged_aad_fingerprint_bypass() {
    let key = [0x77; 32];
    let plaintext = b"Attempt to bypass AAD fingerprint fast-fail check";

    let legitimate_aad = AadBinding::new("tenant-genuine", "user-alice", "vault", "item-1", 1).to_bytes();
    let adversary_aad = AadBinding::new("tenant-evil", "user-bob", "vault", "item-1", 1).to_bytes();

    let mut blob = XChaChaCipher::encrypt(plaintext, &key, &legitimate_aad).unwrap();

    // Adversary forges the aad_fingerprint to match adversary_aad:
    let evil_fingerprint = Sha256::digest(&adversary_aad);
    blob.aad_fingerprint.copy_from_slice(&evil_fingerprint);

    // Now aad_hash == blob.aad_fingerprint, so fast-fail check is passed.
    // The Poly1305 MAC verification MUST detect the forgery and fail closed with DecryptionFailed.
    let decrypt_result = XChaChaCipher::decrypt(&blob, &key, &adversary_aad);
    assert!(
        matches!(decrypt_result, Err(CryptoError::DecryptionFailed(_))),
        "Poly1305 MAC failed to catch forged AAD fingerprint bypass! Result: {:?}",
        decrypt_result
    );
}

#[test]
fn test_adversarial_xchacha20_aad_delimiter_injection_boundary() {
    // Probe: Can an adversary craft fields with \x1f to produce identical AAD bytes for different field assignments?
    let b1 = AadBinding::new("t1", "u1\x1ftable1", "rec1", "foo", 1);
    let b2 = AadBinding::new("t1", "u1", "table1", "rec1\x1ffoo", 1);

    // Confirm that if input fields contain raw 0x1f delimiters, they serialize to identical AAD byte vectors
    let aad1 = b1.to_bytes();
    let aad2 = b2.to_bytes();

    assert_eq!(
        aad1, aad2,
        "Boundary condition confirmed: 0x1f injection in unvalidated fields allows cross-field AAD byte collision"
    );
}
