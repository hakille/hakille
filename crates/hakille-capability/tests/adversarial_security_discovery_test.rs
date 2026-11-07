//! Adversarial security and discovery test suite for Milestone 2 Gate.
//!
//! Stress-tests:
//! 1. Anti-SSRF bypass attempts (IPv4-mapped IPv6, ULA, link-local, multicast, alternative hex representations, URLs).
//! 2. Biscuit v5 token tampering (signature corruption, forged root keys, expired tokens, scope/action mismatches).
//! 3. ActionCard 600ms physical hold verification (599ms boundary, 31s expiry, future timestamps, hash tampering).
//! 4. SecretVault zero-knowledge egress security (missing secrets, lifecycle drop, header sensitivity flags, invalid header smuggling).
//! 5. CapabilityDiscoveryEngine adversarial edge cases (zero/1 token budgets, prompt knapsack overflow, vector similarity anomalies, caller token expiry).
//! 6. Empirical defect verification tests documenting specific security and discovery bypass vectors for remediation.

use chrono::{Duration, Utc};
use semver::Version;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use hakille_capability::types::{CanonicalToolDefinition, CapabilityManifest, CapabilityType};
use hakille_capability::{
    ActionConfirmationProof, AntiSsrfValidator, BiscuitVerifier, CapabilityDiscoveryEngine,
    DiscoveryQuery, RiskVerificationError, SecretRef, SecretVault, SecretVaultError, SsrfError,
    UniversalCapabilityRegistry, cosine_similarity,
};
use hakille_core::{
    CapabilityAction, CapabilityAuthority, CapabilityScope, CapabilityToken, HakilleError,
    Timestamp,
};

// ============================================================================
// 1. Anti-SSRF Network Security Adversarial Suite
// ============================================================================

#[test]
fn test_ssrf_ipv4_mapped_ipv6_exhaustive() {
    // IPv4-mapped IPv6 representations of blocked subnets
    let test_cases = vec![
        // Loopback (127.0.0.0/8)
        ("::ffff:127.0.0.1", true),
        ("::ffff:127.0.0.254", true),
        ("::ffff:127.255.255.255", true),
        // RFC 1918 Private 10.0.0.0/8
        ("::ffff:10.0.0.1", true),
        ("::ffff:10.254.1.99", true),
        ("::ffff:10.255.255.255", true),
        // RFC 1918 Private 172.16.0.0/12
        ("::ffff:172.16.0.1", true),
        ("::ffff:172.24.10.5", true),
        ("::ffff:172.31.255.255", true),
        // RFC 1918 Private 192.168.0.0/16
        ("::ffff:192.168.0.1", true),
        ("::ffff:192.168.1.100", true),
        ("::ffff:192.168.255.255", true),
        // Carrier-Grade NAT 100.64.0.0/10
        ("::ffff:100.64.0.1", true),
        ("::ffff:100.100.50.2", true),
        ("::ffff:100.127.255.255", true),
        // Cloud Metadata & Link-Local (169.254.0.0/16)
        ("::ffff:169.254.169.254", true),
        ("::ffff:169.254.1.1", true),
        ("::ffff:169.254.255.254", true),
        // Current/This Network (0.0.0.0/8)
        ("::ffff:0.0.0.0", true),
        ("::ffff:0.1.2.3", true),
        // Multicast (224.0.0.0/4)
        ("::ffff:224.0.0.1", true),
        ("::ffff:239.255.255.255", true),
        // Reserved (240.0.0.0/4) and Broadcast
        ("::ffff:240.0.0.1", true),
        ("::ffff:250.1.2.3", true),
        ("::ffff:255.255.255.255", true),
        // Public IPs (MUST be allowed)
        ("::ffff:1.1.1.1", false),
        ("::ffff:8.8.8.8", false),
        ("::ffff:93.184.216.34", false),
    ];

    for (addr_str, should_block) in test_cases {
        let ip = IpAddr::V6(Ipv6Addr::from_str(addr_str).unwrap());
        let res = AntiSsrfValidator::validate_ip(ip);
        if should_block {
            assert!(
                res.is_err(),
                "Hostile IPv4-mapped address '{addr_str}' MUST be blocked, but was accepted"
            );
        } else {
            assert!(
                res.is_ok(),
                "Public IPv4-mapped address '{addr_str}' MUST be allowed, but got: {res:?}"
            );
        }
    }
}

#[test]
fn test_ssrf_ipv6_alternative_hex_representations() {
    // Full 8-group hexadecimal representations of IPv4-mapped IPv6 addresses
    let hex_cases = vec![
        // ::ffff:127.0.0.1 -> 0:0:0:0:0:ffff:7f00:1
        ("0:0:0:0:0:ffff:7f00:1", true),
        ("0000:0000:0000:0000:0000:ffff:7f00:0001", true),
        // ::ffff:169.254.169.254 -> 0:0:0:0:0:ffff:a9fe:a9fe
        ("0:0:0:0:0:ffff:a9fe:a9fe", true),
        ("0000:0000:0000:0000:0000:ffff:a9fe:a9fe", true),
        // ::ffff:10.0.0.1 -> 0:0:0:0:0:ffff:0a00:0001
        ("0:0:0:0:0:ffff:a00:1", true),
        ("0000:0000:0000:0000:0000:ffff:0a00:0001", true),
        // ::ffff:192.168.1.1 -> 0:0:0:0:0:ffff:c0a8:101
        ("0:0:0:0:0:ffff:c0a8:101", true),
        ("0000:0000:0000:0000:0000:ffff:c0a8:0101", true),
        // ::ffff:1.1.1.1 -> 0:0:0:0:0:ffff:0101:0101 (Public)
        ("0000:0000:0000:0000:0000:ffff:0101:0101", false),
    ];

    for (hex_str, should_block) in hex_cases {
        let ip = IpAddr::V6(Ipv6Addr::from_str(hex_str).unwrap());
        let res = AntiSsrfValidator::validate_ip(ip);
        if should_block {
            assert!(
                res.is_err(),
                "Hex representation '{hex_str}' MUST be blocked, but was accepted"
            );
        } else {
            assert!(
                res.is_ok(),
                "Hex representation '{hex_str}' MUST be allowed, but got: {res:?}"
            );
        }
    }
}

#[test]
fn test_ssrf_ipv6_ula_and_link_local_boundaries() {
    // Unique Local Addresses (fc00::/7 -> fc00.. and fd00..)
    let ula_cases = vec![
        "fc00::",
        "fc00::1",
        "fc00:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
        "fd00::",
        "fd00::1",
        "fd12:3456:789a:bcde:f012:3456:789a:bcde",
        "fdff:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
    ];

    for addr in ula_cases {
        let ip = IpAddr::V6(Ipv6Addr::from_str(addr).unwrap());
        let res = AntiSsrfValidator::validate_ip(ip);
        assert!(
            matches!(res, Err(SsrfError::PrivateNetworkBlocked(_))),
            "ULA IPv6 address '{addr}' must be blocked as private network, got: {res:?}"
        );
    }

    // Link-local unicast (fe80::/10 -> fe80.. to febf..)
    let link_local_cases = vec![
        "fe80::",
        "fe80::1",
        "fe80::a00:20ff:fea0:c99",
        "fe90::1",
        "fea0::1",
        "febf:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
    ];

    for addr in link_local_cases {
        let ip = IpAddr::V6(Ipv6Addr::from_str(addr).unwrap());
        let res = AntiSsrfValidator::validate_ip(ip);
        assert!(
            matches!(res, Err(SsrfError::LoopbackBlocked(_))),
            "Link-local IPv6 address '{addr}' must be blocked, got: {res:?}"
        );
    }

    // Multicast (ff00::/8)
    let multicast_cases = vec![
        "ff00::",
        "ff01::1",
        "ff02::1", // all nodes
        "ff02::2", // all routers
        "ff05::2",
        "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
    ];

    for addr in multicast_cases {
        let ip = IpAddr::V6(Ipv6Addr::from_str(addr).unwrap());
        let res = AntiSsrfValidator::validate_ip(ip);
        assert!(
            matches!(res, Err(SsrfError::ReservedAddressBlocked(_))),
            "Multicast IPv6 address '{addr}' must be blocked as reserved, got: {res:?}"
        );
    }
}

#[tokio::test]
async fn test_ssrf_resolve_and_validate_url_sanitization() {
    // Protocol prefixes and paths with blocked IPs
    let blocked_urls = vec![
        "http://127.0.0.1:8080/v1/internal",
        "https://127.0.0.1/admin",
        "http://169.254.169.254/latest/meta-data/",
        "http://10.0.0.1:8000/api",
        "https://192.168.1.1:8443/config",
        "http://172.16.5.1/metrics",
    ];

    for url in blocked_urls {
        let res = AntiSsrfValidator::resolve_and_validate(url).await;
        assert!(
            res.is_err(),
            "Blocked URL '{url}' must fail resolve_and_validate, but succeeded: {res:?}"
        );
    }

    // Public literal IPs with protocol and port
    let public_urls = vec![
        (
            "http://1.1.1.1:80/index.html",
            IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
        ),
        ("https://8.8.8.8", IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))),
    ];

    for (url, expected_ip) in public_urls {
        let res = AntiSsrfValidator::resolve_and_validate(url).await;
        assert!(
            res.is_ok(),
            "Public URL '{url}' should resolve successfully, got: {res:?}"
        );
        assert_eq!(res.unwrap(), expected_ip);
    }

    // Completely invalid or unresolvable host
    let invalid_res =
        AntiSsrfValidator::resolve_and_validate("nonexistent.unresolvable.hakille.invalid").await;
    assert!(
        matches!(invalid_res, Err(SsrfError::ResolutionFailed(_, _))),
        "Unresolvable host must fail with ResolutionFailed"
    );
}

// ============================================================================
// 2. Biscuit Token Tampering & Datalog Privilege Adversarial Suite
// ============================================================================

#[test]
fn test_biscuit_tampered_bytes_and_invalid_signatures() {
    let authority = CapabilityAuthority::new();
    let root_pub = authority.root_public_key();
    let future = Timestamp(Utc::now() + Duration::hours(2));

    let token = authority
        .create_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            future,
        )
        .unwrap();

    let valid_bytes = token.to_vec().unwrap();

    // Adversarial vector 1: Bit-flip mutation at varied offsets
    for offset in [0, 1, 5, 10, valid_bytes.len() / 2, valid_bytes.len() - 1] {
        let mut tampered = valid_bytes.clone();
        tampered[offset] ^= 0x55;

        let corrupted_token = CapabilityToken::from_vec(tampered, &root_pub);
        match corrupted_token {
            Ok(tok) => {
                let res = BiscuitVerifier::verify_token(
                    &tok,
                    &root_pub,
                    &CapabilityScope::Filesystem,
                    CapabilityAction::Read,
                );
                assert!(
                    res.is_err(),
                    "Bit-flipped token at offset {offset} must fail verification"
                );
            }
            Err(_) => {
                // Deserialization failure is also a secure fail-closed outcome
            }
        }
    }

    // Adversarial vector 2: Truncation
    for cut in [1, 5, 20, valid_bytes.len() / 2] {
        let truncated = valid_bytes[..valid_bytes.len() - cut].to_vec();
        let trunc_token = CapabilityToken::from_vec(truncated, &root_pub);
        match trunc_token {
            Ok(tok) => {
                let res = BiscuitVerifier::verify_token(
                    &tok,
                    &root_pub,
                    &CapabilityScope::Filesystem,
                    CapabilityAction::Read,
                );
                assert!(res.is_err(), "Truncated token must fail verification");
            }
            Err(_) => {
                // Expected fail-closed behavior
            }
        }
    }

    // Adversarial vector 3: Appended malicious payload
    let mut extended = valid_bytes.clone();
    extended.extend_from_slice(b"MALICIOUS_EXTRA_DATALOG_BLOCK");
    let ext_token = CapabilityToken::from_vec(extended, &root_pub);
    if let Ok(tok) = ext_token {
        let res = BiscuitVerifier::verify_token(
            &tok,
            &root_pub,
            &CapabilityScope::Filesystem,
            CapabilityAction::Read,
        );
        assert!(res.is_err(), "Appended token must fail verification");
    }
}

#[test]
fn test_biscuit_forged_root_key_and_privilege_escalation() {
    let sovereign_authority = CapabilityAuthority::new();
    let sovereign_root_pub = sovereign_authority.root_public_key();

    let rogue_authority = CapabilityAuthority::new();
    let future = Timestamp(Utc::now() + Duration::hours(1));

    // Rogue authority grants broad scopes and actions
    let forged_token = rogue_authority
        .create_root_token(
            vec![
                CapabilityScope::Filesystem,
                CapabilityScope::Network,
                CapabilityScope::FullSystem,
            ],
            vec![
                CapabilityAction::Read,
                CapabilityAction::Write,
                CapabilityAction::Execute,
                CapabilityAction::Admin,
            ],
            future,
        )
        .unwrap();

    // Verify against sovereign root public key
    let res = BiscuitVerifier::verify_token(
        &forged_token,
        &sovereign_root_pub,
        &CapabilityScope::FullSystem,
        CapabilityAction::Admin,
    );

    assert!(
        matches!(res, Err(HakilleError::CapabilityDenied(_))),
        "Forged root key MUST fail sovereign pinned verification"
    );
}

#[test]
fn test_biscuit_temporal_expiration_boundaries() {
    let authority = CapabilityAuthority::new();
    let root_pub = authority.root_public_key();

    // 10 seconds expired token (integer seconds resolution in Datalog timestamp)
    let past = Timestamp(Utc::now() - Duration::seconds(10));
    let expired_token = authority
        .create_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            past,
        )
        .unwrap();

    let res = BiscuitVerifier::verify_token(
        &expired_token,
        &root_pub,
        &CapabilityScope::Filesystem,
        CapabilityAction::Read,
    );

    assert!(
        matches!(res, Err(HakilleError::CapabilityExpired)),
        "Expired token must return CapabilityExpired, got: {res:?}"
    );
}

#[test]
fn test_biscuit_scope_and_action_mismatch_refusal() {
    let authority = CapabilityAuthority::new();
    let root_pub = authority.root_public_key();
    let future = Timestamp(Utc::now() + Duration::hours(1));

    // Token granted only: Filesystem + Read
    let token = authority
        .create_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            future,
        )
        .unwrap();

    // Attempt Write on Filesystem
    let res_write = BiscuitVerifier::verify_token(
        &token,
        &root_pub,
        &CapabilityScope::Filesystem,
        CapabilityAction::Write,
    );
    assert!(matches!(res_write, Err(HakilleError::CapabilityDenied(_))));

    // Attempt Execute on Filesystem
    let res_exec = BiscuitVerifier::verify_token(
        &token,
        &root_pub,
        &CapabilityScope::Filesystem,
        CapabilityAction::Execute,
    );
    assert!(matches!(res_exec, Err(HakilleError::CapabilityDenied(_))));

    // Attempt Read on Network
    let res_net = BiscuitVerifier::verify_token(
        &token,
        &root_pub,
        &CapabilityScope::Network,
        CapabilityAction::Read,
    );
    assert!(matches!(res_net, Err(HakilleError::CapabilityDenied(_))));

    // Attempt Read on FullSystem
    let res_admin = BiscuitVerifier::verify_token(
        &token,
        &root_pub,
        &CapabilityScope::FullSystem,
        CapabilityAction::Read,
    );
    assert!(matches!(res_admin, Err(HakilleError::CapabilityDenied(_))));
}

// ============================================================================
// 3. ActionCard 600ms Physical Hold & Proof Verification Adversarial Suite
// ============================================================================

#[test]
fn test_action_card_sub_600ms_physical_hold_boundary() {
    let args = serde_json::json!({"action": "wipe_database", "target": "all"});
    let action_hash = ActionConfirmationProof::compute_action_hash("db_wipe", &args);
    let now = Utc::now();

    // Exact sub-millisecond boundary: 599 ms (MUST be rejected)
    let proof_599 = ActionConfirmationProof::new(
        "card_edge",
        action_hash,
        "operator",
        599,
        now - Duration::seconds(1),
    );
    assert_eq!(
        proof_599.verify(&action_hash),
        Err(RiskVerificationError::InsufficientHoldDuration { held_ms: 599 })
    );

    // Other sub-600ms values
    for ms in [0, 1, 100, 300, 500, 598] {
        let proof = ActionConfirmationProof::new(
            "card_sub",
            action_hash,
            "operator",
            ms,
            now - Duration::seconds(1),
        );
        assert_eq!(
            proof.verify(&action_hash),
            Err(RiskVerificationError::InsufficientHoldDuration { held_ms: ms })
        );
    }

    // Exact boundary: 600 ms (MUST be accepted)
    let proof_600 = ActionConfirmationProof::new(
        "card_600",
        action_hash,
        "operator",
        600,
        now - Duration::seconds(1),
    );
    assert!(proof_600.verify(&action_hash).is_ok());

    // 601 ms (MUST be accepted)
    let proof_601 = ActionConfirmationProof::new(
        "card_601",
        action_hash,
        "operator",
        601,
        now - Duration::seconds(1),
    );
    assert!(proof_601.verify(&action_hash).is_ok());
}

#[test]
fn test_action_card_expiry_and_future_timestamp_manipulation() {
    let args = serde_json::json!({"action": "power_off"});
    let action_hash = ActionConfirmationProof::compute_action_hash("sys_poweroff", &args);
    let now = Utc::now();

    // Expired: 31 seconds old (limit is 30s)
    let proof_31s = ActionConfirmationProof::new(
        "card_exp",
        action_hash,
        "operator",
        700,
        now - Duration::seconds(31),
    );
    assert!(matches!(
        proof_31s.verify(&action_hash),
        Err(RiskVerificationError::ProofExpired { .. })
    ));

    // Expired: 60 seconds old
    let proof_60s = ActionConfirmationProof::new(
        "card_exp60",
        action_hash,
        "operator",
        700,
        now - Duration::seconds(60),
    );
    assert!(matches!(
        proof_60s.verify(&action_hash),
        Err(RiskVerificationError::ProofExpired { .. })
    ));

    // Future timestamp manipulation: 6 seconds in future (tolerance is 5s)
    let proof_future = ActionConfirmationProof::new(
        "card_future",
        action_hash,
        "operator",
        700,
        now + Duration::seconds(6),
    );
    assert_eq!(
        proof_future.verify(&action_hash),
        Err(RiskVerificationError::FutureTimestamp)
    );

    // Within tolerance future timestamp: 3 seconds in future
    let proof_skew = ActionConfirmationProof::new(
        "card_skew",
        action_hash,
        "operator",
        700,
        now + Duration::seconds(3),
    );
    assert!(proof_skew.verify(&action_hash).is_ok());
}

#[test]
fn test_action_card_hash_tampering_and_parameter_modification() {
    let legit_args = serde_json::json!({
        "recipient": "bob",
        "amount": 100
    });
    let legit_hash = ActionConfirmationProof::compute_action_hash("transfer", &legit_args);
    let now = Utc::now();

    let proof = ActionConfirmationProof::new(
        "card_hash",
        legit_hash,
        "operator",
        650,
        now - Duration::seconds(1),
    );

    // Vector 1: Altered amount in arguments
    let tampered_args_amount = serde_json::json!({
        "recipient": "bob",
        "amount": 1000000
    });
    let tampered_hash_1 =
        ActionConfirmationProof::compute_action_hash("transfer", &tampered_args_amount);
    assert!(matches!(
        proof.verify(&tampered_hash_1),
        Err(RiskVerificationError::HashMismatch { .. })
    ));

    // Vector 2: Altered tool name
    let tampered_hash_tool =
        ActionConfirmationProof::compute_action_hash("transfer_all", &legit_args);
    assert!(matches!(
        proof.verify(&tampered_hash_tool),
        Err(RiskVerificationError::HashMismatch { .. })
    ));

    // Vector 3: Additional injected key
    let tampered_args_extra = serde_json::json!({
        "recipient": "bob",
        "amount": 100,
        "memo": "hacked"
    });
    let tampered_hash_extra =
        ActionConfirmationProof::compute_action_hash("transfer", &tampered_args_extra);
    assert!(matches!(
        proof.verify(&tampered_hash_extra),
        Err(RiskVerificationError::HashMismatch { .. })
    ));
}

// ============================================================================
// 4. SecretVault Zero-Knowledge Egress Security Suite
// ============================================================================

#[tokio::test]
async fn test_secret_vault_adversarial_queries_and_lifecycle() {
    let vault = SecretVault::new();
    let mut headers = reqwest::header::HeaderMap::new();

    // 1. Access non-existent secret
    let non_existent = SecretRef::new("vault_non_existent");
    let err = vault
        .inject_authorization_header(&non_existent, &mut headers)
        .await;
    assert_eq!(
        err,
        Err(SecretVaultError::SecretNotFound(
            "vault_non_existent".to_string()
        ))
    );

    // 2. Store secret
    vault
        .store_secret("api_token", "super_secret_bearer_value".to_string())
        .await;
    assert!(vault.has_secret("api_token").await);

    // 3. Inject Bearer header and verify sensitivity
    let ok_inject = vault
        .inject_authorization_header(&SecretRef::new("api_token"), &mut headers)
        .await;
    assert!(ok_inject.is_ok());

    let header_val = headers.get(reqwest::header::AUTHORIZATION).unwrap();
    assert_eq!(
        header_val.to_str().unwrap(),
        "Bearer super_secret_bearer_value"
    );
    assert!(
        header_val.is_sensitive(),
        "Authorization header MUST have sensitive flag enabled"
    );

    // 4. Inject Custom header and verify sensitivity
    let mut custom_headers = reqwest::header::HeaderMap::new();
    let custom_name = reqwest::header::HeaderName::from_static("x-api-key");
    let ok_custom = vault
        .inject_header(
            &SecretRef::new("api_token"),
            custom_name.clone(),
            None,
            &mut custom_headers,
        )
        .await;
    assert!(ok_custom.is_ok());

    let custom_val = custom_headers.get(&custom_name).unwrap();
    assert_eq!(custom_val.to_str().unwrap(), "super_secret_bearer_value");
    assert!(
        custom_val.is_sensitive(),
        "Custom secret header MUST have sensitive flag enabled"
    );

    // 5. Remove secret -> verify lifecycle drop
    let removed = vault.remove_secret("api_token").await;
    assert!(removed, "Secret must be removed successfully");
    assert!(!vault.has_secret("api_token").await);

    // 6. Secondary remove returns false
    assert!(!vault.remove_secret("api_token").await);

    // 7. Injecting after removal fails
    let post_remove_err = vault
        .inject_authorization_header(&SecretRef::new("api_token"), &mut headers)
        .await;
    assert_eq!(
        post_remove_err,
        Err(SecretVaultError::SecretNotFound("api_token".to_string()))
    );
}

#[tokio::test]
async fn test_secret_vault_invalid_header_characters_injection() {
    let vault = SecretVault::new();
    let mut headers = reqwest::header::HeaderMap::new();

    // Adversarial vector: Secrets containing HTTP header smuggling delimiters (\r, \n, \0)
    let hostile_secrets = vec![
        ("newline_inject", "secret\nInjected-Header: evil.com"),
        ("crlf_inject", "secret\r\nInjected-Header: evil.com"),
        ("null_byte", "secret\0byte"),
    ];

    for (id, val) in hostile_secrets {
        vault.store_secret(id, val.to_string()).await;
        let res = vault
            .inject_authorization_header(&SecretRef::new(id), &mut headers)
            .await;
        assert!(
            matches!(res, Err(SecretVaultError::InvalidHeaderValue(_, _))),
            "Hostile secret containing control bytes must produce InvalidHeaderValue, got: {res:?}"
        );
    }
}

// ============================================================================
// 5. CapabilityDiscoveryEngine Adversarial Suite
// ============================================================================

fn make_tool_manifest(id: &str, tools: Vec<(&str, &str, serde_json::Value)>) -> CapabilityManifest {
    let mut manifest = CapabilityManifest::new(
        id,
        format!("Display {id}"),
        Version::parse("1.0.0").unwrap(),
        "Description",
        "Author",
        CapabilityType::Native,
    );
    for (name, desc, schema) in tools {
        manifest = manifest.with_tool(CanonicalToolDefinition::new(name, desc, schema));
    }
    manifest
}

#[test]
fn test_discovery_vector_similarity_edge_cases() {
    // 1. Empty vectors
    let empty: Vec<f32> = vec![];
    assert_eq!(cosine_similarity(&empty, &empty), 0.0);

    // 2. Dimension mismatch
    let v2 = vec![1.0, 2.0];
    let v3 = vec![1.0, 2.0, 3.0];
    assert_eq!(cosine_similarity(&v2, &v3), 0.0);

    // 3. Zero norm vectors
    let zero = vec![0.0, 0.0, 0.0];
    let nonzero = vec![1.0, 2.0, 3.0];
    assert_eq!(cosine_similarity(&zero, &nonzero), 0.0);
    assert_eq!(cosine_similarity(&zero, &zero), 0.0);

    // 4. Orthogonal vectors
    let v_x = vec![1.0, 0.0, 0.0];
    let v_y = vec![0.0, 1.0, 0.0];
    assert_eq!(cosine_similarity(&v_x, &v_y), 0.0);

    // 5. Antiparallel / Inverse vectors
    let v_pos = vec![1.0, 0.0];
    let v_neg = vec![-1.0, 0.0];
    assert!((cosine_similarity(&v_pos, &v_neg) - (-1.0)).abs() < 1e-6);

    // 6. Identical vectors
    assert!((cosine_similarity(&v_pos, &v_pos) - 1.0).abs() < 1e-6);

    // 7. High-dimensional vector (384-dim normalized)
    let h1 = vec![1.0 / (384.0f32).sqrt(); 384];
    let h2 = h1.clone();
    assert!((cosine_similarity(&h1, &h2) - 1.0).abs() < 1e-5);
}

#[tokio::test]
async fn test_discovery_prompt_knapsack_budget_overflow_and_extremes() {
    let registry = UniversalCapabilityRegistry::new();

    // Create a regular tool (~40 tokens)
    let standard_tool = (
        "fetch_records",
        "Fetch database records matching filter parameters",
        serde_json::json!({
            "type": "object",
            "properties": {
                "filter": { "type": "string" }
            }
        }),
    );

    // Create an extremely large tool declaration (several thousand tokens)
    let mut huge_properties = serde_json::Map::new();
    for i in 0..150 {
        huge_properties.insert(
            format!("adversarial_large_property_parameter_{i:04}"),
            serde_json::json!({
                "type": "string",
                "description": "Extremely verbose description designed to artificially inflate LLM prompt tokens"
            }),
        );
    }
    let huge_desc = "X".repeat(2000);
    let huge_tool_schema = serde_json::json!({
        "type": "object",
        "properties": huge_properties
    });
    let huge_tool = ("huge_enterprise_tool", huge_desc.as_str(), huge_tool_schema);

    let manifest = make_tool_manifest("hakille.stress", vec![standard_tool, huge_tool]);
    registry.register(manifest).await.unwrap();
    registry.activate("hakille.stress", None).await.unwrap();

    let engine = CapabilityDiscoveryEngine::new(registry);
    let embeddings = HashMap::new();

    // Case A: Query with budget allowing standard tool, but NOT the huge tool
    let query_modest = DiscoveryQuery::default().with_token_budget(80);
    let discovered_modest = engine.discover(&query_modest, &embeddings).await.unwrap();

    let names_modest: Vec<String> = discovered_modest.into_iter().map(|d| d.tool.name).collect();
    assert!(
        names_modest.contains(&"fetch_records".to_string()),
        "Standard tool must fit within budget 80"
    );
    assert!(
        !names_modest.contains(&"huge_enterprise_tool".to_string()),
        "Huge tool MUST NOT overflow modest token budget"
    );
}

#[tokio::test]
async fn test_discovery_caller_token_expiry_prefiltering() {
    let registry = UniversalCapabilityRegistry::new();
    let cap = make_tool_manifest(
        "hakille.authorized",
        vec![(
            "secure_tool",
            "Secure tool requiring valid token",
            serde_json::json!({"type": "object", "properties": {}}),
        )],
    );
    registry.register(cap).await.unwrap();
    registry.activate("hakille.authorized", None).await.unwrap();

    let engine = CapabilityDiscoveryEngine::new(registry);
    let embeddings = HashMap::new();

    // 1. Query with expired caller token
    let authority = CapabilityAuthority::new();
    let past = Timestamp(Utc::now() - Duration::hours(1));
    let expired_token = authority
        .create_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            past,
        )
        .unwrap();

    let query_expired = DiscoveryQuery {
        caller_token: Some(expired_token),
        ..Default::default()
    };

    let discovered = engine.discover(&query_expired, &embeddings).await.unwrap();
    assert!(
        discovered.is_empty(),
        "Discovery with expired caller token MUST pre-filter out all tools"
    );

    // 2. Query with valid active token
    let future = Timestamp(Utc::now() + Duration::hours(1));
    let valid_token = authority
        .create_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            future,
        )
        .unwrap();

    let query_valid = DiscoveryQuery {
        caller_token: Some(valid_token),
        ..Default::default()
    };

    let discovered_valid = engine.discover(&query_valid, &embeddings).await.unwrap();
    assert_eq!(
        discovered_valid.len(),
        1,
        "Discovery with valid caller token must discover active tool"
    );
}

// ============================================================================
// 6. Empirical Defect Verification Tests (Remediated Secure Behavior)
// ============================================================================

/// DEFECT 1 REMEDIATED: IPv4-compatible IPv6 addresses are strictly blocked by AntiSsrfValidator.
#[test]
fn test_adversarial_defect_ssrf_ipv4_compatible_ipv6_bypass() {
    let compat_loopback = IpAddr::V6(Ipv6Addr::from_str("::127.0.0.1").unwrap());
    let res = AntiSsrfValidator::validate_ip(compat_loopback);
    assert!(
        matches!(res, Err(SsrfError::LoopbackBlocked(_))),
        "IPv4-compatible loopback ::127.0.0.1 must be blocked, got: {res:?}"
    );

    let compat_meta = IpAddr::V6(Ipv6Addr::from_str("::169.254.169.254").unwrap());
    let res_meta = AntiSsrfValidator::validate_ip(compat_meta);
    assert!(
        matches!(res_meta, Err(SsrfError::CloudMetadataBlocked(_))),
        "IPv4-compatible metadata ::169.254.169.254 must be blocked, got: {res_meta:?}"
    );

    let compat_private = IpAddr::V6(Ipv6Addr::from_str("::10.0.0.1").unwrap());
    let res_private = AntiSsrfValidator::validate_ip(compat_private);
    assert!(
        matches!(res_private, Err(SsrfError::PrivateNetworkBlocked(_))),
        "IPv4-compatible private ::10.0.0.1 must be blocked, got: {res_private:?}"
    );
}

/// DEFECT 2 REMEDIATED: Prompt knapsack budget optimizer strictly enforces 0 token budget.
#[tokio::test]
async fn test_adversarial_defect_discovery_zero_budget_bypass() {
    let registry = UniversalCapabilityRegistry::new();
    let tool = (
        "budget_bypass_tool",
        "A sample tool requiring prompt tokens",
        serde_json::json!({
            "type": "object",
            "properties": {
                "param": { "type": "string" }
            }
        }),
    );
    let manifest = make_tool_manifest("hakille.budget_defect", vec![tool]);
    registry.register(manifest).await.unwrap();
    registry
        .activate("hakille.budget_defect", None)
        .await
        .unwrap();

    let engine = CapabilityDiscoveryEngine::new(registry);
    let embeddings = HashMap::new();

    let query_zero = DiscoveryQuery::default().with_token_budget(0);
    let discovered = engine.discover(&query_zero, &embeddings).await.unwrap();

    assert!(
        discovered.is_empty(),
        "Discovery with token_budget = 0 must return 0 tools to prevent prompt budget overflow, got {} tools",
        discovered.len()
    );
}

/// DEFECT 3 REMEDIATED: Host parsing correctly extracts bracketed IPv6 URLs and blocks loopback.
#[tokio::test]
async fn test_adversarial_defect_ssrf_bracketed_ipv6_host_resolution() {
    let bracketed_loopback_url = "http://[::ffff:127.0.0.1]:8080/foo";
    let res = AntiSsrfValidator::resolve_and_validate(bracketed_loopback_url).await;
    assert!(
        matches!(res, Err(SsrfError::LoopbackBlocked(_))),
        "Bracketed IPv4-mapped loopback URL must be blocked with LoopbackBlocked, got: {res:?}"
    );

    let bracketed_v6_url = "http://[::1]:8080/foo";
    let res_v6 = AntiSsrfValidator::resolve_and_validate(bracketed_v6_url).await;
    assert!(
        matches!(res_v6, Err(SsrfError::LoopbackBlocked(_))),
        "Bracketed IPv6 loopback URL must be blocked with LoopbackBlocked, got: {res_v6:?}"
    );

    let bracketed_compat_meta = "http://[::169.254.169.254]:8080/latest/meta-data";
    let res_meta = AntiSsrfValidator::resolve_and_validate(bracketed_compat_meta).await;
    assert!(
        matches!(res_meta, Err(SsrfError::CloudMetadataBlocked(_))),
        "Bracketed IPv4-compatible metadata URL must be blocked with CloudMetadataBlocked, got: {res_meta:?}"
    );
}

/// DEFECT 4 REMEDIATED: Cosine similarity returns 0.0 when inputs contain NaN or Infinity.
#[test]
fn test_adversarial_defect_cosine_similarity_nan_infinity_unhandled() {
    let v_nan = vec![f32::NAN, 1.0];
    let v_normal = vec![1.0, 1.0];
    let sim_nan = cosine_similarity(&v_nan, &v_normal);

    assert_eq!(
        sim_nan, 0.0,
        "cosine_similarity MUST return 0.0 when vector contains NaN, got {sim_nan}"
    );

    let v_inf = vec![f32::INFINITY, 1.0];
    let sim_inf = cosine_similarity(&v_inf, &v_normal);
    assert_eq!(
        sim_inf, 0.0,
        "cosine_similarity MUST return 0.0 when vector contains Infinity, got {sim_inf}"
    );

    let v_neg_inf = vec![f32::NEG_INFINITY, 1.0];
    let sim_neg_inf = cosine_similarity(&v_neg_inf, &v_normal);
    assert_eq!(
        sim_neg_inf, 0.0,
        "cosine_similarity MUST return 0.0 when vector contains NegInfinity, got {sim_neg_inf}"
    );
}
