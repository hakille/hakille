//! Tier 1: Feature Coverage Test Suite for Hakille Sovereign Identity
//! Features: F01 through F13, plus F22.
//! Requirements: At least 5 distinct test cases per feature (14 features x 5 = 70 test cases).

use ed25519_dalek::SigningKey;
use hakille_core::capability::{CapabilityAction, CapabilityScope};
use hakille_core::types::Timestamp;
use hakille_core::UserId;
use hakille_identity::*;
use rand::rngs::OsRng;
use uuid::Uuid;

// =============================================================================
// F01: Argon2id KDF (RFC 9106)
// =============================================================================

#[test]
fn test_f01_argon2id_happy_path_derivation() {
    let passphrase = b"CorrectHorseBatteryStaple-2026!";
    let salt = [0x42u8; 32];
    let master = MasterKeyKdf::derive_master_key(passphrase, &salt).expect("derivation succeeds");
    assert_eq!(master.as_bytes().len(), 32);
    assert!(master.as_bytes().iter().any(|&b| b != 0));
}

#[test]
fn test_f01_argon2id_deterministic_identical_salt() {
    let passphrase = b"SovereignIdentityKeyPassphrase";
    let salt = [0x55u8; 32];
    let k1 = MasterKeyKdf::derive_master_key(passphrase, &salt).unwrap();
    let k2 = MasterKeyKdf::derive_master_key(passphrase, &salt).unwrap();
    assert_eq!(k1.as_bytes(), k2.as_bytes());
}

#[test]
fn test_f01_argon2id_distinct_different_salts() {
    let passphrase = b"CommonPassphrase";
    let salt1 = [0x01u8; 32];
    let salt2 = [0x02u8; 32];
    let k1 = MasterKeyKdf::derive_master_key(passphrase, &salt1).unwrap();
    let k2 = MasterKeyKdf::derive_master_key(passphrase, &salt2).unwrap();
    assert_ne!(k1.as_bytes(), k2.as_bytes());
}

#[test]
fn test_f01_argon2id_distinct_different_passphrases() {
    let salt = [0x99u8; 32];
    let k1 = MasterKeyKdf::derive_master_key(b"PassphraseAlpha", &salt).unwrap();
    let k2 = MasterKeyKdf::derive_master_key(b"PassphraseBeta", &salt).unwrap();
    assert_ne!(k1.as_bytes(), k2.as_bytes());
}

#[test]
fn test_f01_argon2id_memory_block_permutation_scaling() {
    let salt = [0x77u8; 32];
    let params_light = Argon2Params {
        memory_kib: 64,
        passes: 1,
        parallelism: 1,
    };
    let params_standard = Argon2Params {
        memory_kib: 256,
        passes: 2,
        parallelism: 2,
    };
    let k_light =
        MasterKeyKdf::derive_master_key_with_params(b"scale_test", &salt, &params_light).unwrap();
    let k_std =
        MasterKeyKdf::derive_master_key_with_params(b"scale_test", &salt, &params_standard).unwrap();
    assert_ne!(k_light.as_bytes(), k_std.as_bytes());
}

// =============================================================================
// F02: HKDF-SHA256 Subkey Tree (RFC 5869)
// =============================================================================

#[test]
fn test_f02_hkdf_domain_separation_identity_vs_credentials() {
    let master = MasterKey::new([0x33u8; 32]);
    let tenant = "tenant-alpha";
    let k_id = HkdfSubkeyTree::derive_subkey(&master, &SubkeyDomain::Identity, tenant);
    let k_cred = HkdfSubkeyTree::derive_subkey(&master, &SubkeyDomain::Credentials, tenant);
    assert_ne!(k_id, k_cred);
}

#[test]
fn test_f02_hkdf_tenant_isolation_different_tenant_ids() {
    let master = MasterKey::new([0x33u8; 32]);
    let k_t1 = HkdfSubkeyTree::derive_subkey(&master, &SubkeyDomain::Identity, "tenant-1");
    let k_t2 = HkdfSubkeyTree::derive_subkey(&master, &SubkeyDomain::Identity, "tenant-2");
    assert_ne!(k_t1, k_t2);
}

#[test]
fn test_f02_hkdf_memory_partition_domains() {
    let master = MasterKey::new([0x44u8; 32]);
    let k_p1 = HkdfSubkeyTree::derive_subkey(
        &master,
        &SubkeyDomain::MemoryPartition("financial".to_string()),
        "tenant-main",
    );
    let k_p2 = HkdfSubkeyTree::derive_subkey(
        &master,
        &SubkeyDomain::MemoryPartition("health".to_string()),
        "tenant-main",
    );
    assert_ne!(k_p1, k_p2);
}

#[test]
fn test_f02_hkdf_deterministic_expansion() {
    let master = MasterKey::new([0x12u8; 32]);
    let k1 = HkdfSubkeyTree::derive_subkey(&master, &SubkeyDomain::BlindIndex, "tenant-x");
    let k2 = HkdfSubkeyTree::derive_subkey(&master, &SubkeyDomain::BlindIndex, "tenant-x");
    assert_eq!(k1, k2);
}

#[test]
fn test_f02_hkdf_cryptographic_independence() {
    let master = MasterKey::new([0xabu8; 32]);
    let k_bio = HkdfSubkeyTree::derive_subkey(&master, &SubkeyDomain::Biometrics, "t");
    let k_blind = HkdfSubkeyTree::derive_subkey(&master, &SubkeyDomain::BlindIndex, "t");
    assert_ne!(k_bio, k_blind);
    // Overlap should be near-zero bits identical
    let mut matching_bytes = 0;
    for (a, b) in k_bio.iter().zip(k_blind.iter()) {
        if a == b {
            matching_bytes += 1;
        }
    }
    assert!(matching_bytes < 5, "Independence guarantee violation");
}

// =============================================================================
// F03: XChaCha20-Poly1305 AEAD
// =============================================================================

#[test]
fn test_f03_xchacha_happy_path_encrypt_decrypt() {
    let key = [0x77u8; 32];
    let plaintext = b"Confidential biometric presence commitment record";
    let aad = b"tenant:org-42|table:users|ver:1";

    let encrypted = XChaChaCipher::encrypt(plaintext, &key, aad).expect("encryption succeeds");
    assert_eq!(encrypted.nonce.len(), 24);
    assert_eq!(encrypted.tag.len(), 16);
    assert_ne!(encrypted.ciphertext, plaintext);

    let decrypted = XChaChaCipher::decrypt(&encrypted, &key, aad).expect("decryption succeeds");
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_f03_xchacha_24byte_nonce_uniqueness() {
    let key = [0x88u8; 32];
    let plaintext = b"Identical Plaintext Payload";
    let aad = b"aad-metadata";

    let blob1 = XChaChaCipher::encrypt(plaintext, &key, aad).unwrap();
    let blob2 = XChaChaCipher::encrypt(plaintext, &key, aad).unwrap();
    assert_ne!(blob1.nonce, blob2.nonce);
    assert_ne!(blob1.ciphertext, blob2.ciphertext);
}

#[test]
fn test_f03_xchacha_aad_binding_verification() {
    let key = [0x88u8; 32];
    let plaintext = b"Secret Financial Triplet";
    let aad_tenant_a = b"tenant:subsidiary-a|table:ledgers";
    let aad_tenant_b = b"tenant:subsidiary-b|table:ledgers";

    let blob = XChaChaCipher::encrypt(plaintext, &key, aad_tenant_a).unwrap();
    // Cross-tenant ciphertext splicing fails MAC / AAD verification
    let res = XChaChaCipher::decrypt(&blob, &key, aad_tenant_b);
    assert!(res.is_err());
}

#[test]
fn test_f03_xchacha_tampered_ciphertext_fails_mac() {
    let key = [0x99u8; 32];
    let plaintext = b"Critical System Kernel Token";
    let aad = b"context-aad";

    let mut blob = XChaChaCipher::encrypt(plaintext, &key, aad).unwrap();
    // Tamper single bit
    if let Some(b) = blob.ciphertext.first_mut() {
        *b ^= 0x01;
    }
    let res = XChaChaCipher::decrypt(&blob, &key, aad);
    assert!(res.is_err());
}

#[test]
fn test_f03_xchacha_tampered_tag_fails_mac() {
    let key = [0x99u8; 32];
    let plaintext = b"Critical System Kernel Token";
    let aad = b"context-aad";

    let mut blob = XChaChaCipher::encrypt(plaintext, &key, aad).unwrap();
    blob.tag[0] ^= 0x80;
    let res = XChaChaCipher::decrypt(&blob, &key, aad);
    assert!(res.is_err());
}

// =============================================================================
// F04: HMAC-SHA256 Blind Indexing
// =============================================================================

#[test]
fn test_f04_blind_index_exact_match_lookup() {
    let blind_key = [0x5au8; 32];
    let mut store = BlindIndexStore::new();

    store.insert("alice@hakille.org", "record-uuid-101", &blind_key);
    store.insert("bob@hakille.org", "record-uuid-102", &blind_key);

    let hits = store.search_exact("alice@hakille.org", &blind_key);
    assert_eq!(hits, vec!["record-uuid-101"]);
}

#[test]
fn test_f04_blind_index_normalization_case_and_whitespace() {
    let blind_key = [0x5au8; 32];
    let t1 = compute_blind_index("  User.Alice@Hakille.org  ", &blind_key);
    let t2 = compute_blind_index("user.alice@hakille.org", &blind_key);
    assert_eq!(t1, t2);
}

#[test]
fn test_f04_blind_index_prefix_token_search() {
    let blind_key = [0x6bu8; 32];
    let mut store = BlindIndexStore::new();

    store.insert("telemetry-worker-node-1", "rec-1", &blind_key);
    store.insert("telemetry-worker-node-2", "rec-2", &blind_key);
    store.insert("billing-service", "rec-3", &blind_key);

    let hits = store.search_prefix("telemetry", &blind_key);
    assert_eq!(hits.len(), 2);
    assert!(hits.contains(&"rec-1".to_string()));
    assert!(hits.contains(&"rec-2".to_string()));
}

#[test]
fn test_f04_blind_index_no_plaintext_leak_on_mismatch() {
    let blind_key = [0x6bu8; 32];
    let store = BlindIndexStore::new();
    let hits = store.search_exact("nonexistent-subject", &blind_key);
    assert!(hits.is_empty());
}

#[test]
fn test_f04_blind_index_distinct_keys_uncorrelated() {
    let k1 = [0x11u8; 32];
    let k2 = [0x22u8; 32];
    let term = "sovereign_identity";
    let token1 = compute_blind_index(term, &k1);
    let token2 = compute_blind_index(term, &k2);
    assert_ne!(token1, token2);
}

// =============================================================================
// F05: Shamir's Secret Sharing (k-of-n)
// =============================================================================

#[test]
fn test_f05_shamir_3_of_5_happy_path_reconstruct() {
    let secret = [0x42u8; 32];
    let shares = split_secret(&secret, 3, 5).expect("split succeeds");
    assert_eq!(shares.len(), 5);

    // Reconstruct with first 3 shares
    let recovered = reconstruct_secret(&shares[0..3]).expect("reconstruction succeeds");
    assert_eq!(recovered, secret);
}

#[test]
fn test_f05_shamir_threshold_k_equals_total_n() {
    let secret = [0x99u8; 32];
    let shares = split_secret(&secret, 4, 4).unwrap();
    let recovered = reconstruct_secret(&shares).unwrap();
    assert_eq!(recovered, secret);
}

#[test]
fn test_f05_shamir_any_combination_of_k_shares_works() {
    let secret = [0x13u8; 32];
    let shares = split_secret(&secret, 3, 5).unwrap();

    // Subset {0, 2, 4}
    let subset1 = vec![shares[0].clone(), shares[2].clone(), shares[4].clone()];
    assert_eq!(reconstruct_secret(&subset1).unwrap(), secret);

    // Subset {1, 3, 4}
    let subset2 = vec![shares[1].clone(), shares[3].clone(), shares[4].clone()];
    assert_eq!(reconstruct_secret(&subset2).unwrap(), secret);
}

#[test]
fn test_f05_shamir_feldman_vss_commitment_verification() {
    let secret = [0xbeef_u16 as u8; 32];
    let shares = split_secret(&secret, 2, 3).unwrap();
    for share in &shares {
        assert!(share.commitment.iter().any(|&b| b != 0));
    }
}

#[test]
fn test_f05_shamir_tampered_share_fails_verification() {
    let secret = [0x33u8; 32];
    let mut shares = split_secret(&secret, 2, 3).unwrap();
    shares[0].data[0] ^= 0x55; // Tamper share 1

    let res = reconstruct_secret(&shares[0..2]);
    assert!(res.is_err(), "Corrupt share must fail commitment check");
}

// =============================================================================
// F06: Fuzzy Extractor Biometric Commitments
// =============================================================================

#[test]
fn test_f06_fuzzy_extractor_exact_match() {
    let fe = FuzzyExtractor::new(16);
    let sample = vec![0.5f32; 256];
    let (key, helper, commit) = fe.generate(&sample).expect("generation succeeds");

    let recovered_key = fe
        .reproduce(&sample, &helper, &commit)
        .expect("reproduction succeeds");
    assert_eq!(recovered_key, key);
}

#[test]
fn test_f06_fuzzy_extractor_noise_tolerance_within_delta() {
    let fe = FuzzyExtractor::new(16);
    let mut enrolled = vec![0.5f32; 256];
    let (key, helper, commit) = fe.generate(&enrolled).unwrap();

    // Perturb 3 coordinates (flips 3 bits in quantization)
    enrolled[0] = -0.5;
    enrolled[1] = -0.5;
    enrolled[2] = -0.5;

    let recovered = fe.reproduce(&enrolled, &helper, &commit).unwrap();
    assert_eq!(recovered, key);
}

#[test]
fn test_f06_fuzzy_extractor_commitment_hash_verification() {
    let fe = FuzzyExtractor::new(16);
    let sample = vec![0.2f32; 256];
    let (_key, helper, commit) = fe.generate(&sample).unwrap();
    assert_eq!(commit.0.len(), 32);
    assert_eq!(helper.salt.len(), 16);
}

#[test]
fn test_f06_fuzzy_extractor_syndrome_helper_data_integrity() {
    let fe = FuzzyExtractor::new(16);
    let sample = vec![0.4f32; 256];
    let (_key, mut helper, commit) = fe.generate(&sample).unwrap();

    // Corrupt helper syndrome across multiple repetitions to exceed error-correction capacity
    for i in (0..32).step_by(4) {
        helper.syndrome[i] ^= 0xff;
    }
    let res = fe.reproduce(&sample, &helper, &commit);
    assert!(res.is_err(), "Tampered helper data must fail commit check");
}

#[test]
fn test_f06_fuzzy_extractor_distinct_biometrics_produce_distinct_keys() {
    let fe = FuzzyExtractor::new(16);
    let b1 = vec![0.8f32; 256];
    let b2 = vec![-0.8f32; 256];

    let (k1, _, _) = fe.generate(&b1).unwrap();
    let (k2, _, _) = fe.generate(&b2).unwrap();
    assert_ne!(k1, k2);
}

// =============================================================================
// F07: SSI DIDs & Verifiable Credentials
// =============================================================================

#[test]
fn test_f07_did_key_ed25519_generation_and_multicodec_roundtrip() {
    let keypair = SigningKey::generate(&mut OsRng);
    let pk = keypair.verifying_key().to_bytes();

    let did = DidKey::from_ed25519_bytes(&pk);
    assert!(did.as_str().starts_with("did:key:z"));
    assert_eq!(did.method(), Some("key"));

    let recovered_pk = DidKey::extract_ed25519_bytes(&did).expect("extraction succeeds");
    assert_eq!(recovered_pk, pk);
}

#[test]
fn test_f07_did_peer2_format_and_extraction() {
    let sig_key = [0x11u8; 32];
    let enc_key = [0x22u8; 32];
    let peer_did = DidPeer::create_peer2(&sig_key, &enc_key);
    assert!(peer_did.as_str().starts_with("did:peer:2.V"));
    assert_eq!(peer_did.method(), Some("peer"));
}

#[test]
fn test_f07_w3c_vc_issue_and_verify_happy_path() {
    let issuer_key = SigningKey::generate(&mut OsRng);
    let now = chrono::Utc::now().timestamp();
    let claims = serde_json::json!({
        "subjectId": "did:key:zUser123",
        "role": "SecurityLead",
        "clearanceLevel": 3
    });

    let vc = VerifiableCredential::issue(
        "urn:uuid:vc-test-01",
        vec!["VerifiableCredential".into(), "HakilleRole".into()],
        &issuer_key,
        now - 10,
        now + 3600,
        claims,
    );

    assert!(vc.verify(now).is_ok());
}

#[test]
fn test_f07_w3c_vc_tampered_claims_fail_verification() {
    let issuer_key = SigningKey::generate(&mut OsRng);
    let now = chrono::Utc::now().timestamp();
    let claims = serde_json::json!({ "role": "Observer" });

    let mut vc = VerifiableCredential::issue(
        "urn:uuid:vc-test-02",
        vec!["VerifiableCredential".into()],
        &issuer_key,
        now - 10,
        now + 3600,
        claims,
    );

    // Attacker tampers role to Admin
    vc.credential_subject = serde_json::json!({ "role": "Admin" });
    assert!(vc.verify(now).is_err());
}

#[test]
fn test_f07_w3c_vc_expired_credential_rejected() {
    let issuer_key = SigningKey::generate(&mut OsRng);
    let now = chrono::Utc::now().timestamp();

    let vc = VerifiableCredential::issue(
        "urn:uuid:vc-test-03",
        vec!["VerifiableCredential".into()],
        &issuer_key,
        now - 3600,
        now - 60, // Expired 1 minute ago
        serde_json::json!({ "membership": "active" }),
    );

    let res = vc.verify(now);
    assert!(matches!(res, Err(hakille_core::HakilleError::CapabilityExpired)));
}

// =============================================================================
// F08: 5-Tier Domain Models
// =============================================================================

#[test]
fn test_f08_tier_team_model_and_agent_pool() {
    let did = Did::new("did:key:zTeamRoot");
    let config = TierConfiguration::Team(TeamConfig {
        max_members: 10,
        shared_agent_pool_enabled: true,
        working_memory_quota_mb: 256,
        auto_archive_days_inactive: Some(30),
    });

    let record = TenantEntityRecord::new(
        EntityTier::Team,
        "Cognitive Swarm Alpha",
        did,
        config,
        Some(TenantId::new()),
    )
    .expect("team creation succeeds");

    assert_eq!(record.tier, EntityTier::Team);
    assert_eq!(record.state, TenantState::Active);
}

#[test]
fn test_f08_tier_organization_scim_and_compliance() {
    let did = Did::new("did:key:zOrgRoot");
    let config = TierConfiguration::Organization(OrgConfig {
        registration_number: "FR-ORG-2026-X".into(),
        compliance_framework: vec!["GDPR".into(), "SOC2".into()],
        scim_endpoint: Some("https://idp.hakille.internal/scim/v2".into()),
        audit_retention_years: 10,
        requires_mfa: true,
    });

    let org = TenantEntityRecord::new(
        EntityTier::Organization,
        "Hakille R&D Core",
        did,
        config,
        None,
    )
    .expect("organization creation succeeds");

    assert_eq!(org.tier, EntityTier::Organization);
}

#[test]
fn test_f08_tier_enterprise_conglomerate_and_kms() {
    let did = Did::new("did:key:zEnterpriseRoot");
    let config = TierConfiguration::Enterprise(EnterpriseConfig {
        holding_legal_name: "AeroDynamics Global Holding".into(),
        cross_tenant_isolation_strict: true,
        master_kms_arn: Some("arn:aws:kms:eu-west-1:root-key".into()),
        global_constitutional_overrides: vec![],
    });

    let ent = TenantEntityRecord::new(
        EntityTier::Enterprise,
        "AeroDynamics Global",
        did,
        config,
        None,
    )
    .expect("enterprise root created");

    assert_eq!(ent.tier, EntityTier::Enterprise);
}

#[test]
fn test_f08_tier_family_domestic_safeguards_and_inheritance() {
    let did = Did::new("did:key:zFamilyRoot");
    let config = TierConfiguration::Family(FamilyConfig {
        domestic_hardware_dids: vec![Did::new("did:key:zSmartScreen")],
        minor_protection_strict: true,
        inheritance_vault_enabled: true,
        guest_auto_expiry_minutes: 1440,
    });

    let fam = TenantEntityRecord::new(
        EntityTier::Family,
        "Dupont Household",
        did,
        config,
        None,
    )
    .expect("family created");

    assert_eq!(fam.tier, EntityTier::Family);
}

#[test]
fn test_f08_tier_group_ephemeral_and_quorum() {
    let did = Did::new("did:key:zGroupRoot");
    let config = TierConfiguration::Group(GroupConfig {
        allow_public_invite: true,
        join_quorum_threshold: 0.66,
        ephemeral_ttl_seconds: Some(86400),
    });

    let grp = TenantEntityRecord::new(
        EntityTier::Group,
        "OpenSource Security Researchers",
        did,
        config,
        None,
    )
    .expect("group created");

    assert_eq!(grp.tier, EntityTier::Group);
}

// =============================================================================
// F09: Zanzibar ReBAC Engine
// =============================================================================

#[test]
fn test_f09_rebac_direct_relation_tuple() {
    let mut rebac = RebacEngine::new();
    rebac.add_tuple(RelationTuple::new(
        "team",
        "alpha",
        "member",
        "user",
        "alice",
    ));
    assert!(rebac.has_direct_relation("team", "alpha", "member", "user", "alice"));
    assert!(!rebac.has_direct_relation("team", "alpha", "member", "user", "bob"));
}

#[test]
fn test_f09_rebac_transitive_parent_of_traversal() {
    let mut rebac = RebacEngine::new();
    rebac.add_tuple(RelationTuple::new("enterprise", "acme", "parent_of", "org", "rnd"));
    rebac.add_tuple(RelationTuple::new("org", "rnd", "parent_of", "team", "telemetry"));
    rebac.add_tuple(RelationTuple::new("team", "telemetry", "member", "user", "carol"));

    assert!(rebac.check_relation_transitive("enterprise", "acme", "member", "user", "carol"));
}

#[test]
fn test_f09_rebac_userset_group_membership_indirection() {
    let mut rebac = RebacEngine::new();
    // workspace:notes#viewer@group:security#member
    rebac.add_tuple(RelationTuple::new("workspace", "notes", "viewer", "group", "security"));
    rebac.add_tuple(RelationTuple::new("group", "security", "member", "user", "david"));

    assert!(rebac.check_relation_transitive("workspace", "notes", "viewer", "user", "david"));
}

#[test]
fn test_f09_rebac_cycle_detection_terminates() {
    let mut rebac = RebacEngine::new();
    // Cycle: A -> B -> C -> A
    rebac.add_tuple(RelationTuple::new("org", "a", "parent_of", "org", "b"));
    rebac.add_tuple(RelationTuple::new("org", "b", "parent_of", "org", "c"));
    rebac.add_tuple(RelationTuple::new("org", "c", "parent_of", "org", "a"));

    // Traversal must terminate cleanly without stack overflow
    let res = rebac.check_relation_transitive("org", "a", "member", "user", "nonexistent");
    assert!(!res);
}

#[test]
fn test_f09_rebac_downward_containment_no_upward_leak() {
    let mut rebac = RebacEngine::new();
    rebac.add_tuple(RelationTuple::new("enterprise", "holding", "parent_of", "org", "sub1"));
    rebac.add_tuple(RelationTuple::new("enterprise", "holding", "admin", "user", "ciso"));

    // CISO is admin of enterprise; but sub1 member relation does not leak upwards to enterprise
    rebac.add_tuple(RelationTuple::new("org", "sub1", "member", "user", "junior_dev"));
    assert!(!rebac.check_relation_transitive("enterprise", "holding", "admin", "user", "junior_dev"));
}

// =============================================================================
// F10: Hybrid Access Control Engine
// =============================================================================

#[test]
fn test_f10_hybrid_engine_happy_path_all_agree() {
    let mut engine = AccessEngine::new();
    engine.rebac.add_tuple(RelationTuple::new("workspace", "doc1", "editor", "user", "alice"));

    let ctx = AuthContext {
        subject: "user:alice".into(),
        base_role: "Editor".into(),
        confidence_score: 0.85,
        device_tier: DeviceTrustTier::Tier1HardwareEnclave,
        tenant_id: TenantId::new(),
        scope: CapabilityScope::Filesystem,
        action: CapabilityAction::Write,
        resource: "doc1".into(),
    };

    let tuple = RelationTuple::new("workspace", "doc1", "editor", "user", "alice");
    assert!(engine.check_permission(&ctx, &tuple).is_allowed());
}

#[test]
fn test_f10_hybrid_rebac_missing_fails_closed() {
    let engine = AccessEngine::new();
    let ctx = AuthContext {
        subject: "user:bob".into(),
        base_role: "Editor".into(),
        confidence_score: 0.95,
        device_tier: DeviceTrustTier::Tier1HardwareEnclave,
        tenant_id: TenantId::new(),
        scope: CapabilityScope::Filesystem,
        action: CapabilityAction::Read,
        resource: "doc1".into(),
    };

    let tuple = RelationTuple::new("workspace", "doc1", "editor", "user", "bob");
    assert!(!engine.check_permission(&ctx, &tuple).is_allowed());
}

#[test]
fn test_f10_hybrid_rbac_guest_wallet_denied() {
    let mut engine = AccessEngine::new();
    engine.rebac.add_tuple(RelationTuple::new("wallet", "main", "spender", "user", "guest1"));

    let ctx = AuthContext {
        subject: "user:guest1".into(),
        base_role: "Guest".into(),
        confidence_score: 0.95,
        device_tier: DeviceTrustTier::Tier1HardwareEnclave,
        tenant_id: TenantId::new(),
        scope: CapabilityScope::Wallet,
        action: CapabilityAction::Execute,
        resource: "main".into(),
    };

    let tuple = RelationTuple::new("wallet", "main", "spender", "user", "guest1");
    let dec = engine.check_permission(&ctx, &tuple);
    assert!(!dec.is_allowed());
}

#[test]
fn test_f10_hybrid_abac_insufficient_confidence_denied() {
    let mut engine = AccessEngine::new();
    engine.rebac.add_tuple(RelationTuple::new("wallet", "main", "spender", "user", "alice"));

    let ctx = AuthContext {
        subject: "user:alice".into(),
        base_role: "Owner".into(),
        confidence_score: 0.60, // Below 0.90 required for Wallet
        device_tier: DeviceTrustTier::Tier1HardwareEnclave,
        tenant_id: TenantId::new(),
        scope: CapabilityScope::Wallet,
        action: CapabilityAction::Execute,
        resource: "main".into(),
    };

    let tuple = RelationTuple::new("wallet", "main", "spender", "user", "alice");
    assert!(!engine.check_permission(&ctx, &tuple).is_allowed());
}

#[test]
fn test_f10_hybrid_abac_untrusted_device_tier_denied() {
    let mut engine = AccessEngine::new();
    engine.rebac.add_tuple(RelationTuple::new("wallet", "main", "spender", "user", "alice"));

    let ctx = AuthContext {
        subject: "user:alice".into(),
        base_role: "Owner".into(),
        confidence_score: 0.95,
        device_tier: DeviceTrustTier::Tier3UntrustedWeb, // Untrusted device tier
        tenant_id: TenantId::new(),
        scope: CapabilityScope::Wallet,
        action: CapabilityAction::Execute,
        resource: "main".into(),
    };

    let tuple = RelationTuple::new("wallet", "main", "spender", "user", "alice");
    assert!(!engine.check_permission(&ctx, &tuple).is_allowed());
}

// =============================================================================
// F11: Biscuit v5 Capabilities
// =============================================================================

#[test]
fn test_f11_biscuit_mint_root_and_verify() {
    let mgr = BiscuitCapabilityManager::new();
    let now = chrono::Utc::now();
    let exp = Timestamp(now + chrono::Duration::hours(2));

    let token = mgr
        .mint_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            exp,
        )
        .expect("mint succeeds");

    assert!(mgr.verify_token(&token, CapabilityScope::Filesystem, CapabilityAction::Read).is_ok());
}

#[test]
fn test_f11_biscuit_offline_attenuation_scope_narrowing() {
    let mgr = BiscuitCapabilityManager::new();
    let now = chrono::Utc::now();
    let exp1 = Timestamp(now + chrono::Duration::hours(2));
    let exp2 = Timestamp(now + chrono::Duration::minutes(30));

    let root = mgr
        .mint_root_token(
            vec![CapabilityScope::Filesystem, CapabilityScope::ModelInference],
            vec![CapabilityAction::Read, CapabilityAction::Write],
            exp1,
        )
        .unwrap();

    let child = mgr
        .attenuate_token(
            &root,
            &[CapabilityScope::Filesystem, CapabilityScope::ModelInference],
            &[CapabilityAction::Read, CapabilityAction::Write],
            &[CapabilityScope::Filesystem],
            &[CapabilityAction::Read],
            exp2,
        )
        .expect("attenuation succeeds");

    assert!(mgr.verify_token(&child, CapabilityScope::Filesystem, CapabilityAction::Read).is_ok());
}

#[test]
fn test_f11_biscuit_temporal_monotonicity_enforcement() {
    let mgr = BiscuitCapabilityManager::new();
    let now = chrono::Utc::now();
    let exp_parent = Timestamp(now + chrono::Duration::minutes(30));
    let exp_child_future = Timestamp(now + chrono::Duration::hours(5));

    let root = mgr
        .mint_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            exp_parent,
        )
        .unwrap();

    // Attempting to grant child token a longer expiry than parent MUST fail
    let res = mgr.attenuate_token(
        &root,
        &[CapabilityScope::Filesystem],
        &[CapabilityAction::Read],
        &[CapabilityScope::Filesystem],
        &[CapabilityAction::Read],
        exp_child_future,
    );
    assert!(res.is_err());
}

#[test]
fn test_f11_biscuit_revocation_list_blocks_token() {
    let mut mgr = BiscuitCapabilityManager::new();
    let exp = Timestamp(chrono::Utc::now() + chrono::Duration::hours(1));

    let token = mgr
        .mint_root_token(
            vec![CapabilityScope::Audio],
            vec![CapabilityAction::Read],
            exp,
        )
        .unwrap();

    assert!(mgr.verify_token(&token, CapabilityScope::Audio, CapabilityAction::Read).is_ok());

    mgr.revoke_token(&token).unwrap();
    assert!(mgr.is_revoked(&token));
    assert!(mgr.verify_token(&token, CapabilityScope::Audio, CapabilityAction::Read).is_err());
}

#[test]
fn test_f11_biscuit_wrong_scope_fails_verification() {
    let mgr = BiscuitCapabilityManager::new();
    let exp = Timestamp(chrono::Utc::now() + chrono::Duration::hours(1));

    let token = mgr
        .mint_root_token(
            vec![CapabilityScope::Audio],
            vec![CapabilityAction::Read],
            exp,
        )
        .unwrap();

    // Verify against ungranted Network scope fails
    assert!(mgr.verify_token(&token, CapabilityScope::Network, CapabilityAction::Read).is_err());
}

// =============================================================================
// F12: Sovereign Agent Delegations
// =============================================================================

#[test]
fn test_f12_agent_spawn_did_and_passport_issue() {
    let (_, agent_did) = AgentFactory::spawn_agent();
    let principal_did = Did::new("did:key:zPrincipal");
    let tenant_id = TenantId::new();

    let passport = AgentFactory::issue_passport(
        agent_did.clone(),
        principal_did.clone(),
        tenant_id,
        3600,
        100000,
        500,
        vec![1, 2, 3],
        1,
    )
    .expect("passport issuance succeeds");

    assert_eq!(passport.agent_did, agent_did);
    assert_eq!(passport.delegation_depth, 1);
}

#[test]
fn test_f12_agent_sub_delegation_chain_depth_1_to_4() {
    let (_, root_agent) = AgentFactory::spawn_agent();
    let mut current_passport = AgentFactory::issue_passport(
        root_agent,
        Did::new("did:key:zHuman"),
        TenantId::new(),
        3600,
        100000,
        500,
        vec![1, 2, 3],
        1,
    )
    .unwrap();

    // Cascade delegations up to depth 4
    for _ in 2..=4 {
        let (_, next_agent) = AgentFactory::spawn_agent();
        current_passport = AgentFactory::delegate_sub_agent(
            &current_passport,
            next_agent,
            vec![1, 2, 3],
            current_passport.max_execution_fuel / 2,
            current_passport.budget_limit_cents / 2,
        )
        .expect("sub-delegation up to depth 4 must succeed");
    }
    assert_eq!(current_passport.delegation_depth, 4);
}

#[test]
fn test_f12_agent_fuel_and_budget_monotonicity() {
    let (_, agent1) = AgentFactory::spawn_agent();
    let passport = AgentFactory::issue_passport(
        agent1,
        Did::new("did:key:zRoot"),
        TenantId::new(),
        3600,
        50000,
        200,
        vec![],
        1,
    )
    .unwrap();

    let (_, child) = AgentFactory::spawn_agent();
    // Demanding more fuel than parent is rejected
    assert!(
        AgentFactory::delegate_sub_agent(&passport, child.clone(), vec![], 60000, 100).is_err()
    );
    // Demanding more budget than parent is rejected
    assert!(
        AgentFactory::delegate_sub_agent(&passport, child, vec![], 40000, 300).is_err()
    );
}

#[test]
fn test_f12_agent_constitutional_hash_integrity() {
    let (_, agent) = AgentFactory::spawn_agent();
    let passport = AgentFactory::issue_passport(
        agent,
        Did::new("did:key:zP"),
        TenantId::new(),
        3600,
        1000,
        100,
        vec![],
        1,
    )
    .unwrap();

    assert_eq!(passport.constitutional_hash.len(), 32);
    assert!(passport.constitutional_hash.iter().any(|&b| b != 0));
}

#[test]
fn test_f12_agent_minor_wallet_guard_rule7() {
    // Interacting with minor cannot grant Wallet scope
    let res = AgentFactory::enforce_minor_protection_invariant(true, &CapabilityScope::Wallet);
    assert!(res.is_err(), "Rule 7 violation must be blocked");

    // Interacting with adult can grant Wallet scope
    let res_adult =
        AgentFactory::enforce_minor_protection_invariant(false, &CapabilityScope::Wallet);
    assert!(res_adult.is_ok());

    // Interacting with minor can grant Audio scope
    let res_audio = AgentFactory::enforce_minor_protection_invariant(true, &CapabilityScope::Audio);
    assert!(res_audio.is_ok());
}

// =============================================================================
// F13: Cross-Tenant Firewalls
// =============================================================================

#[test]
fn test_f13_firewall_intra_tenant_always_allowed() {
    let mut fw = CrossTenantFirewall::new();
    let t1 = TenantId::new();
    fw.register_tenant(t1, EntityTier::Organization);
    assert!(fw.check_cross_tenant_access(&t1, &t1).is_ok());
}

#[test]
fn test_f13_firewall_cross_tenant_denied_by_default() {
    let mut fw = CrossTenantFirewall::new();
    let org1 = TenantId::new();
    let org2 = TenantId::new();
    fw.register_tenant(org1, EntityTier::Organization);
    fw.register_tenant(org2, EntityTier::Organization);

    assert!(fw.check_cross_tenant_access(&org1, &org2).is_err());
}

#[test]
fn test_f13_firewall_bilateral_grant_permits_inter_org() {
    let mut fw = CrossTenantFirewall::new();
    let org1 = TenantId::new();
    let org2 = TenantId::new();
    fw.register_tenant(org1, EntityTier::Organization);
    fw.register_tenant(org2, EntityTier::Organization);

    fw.grant_bilateral_delegation(org1, org2).expect("grant succeeds");
    assert!(fw.check_cross_tenant_access(&org1, &org2).is_ok());
    // Directional: org2 to org1 is still blocked without grant
    assert!(fw.check_cross_tenant_access(&org2, &org1).is_err());
}

#[test]
fn test_f13_firewall_personal_family_airgap_strictly_enforced() {
    let mut fw = CrossTenantFirewall::new();
    let family = TenantId::new();
    let enterprise = TenantId::new();
    fw.register_tenant(family, EntityTier::Family);
    fw.register_tenant(enterprise, EntityTier::Enterprise);

    // Attempting to bridge family to enterprise fails
    let grant_res = fw.grant_bilateral_delegation(family, enterprise);
    assert!(grant_res.is_err(), "Family sphere airgap must reject corporate delegation");

    assert!(fw.check_cross_tenant_access(&family, &enterprise).is_err());
}

#[test]
fn test_f13_firewall_unknown_tenant_rejected() {
    let fw = CrossTenantFirewall::new();
    let t1 = TenantId::new();
    let t2 = TenantId::new();
    assert!(fw.check_cross_tenant_access(&t1, &t2).is_err());
}

// =============================================================================
// F22: ZK Storage Schemas
// =============================================================================

#[test]
fn test_f22_zk_store_user_record_insert_and_blind_lookup() {
    let mut store = RedbZkStore::new();
    let blind_key = [0x44u8; 32];
    let username = "alice_sovereign";
    let blind_hash = compute_blind_index(username, &blind_key);

    let dummy_blob = XChaChaBlob {
        nonce: [0u8; 24],
        ciphertext: vec![1, 2, 3],
        tag: [0u8; 16],
        aad_fingerprint: [0u8; 16],
    };

    let user_record = UserRecord {
        user_id: UserId(Uuid::new_v4()),
        primary_did: Did::new("did:key:zAlice"),
        blind_username_hash: blind_hash,
        encrypted_profile_blob: dummy_blob,
        state: AccountStatus::Active,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    store.insert_user(user_record).unwrap();
    let found = store.lookup_user_by_blind_username(username, &blind_key);
    assert!(found.is_some());
    assert_eq!(found.unwrap().primary_did.as_str(), "did:key:zAlice");
}

#[test]
fn test_f22_zk_store_duplicate_blind_index_rejected() {
    let mut store = RedbZkStore::new();
    let blind_hash = [0xaa_u8; 32];
    let dummy_blob = XChaChaBlob {
        nonce: [0u8; 24],
        ciphertext: vec![],
        tag: [0u8; 16],
        aad_fingerprint: [0u8; 16],
    };

    let u1 = UserRecord {
        user_id: UserId(Uuid::new_v4()),
        primary_did: Did::new("did:key:z1"),
        blind_username_hash: blind_hash,
        encrypted_profile_blob: dummy_blob.clone(),
        state: AccountStatus::Active,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let u2 = UserRecord {
        user_id: UserId(Uuid::new_v4()),
        primary_did: Did::new("did:key:z2"),
        blind_username_hash: blind_hash,
        encrypted_profile_blob: dummy_blob,
        state: AccountStatus::Active,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    assert!(store.insert_user(u1).is_ok());
    assert!(store.insert_user(u2).is_err());
}

#[test]
fn test_f22_zk_store_recovery_share_persistence() {
    let mut store = RedbZkStore::new();
    let uid = UserId(Uuid::new_v4());

    let share_rec = RecoveryShareRecord {
        share_id: Uuid::new_v4(),
        user_id: uid.clone(),
        trustee_did: Did::new("did:key:zTrustee"),
        threshold: 3,
        total_shares: 5,
        share_index: 1,
        encrypted_share_blob: vec![10, 20, 30],
    };

    store.store_recovery_share(share_rec);
    let shares = store.get_recovery_shares_for_user(&uid);
    assert_eq!(shares.len(), 1);
    assert_eq!(shares[0].share_index, 1);
}

#[test]
fn test_f22_zk_store_spo_blind_token_query() {
    let mut store = RedbZkStore::new();
    let blind_sub = [0x55u8; 32];
    store.insert_blind_spo(blind_sub, vec![1, 2, 3]);
    store.insert_blind_spo(blind_sub, vec![4, 5, 6]);

    let res = store.query_blind_spo(&blind_sub);
    assert_eq!(res.len(), 2);
}

#[test]
fn test_f22_zk_store_digital_twilight_crypto_shred_trigger() {
    let mut store = RedbZkStore::new();
    let uid = UserId(Uuid::new_v4());
    let blind_key = [0x99u8; 32];
    let blind_hash = compute_blind_index("twilight_user", &blind_key);

    let dummy_blob = XChaChaBlob {
        nonce: [0u8; 24],
        ciphertext: vec![],
        tag: [0u8; 16],
        aad_fingerprint: [0u8; 16],
    };

    let user = UserRecord {
        user_id: uid.clone(),
        primary_did: Did::new("did:key:zTwilight"),
        blind_username_hash: blind_hash,
        encrypted_profile_blob: dummy_blob,
        state: AccountStatus::Active,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    store.insert_user(user).unwrap();
    store.trigger_digital_twilight(&uid).unwrap();

    assert!(store.is_salt_shredded(&uid));
    let u = store.lookup_user_by_blind_username("twilight_user", &blind_key).unwrap();
    assert_eq!(u.state, AccountStatus::DigitalTwilightGrace);
}
