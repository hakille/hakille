//! Feature 23: Single-Shot GPS with Age (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F23_01 to T2_F23_05 (5 tests total).

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

/// `T2_F23_01`: Coordinates exact boundaries (-90, 90, -180, 180).
///
/// Latitude must be in [-90, 90], longitude in [-180, 180].
#[test]
fn test_t2_f23_01_coordinate_exact_boundaries() {
    let now = Utc::now();

    let valid_corner = PhoneLocation {
        latitude: 90.0,
        longitude: 180.0,
        accuracy: 10.0,
        timestamp: now,
        approximate: false,
        mock: false,
    };
    assert!(valid_corner.is_valid(now).is_ok());

    let lat_over = PhoneLocation {
        latitude: 90.0001,
        ..valid_corner.clone()
    };
    assert_eq!(
        lat_over.is_valid(now),
        Err("Latitude hors limites [-90, +90]")
    );

    let lon_over = PhoneLocation {
        longitude: 180.0001,
        ..valid_corner.clone()
    };
    assert_eq!(
        lon_over.is_valid(now),
        Err("Longitude hors limites [-180, +180]")
    );
}

/// `T2_F23_02`: NaN and Inf coordinates rejected.
///
/// Non-finite numbers are rejected.
#[test]
fn test_t2_f23_02_nan_and_inf_coordinates_rejected() {
    let now = Utc::now();
    let loc_base = PhoneLocation {
        latitude: 0.0,
        longitude: 0.0,
        accuracy: 10.0,
        timestamp: now,
        approximate: false,
        mock: false,
    };

    let nan_loc = PhoneLocation {
        latitude: f64::NAN,
        ..loc_base.clone()
    };
    assert_eq!(
        nan_loc.is_valid(now),
        Err("Coordonnées GPS non finies (NaN ou Inf)")
    );

    let inf_loc = PhoneLocation {
        longitude: f64::INFINITY,
        ..loc_base.clone()
    };
    assert_eq!(
        inf_loc.is_valid(now),
        Err("Coordonnées GPS non finies (NaN ou Inf)")
    );
}

/// `T2_F23_03`: Location fix age: 15s accepted vs 16s rejected as stale.
///
/// Fix measured within 15s is fresh; fix older than 15s is rejected.
#[test]
fn test_t2_f23_03_location_fix_age_boundary() {
    let now = Utc::now();

    let fresh_15s = PhoneLocation {
        latitude: 14.7167,
        longitude: -17.4677,
        accuracy: 10.0,
        timestamp: now - chrono::Duration::seconds(15),
        approximate: false,
        mock: false,
    };
    assert!(fresh_15s.is_valid(now).is_ok());

    let stale_16s = PhoneLocation {
        latitude: 14.7167,
        longitude: -17.4677,
        accuracy: 10.0,
        timestamp: now - chrono::Duration::seconds(16),
        approximate: false,
        mock: false,
    };
    assert_eq!(
        stale_16s.is_valid(now),
        Err("Position GPS périmée (> 15s) ou horodatage futur invalide")
    );
}

/// `T2_F23_04`: Mock provider flagged as untrusted.
///
/// Mock GPS fixes are rejected.
#[test]
fn test_t2_f23_04_mock_provider_flagged_as_untrusted() {
    let now = Utc::now();
    let mock_loc = PhoneLocation {
        latitude: 48.8566,
        longitude: 2.3522,
        accuracy: 5.0,
        timestamp: now,
        approximate: false,
        mock: true,
    };
    assert_eq!(
        mock_loc.is_valid(now),
        Err("Position simulée (mock provider) refusée")
    );
}

/// `T2_F23_05`: Accuracy <= 0 or > 40,100,000 rejected.
///
/// Accuracy radius must be non-negative and bounded by Earth circumference.
#[test]
fn test_t2_f23_05_accuracy_bounds_validation() {
    let now = Utc::now();
    let base = PhoneLocation {
        latitude: 0.0,
        longitude: 0.0,
        accuracy: 10.0,
        timestamp: now,
        approximate: false,
        mock: false,
    };

    let neg_acc = PhoneLocation {
        accuracy: -0.1,
        ..base.clone()
    };
    assert_eq!(
        neg_acc.is_valid(now),
        Err("Rayon de précision invalide [0, 40100000 m]")
    );

    let huge_acc = PhoneLocation {
        accuracy: 50_000_000.0,
        ..base.clone()
    };
    assert_eq!(
        huge_acc.is_valid(now),
        Err("Rayon de précision invalide [0, 40100000 m]")
    );
}
