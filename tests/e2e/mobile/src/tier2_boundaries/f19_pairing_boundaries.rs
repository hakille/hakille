//! Feature 19: Gateway Pairing & Credentials (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F19_01 to T2_F19_05 (5 tests total).

#![allow(unused_imports)]

use crate::harness::{CheckpointHarness, MobileTestRig};
use crate::tier1_features::f21_bounded_camera::{
    MAX_LONG_EDGE_PX, MAX_PHOTO_BYTES, MAX_TOTAL_PIXELS, validate_camera_frame,
};
use crate::tier1_features::f22_bounded_doc_picker::{
    DOCUMENT_READ_TIMEOUT_SECS, MAX_DOCUMENT_BYTES, MAX_PREVIEW_CHARS, inspect_document_bytes,
};
use crate::tier1_features::f23_single_shot_gps::PhoneLocation;
use crate::tier1_features::f24_allowlisted_launcher::{AllowedDestination, ExternalLaunchPolicy};
use chrono::Utc;
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_mobile_gateway::{BearerToken, read_private_token};
use hakille_ui_protocol::UiToKernel;
use reqwest::header::{AUTHORIZATION, ORIGIN};
use serde_json::json;
use std::fs::File;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// `T2_F19_01`: Bearer token with invalid hex characters.
///
/// BearerToken::parse rejects strings containing non-hex characters.
#[test]
fn test_t2_f19_01_bearer_token_invalid_hex_rejected() {
    let non_hex_1 = "g123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    assert!(BearerToken::parse(non_hex_1).is_err());

    let non_hex_2 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdeZ";
    assert!(BearerToken::parse(non_hex_2).is_err());

    let non_hex_symbols = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde!";
    assert!(BearerToken::parse(non_hex_symbols).is_err());
}

/// `T2_F19_02`: Token length boundary: 63 chars and 65 chars rejected.
///
/// Exactly 64 chars required; 63 chars and 65 chars are strictly rejected.
#[test]
fn test_t2_f19_02_bearer_token_length_boundaries() {
    let exact_64 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    assert!(BearerToken::parse(exact_64).is_ok());

    // 63 chars
    let short_63 = &exact_64[..63];
    assert_eq!(short_63.len(), 63);
    assert!(BearerToken::parse(short_63).is_err());

    // 65 chars
    let long_65 = format!("{exact_64}0");
    assert_eq!(long_65.len(), 65);
    assert!(BearerToken::parse(&long_65).is_err());
}

/// `T2_F19_03`: Token file with 0644/0666 permissions rejected.
///
/// On Unix, read_private_token verifies mode 0600 and rejects open permissions (0644, 0666).
#[test]
fn test_t2_f19_03_token_file_open_permissions_rejected() {
    let temp_dir = tempfile::tempdir().unwrap();
    let token_content = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[cfg(unix)]
    {
        // 0644 file
        let path_644 = temp_dir.path().join("token_644");
        let mut f = File::create(&path_644).unwrap();
        f.write_all(token_content.as_bytes()).unwrap();
        std::fs::set_permissions(&path_644, std::fs::Permissions::from_mode(0o644)).unwrap();

        let res_644 = read_private_token(&path_644);
        assert!(res_644.is_err(), "0644 token file must be rejected");
        assert!(res_644.unwrap_err().to_string().contains("permissions"));

        // 0666 file
        let path_666 = temp_dir.path().join("token_666");
        let mut f2 = File::create(&path_666).unwrap();
        f2.write_all(token_content.as_bytes()).unwrap();
        std::fs::set_permissions(&path_666, std::fs::Permissions::from_mode(0o666)).unwrap();

        let res_666 = read_private_token(&path_666);
        assert!(res_666.is_err(), "0666 token file must be rejected");
    }
}

/// `T2_F19_04`: Origin header present rejected with HTTP 403 origin_refused.
///
/// Native isolation contract: gateway rejects any request containing an Origin header.
#[tokio::test]
async fn test_t2_f19_04_origin_header_rejected_with_403() {
    let (rig, _daemon) = MobileTestRig::setup().await;

    let origins = [
        "http://localhost:3000",
        "https://evil-browser-site.com",
        "null",
        "file://",
    ];

    for origin in origins {
        let resp = rig
            .client
            .post(format!("{}/v1/request", rig.base_url))
            .header(ORIGIN, origin)
            .header(AUTHORIZATION, format!("Bearer {}", rig.mobile_bearer))
            .json(&Frame::request(1, UiToKernel::StatusRequest))
            .send()
            .await
            .expect("send request");

        assert_eq!(
            resp.status().as_u16(),
            403,
            "Origin {origin} must be rejected with 403"
        );
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["error"]["code"], "origin_refused");
    }
}

/// `T2_F19_05`: URI query parameters rejected with HTTP 400 bad_target.
///
/// Requests containing query strings or full URIs are rejected with HTTP 400 bad_target.
#[tokio::test]
async fn test_t2_f19_05_uri_query_parameters_rejected_with_400() {
    let (rig, _daemon) = MobileTestRig::setup().await;

    let bad_urls = [
        format!("{}/v1/request?token=secret", rig.base_url),
        format!("{}/v1/request?debug=true", rig.base_url),
        format!("{}/health?format=json", rig.base_url),
    ];

    for bad_url in bad_urls {
        let resp = rig
            .client
            .post(&bad_url)
            .header(AUTHORIZATION, format!("Bearer {}", rig.mobile_bearer))
            .json(&Frame::request(1, UiToKernel::StatusRequest))
            .send()
            .await
            .expect("send request");

        assert_eq!(
            resp.status().as_u16(),
            400,
            "Query params on {bad_url} must return 400"
        );
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["error"]["code"], "bad_target");
    }
}
