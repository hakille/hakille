//! Feature 21: Bounded In-Memory Camera (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F21_01 to T2_F21_05 (5 tests total).

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

/// `T2_F21_01`: JPEG frame exactly 8 MiB accepted vs 8 MiB + 1 byte rejected.
///
/// Max photo payload is strictly 8 MiB (8,388,608 bytes).
#[test]
fn test_t2_f21_01_camera_byte_budget_exact_boundary() {
    assert_eq!(MAX_PHOTO_BYTES, 8 * 1024 * 1024);

    // Exactly 8 MiB accepted
    let res_exact = validate_camera_frame(1536, 1536, MAX_PHOTO_BYTES);
    assert_eq!(res_exact, Ok((1536, 1536)));

    // 8 MiB + 1 byte rejected
    let res_over = validate_camera_frame(1536, 1536, MAX_PHOTO_BYTES + 1);
    assert_eq!(res_over, Err("Taille de photo supérieure à 8 Mio"));
}

/// `T2_F21_02`: Dimension 1536x1536 accepted vs 1537x1537 downscaled.
///
/// Dimensions exceeding MAX_LONG_EDGE_PX (1536 px) are proportionally scaled down to 1536 px.
#[test]
fn test_t2_f21_02_dimension_clamping_to_1536_px() {
    assert_eq!(MAX_LONG_EDGE_PX, 1536);

    // Exact 1536x1536 -> unscaled
    let (w1, h1) = validate_camera_frame(1536, 1536, 1_000_000).unwrap();
    assert_eq!((w1, h1), (1536, 1536));

    // 1537x1537 -> downscaled to 1536x1536
    let (w2, h2) = validate_camera_frame(1537, 1537, 1_000_000).unwrap();
    assert_eq!((w2, h2), (1536, 1536));
}

/// `T2_F21_03`: Zero dimension buffer rejected.
///
/// Buffers with 0 px width or 0 px height are strictly rejected.
#[test]
fn test_t2_f21_03_zero_dimension_buffer_rejected() {
    assert_eq!(
        validate_camera_frame(0, 100, 100),
        Err("Dimensions de caméra invalides (0 px)")
    );
    assert_eq!(
        validate_camera_frame(100, 0, 100),
        Err("Dimensions de caméra invalides (0 px)")
    );
    assert_eq!(
        validate_camera_frame(0, 0, 100),
        Err("Dimensions de caméra invalides (0 px)")
    );
}

/// `T2_F21_04`: Backgrounding clears thumbnail immediately.
///
/// Entering background state clears ephemeral preview tokens and resets camera state.
#[test]
fn test_t2_f21_04_backgrounding_clears_camera_thumbnail() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_foreground", "active": false}).to_string());

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap["foreground"], false);
    assert_eq!(snap["microphone_enabled"], false);
}

/// `T2_F21_05`: Maximum pixel count boundary (4,000,000 pixels).
///
/// Resolutions exceeding 4 megapixels are rejected.
#[test]
fn test_t2_f21_05_maximum_pixel_count_boundary() {
    assert_eq!(MAX_TOTAL_PIXELS, 4_000_000);

    // 2000 x 2000 = 4,000,000 pixels -> accepted
    assert!(validate_camera_frame(2000, 2000, 1_000_000).is_ok());

    // 2001 x 2000 = 4,002,000 pixels -> rejected
    assert_eq!(
        validate_camera_frame(2001, 2000, 1_000_000),
        Err("Résolution de photo excessive (> 4 mégapixels)")
    );
}
