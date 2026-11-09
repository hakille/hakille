//! Feature 22: Bounded Document Picker (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F22_01 to T2_F22_05 (5 tests total).

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

/// `T2_F22_01`: File exactly 8 MiB accepted vs > 8 MiB rejected.
///
/// Max document size is 8 MiB (8,388,608 bytes).
#[test]
fn test_t2_f22_01_document_size_boundary_8_mib() {
    assert_eq!(MAX_DOCUMENT_BYTES, 8 * 1024 * 1024);

    let exact_data = vec![b'A'; MAX_DOCUMENT_BYTES];
    assert!(inspect_document_bytes(&exact_data, true).is_ok());

    let over_data = vec![b'A'; MAX_DOCUMENT_BYTES + 1];
    assert_eq!(
        inspect_document_bytes(&over_data, true),
        Err("Document supérieur à 8 Mio")
    );
}

/// `T2_F22_02`: Non-regular file (FIFO/socket) rejected.
///
/// Pipes, FIFOs, and character devices are rejected fail-closed.
#[test]
fn test_t2_f22_02_non_regular_file_rejected() {
    let dummy = b"Stream data";
    assert_eq!(
        inspect_document_bytes(dummy, false),
        Err("Seuls les fichiers ordinaires sont autorisés (pipes et sockets refusés)")
    );
}

/// `T2_F22_03`: Preview text truncated at exactly 4000 chars.
///
/// Preview strictly truncates at 4000 characters.
#[test]
fn test_t2_f22_03_preview_text_truncated_at_4000_chars() {
    assert_eq!(MAX_PREVIEW_CHARS, 4000);

    let text_exact = "C".repeat(4000);
    let (_, prev_exact) = inspect_document_bytes(text_exact.as_bytes(), true).unwrap();
    assert_eq!(prev_exact.unwrap().chars().count(), 4000);

    let text_over = "C".repeat(4005);
    let (_, prev_over) = inspect_document_bytes(text_over.as_bytes(), true).unwrap();
    assert_eq!(prev_over.unwrap().chars().count(), 4000);
}

/// `T2_F22_04`: IO read timeout (10s deadline).
///
/// Timeout constant is 10 seconds.
#[test]
fn test_t2_f22_04_document_read_timeout_10s() {
    assert_eq!(DOCUMENT_READ_TIMEOUT_SECS, 10);
}

/// `T2_F22_05`: Empty document (0 bytes) handled cleanly.
///
/// 0-byte document produces SHA-256 of empty bytes without error.
#[test]
fn test_t2_f22_05_empty_document_handling() {
    let (hash, preview) = inspect_document_bytes(&[], true).unwrap();
    assert_eq!(
        hash,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(preview, Some("".into()));
}
