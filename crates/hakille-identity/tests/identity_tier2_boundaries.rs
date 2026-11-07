//! Tier 2: Boundary & Corner Cases Test Suite for Hakille Sovereign Identity
//! Features: F01 through F13, plus F22.
//! Requirements: At least 5 boundary/adversarial test cases per feature (14 features x 5 = 70 test cases).

use ed25519_dalek::SigningKey;
use hakille_core::UserId;
use hakille_core::capability::{CapabilityAction, CapabilityScope};
use hakille_core::types::Timestamp;
use hakille_identity::*;
use rand::rngs::OsRng;
use uuid::Uuid;

// =============================================================================
// F01: Argon2id KDF Boundaries
// =============================================================================

#[test]
fn test_f01_boundary_zero_memory_rejected() {
    let p = Argon2Params {
        memory_kib: 0,
        passes: 3,
        parallelism: 4,
    };
    assert!(MasterKeyKdf::derive_master_key_with_params(b"pass", &[0u8; 32], &p).is_err());
}

#[test]
fn test_f01_boundary_zero_passes_rejected() {
    let p = Argon2Params {
        memory_kib: 64,
        passes: 0,
        parallelism: 4,
    };
    assert!(MasterKeyKdf::derive_master_key_with_params(b"pass", &[0u8; 32], &p).is_err());
}

#[test]
fn test_f01_boundary_zero_parallelism_rejected() {
    let p = Argon2Params {
        memory_kib: 64,
        passes: 3,
        parallelism: 0,
    };
    assert!(MasterKeyKdf::derive_master_key_with_params(b"pass", &[0u8; 32], &p).is_err());
}

#[test]
fn test_f01_boundary_1_byte_passphrase() {
    let res = MasterKeyKdf::derive_master_key(b"x", &[1u8; 32]);
    assert!(res.is_ok());
    assert_eq!(res.unwrap().as_bytes().len(), 32);
}

#[test]
fn test_f01_boundary_large_64kb_passphrase() {
    let large_pass = vec![0x61u8; 65536];
    let res = MasterKeyKdf::derive_master_key(&large_pass, &[2u8; 32]);
    assert!(res.is_ok());
}

// =============================================================================
// F02: HKDF Subkey Tree Boundaries
// =============================================================================

#[test]
fn test_f02_boundary_empty_tenant_id() {
    let master = MasterKey::new([5u8; 32]);
    let k = HkdfSubkeyTree::derive_subkey(&master, &SubkeyDomain::Identity, "");
    assert_eq!(k.len(), 32);
}

#[test]
fn test_f02_boundary_large_1024char_tenant_id() {
    let master = MasterKey::new([5u8; 32]);
    let long_id = "tenant-".repeat(150);
    let k = HkdfSubkeyTree::derive_subkey(&master, &SubkeyDomain::Identity, &long_id);
    assert_eq!(k.len(), 32);
}

#[test]
fn test_f02_boundary_special_characters_in_tenant_id() {
    let master = MasterKey::new([5u8; 32]);
    let k1 =
        HkdfSubkeyTree::derive_subkey(&master, &SubkeyDomain::Credentials, "tenant:sub/1?x=1#y");
    let k2 =
        HkdfSubkeyTree::derive_subkey(&master, &SubkeyDomain::Credentials, "tenant:sub/2?x=1#y");
    assert_ne!(k1, k2);
}

#[test]
fn test_f02_boundary_memory_partition_empty_string() {
    let master = MasterKey::new([6u8; 32]);
    let k = HkdfSubkeyTree::derive_subkey(
        &master,
        &SubkeyDomain::MemoryPartition("".to_string()),
        "tenant",
    );
    assert_eq!(k.len(), 32);
}

#[test]
fn test_f02_boundary_cross_domain_zero_collision() {
    let master = MasterKey::new([7u8; 32]);
    let k_id = HkdfSubkeyTree::derive_subkey(&master, &SubkeyDomain::Identity, "t");
    let k_bio = HkdfSubkeyTree::derive_subkey(&master, &SubkeyDomain::Biometrics, "t");
    assert_ne!(k_id, k_bio);
}

// =============================================================================
// F03: XChaCha20-Poly1305 AEAD Boundaries
// =============================================================================

#[test]
fn test_f03_boundary_0_byte_plaintext() {
    let key = [9u8; 32];
    let aad = b"empty-test";
    let blob = XChaChaCipher::encrypt(b"", &key, aad).expect("empty plaintext encryption succeeds");
    assert_eq!(blob.ciphertext.len(), 0);
    let decrypted = XChaChaCipher::decrypt(&blob, &key, aad).expect("empty decrypt succeeds");
    assert_eq!(decrypted, b"");
}

#[test]
fn test_f03_boundary_large_payload() {
    let key = [10u8; 32];
    let large_payload = vec![0x42u8; 100_000]; // 100 KB payload
    let blob = XChaChaCipher::encrypt(&large_payload, &key, b"large-aad").unwrap();
    let recovered = XChaChaCipher::decrypt(&blob, &key, b"large-aad").unwrap();
    assert_eq!(recovered, large_payload);
}

#[test]
fn test_f03_boundary_corrupted_nonce_fails_decrypt() {
    let key = [11u8; 32];
    let mut blob = XChaChaCipher::encrypt(b"secret", &key, b"aad").unwrap();
    blob.nonce[0] ^= 0x01; // Corrupt nonce
    assert!(XChaChaCipher::decrypt(&blob, &key, b"aad").is_err());
}

#[test]
fn test_f03_boundary_cross_table_splicing_detected() {
    let key = [12u8; 32];
    let blob = XChaChaCipher::encrypt(b"table-a-data", &key, b"table:A").unwrap();
    // Attempting to inject into table:B fails fingerprint check
    assert!(XChaChaCipher::decrypt(&blob, &key, b"table:B").is_err());
}

#[test]
fn test_f03_boundary_corrupt_tag_fails_decrypt() {
    let key = [13u8; 32];
    let mut blob = XChaChaCipher::encrypt(b"secret", &key, b"aad").unwrap();
    blob.tag[15] ^= 0x01;
    assert!(XChaChaCipher::decrypt(&blob, &key, b"aad").is_err());
}

// =============================================================================
// F04: HMAC Blind Indexing Boundaries
// =============================================================================

#[test]
fn test_f04_boundary_empty_string_term() {
    let k = [1u8; 32];
    let token = compute_blind_index("", &k);
    assert_eq!(token.len(), 32);
}

#[test]
fn test_f04_boundary_long_term_1000chars() {
    let k = [2u8; 32];
    let long_term = "query_token_".repeat(100);
    let token = compute_blind_index(&long_term, &k);
    assert_eq!(token.len(), 32);
}

#[test]
fn test_f04_boundary_unicode_characters() {
    let k = [3u8; 32];
    let t1 = compute_blind_index("éèçà-identité-souveraine", &k);
    let t2 = compute_blind_index("ÉÈÇÀ-IDENTITÉ-SOUVERAINE", &k);
    assert_eq!(t1, t2);
}

#[test]
fn test_f04_boundary_prefix_length_exceeds_term_length() {
    let k = [4u8; 32];
    let p = compute_prefix_blind_index("hi", 10, &k);
    assert_eq!(p.len(), 16);
}

#[test]
fn test_f04_boundary_search_nonexistent_returns_empty() {
    let k = [5u8; 32];
    let store = BlindIndexStore::new();
    assert!(store.search_prefix("unknown", &k).is_empty());
}

// =============================================================================
// F05: Shamir's Secret Sharing Boundaries
// =============================================================================

#[test]
fn test_f05_boundary_k_greater_than_n_rejected() {
    let secret = [8u8; 32];
    assert!(split_secret(&secret, 6, 5).is_err());
}

#[test]
fn test_f05_boundary_1_of_1_threshold() {
    let secret = [9u8; 32];
    let shares = split_secret(&secret, 1, 1).expect("1-of-1 succeeds");
    let recovered = reconstruct_secret(&shares).expect("1-of-1 reconstructs");
    assert_eq!(recovered, secret);
}

#[test]
fn test_f05_boundary_k_minus_1_shares_fail_reconstruction() {
    let secret = [10u8; 32];
    let shares = split_secret(&secret, 3, 5).unwrap();
    // 2 shares provided when 3 required -> Lagrange yields different polynomial
    let res = reconstruct_secret(&shares[0..2]);
    if let Ok(rec) = res {
        assert_ne!(rec, secret, "k-1 shares cannot reconstruct master secret");
    }
}

#[test]
fn test_f05_boundary_duplicate_shares_rejected() {
    let secret = [11u8; 32];
    let shares = split_secret(&secret, 2, 4).unwrap();
    let dup = vec![shares[0].clone(), shares[0].clone()];
    assert!(reconstruct_secret(&dup).is_err());
}

#[test]
fn test_f05_boundary_corrupt_data_detected_by_commitment() {
    let secret = [12u8; 32];
    let mut shares = split_secret(&secret, 2, 3).unwrap();
    shares[1].data[10] ^= 0x01; // Mutate single byte
    assert!(reconstruct_secret(&shares[0..2]).is_err());
}

// =============================================================================
// F06: Fuzzy Extractor Biometric Boundaries
// =============================================================================

#[test]
fn test_f06_boundary_all_zeros_biometric() {
    let fe = FuzzyExtractor::new(16);
    let sample = vec![0.0f32; 256];
    let (key, helper, commit) = fe.generate(&sample).unwrap();
    let rec = fe.reproduce(&sample, &helper, &commit).unwrap();
    assert_eq!(rec, key);
}

#[test]
fn test_f06_boundary_all_negative_biometric() {
    let fe = FuzzyExtractor::new(16);
    let sample = vec![-1.0f32; 256];
    let (key, helper, commit) = fe.generate(&sample).unwrap();
    let rec = fe.reproduce(&sample, &helper, &commit).unwrap();
    assert_eq!(rec, key);
}

#[test]
fn test_f06_boundary_noise_beyond_delta_rejected() {
    let fe = FuzzyExtractor::new(8);
    let mut enrolled = vec![1.0f32; 256];
    let (_, helper, commit) = fe.generate(&enrolled).unwrap();

    // Perturb 128 coordinates across repetitions (well beyond error-correction capacity)
    for i in (0..256).step_by(2) {
        enrolled[i] = -1.0;
    }
    assert!(fe.reproduce(&enrolled, &helper, &commit).is_err());
}

#[test]
fn test_f06_boundary_invalid_helper_syndrome_len() {
    let fe = FuzzyExtractor::new(16);
    let helper = HelperData {
        syndrome: vec![1, 2, 3], // Truncated syndrome
        salt: [0u8; 16],
    };
    let commit = CommitmentHash([0u8; 32]);
    assert!(fe.reproduce(&[1.0; 256], &helper, &commit).is_err());
}

#[test]
fn test_f06_boundary_mismatched_commitment() {
    let fe = FuzzyExtractor::new(16);
    let sample = vec![0.5f32; 256];
    let (_, helper, _) = fe.generate(&sample).unwrap();
    let fake_commit = CommitmentHash([0xfeu8; 32]);
    assert!(fe.reproduce(&sample, &helper, &fake_commit).is_err());
}

// =============================================================================
// F07: SSI DIDs & Verifiable Credentials Boundaries
// =============================================================================

#[test]
fn test_f07_boundary_invalid_base58_in_did_key() {
    let did = Did::new("did:key:zInvalidBase58_0OIl");
    assert!(DidKey::extract_ed25519_bytes(&did).is_err());
}

#[test]
fn test_f07_boundary_wrong_multicodec_prefix() {
    // 0x99 0x99 instead of 0xed 0x01
    let did = Did::new("did:key:z11111111111111111111111111111111111");
    assert!(DidKey::extract_ed25519_bytes(&did).is_err());
}

#[test]
fn test_f07_boundary_vc_valid_in_future_rejected() {
    let key = SigningKey::generate(&mut OsRng);
    let now = 1000;
    let vc = VerifiableCredential::issue(
        "urn:future",
        vec!["TestVC".into()],
        &key,
        2000, // Valid from t=2000
        3000,
        serde_json::json!({}),
    );
    assert!(vc.verify(now).is_err());
}

#[test]
fn test_f07_boundary_vc_expired_by_one_second() {
    let key = SigningKey::generate(&mut OsRng);
    let now = 2001;
    let vc = VerifiableCredential::issue(
        "urn:expired",
        vec!["TestVC".into()],
        &key,
        1000,
        2000, // Expired at t=2000
        serde_json::json!({}),
    );
    assert!(matches!(
        vc.verify(now),
        Err(hakille_core::HakilleError::CapabilityExpired)
    ));
}

#[test]
fn test_f07_boundary_vc_malformed_signature_string() {
    let key = SigningKey::generate(&mut OsRng);
    let mut vc = VerifiableCredential::issue(
        "urn:malformed",
        vec!["TestVC".into()],
        &key,
        1000,
        3000,
        serde_json::json!({}),
    );
    vc.proof.proof_value = "NotValidBase58!!".to_string();
    assert!(vc.verify(2000).is_err());
}

// =============================================================================
// F08: 5-Tier Domain Models Boundaries
// =============================================================================

#[test]
fn test_f08_boundary_enterprise_with_parent_rejected() {
    let did = Did::new("did:key:zRoot");
    let cfg = TierConfiguration::Enterprise(EnterpriseConfig::default());
    let parent = Some(TenantId::new());
    assert!(TenantEntityRecord::new(EntityTier::Enterprise, "Holding", did, cfg, parent).is_err());
}

#[test]
fn test_f08_boundary_family_with_parent_rejected() {
    let did = Did::new("did:key:zFam");
    let cfg = TierConfiguration::Family(FamilyConfig::default());
    let parent = Some(TenantId::new());
    assert!(TenantEntityRecord::new(EntityTier::Family, "Household", did, cfg, parent).is_err());
}

#[test]
fn test_f08_boundary_empty_tenant_name_rejected() {
    let did = Did::new("did:key:zX");
    let cfg = TierConfiguration::Group(GroupConfig::default());
    assert!(TenantEntityRecord::new(EntityTier::Group, "   ", did, cfg, None).is_err());
}

#[test]
fn test_f08_boundary_dissolved_tenant_transition_rejected() {
    let did = Did::new("did:key:zX");
    let cfg = TierConfiguration::Group(GroupConfig::default());
    let mut rec = TenantEntityRecord::new(EntityTier::Group, "Circle", did, cfg, None).unwrap();
    rec.transition_state(TenantState::Dissolved).unwrap();
    // Cannot transition from dissolved
    assert!(rec.transition_state(TenantState::Active).is_err());
}

#[test]
fn test_f08_boundary_membership_expiry_check() {
    let mem = TenantMembership::new(
        TenantId::new(),
        Did::new("did:key:zUser"),
        "Member",
        vec![],
        2,
    );
    let past = chrono::Utc::now() - chrono::Duration::hours(1);
    assert!(
        !mem.is_valid_at(past),
        "Membership cannot be valid in the past before valid_from"
    );
}

// =============================================================================
// F09: Zanzibar ReBAC Engine Boundaries
// =============================================================================

#[test]
fn test_f09_boundary_parse_missing_hash_separator() {
    assert!(RelationTuple::parse("team:alpha_member@user:bob").is_err());
}

#[test]
fn test_f09_boundary_parse_missing_at_separator() {
    assert!(RelationTuple::parse("team:alpha#member_user:bob").is_err());
}

#[test]
fn test_f09_boundary_parse_missing_entity_type_colon() {
    assert!(RelationTuple::parse("teamalpha#member@user:bob").is_err());
}

#[test]
fn test_f09_boundary_nonexistent_traversal_returns_false() {
    let rebac = RebacEngine::new();
    assert!(!rebac.check_relation_transitive("org", "x", "view", "user", "y"));
}

#[test]
fn test_f09_boundary_deep_chain_10_nodes_traversal() {
    let mut rebac = RebacEngine::new();
    for i in 1..10 {
        rebac.add_tuple(RelationTuple::new(
            "org",
            format!("node_{}", i),
            "parent_of",
            "org",
            format!("node_{}", i + 1),
        ));
    }
    rebac.add_tuple(RelationTuple::new(
        "org",
        "node_10",
        "member",
        "user",
        "leaf_user",
    ));

    assert!(rebac.check_relation_transitive("org", "node_1", "member", "user", "leaf_user"));
}

// =============================================================================
// F10: Hybrid Access Control Engine Boundaries
// =============================================================================

#[test]
fn test_f10_boundary_confidence_exact_threshold_0_90() {
    let mut engine = AccessEngine::new();
    engine
        .rebac
        .add_tuple(RelationTuple::new("wallet", "w1", "owner", "user", "u"));

    let ctx_below = AuthContext {
        subject: "user:u".into(),
        base_role: "Owner".into(),
        confidence_score: 0.899, // Just below 0.90
        device_tier: DeviceTrustTier::Tier1HardwareEnclave,
        tenant_id: TenantId::new(),
        scope: CapabilityScope::Wallet,
        action: CapabilityAction::Execute,
        resource: "w1".into(),
    };
    let tuple = RelationTuple::new("wallet", "w1", "owner", "user", "u");
    assert!(!engine.check_permission(&ctx_below, &tuple).is_allowed());

    let ctx_exact = AuthContext {
        confidence_score: 0.900,
        ..ctx_below
    };
    assert!(engine.check_permission(&ctx_exact, &tuple).is_allowed());
}

#[test]
fn test_f10_boundary_guest_attempting_fullsystem_denied() {
    let mut engine = AccessEngine::new();
    engine
        .rebac
        .add_tuple(RelationTuple::new("sys", "root", "admin", "user", "guest"));

    let ctx = AuthContext {
        subject: "user:guest".into(),
        base_role: "Guest".into(),
        confidence_score: 0.99,
        device_tier: DeviceTrustTier::Tier1HardwareEnclave,
        tenant_id: TenantId::new(),
        scope: CapabilityScope::FullSystem,
        action: CapabilityAction::Admin,
        resource: "root".into(),
    };
    let tuple = RelationTuple::new("sys", "root", "admin", "user", "guest");
    assert!(!engine.check_permission(&ctx, &tuple).is_allowed());
}

#[test]
fn test_f10_boundary_tier3_device_wallet_denied() {
    let mut engine = AccessEngine::new();
    engine
        .rebac
        .add_tuple(RelationTuple::new("wallet", "w1", "owner", "user", "u"));

    let ctx = AuthContext {
        subject: "user:u".into(),
        base_role: "Owner".into(),
        confidence_score: 0.99,
        device_tier: DeviceTrustTier::Tier3UntrustedWeb,
        tenant_id: TenantId::new(),
        scope: CapabilityScope::Wallet,
        action: CapabilityAction::Execute,
        resource: "w1".into(),
    };
    let tuple = RelationTuple::new("wallet", "w1", "owner", "user", "u");
    assert!(!engine.check_permission(&ctx, &tuple).is_allowed());
}

#[test]
fn test_f10_boundary_write_filesystem_requires_0_70() {
    let mut engine = AccessEngine::new();
    engine
        .rebac
        .add_tuple(RelationTuple::new("fs", "file", "write", "user", "u"));

    let ctx = AuthContext {
        subject: "user:u".into(),
        base_role: "Editor".into(),
        confidence_score: 0.65, // Below 0.70 for write
        device_tier: DeviceTrustTier::Tier2SoftwareHost,
        tenant_id: TenantId::new(),
        scope: CapabilityScope::Filesystem,
        action: CapabilityAction::Write,
        resource: "file".into(),
    };
    let tuple = RelationTuple::new("fs", "file", "write", "user", "u");
    assert!(!engine.check_permission(&ctx, &tuple).is_allowed());
}

#[test]
fn test_f10_boundary_read_filesystem_allows_0_40() {
    let mut engine = AccessEngine::new();
    engine
        .rebac
        .add_tuple(RelationTuple::new("fs", "file", "read", "user", "u"));

    let ctx = AuthContext {
        subject: "user:u".into(),
        base_role: "Reader".into(),
        confidence_score: 0.45, // >= 0.40 for read
        device_tier: DeviceTrustTier::Tier2SoftwareHost,
        tenant_id: TenantId::new(),
        scope: CapabilityScope::Filesystem,
        action: CapabilityAction::Read,
        resource: "file".into(),
    };
    let tuple = RelationTuple::new("fs", "file", "read", "user", "u");
    assert!(engine.check_permission(&ctx, &tuple).is_allowed());
}

// =============================================================================
// F11: Biscuit Capabilities Boundaries
// =============================================================================

#[test]
fn test_f11_boundary_scope_expansion_rejected() {
    let mgr = BiscuitCapabilityManager::new();
    let exp = Timestamp(chrono::Utc::now() + chrono::Duration::hours(1));

    let root = mgr
        .mint_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            exp.clone(),
        )
        .unwrap();

    // Attempting to expand to Wallet MUST fail
    let res = mgr.attenuate_token(
        &root,
        &[CapabilityScope::Filesystem],
        &[CapabilityAction::Read],
        &[CapabilityScope::Wallet],
        &[CapabilityAction::Read],
        exp,
    );
    assert!(res.is_err());
}

#[test]
fn test_f11_boundary_action_expansion_rejected() {
    let mgr = BiscuitCapabilityManager::new();
    let exp = Timestamp(chrono::Utc::now() + chrono::Duration::hours(1));

    let root = mgr
        .mint_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            exp.clone(),
        )
        .unwrap();

    // Attempting to expand to Admin action MUST fail
    let res = mgr.attenuate_token(
        &root,
        &[CapabilityScope::Filesystem],
        &[CapabilityAction::Read],
        &[CapabilityScope::Filesystem],
        &[CapabilityAction::Admin],
        exp,
    );
    assert!(res.is_err());
}

#[test]
fn test_f11_boundary_expiry_extension_rejected() {
    let mgr = BiscuitCapabilityManager::new();
    let exp_parent = Timestamp(chrono::Utc::now() + chrono::Duration::minutes(10));
    let exp_child = Timestamp(chrono::Utc::now() + chrono::Duration::minutes(20));

    let root = mgr
        .mint_root_token(
            vec![CapabilityScope::Sensor],
            vec![CapabilityAction::Read],
            exp_parent,
        )
        .unwrap();

    let res = mgr.attenuate_token(
        &root,
        &[CapabilityScope::Sensor],
        &[CapabilityAction::Read],
        &[CapabilityScope::Sensor],
        &[CapabilityAction::Read],
        exp_child,
    );
    assert!(res.is_err());
}

#[test]
fn test_f11_boundary_expired_token_rejected_on_verify() {
    let mgr = BiscuitCapabilityManager::new();
    let past = Timestamp(chrono::Utc::now() - chrono::Duration::seconds(10));
    let token = mgr
        .mint_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            past,
        )
        .unwrap();

    assert!(
        mgr.verify_token(&token, CapabilityScope::Filesystem, CapabilityAction::Read)
            .is_err()
    );
}

#[test]
fn test_f11_boundary_revocation_idempotence() {
    let mut mgr = BiscuitCapabilityManager::new();
    let exp = Timestamp(chrono::Utc::now() + chrono::Duration::hours(1));
    let token = mgr
        .mint_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            exp,
        )
        .unwrap();

    mgr.revoke_token(&token).unwrap();
    assert!(mgr.is_revoked(&token));
    // Revoking again is safe and remains revoked
    mgr.revoke_token(&token).unwrap();
    assert!(mgr.is_revoked(&token));
}

// =============================================================================
// F12: Sovereign Agent Delegations Boundaries
// =============================================================================

#[test]
fn test_f12_boundary_depth_4_succeeds() {
    let (_, agent) = AgentFactory::spawn_agent();
    let passport = AgentFactory::issue_passport(
        agent,
        Did::new("did:key:zP"),
        TenantId::new(),
        3600,
        1000,
        100,
        vec![],
        4,
    );
    assert!(passport.is_ok());
}

#[test]
fn test_f12_boundary_depth_5_rejected_fail_closed() {
    let (_, agent) = AgentFactory::spawn_agent();
    // Attempting depth 5 must fail with CapabilityDenied
    let passport = AgentFactory::issue_passport(
        agent,
        Did::new("did:key:zP"),
        TenantId::new(),
        3600,
        1000,
        100,
        vec![],
        5,
    );
    assert!(passport.is_err());
}

#[test]
fn test_f12_boundary_subagent_demanding_excess_fuel() {
    let (_, p_agent) = AgentFactory::spawn_agent();
    let passport = AgentFactory::issue_passport(
        p_agent,
        Did::new("did:key:zP"),
        TenantId::new(),
        3600,
        1000,
        100,
        vec![],
        1,
    )
    .unwrap();

    let (_, child) = AgentFactory::spawn_agent();
    let res = AgentFactory::delegate_sub_agent(&passport, child, vec![], 1001, 50);
    assert!(res.is_err());
}

#[test]
fn test_f12_boundary_subagent_demanding_excess_budget() {
    let (_, p_agent) = AgentFactory::spawn_agent();
    let passport = AgentFactory::issue_passport(
        p_agent,
        Did::new("did:key:zP"),
        TenantId::new(),
        3600,
        1000,
        100,
        vec![],
        1,
    )
    .unwrap();

    let (_, child) = AgentFactory::spawn_agent();
    let res = AgentFactory::delegate_sub_agent(&passport, child, vec![], 500, 101);
    assert!(res.is_err());
}

#[test]
fn test_f12_boundary_minor_protection_guard_rule7_blocks_wallet() {
    let res = AgentFactory::enforce_minor_protection_invariant(true, &CapabilityScope::Wallet);
    assert!(res.is_err());
}

// =============================================================================
// F13: Cross-Tenant Firewalls Boundaries
// =============================================================================

#[test]
fn test_f13_boundary_family_cannot_grant_corporate_delegation() {
    let mut fw = CrossTenantFirewall::new();
    let fam = TenantId::new();
    let org = TenantId::new();
    fw.register_tenant(fam, EntityTier::Family);
    fw.register_tenant(org, EntityTier::Organization);

    assert!(fw.grant_bilateral_delegation(fam, org).is_err());
}

#[test]
fn test_f13_boundary_org_cannot_bridge_to_family() {
    let mut fw = CrossTenantFirewall::new();
    let org = TenantId::new();
    let fam = TenantId::new();
    fw.register_tenant(org, EntityTier::Organization);
    fw.register_tenant(fam, EntityTier::Family);

    assert!(fw.grant_bilateral_delegation(org, fam).is_err());
}

#[test]
fn test_f13_boundary_unregistered_tenant_fails_grant() {
    let mut fw = CrossTenantFirewall::new();
    let t1 = TenantId::new();
    let t2 = TenantId::new();
    assert!(fw.grant_bilateral_delegation(t1, t2).is_err());
}

#[test]
fn test_f13_boundary_cross_tenant_request_without_grant_fails() {
    let mut fw = CrossTenantFirewall::new();
    let o1 = TenantId::new();
    let o2 = TenantId::new();
    fw.register_tenant(o1, EntityTier::Organization);
    fw.register_tenant(o2, EntityTier::Organization);

    assert!(fw.check_cross_tenant_access(&o1, &o2).is_err());
}

#[test]
fn test_f13_boundary_directional_grant_not_automatically_symmetric() {
    let mut fw = CrossTenantFirewall::new();
    let o1 = TenantId::new();
    let o2 = TenantId::new();
    fw.register_tenant(o1, EntityTier::Organization);
    fw.register_tenant(o2, EntityTier::Organization);

    fw.grant_bilateral_delegation(o1, o2).unwrap();
    assert!(fw.check_cross_tenant_access(&o1, &o2).is_ok());
    assert!(fw.check_cross_tenant_access(&o2, &o1).is_err());
}

// =============================================================================
// F22: ZK Storage Schemas Boundaries
// =============================================================================

#[test]
fn test_f22_boundary_query_nonexistent_blind_token_returns_none() {
    let store = RedbZkStore::new();
    assert!(
        store
            .lookup_user_by_blind_username("ghost", &[1u8; 32])
            .is_none()
    );
}

#[test]
fn test_f22_boundary_trigger_twilight_on_nonexistent_user_fails() {
    let mut store = RedbZkStore::new();
    let ghost = UserId(Uuid::new_v4());
    assert!(store.trigger_digital_twilight(&ghost).is_err());
}

#[test]
fn test_f22_boundary_insert_duplicate_blind_user_fails() {
    let mut store = RedbZkStore::new();
    let dummy = XChaChaBlob {
        nonce: [0u8; 24],
        ciphertext: vec![],
        tag: [0u8; 16],
        aad_fingerprint: [0u8; 16],
    };
    let rec1 = UserRecord {
        user_id: UserId(Uuid::new_v4()),
        primary_did: Did::new("did:key:z1"),
        blind_username_hash: [0x55u8; 32],
        encrypted_profile_blob: dummy.clone(),
        state: AccountStatus::Active,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let rec2 = UserRecord {
        user_id: UserId(Uuid::new_v4()),
        primary_did: Did::new("did:key:z2"),
        blind_username_hash: [0x55u8; 32],
        encrypted_profile_blob: dummy,
        state: AccountStatus::Active,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    assert!(store.insert_user(rec1).is_ok());
    assert!(store.insert_user(rec2).is_err());
}

#[test]
fn test_f22_boundary_query_empty_blind_spo() {
    let store = RedbZkStore::new();
    assert!(store.query_blind_spo(&[0x11u8; 32]).is_empty());
}

#[test]
fn test_f22_boundary_crypto_shredding_state_persistence() {
    let mut store = RedbZkStore::new();
    let uid = UserId(Uuid::new_v4());
    let dummy = XChaChaBlob {
        nonce: [0u8; 24],
        ciphertext: vec![],
        tag: [0u8; 16],
        aad_fingerprint: [0u8; 16],
    };
    let rec = UserRecord {
        user_id: uid.clone(),
        primary_did: Did::new("did:key:z3"),
        blind_username_hash: [0x66u8; 32],
        encrypted_profile_blob: dummy,
        state: AccountStatus::Active,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    store.insert_user(rec).unwrap();
    store.trigger_digital_twilight(&uid).unwrap();
    assert!(store.is_salt_shredded(&uid));
}
