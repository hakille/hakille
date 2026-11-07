//! Challenger 2: Adversarial Stress-Test Suite for Milestone 1 (F04–F07)
//! Probing edge cases, boundary conditions, malformed inputs, replay/splicing, and noise limits.

use hakille_core::crypto::{
    BlindIndexStore, CryptoError, DidJwk, DidKey, FuzzyExtractor,
    VerifiableCredential, compute_blind_index, compute_blind_prefix_index,
    format_did_jwk, format_did_key, normalize_term, parse_did_jwk, parse_did_key,
    reconstruct_secret, split_secret, SecretShare,
};
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;

// =============================================================================
// F04: Blind Indexing Adversarial Tests
// =============================================================================

#[test]
fn test_f04_blind_indexing_unicode_multibyte() {
    let key = [0x42u8; 32];

    // Normalization edge cases
    assert_eq!(normalize_term("   Hello   World   "), "hello world");
    assert_eq!(normalize_term("ÉQUIPE"), "équipe");
    assert_eq!(normalize_term(""), "");

    // CJK characters
    let cjk1 = "北京欢迎你";
    let cjk2 = "  北京欢迎你  ";
    let token_cjk1 = compute_blind_index(cjk1, &key);
    let token_cjk2 = compute_blind_index(cjk2, &key);
    assert_eq!(token_cjk1, token_cjk2, "CJK whitespace normalization failed");
    assert_ne!(token_cjk1, [0u8; 32]);

    // Arabic characters (right-to-left script)
    let arabic = "مرحبا بك في نظام هاكيلي";
    let token_ar = compute_blind_index(arabic, &key);
    assert_ne!(token_ar, [0u8; 32]);

    // Cyrillic characters with mixed casing
    let cyr_upper = "СОВЕРШЕННО СЕКРЕТНО";
    let cyr_lower = "совершенно секретно";
    let token_cyr_upper = compute_blind_index(cyr_upper, &key);
    let token_cyr_lower = compute_blind_index(cyr_lower, &key);
    assert_eq!(token_cyr_upper, token_cyr_lower, "Cyrillic case normalization failed");

    // Latin with diverse diacritics
    let latin_accents = "Crème brûlée à l'Élysée & São Paulo";
    let token_accents = compute_blind_index(latin_accents, &key);
    assert_ne!(token_accents, [0u8; 32]);
}

#[test]
fn test_f04_blind_indexing_emojis_and_zwj_sequences() {
    let key = [0x55u8; 32];

    // Compound emoji with Zero-Width Joiner (ZWJ): family 👨‍👩‍👧‍👦
    let family_emoji = "👨‍👩‍👧‍👦";
    let token_family = compute_blind_index(family_emoji, &key);
    assert_ne!(token_family, [0u8; 32]);

    // Rainbow flag sequence (🏳️ + ZWJ + 🌈)
    let flag_emoji = "🏳️‍🌈";
    let token_flag = compute_blind_index(flag_emoji, &key);
    assert_ne!(token_flag, [0u8; 32]);
    assert_ne!(token_family, token_flag);

    // Prefix indexing must not panic on character boundaries of multi-byte glyphs
    let prefix_token_1 = compute_blind_prefix_index(family_emoji, 1, &key);
    let prefix_token_2 = compute_blind_prefix_index(family_emoji, 3, &key);
    assert_ne!(prefix_token_1, [0u8; 32]);
    assert_ne!(prefix_token_2, [0u8; 32]);

    // String with mixed emojis and text
    let mixed = "Agent 🕵️‍♂️ classified in Paris 🥐";
    let mixed_token = compute_blind_index(mixed, &key);
    assert_ne!(mixed_token, [0u8; 32]);
}

#[test]
fn test_f04_blind_indexing_empty_and_whitespace_only() {
    let key = [0x77u8; 32];

    let empty = "";
    let spaces = "     ";
    let tabs_and_newlines = "\t\t\r\n   \n\t ";

    let token_empty = compute_blind_index(empty, &key);
    let token_spaces = compute_blind_index(spaces, &key);
    let token_tabs = compute_blind_index(tabs_and_newlines, &key);

    assert_eq!(token_empty, token_spaces);
    assert_eq!(token_spaces, token_tabs);
    assert_ne!(token_empty, [0u8; 32], "Empty string should have valid HMAC token");

    // Prefix on empty string
    let p0 = compute_blind_prefix_index(empty, 0, &key);
    let p5 = compute_blind_prefix_index(empty, 5, &key);
    assert_ne!(p0, [0u8; 32]);
    assert_ne!(p5, [0u8; 32]);
}

#[test]
fn test_f04_blind_indexing_prefix_larger_than_input() {
    let key = [0x88u8; 32];
    let term = "Hakille"; // 7 characters

    // Requesting prefix lengths greater than 7 must not panic
    let p7 = compute_blind_prefix_index(term, 7, &key);
    let p10 = compute_blind_prefix_index(term, 10, &key);
    let p100 = compute_blind_prefix_index(term, 100, &key);
    let p1000 = compute_blind_prefix_index(term, 1000, &key);

    assert_ne!(p7, [0u8; 32]);
    assert_ne!(p10, [0u8; 32]);
    assert_ne!(p100, [0u8; 32]);
    assert_ne!(p1000, [0u8; 32]);

    // Since prefix length is bound into the HMAC, different requested prefix lengths
    // are domain-separated even if the underlying string has fewer characters.
    assert_ne!(p10, p100);
}

#[test]
fn test_f04_blind_index_store_adversarial_queries() {
    let mut store = BlindIndexStore::new();
    let key_a = [0x11u8; 32];
    let key_b = [0x22u8; 32];

    store.insert("Jean-Luc Picard", "rec-001", &key_a);
    store.insert("🚀 Mission Mars", "rec-002", &key_a);
    store.insert("", "rec-003", &key_a);

    // Exact matches
    assert_eq!(store.search_exact("jean-luc picard", &key_a), vec!["rec-001"]);
    assert_eq!(store.search_exact("🚀 mission mars", &key_a), vec!["rec-002"]);
    assert_eq!(store.search_exact("   ", &key_a), vec!["rec-003"]);

    // Key isolation: searching with key_b returns empty
    assert!(store.search_exact("jean-luc picard", &key_b).is_empty());
    assert!(store.search_prefix("jean", 4, &key_b).is_empty());

    // Searching nonexistent term
    assert!(store.search_exact("nonexistent-person", &key_a).is_empty());
}

// =============================================================================
// F05: Shamir's Secret Sharing Adversarial Tests
// =============================================================================

#[test]
fn test_f05_shamir_k_minus_1_shares_cannot_reconstruct() {
    let secret = [0x9au8; 32];
    // (3, 5) threshold: threshold k = 3, total n = 5
    let shares = split_secret(&secret, 3, 5).unwrap();

    // Passing only 2 shares (k - 1)
    let two_shares = vec![shares[0].clone(), shares[1].clone()];
    let candidate = reconstruct_secret(&two_shares).unwrap();

    // A degree-2 polynomial cannot be reconstructed from 2 points.
    // The candidate MUST NOT match the original secret.
    assert_ne!(candidate, secret, "k-1 shares must never yield original secret");

    // Try another 2-share subset
    let two_shares_other = vec![shares[2].clone(), shares[4].clone()];
    let candidate_other = reconstruct_secret(&two_shares_other).unwrap();
    assert_ne!(candidate_other, secret);
    assert_ne!(candidate, candidate_other);
}

#[test]
fn test_f05_shamir_corrupted_shares_detected() {
    let secret = [0xfe; 32];
    let shares = split_secret(&secret, 3, 5).unwrap();

    // Corrupt single bit at index 0 of share data
    let mut bad_share_0 = shares[0].clone();
    bad_share_0.data[0] ^= 0x01;
    let subset_bad0 = vec![bad_share_0, shares[1].clone(), shares[2].clone()];
    let err0 = reconstruct_secret(&subset_bad0).unwrap_err();
    assert!(matches!(err0, CryptoError::CorruptedShare(_)));

    // Corrupt last byte of share data
    let mut bad_share_31 = shares[2].clone();
    bad_share_31.data[31] ^= 0x80;
    let subset_bad31 = vec![shares[0].clone(), shares[1].clone(), bad_share_31];
    let err31 = reconstruct_secret(&subset_bad31).unwrap_err();
    assert!(matches!(err31, CryptoError::CorruptedShare(_)));

    // Tamper with checksum directly
    let mut bad_checksum_share = shares[1].clone();
    bad_checksum_share.checksum[0] ^= 0xff;
    let subset_bad_cs = vec![shares[0].clone(), bad_checksum_share, shares[2].clone()];
    let err_cs = reconstruct_secret(&subset_bad_cs).unwrap_err();
    assert!(matches!(err_cs, CryptoError::CorruptedShare(_)));
}

#[test]
fn test_f05_shamir_duplicate_and_zero_share_ids() {
    let secret = [0x33u8; 32];
    let shares = split_secret(&secret, 3, 5).unwrap();

    // Duplicate share IDs in subset
    let duplicates = vec![shares[0].clone(), shares[0].clone(), shares[1].clone()];
    let err_dup = reconstruct_secret(&duplicates).unwrap_err();
    assert_eq!(err_dup, CryptoError::DuplicateShareIndex(1));

    // Share index 0 is invalid
    let zero_share = SecretShare {
        index: 0,
        data: [0x55u8; 32],
        checksum: SecretShare::compute_checksum(0, &[0x55u8; 32]),
    };
    let with_zero = vec![zero_share, shares[1].clone(), shares[2].clone()];
    let err_zero = reconstruct_secret(&with_zero).unwrap_err();
    assert!(matches!(err_zero, CryptoError::CorruptedShare(_)));
}

#[test]
fn test_f05_shamir_invalid_threshold_parameters() {
    let secret = [0x44u8; 32];

    // k > n
    let err_k_gt_n = split_secret(&secret, 5, 3).unwrap_err();
    assert_eq!(
        err_k_gt_n,
        CryptoError::InvalidShareParameters { threshold: 5, total: 3 }
    );

    // k = 0
    let err_k_zero = split_secret(&secret, 0, 5).unwrap_err();
    assert_eq!(
        err_k_zero,
        CryptoError::InvalidShareParameters { threshold: 0, total: 5 }
    );

    // n = 0
    let err_n_zero = split_secret(&secret, 3, 0).unwrap_err();
    assert_eq!(
        err_n_zero,
        CryptoError::InvalidShareParameters { threshold: 3, total: 0 }
    );

    // k = 0, n = 0
    let err_both_zero = split_secret(&secret, 0, 0).unwrap_err();
    assert_eq!(
        err_both_zero,
        CryptoError::InvalidShareParameters { threshold: 0, total: 0 }
    );

    // Empty shares slice passed to reconstruct
    let err_empty = reconstruct_secret(&[]).unwrap_err();
    assert_eq!(
        err_empty,
        CryptoError::InsufficientShares { required: 1, provided: 0 }
    );
}

#[test]
fn test_f05_shamir_boundary_k_equals_n_and_1_of_1() {
    let secret = [0xab; 32];

    // (1, 1) scheme
    let shares_1 = split_secret(&secret, 1, 1).unwrap();
    assert_eq!(shares_1.len(), 1);
    let reconstructed_1 = reconstruct_secret(&shares_1).unwrap();
    assert_eq!(reconstructed_1, secret);

    // (5, 5) scheme: all shares required
    let shares_5 = split_secret(&secret, 5, 5).unwrap();
    assert_eq!(shares_5.len(), 5);
    let reconstructed_5 = reconstruct_secret(&shares_5).unwrap();
    assert_eq!(reconstructed_5, secret);

    // Any 4 shares fail to reconstruct (5, 5)
    let candidate_4 = reconstruct_secret(&shares_5[0..4]).unwrap();
    assert_ne!(candidate_4, secret);
}

// =============================================================================
// F06: Fuzzy Extractor Adversarial Tests
// =============================================================================

#[test]
fn test_f06_fuzzy_extractor_noise_thresholds() {
    let extractor = FuzzyExtractor::default();
    let embedding: Vec<f32> = (0..512).map(|i| ((i as f32) * 0.05).cos()).collect();
    let salt = [0x5cu8; 32];

    let (master_key, record) = extractor
        .generate_commitment(&embedding, &salt)
        .expect("Enrollment must succeed");

    // Low noise (10% bit flips): MUST succeed
    let mut probe_10 = FuzzyExtractor::quantize(&embedding);
    let total_bits = hakille_core::crypto::fuzzy::CODEWORD_BITS;
    let flips_10 = (total_bits as f32 * 0.10) as usize;
    for i in 0..flips_10 {
        let bit = (i * 7) % total_bits;
        probe_10[bit / 8] ^= 1 << (bit % 8);
    }
    let recovered_10 = extractor.reproduce_from_bits(&probe_10, &record).unwrap();
    assert_eq!(master_key.as_bytes(), recovered_10.as_bytes());

    // Extreme noise (40% bit flips clustered across replicas): MUST be rejected
    let mut probe_40 = FuzzyExtractor::quantize(&embedding);
    let flips_40 = (total_bits as f32 * 0.40) as usize;
    for i in 0..flips_40 {
        let bit = (i * 2) % total_bits;
        probe_40[bit / 8] ^= 1 << (bit % 8);
    }
    let res_40 = extractor.reproduce_from_bits(&probe_40, &record);
    assert_eq!(res_40.unwrap_err(), CryptoError::BiometricVerificationFailed);

    // Uniform noise above 50% (55% bit flips flips majority of replicas): MUST be rejected
    let mut probe_55 = FuzzyExtractor::quantize(&embedding);
    let flips_55 = (total_bits as f32 * 0.55) as usize;
    for i in 0..flips_55 {
        let bit = (i * 3) % total_bits;
        probe_55[bit / 8] ^= 1 << (bit % 8);
    }
    let res_55 = extractor.reproduce_from_bits(&probe_55, &record);
    assert_eq!(res_55.unwrap_err(), CryptoError::BiometricVerificationFailed);

    // Independent continuous face embedding from a different person: MUST be rejected
    let different_person_embedding: Vec<f32> = (0..512).map(|i| ((i as f32) * 0.13).sin()).collect();
    let res_different = extractor.reproduce_key(&different_person_embedding, &record);
    assert_eq!(res_different.unwrap_err(), CryptoError::BiometricVerificationFailed);

    // Continuous embedding with extreme uncorrelated alternating noise: MUST be rejected
    let noisy_embedding: Vec<f32> = embedding
        .iter()
        .enumerate()
        .map(|(i, &x)| if i % 2 == 0 { x + 2.0 } else { x - 2.0 })
        .collect();
    let res_continuous = extractor.reproduce_key(&noisy_embedding, &record);
    assert_eq!(res_continuous.unwrap_err(), CryptoError::BiometricVerificationFailed);
}

#[test]
fn test_f06_fuzzy_extractor_orthogonal_and_inverted_embeddings() {
    let extractor = FuzzyExtractor::default();
    let salt = [0xeeu8; 32];

    // Orthogonal embeddings: A along [1, 0, 0, ...], B along [0, 1, 0, ...]
    let mut emb_a = vec![0.0f32; 512];
    emb_a[0] = 1.0;
    let mut emb_b = vec![0.0f32; 512];
    emb_b[1] = 1.0;

    let (_key_a, record_a) = extractor.generate_commitment(&emb_a, &salt).unwrap();

    // Querying with orthogonal embedding B must fail
    let res_b = extractor.reproduce_key(&emb_b, &record_a);
    assert_eq!(res_b.unwrap_err(), CryptoError::BiometricVerificationFailed);

    // Querying with inverted embedding -A must fail
    let emb_neg_a: Vec<f32> = emb_a.iter().map(|&x| -x).collect();
    let res_neg_a = extractor.reproduce_key(&emb_neg_a, &record_a);
    assert_eq!(res_neg_a.unwrap_err(), CryptoError::BiometricVerificationFailed);
}

#[test]
fn test_f06_fuzzy_extractor_dimension_mismatch_and_empty() {
    let extractor = FuzzyExtractor::default();
    let salt = [0x9au8; 32];

    let emb_512 = vec![0.5f32; 512];
    let (_key, record) = extractor.generate_commitment(&emb_512, &salt).unwrap();

    // Query with 128-dim vector (different dimension changes hyperplane seed)
    let emb_128 = vec![0.5f32; 128];
    let res_dim = extractor.reproduce_key(&emb_128, &record);
    assert_eq!(res_dim.unwrap_err(), CryptoError::BiometricVerificationFailed);

    // Query with empty vector
    let emb_empty: Vec<f32> = vec![];
    let res_empty = extractor.reproduce_key(&emb_empty, &record);
    assert_eq!(res_empty.unwrap_err(), CryptoError::BiometricVerificationFailed);
}

#[test]
fn test_f06_fuzzy_extractor_tampered_records() {
    let extractor = FuzzyExtractor::default();
    let salt = [0x12u8; 32];
    let emb = vec![0.3f32; 512];

    let (_key, record) = extractor.generate_commitment(&emb, &salt).unwrap();

    // Truncated helper data
    let mut bad_helper_record = record.clone();
    bad_helper_record.helper_data.truncate(100);
    let res_trunc = extractor.reproduce_key(&emb, &bad_helper_record);
    assert!(matches!(res_trunc.unwrap_err(), CryptoError::BiometricQuantizationError(_)));

    // Tampered salt
    let mut bad_salt_record = record.clone();
    bad_salt_record.salt[0] ^= 0xff;
    let res_salt = extractor.reproduce_key(&emb, &bad_salt_record);
    assert_eq!(res_salt.unwrap_err(), CryptoError::BiometricVerificationFailed);

    // Tampered commitment hash
    let mut bad_commit_record = record.clone();
    bad_commit_record.commitment[0] ^= 0xff;
    let res_commit = extractor.reproduce_key(&emb, &bad_commit_record);
    assert_eq!(res_commit.unwrap_err(), CryptoError::BiometricVerificationFailed);
}

// =============================================================================
// F07: SSI Primitives Adversarial Tests
// =============================================================================

#[test]
fn test_f07_did_key_malformed_inputs() {
    // 1. Wrong prefix
    assert!(matches!(
        parse_did_key("did:example:12345"),
        Err(CryptoError::InvalidDid(_))
    ));
    assert!(matches!(
        parse_did_key("did:key:"),
        Err(CryptoError::InvalidDid(_))
    ));
    assert!(matches!(
        parse_did_key("did:key:x6Mkj..."),
        Err(CryptoError::InvalidDid(_))
    ));

    // 2. Invalid Base58 characters ('0', 'O', 'I', 'l')
    let did_with_invalid_b58 = "did:key:z6Mk0OIlInvalidChars";
    assert!(matches!(
        parse_did_key(did_with_invalid_b58),
        Err(CryptoError::InvalidDid(_))
    ));

    // 3. Wrong payload length (too short)
    let short_payload = hakille_core::crypto::base58_encode(&[0xed, 0x01, 0xaa]);
    let did_short = format!("did:key:z{short_payload}");
    assert!(matches!(
        parse_did_key(&did_short),
        Err(CryptoError::InvalidDid(_))
    ));

    // 4. Bad multicodec prefix (0x1201 instead of 0xed01)
    let mut bad_codec_bytes = vec![0x12, 0x01];
    bad_codec_bytes.extend_from_slice(&[0xaa; 32]);
    let bad_codec_payload = hakille_core::crypto::base58_encode(&bad_codec_bytes);
    let did_bad_codec = format!("did:key:z{bad_codec_payload}");
    assert!(matches!(
        parse_did_key(&did_bad_codec),
        Err(CryptoError::InvalidDid(_))
    ));

    // 5. Test DidKey struct wrapper error propagation
    assert!(matches!(
        DidKey::to_bytes(&did_bad_codec),
        Err(CryptoError::InvalidDid(_))
    ));
    let valid_pk = [0x55u8; 32];
    let valid_did = DidKey::from_bytes(&valid_pk);
    assert_eq!(format_did_key(&valid_pk), valid_did);
    assert_eq!(DidKey::to_bytes(&valid_did).unwrap(), valid_pk);
}

#[test]
fn test_f07_did_jwk_malformed_inputs() {
    // Test DidJwk struct wrapper happy path
    let valid_pk = [0x77u8; 32];
    let valid_jwk = DidJwk::from_bytes(&valid_pk);
    assert_eq!(format_did_jwk(&valid_pk), valid_jwk);
    assert_eq!(DidJwk::to_bytes(&valid_jwk).unwrap(), valid_pk);

    // 1. Wrong method
    assert!(matches!(
        parse_did_jwk("did:key:z6Mk..."),
        Err(CryptoError::InvalidDid(_))
    ));
    assert!(matches!(
        DidJwk::to_bytes("did:key:z6Mk..."),
        Err(CryptoError::InvalidDid(_))
    ));

    // 2. Malformed Base64URL
    assert!(matches!(
        parse_did_jwk("did:jwk:Invalid+Chars/With=Padding"),
        Err(CryptoError::InvalidDid(_))
    ));

    // 3. Not JSON
    let non_json = hakille_core::crypto::base64url_encode(b"not-a-json-object");
    assert!(matches!(
        parse_did_jwk(&format!("did:jwk:{non_json}")),
        Err(CryptoError::InvalidDid(_))
    ));

    // 4. Wrong kty (RSA instead of OKP)
    let rsa_jwk = serde_json::json!({
        "kty": "RSA",
        "crv": "Ed25519",
        "x": hakille_core::crypto::base64url_encode(&[0x42; 32])
    });
    let rsa_encoded = hakille_core::crypto::base64url_encode(&serde_json::to_vec(&rsa_jwk).unwrap());
    assert!(matches!(
        parse_did_jwk(&format!("did:jwk:{rsa_encoded}")),
        Err(CryptoError::InvalidDid(_))
    ));

    // 5. Wrong crv (P-256 instead of Ed25519)
    let p256_jwk = serde_json::json!({
        "kty": "OKP",
        "crv": "P-256",
        "x": hakille_core::crypto::base64url_encode(&[0x42; 32])
    });
    let p256_encoded = hakille_core::crypto::base64url_encode(&serde_json::to_vec(&p256_jwk).unwrap());
    assert!(matches!(
        parse_did_jwk(&format!("did:jwk:{p256_encoded}")),
        Err(CryptoError::InvalidDid(_))
    ));

    // 6. Wrong x length (16 bytes instead of 32)
    let short_x_jwk = serde_json::json!({
        "kty": "OKP",
        "crv": "Ed25519",
        "x": hakille_core::crypto::base64url_encode(&[0x42; 16])
    });
    let short_x_encoded = hakille_core::crypto::base64url_encode(&serde_json::to_vec(&short_x_jwk).unwrap());
    assert!(matches!(
        parse_did_jwk(&format!("did:jwk:{short_x_encoded}")),
        Err(CryptoError::InvalidDid(_))
    ));
}

#[test]
fn test_f07_w3c_vc_tampered_claims_and_forged_signatures() {
    let mut csprng = OsRng;
    let issuer_key = SigningKey::generate(&mut csprng);
    let forged_key = SigningKey::generate(&mut csprng);

    let mut vc = VerifiableCredential {
        context: vec![
            "https://www.w3.org/ns/credentials/v2".to_string(),
            "https://schema.hakille.org/identity/v1".to_string(),
        ],
        id: "urn:uuid:11223344-5566-7788-9900-aabbccddeeff".to_string(),
        credential_type: vec![
            "VerifiableCredential".to_string(),
            "AgentAttestation".to_string(),
        ],
        issuer: String::new(),
        valid_from: "2026-01-01T00:00:00Z".to_string(),
        valid_until: Some("2027-01-01T00:00:00Z".to_string()),
        credential_subject: serde_json::json!({
            "agent_id": "agent:executor-01",
            "tier": "enterprise",
            "permissions": ["read", "write"]
        }),
        proof: None,
    };

    // Issue authentic credential
    vc.issue(&issuer_key).unwrap();
    assert!(vc.verify().unwrap());

    // 1. Missing proof
    let mut vc_no_proof = vc.clone();
    vc_no_proof.proof = None;
    assert_eq!(
        vc_no_proof.verify().unwrap_err(),
        CryptoError::InvalidCredential("Missing proof".to_string())
    );

    // 2. Tampered Subject Claims (privilege escalation)
    let mut vc_tampered_subject = vc.clone();
    vc_tampered_subject.credential_subject = serde_json::json!({
        "agent_id": "agent:executor-01",
        "tier": "enterprise",
        "permissions": ["read", "write", "admin:fullsystem"]
    });
    assert!(matches!(
        vc_tampered_subject.verify().unwrap_err(),
        CryptoError::SignatureVerificationFailed(_)
    ));

    // 3. Tampered Expiry Date (prolonging expired credential)
    let mut vc_tampered_expiry = vc.clone();
    vc_tampered_expiry.valid_until = Some("2099-01-01T00:00:00Z".to_string());
    assert!(matches!(
        vc_tampered_expiry.verify().unwrap_err(),
        CryptoError::SignatureVerificationFailed(_)
    ));

    // 4. Tampered Credential ID
    let mut vc_tampered_id = vc.clone();
    vc_tampered_id.id = "urn:uuid:forged-id-9999".to_string();
    assert!(matches!(
        vc_tampered_id.verify().unwrap_err(),
        CryptoError::SignatureVerificationFailed(_)
    ));

    // 5. Forged Signature: signed by different key
    let mut vc_forged_sig = vc.clone();
    let digest = vc_forged_sig.compute_digest().unwrap();
    let forged_sig: ed25519_dalek::Signature = forged_key.sign(&digest);
    vc_forged_sig.proof.as_mut().unwrap().proof_value =
        hakille_core::crypto::base58_encode(&forged_sig.to_bytes());
    assert!(matches!(
        vc_forged_sig.verify().unwrap_err(),
        CryptoError::SignatureVerificationFailed(_)
    ));

    // 6. Malformed Signature: truncated bytes
    let mut vc_truncated_sig = vc.clone();
    vc_truncated_sig.proof.as_mut().unwrap().proof_value =
        hakille_core::crypto::base58_encode(&[0x11; 32]); // 32 bytes instead of 64
    assert!(matches!(
        vc_truncated_sig.verify().unwrap_err(),
        CryptoError::SignatureVerificationFailed(_)
    ));
}
