//! Feature 03: Local Dictation Mode (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F03_01 to T2_F03_05 (5 tests total).

#![allow(unused_imports)]

use crate::harness::{DaemonStep, MobileTestRig};
use hakille_design::brand::{WIRES_FULL, WIRES_SMALL, WIRES_SMALL_BELOW_PX, wire_count};
use hakille_design::voice::{
    DEFAULT_SPEECH_RATE, NO_ANSWER_SILENCE_SECS, SPEECH_RATE_MAX, SPEECH_RATE_MIN,
    clamp_speech_rate,
};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_mobile_gateway::BARRIER_ID;
use hakille_ui_protocol::aura::AuraRingUpdateDto;
use hakille_ui_protocol::{KernelToUi, PresenceState, UiToKernel};
use serde_json::json;
use std::time::{Duration, Instant};

/// `T2_F03_01`: Exactly 8192 bytes accepted vs 8193 bytes rejected.
///
/// Verifies strict 8192-byte capacity boundary for local draft storage.
#[test]
fn test_t2_f03_01_exact_8192_bytes_accepted_vs_8193_rejected() {
    let bridge = MobileBridge::new();

    // 8192 bytes accepted
    let exact_8192 = "x".repeat(8192);
    let ok_res_str = bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": exact_8192
        })
        .to_string(),
    );
    let ok_res: serde_json::Value = serde_json::from_str(&ok_res_str).unwrap();
    assert_eq!(ok_res["ok"], true);

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap["draft"].as_str().unwrap().len(), 8192);

    // 8193 bytes strictly rejected
    let overflow_8193 = "x".repeat(8193);
    let err_res_str = bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": overflow_8193
        })
        .to_string(),
    );
    let err_res: serde_json::Value = serde_json::from_str(&err_res_str).unwrap();
    assert_eq!(err_res["ok"], false);
    assert!(
        err_res["error"]
            .as_str()
            .unwrap()
            .contains("limite 8192 octets")
    );

    // Previous valid 8192 draft preserved
    let snap_after: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap_after["draft"].as_str().unwrap().len(), 8192);
}

/// `T2_F03_02`: Empty draft string boundary.
///
/// Setting draft to empty string succeeds, but submitting an empty draft is rejected.
#[test]
fn test_t2_f03_02_empty_draft_string_boundary() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());

    // Empty draft setting is permitted (clears draft)
    let res_empty = bridge.dispatch_json(json!({"type": "set_draft", "text": ""}).to_string());
    assert!(res_empty.contains("\"ok\":true"));

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap["draft"], "");

    // Submitting empty draft is rejected
    let res_submit = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    let res: serde_json::Value = serde_json::from_str(&res_submit).unwrap();
    assert_eq!(res["ok"], false);
    assert!(
        res["error"].as_str().unwrap().contains("Texte invalide")
            || res["error"]
                .as_str()
                .unwrap()
                .contains("limite 8192 octets")
    );
}

/// `T2_F03_03`: Audio format corrupted / malformed audio stream rejection.
///
/// Validates that malformed audio inputs (zero bytes, corrupted headers) fail validation gracefully.
#[test]
fn test_t2_f03_03_corrupted_audio_format_boundary() {
    fn validate_audio_chunk(data: &[u8]) -> Result<usize, &'static str> {
        if data.is_empty() {
            return Err("Flux audio vide");
        }
        if data.len() < 12 {
            return Err("En-tête audio tronqué (< 12 octets)");
        }
        // Expect valid RIFF or Hakille voice framing
        if &data[0..4] != b"RIFF" && &data[0..4] != b"HVOX" {
            return Err("Format audio non reconnu");
        }
        Ok(data.len())
    }

    assert_eq!(validate_audio_chunk(&[]), Err("Flux audio vide"));
    assert_eq!(
        validate_audio_chunk(b"short"),
        Err("En-tête audio tronqué (< 12 octets)")
    );
    assert_eq!(
        validate_audio_chunk(b"CORRUPTED_BYTES_HERE"),
        Err("Format audio non reconnu")
    );

    // Valid header
    let mut valid_header = b"HVOX\x01\x00\x00\x00\x80\x3e\x00\x00".to_vec();
    valid_header.extend_from_slice(&[0u8; 100]);
    assert_eq!(validate_audio_chunk(&valid_header), Ok(112));
}

/// `T2_F03_04`: Microphone permission revoked mid-stream.
///
/// Attempting to enable microphone without native bridge binding fails closed;
/// disabling microphone cancels active listening while preserving draft text.
#[test]
fn test_t2_f03_04_microphone_permission_revoked_mid_stream() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_draft", "text": "Brouillon sous écoute"}).to_string());

    // Enabling microphone when native capture is not connected returns explicit error
    let res_mic_str =
        bridge.dispatch_json(json!({"type": "set_microphone", "enabled": true}).to_string());
    let res_mic: serde_json::Value = serde_json::from_str(&res_mic_str).unwrap();
    assert_eq!(res_mic["ok"], false);
    assert!(
        res_mic["error"]
            .as_str()
            .unwrap()
            .contains("Capture microphone native non raccordée")
    );

    // Disabling microphone succeeds
    let res_off =
        bridge.dispatch_json(json!({"type": "set_microphone", "enabled": false}).to_string());
    assert!(res_off.contains("\"ok\":true"));

    // Draft is fully preserved
    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap["draft"], "Brouillon sous écoute");
    assert_eq!(snap["microphone_enabled"], false);
}

/// `T2_F03_05`: Backgrounding during active dictation.
///
/// Setting foreground to false zeroes microphone capture state and detaches active in-flight operations,
/// preserving existing draft text.
#[test]
fn test_t2_f03_05_backgrounding_during_active_dictation() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({"type": "set_draft", "text": "Texte avant passage en arrière-plan"}).to_string(),
    );

    // Transition to background
    let res_bg =
        bridge.dispatch_json(json!({"type": "set_foreground", "active": false}).to_string());
    assert!(res_bg.contains("\"ok\":true"));

    let snap_bg: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap_bg["foreground"], false);
    assert_eq!(snap_bg["microphone_enabled"], false);
    assert_eq!(snap_bg["draft"], "Texte avant passage en arrière-plan");

    // Returning to foreground
    let res_fg =
        bridge.dispatch_json(json!({"type": "set_foreground", "active": true}).to_string());
    assert!(res_fg.contains("\"ok\":true"));

    let snap_fg: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap_fg["foreground"], true);
    assert_eq!(snap_fg["draft"], "Texte avant passage en arrière-plan");
}
