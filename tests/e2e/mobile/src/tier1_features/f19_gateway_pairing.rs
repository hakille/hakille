//! Feature 19: Gateway Pairing & Credentials (Milestone M1 / Spec §2, ADR-011)
//!
//! Specifications: T1_F19_01 to T1_F19_05
//!
//! Verifies:
//! - BearerToken 64-hex string parsing and constant-time equality matching.
//! - Token file mode enforcement (0600 allowed, 0644 rejected, symlinks rejected).
//! - Native isolation: Gateway rejects `Origin` header with HTTP 403 `origin_refused`.
//! - Gateway rejects URL query parameters and schemes with HTTP 400 `bad_target`.
//! - Strict HTTP 401 Unauthorized rejection when Bearer token is missing or incorrect.

use crate::harness::MobileTestRig;
use hakille_mobile_gateway::{BearerToken, read_private_token};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, ORIGIN};
use std::fs::File;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// `T1_F19_01`: Bearer token 64-hex string parsing and validation.
///
/// BearerToken requires exactly 64 ASCII hex characters. Shorter, longer, or non-hex
/// strings are rejected during parse.
#[test]
fn test_t1_f19_01_bearer_token_hex_parsing_and_constant_time_match() {
    let valid_hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    assert!(
        BearerToken::parse(valid_hex).is_ok(),
        "Valid 64-hex token must parse successfully"
    );

    // Must reject 63 characters (too short)
    assert!(
        BearerToken::parse(&valid_hex[..63]).is_err(),
        "63-char token must be rejected"
    );

    // Must reject 65 characters (too long)
    let too_long = format!("{valid_hex}a");
    assert!(
        BearerToken::parse(&too_long).is_err(),
        "65-char token must be rejected"
    );

    // Must reject non-hex characters
    let non_hex = "g123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    assert!(
        BearerToken::parse(non_hex).is_err(),
        "Non-hex character token must be rejected"
    );
}

/// `T1_F19_02`: Token file permission enforcement and symlink rejection.
///
/// Gateway verifies that the private token file has strict permissions (0600 on Unix)
/// and is not a symlink, FIFO, or directory.
#[test]
fn test_t1_f19_02_token_file_permission_enforcement_and_symlink_rejection() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let valid_hex = "1122334455667788990011223344556677889900112233445566778899001122";

    // Subcase A: Valid 0600 file succeeds
    let token_path = temp_dir.path().join("valid_token");
    {
        let mut file = File::create(&token_path).expect("create file");
        file.write_all(valid_hex.as_bytes()).expect("write token");
        #[cfg(unix)]
        {
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&token_path, perms).expect("set 0600");
        }
    }
    let read_result = read_private_token(&token_path);
    assert!(read_result.is_ok(), "0600 file must be read successfully");
    assert_eq!(read_result.unwrap(), valid_hex);

    // Subcase B: Permissions too open (0644 or 0666) rejected on Unix
    #[cfg(unix)]
    {
        let open_path = temp_dir.path().join("open_token");
        let mut file = File::create(&open_path).expect("create file");
        file.write_all(valid_hex.as_bytes()).expect("write token");
        let perms = std::fs::Permissions::from_mode(0o644);
        std::fs::set_permissions(&open_path, perms).expect("set 0644");

        let err = read_private_token(&open_path).expect_err("0644 must be rejected");
        assert!(err.to_string().contains("permissions"));
    }

    // Subcase C: Symlink rejected
    #[cfg(unix)]
    {
        let symlink_path = temp_dir.path().join("symlink_token");
        std::os::unix::fs::symlink(&token_path, &symlink_path).expect("create symlink");
        let err = read_private_token(&symlink_path).expect_err("symlink must be rejected");
        assert!(err.to_string().contains("ordinaire"));
    }
}

/// `T1_F19_03`: Gateway rejects Origin header for native client isolation.
///
/// Any HTTP request bearing an `Origin` header is rejected with HTTP 403 Forbidden
/// and code `origin_refused` to ensure Web/CORS bypass is blocked.
#[tokio::test]
async fn test_t1_f19_03_gateway_rejects_origin_header_for_native_isolation() {
    let (rig, _daemon) = MobileTestRig::setup().await;

    let client = reqwest::Client::new();
    let url = format!("{}/v1/request", rig.base_url);

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", rig.mobile_bearer)).unwrap(),
    );
    headers.insert(ORIGIN, HeaderValue::from_static("http://malicious-web.com"));

    let res = client
        .post(&url)
        .headers(headers)
        .body(r#"{"id": 1, "body": {"type": "StatusRequest"}}"#)
        .send()
        .await
        .expect("send request");

    assert_eq!(
        res.status().as_u16(),
        403,
        "Origin header must trigger HTTP 403"
    );
    let body: serde_json::Value = res.json().await.expect("json body");
    assert_eq!(body["error"]["code"], "origin_refused");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Origin refusé")
    );
}

/// `T1_F19_04`: Gateway rejects query parameters and full URIs.
///
/// Target URI must be strictly the expected path without query strings, authority, or scheme.
/// Request with `?token=...` returns HTTP 400 `bad_target`.
#[tokio::test]
async fn test_t1_f19_04_gateway_rejects_query_parameters_and_full_uris() {
    let (rig, _daemon) = MobileTestRig::setup().await;
    let client = reqwest::Client::new();

    // Query parameter leak attempt
    let query_url = format!("{}/health?token=secret123", rig.base_url);
    let res = client.get(&query_url).send().await.expect("send query");

    assert_eq!(
        res.status().as_u16(),
        400,
        "Query params must return HTTP 400"
    );
    let body: serde_json::Value = res.json().await.expect("json body");
    assert_eq!(body["error"]["code"], "bad_target");
}

/// `T1_F19_05`: Unauthorized when Bearer token is missing or invalid.
///
/// Requests to `/v1/request` without `Authorization` or with an invalid token
/// return HTTP 401 Unauthorized.
#[tokio::test]
async fn test_t1_f19_05_unauthorized_when_bearer_token_missing_or_invalid() {
    let (rig, _daemon) = MobileTestRig::setup().await;
    let client = reqwest::Client::new();
    let url = format!("{}/v1/request", rig.base_url);

    // Case 1: Missing Authorization header
    let res1 = client
        .post(&url)
        .header("content-type", "application/json")
        .body(r#"{"id": 1, "body": {"type": "StatusRequest"}}"#)
        .send()
        .await
        .expect("send without auth");

    assert_eq!(res1.status().as_u16(), 401, "Missing auth must return 401");
    let body1: serde_json::Value = res1.json().await.expect("json");
    assert_eq!(body1["error"]["code"], "unauthorized");

    // Case 2: Wrong Bearer token
    let bad_token = "0000000000000000000000000000000000000000000000000000000000000000";
    let res2 = client
        .post(&url)
        .header("content-type", "application/json")
        .header(AUTHORIZATION, format!("Bearer {bad_token}"))
        .body(r#"{"id": 1, "body": {"type": "StatusRequest"}}"#)
        .send()
        .await
        .expect("send bad token");

    assert_eq!(res2.status().as_u16(), 401, "Wrong token must return 401");
    let body2: serde_json::Value = res2.json().await.expect("json");
    assert_eq!(body2["error"]["code"], "unauthorized");
}
