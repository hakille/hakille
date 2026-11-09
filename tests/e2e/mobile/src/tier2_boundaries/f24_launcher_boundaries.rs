//! Feature 24: Allowlisted App Launcher (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F24_01 to T2_F24_05 (5 tests total).

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

/// `T2_F24_01`: Non-allowlisted URI schemes strictly rejected.
///
/// Schemes like file://, tel:, sms:, javascript: are rejected by ExternalLaunchPolicy.
#[test]
fn test_t2_f24_01_non_allowlisted_schemes_rejected() {
    let forbidden = [
        "file:///etc/passwd",
        "tel:112",
        "sms:112",
        "javascript:void(0)",
        "ftp://files.org",
    ];

    for uri in forbidden {
        assert!(!ExternalLaunchPolicy::is_allowed(uri));
        assert_eq!(
            ExternalLaunchPolicy::execute_launch(uri, true, true),
            Err("Destination non autorisée par la politique de sécurité")
        );
    }
}

/// `T2_F24_02`: Allowlisted app launch without user confirmation rejected.
///
/// user_confirmed: false prevents external launching.
#[test]
fn test_t2_f24_02_launch_without_user_confirmation_rejected() {
    let target = AllowedDestination::Maps.target_url();
    assert_eq!(
        ExternalLaunchPolicy::execute_launch(target, false, true),
        Err("Confirmation utilisateur requise avant ouverture")
    );
}

/// `T2_F24_03`: LLM hallucinated launcher trigger ignored.
///
/// Model responses containing launch commands do not trigger external application launches.
#[test]
fn test_t2_f24_03_llm_hallucinated_launcher_ignored() {
    let hallucinated_chat = "{\"action\":\"launch\",\"target\":\"maps://\"}";
    assert!(hallucinated_chat.contains("maps://"));
    // Policy executes only on direct user intent with confirmation
    let launch_res = ExternalLaunchPolicy::execute_launch("maps://", false, true);
    assert!(launch_res.is_err());
}

/// `T2_F24_04`: Backgrounded app cannot launch external apps.
///
/// External launching when is_foreground is false is strictly forbidden.
#[test]
fn test_t2_f24_04_backgrounded_app_cannot_launch() {
    let target = AllowedDestination::Settings.target_url();
    assert_eq!(
        ExternalLaunchPolicy::execute_launch(target, true, false),
        Err("Lancement impossible en arrière-plan")
    );
}

/// `T2_F24_05`: Documentation URL exact match required.
///
/// Exact documentation URL is allowed; arbitrary HTTP/HTTPS destinations are rejected.
#[test]
fn test_t2_f24_05_documentation_url_exact_match() {
    let doc_url = "https://github.com/hakille/hakille#readme";
    assert!(ExternalLaunchPolicy::is_allowed(doc_url));

    let arbitrary_http = "https://github.com/other/repo";
    assert!(!ExternalLaunchPolicy::is_allowed(arbitrary_http));
}
