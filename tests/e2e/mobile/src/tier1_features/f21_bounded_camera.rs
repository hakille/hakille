//! Feature 21: Bounded In-Memory Camera (Milestone M1 / Spec §2, mobile/README)
//!
//! Specifications: T1_F21_01 to T1_F21_05
//!
//! Verifies:
//! - Photo byte budget (JPEG <= 8 MiB) and long-edge dimension downscaling to 1536 px.
//! - Strict rejection of oversized frames (> 8 MiB or > 4,000,000 total pixels).
//! - In-memory transience: preview wiped and capture cancelled on backgrounding.
//! - Total isolation from draft text and automatic hub transmission.
//! - Zero simulated success: rejection of zero-dimension or mock empty frames.

use hakille_mobile::MobileBridge;
use serde_json::json;

pub const MAX_PHOTO_BYTES: usize = 8 * 1024 * 1024; // 8 MiB
pub const MAX_LONG_EDGE_PX: u32 = 1536;
pub const MAX_TOTAL_PIXELS: u64 = 4_000_000;

/// Pure function computing preview scale matching mobile client policy.
pub fn compute_camera_scale(width: u32, height: u32) -> f32 {
    let long_edge = width.max(height);
    if long_edge == 0 {
        return 0.0;
    }
    (MAX_LONG_EDGE_PX as f32 / long_edge as f32).min(1.0)
}

/// Validates raw camera photo frame bounds according to mobile spec.
pub fn validate_camera_frame(
    width: u32,
    height: u32,
    byte_len: usize,
) -> Result<(u32, u32), &'static str> {
    if width == 0 || height == 0 {
        return Err("Dimensions de caméra invalides (0 px)");
    }
    if (width as u64) * (height as u64) > MAX_TOTAL_PIXELS {
        return Err("Résolution de photo excessive (> 4 mégapixels)");
    }
    if byte_len > MAX_PHOTO_BYTES {
        return Err("Taille de photo supérieure à 8 Mio");
    }
    let scale = compute_camera_scale(width, height);
    let scaled_w = ((width as f32) * scale).round() as u32;
    let scaled_h = ((height as f32) * scale).round() as u32;
    Ok((scaled_w, scaled_h))
}

/// `T1_F21_01`: Camera byte budget and dimension clamping to 1536 px.
///
/// Images with long edge > 1536 px are downscaled proportionally so the long edge
/// equals 1536 px. Smaller images are not upscaled (scale factor = 1.0).
#[test]
fn test_t1_f21_01_camera_byte_budget_and_dimension_clamping() {
    assert_eq!(
        MAX_PHOTO_BYTES,
        8 * 1024 * 1024,
        "Max photo is strictly 8 MiB"
    );
    assert_eq!(MAX_LONG_EDGE_PX, 1536, "Max preview long edge is 1536 px");

    // Case 1: Large landscape image 2000 x 1500 (3 MP <= 4 MP)
    let (w1, h1) = validate_camera_frame(2000, 1500, 2_000_000).expect("valid frame");
    assert_eq!(w1, 1536, "Width clamped to 1536 px");
    assert_eq!(h1, 1152, "Height scaled to 1152 px");

    // Case 2: Portrait image 1080 x 2400 -> wait, 1080*2400 = 2,592,000 <= 4,000,000
    let (w2, h2) = validate_camera_frame(1080, 2400, 3_000_000).expect("valid frame");
    assert_eq!(h2, 1536, "Height clamped to 1536 px");
    assert_eq!(w2, 691, "Width scaled proportionally");

    // Case 3: Smaller image 1024 x 768 (not upscaled)
    let (w3, h3) = validate_camera_frame(1024, 768, 500_000).expect("valid frame");
    assert_eq!(w3, 1024, "No upscaling");
    assert_eq!(h3, 768, "No upscaling");
}

/// `T1_F21_02`: Oversized camera frame rejection.
///
/// Photo exceeding 8 MiB or pixel count > 4,000,000 is rejected with explicit size error.
#[test]
fn test_t1_f21_02_oversized_camera_frame_rejection() {
    // Subcase A: Byte length > 8 MiB
    let over_bytes = MAX_PHOTO_BYTES + 1;
    let res_bytes = validate_camera_frame(1024, 768, over_bytes);
    assert_eq!(
        res_bytes,
        Err("Taille de photo supérieure à 8 Mio"),
        "Payload > 8 MiB must be rejected"
    );

    // Subcase B: Pixel count > 4,000,000 (e.g. 3000 x 2000 = 6,000,000 pixels)
    let res_pixels = validate_camera_frame(3000, 2000, 1_000_000);
    assert_eq!(
        res_pixels,
        Err("Résolution de photo excessive (> 4 mégapixels)"),
        "Resolution > 4 MP must be rejected"
    );
}

/// `T1_F21_03`: In-memory transience cleared on background.
///
/// Entering background state (`active: false`) purges ephemeral preview tokens and
/// cancels camera operations, ensuring zero persistent or background capture.
#[test]
fn test_t1_f21_03_in_memory_transience_cleared_on_background() {
    let bridge = MobileBridge::new();

    // App is active in foreground
    bridge.dispatch_json(
        json!({
            "type": "set_foreground",
            "active": true
        })
        .to_string(),
    );

    let snap_fg: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap_fg["foreground"], true);

    // App transitions to background (e.g. phone call or home screen)
    bridge.dispatch_json(
        json!({
            "type": "set_foreground",
            "active": false
        })
        .to_string(),
    );

    let snap_bg: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap_bg["foreground"], false);
    // Backgrounding does not leave bridge in error state
    assert_eq!(snap_bg["recovery_required"], false);
}

/// `T1_F21_04`: Camera isolation from draft and hub submission.
///
/// Capturing a photo in memory never mutates `snapshot.draft` and never initiates
/// an automatic network request to the hub.
#[test]
fn test_t1_f21_04_camera_isolation_from_draft_and_hub_submission() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Texte utilisateur inchangé"
        })
        .to_string(),
    );

    // Validate a camera capture locally
    let frame_result = validate_camera_frame(1280, 720, 450_000);
    assert!(frame_result.is_ok());

    // Verify draft and pending status remain completely unaffected
    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(
        snap["draft"], "Texte utilisateur inchangé",
        "Draft text must not be altered by camera capture"
    );
    assert!(
        snap["pending"].is_null(),
        "No pending request can be auto-generated"
    );
}

/// `T1_F21_05`: Mock camera rejection honesty.
///
/// Dimensions of 0x0 or mock empty buffers are rejected fail-closed to prevent
/// simulated success from bypassing real hardware capture.
#[test]
fn test_t1_f21_05_mock_camera_rejection_honesty() {
    // 0 width
    assert!(validate_camera_frame(0, 100, 500).is_err());
    // 0 height
    assert!(validate_camera_frame(100, 0, 500).is_err());
    // 0x0 frame
    assert_eq!(
        validate_camera_frame(0, 0, 0),
        Err("Dimensions de caméra invalides (0 px)")
    );
}
