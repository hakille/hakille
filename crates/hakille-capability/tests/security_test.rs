//! Comprehensive security engine test suite covering Biscuit authorization,
//! anti-SSRF address filters, 600ms ActionCard physical confirmation, and SecretVault.

use chrono::{Duration, Utc};
use hakille_capability::{
    ActionConfirmationProof, AntiSsrfValidator, BiscuitVerifier, RiskVerificationError, SecretRef,
    SecretVault, SecretVaultError, SsrfError,
};
use hakille_core::{
    CapabilityAction, CapabilityAuthority, CapabilityScope, HakilleError, Timestamp,
};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

#[test]
fn test_biscuit_verifier_authorization() {
    let authority = CapabilityAuthority::new();
    let root_pub = authority.root_public_key();

    let future = Timestamp(Utc::now() + Duration::hours(1));
    let token = authority
        .create_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            future.clone(),
        )
        .unwrap();

    // Valid check: Filesystem + Read
    let ok_res = BiscuitVerifier::verify_token(
        &token,
        &root_pub,
        &CapabilityScope::Filesystem,
        CapabilityAction::Read,
    );
    assert!(
        ok_res.is_ok(),
        "Legitimate token must authorize successfully"
    );

    // Mismatched Action: Filesystem + Write (only Read was granted)
    let bad_action = BiscuitVerifier::verify_token(
        &token,
        &root_pub,
        &CapabilityScope::Filesystem,
        CapabilityAction::Write,
    );
    assert!(
        matches!(bad_action, Err(HakilleError::CapabilityDenied(_))),
        "Mismatched action must fail with CapabilityDenied"
    );

    // Mismatched Scope: Network + Read (only Filesystem was granted)
    let bad_scope = BiscuitVerifier::verify_token(
        &token,
        &root_pub,
        &CapabilityScope::Network,
        CapabilityAction::Read,
    );
    assert!(
        matches!(bad_scope, Err(HakilleError::CapabilityDenied(_))),
        "Mismatched scope must fail with CapabilityDenied"
    );

    // Forged Root Key: An attacker creates their own token
    let attacker_authority = CapabilityAuthority::new();
    let forged_token = attacker_authority
        .create_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            future,
        )
        .unwrap();

    let forged_res = BiscuitVerifier::verify_token(
        &forged_token,
        &root_pub, // Pinned to legitimate root key
        &CapabilityScope::Filesystem,
        CapabilityAction::Read,
    );
    assert!(
        matches!(forged_res, Err(HakilleError::CapabilityDenied(_))),
        "Forged root key must fail verification against pinned root public key"
    );
}

#[test]
fn test_biscuit_verifier_expiration() {
    let authority = CapabilityAuthority::new();
    let root_pub = authority.root_public_key();

    // Expired in past
    let past = Timestamp(Utc::now() - Duration::minutes(10));
    let expired_token = authority
        .create_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            past,
        )
        .unwrap();

    let exp_res = BiscuitVerifier::verify_token(
        &expired_token,
        &root_pub,
        &CapabilityScope::Filesystem,
        CapabilityAction::Read,
    );
    assert!(
        matches!(exp_res, Err(HakilleError::CapabilityExpired)),
        "Expired token must be rejected with CapabilityExpired"
    );
}

#[test]
fn test_anti_ssrf_ipv4_blocked_ranges() {
    // 127.0.0.0/8 (Loopback)
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))),
        Err(SsrfError::LoopbackBlocked(_))
    ));
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V4(Ipv4Addr::new(127, 255, 0, 1))),
        Err(SsrfError::LoopbackBlocked(_))
    ));

    // 169.254.169.254 (Cloud instance metadata)
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254))),
        Err(SsrfError::CloudMetadataBlocked(_))
    ));

    // 169.254.0.0/16 (Link-local)
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1))),
        Err(SsrfError::LoopbackBlocked(_))
    ));

    // 10.0.0.0/8 (RFC 1918 Private)
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))),
        Err(SsrfError::PrivateNetworkBlocked(_))
    ));
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V4(Ipv4Addr::new(10, 254, 1, 1))),
        Err(SsrfError::PrivateNetworkBlocked(_))
    ));

    // 172.16.0.0/12 (RFC 1918 Private)
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))),
        Err(SsrfError::PrivateNetworkBlocked(_))
    ));
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V4(Ipv4Addr::new(172, 31, 255, 254))),
        Err(SsrfError::PrivateNetworkBlocked(_))
    ));

    // 192.168.0.0/16 (RFC 1918 Private)
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
        Err(SsrfError::PrivateNetworkBlocked(_))
    ));

    // 100.64.0.0/10 (Carrier-Grade NAT)
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1))),
        Err(SsrfError::PrivateNetworkBlocked(_))
    ));
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V4(Ipv4Addr::new(100, 127, 255, 1))),
        Err(SsrfError::PrivateNetworkBlocked(_))
    ));

    // 0.0.0.0/8 (Reserved)
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))),
        Err(SsrfError::ReservedAddressBlocked(_))
    ));

    // 224.0.0.0/4 (Multicast)
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V4(Ipv4Addr::new(224, 0, 0, 1))),
        Err(SsrfError::ReservedAddressBlocked(_))
    ));

    // 240.0.0.0/4 (Reserved) & Broadcast
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V4(Ipv4Addr::new(240, 0, 0, 1))),
        Err(SsrfError::ReservedAddressBlocked(_))
    ));
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255))),
        Err(SsrfError::ReservedAddressBlocked(_))
    ));
}

#[test]
fn test_anti_ssrf_ipv6_and_mapped_ranges() {
    // ::1 (Loopback)
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V6(Ipv6Addr::LOCALHOST)),
        Err(SsrfError::LoopbackBlocked(_))
    ));

    // :: (Unspecified)
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V6(Ipv6Addr::UNSPECIFIED)),
        Err(SsrfError::ReservedAddressBlocked(_))
    ));

    // fe80::/10 (Link-local)
    let fe80 = Ipv6Addr::from_str("fe80::1").unwrap();
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V6(fe80)),
        Err(SsrfError::LoopbackBlocked(_))
    ));

    // fc00::/7 (ULA)
    let fc00 = Ipv6Addr::from_str("fc00::1").unwrap();
    let fd12 = Ipv6Addr::from_str("fd12:3456:789a::1").unwrap();
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V6(fc00)),
        Err(SsrfError::PrivateNetworkBlocked(_))
    ));
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V6(fd12)),
        Err(SsrfError::PrivateNetworkBlocked(_))
    ));

    // ff00::/8 (Multicast)
    let ff02 = Ipv6Addr::from_str("ff02::1").unwrap();
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V6(ff02)),
        Err(SsrfError::ReservedAddressBlocked(_))
    ));

    // IPv4-mapped IPv6: ::ffff:127.0.0.1
    let mapped_loopback = Ipv6Addr::from_str("::ffff:127.0.0.1").unwrap();
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V6(mapped_loopback)),
        Err(SsrfError::LoopbackBlocked(_))
    ));

    // IPv4-mapped IPv6: ::ffff:169.254.169.254 (Cloud metadata)
    let mapped_meta = Ipv6Addr::from_str("::ffff:169.254.169.254").unwrap();
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V6(mapped_meta)),
        Err(SsrfError::CloudMetadataBlocked(_))
    ));

    // IPv4-mapped IPv6: ::ffff:10.0.0.1 (Private)
    let mapped_private = Ipv6Addr::from_str("::ffff:10.0.0.1").unwrap();
    assert!(matches!(
        AntiSsrfValidator::validate_ip(IpAddr::V6(mapped_private)),
        Err(SsrfError::PrivateNetworkBlocked(_))
    ));
}

#[test]
fn test_anti_ssrf_valid_public_addresses() {
    let cloudflare_dns = IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1));
    let google_dns = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));
    let google_v6 = IpAddr::V6(Ipv6Addr::from_str("2607:f8b0:4005:805::200e").unwrap());

    assert!(AntiSsrfValidator::validate_ip(cloudflare_dns).is_ok());
    assert!(AntiSsrfValidator::validate_ip(google_dns).is_ok());
    assert!(AntiSsrfValidator::validate_ip(google_v6).is_ok());
}

#[tokio::test]
async fn test_anti_ssrf_resolve_and_validate() {
    let loopback_res = AntiSsrfValidator::resolve_and_validate("127.0.0.1").await;
    assert!(matches!(loopback_res, Err(SsrfError::LoopbackBlocked(_))));

    let meta_res = AntiSsrfValidator::resolve_and_validate("169.254.169.254").await;
    assert!(matches!(meta_res, Err(SsrfError::CloudMetadataBlocked(_))));

    let public_res = AntiSsrfValidator::resolve_and_validate("8.8.8.8").await;
    assert!(public_res.is_ok());
    assert_eq!(public_res.unwrap(), IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)));
}

#[test]
fn test_action_card_physical_hold_verification() {
    let args = serde_json::json!({
        "command": "rm -rf /var/log/*"
    });
    let action_hash = ActionConfirmationProof::compute_action_hash("exec_shell", &args);

    let now = Utc::now();

    // 1. Valid hold: 650 ms, recent timestamp, matching hash
    let valid_proof = ActionConfirmationProof::new(
        "card_001",
        action_hash,
        "user_alice",
        650,
        now - Duration::seconds(2),
    );
    assert!(valid_proof.verify(&action_hash).is_ok());

    // 2. Insufficient hold duration (< 600 ms)
    let short_proof = ActionConfirmationProof::new(
        "card_002",
        action_hash,
        "user_alice",
        500,
        now - Duration::seconds(2),
    );
    assert_eq!(
        short_proof.verify(&action_hash),
        Err(RiskVerificationError::InsufficientHoldDuration { held_ms: 500 })
    );

    // 3. Expired confirmation (> 30s)
    let expired_proof = ActionConfirmationProof::new(
        "card_003",
        action_hash,
        "user_alice",
        700,
        now - Duration::seconds(35),
    );
    assert!(matches!(
        expired_proof.verify(&action_hash),
        Err(RiskVerificationError::ProofExpired { .. })
    ));

    // 4. Hash mismatch (e.g. arguments modified after confirmation)
    let tampered_hash = [0x55u8; 32];
    assert!(matches!(
        valid_proof.verify(&tampered_hash),
        Err(RiskVerificationError::HashMismatch { .. })
    ));
}

#[tokio::test]
async fn test_secret_vault_egress_injection() {
    let vault = SecretVault::new();
    let secret_ref = SecretRef::new("github_api_token");

    // Initially secret does not exist
    let mut headers = reqwest::header::HeaderMap::new();
    let missing_res = vault
        .inject_authorization_header(&secret_ref, &mut headers)
        .await;
    assert_eq!(
        missing_res,
        Err(SecretVaultError::SecretNotFound(
            "github_api_token".to_string()
        ))
    );

    // Store secret
    vault
        .store_secret("github_api_token", "ghp_secure_123456789".to_string())
        .await;
    assert!(vault.has_secret("github_api_token").await);

    // Inject Bearer header
    let inject_res = vault
        .inject_authorization_header(&secret_ref, &mut headers)
        .await;
    assert!(inject_res.is_ok());

    let auth_header = headers.get(reqwest::header::AUTHORIZATION).unwrap();
    assert_eq!(auth_header.to_str().unwrap(), "Bearer ghp_secure_123456789");
    assert!(
        auth_header.is_sensitive(),
        "Header must be marked sensitive"
    );

    // Remove secret
    assert!(vault.remove_secret("github_api_token").await);
    assert!(!vault.has_secret("github_api_token").await);
}
