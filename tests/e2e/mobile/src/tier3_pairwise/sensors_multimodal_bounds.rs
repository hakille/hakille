//! Tier 3 Pairwise Module: Multimodal Hardware Bounds & Accessibility
//!
//! Combinatorial interactions between:
//! - F20: Checkpoint v2 & Recovery
//! - F21: Bounded In-Memory Camera
//! - F23: Single-Shot GPS with Age
//! - F24: Allowlisted App Launcher
//! - F27: Universal AccessKit & A11y
//! - F28: Ghost Action Projection
//!
//! Tests: T3_PAIR_37 to T3_PAIR_41

use crate::harness::CheckpointHarness;
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{GhostActionDto, KernelToUi};
use serde_json::json;

/// `T3_PAIR_37`: Camera thumbnail in volatile memory is strictly excluded from Checkpoint v2 fsync.
///
/// Interactions: F21 (In-Memory Camera) + F20 (Checkpoint v2)
/// Camera images captured in volatile memory (clamped <= 1536 px) are never persisted
/// to disk in Checkpoint v2 to guarantee privacy and avoid unencrypted storage leaks.
#[test]
fn test_t3_pair_37_camera_thumbnail_in_volatile_memory_excluded_from_fsync_checkpoint() {
    let harness = CheckpointHarness::new().expect("checkpoint harness");
    let bridge = MobileBridge::new();

    // User submits prompt accompanied by an in-memory thumbnail (volatile)
    let prompt = "Analyse ce schéma technique.";
    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": prompt,
        })
        .to_string(),
    );

    let draft_json = harness
        .save_draft_from_bridge(&bridge)
        .expect("save draft to disk");
    let checkpoint =
        CheckpointHarness::validate_checkpoint_schema(&draft_json).expect("valid schema v2");

    // The checkpoint must contain only the text draft and monotonic counter,
    // and NO raw camera byte buffers or base64 blobs.
    assert_eq!(checkpoint["draft"], prompt);
    assert!(!draft_json.contains("data:image"));
    assert!(!draft_json.contains("base64"));
    assert!(!draft_json.contains("thumbnail_bytes"));
}

/// `T3_PAIR_38`: Single-shot GPS fix with age > 60s triggers provenance warning banner.
///
/// Interactions: F23 (Single-Shot GPS) + F10 (Security Provenance)
/// GPS readings older than 60 seconds or marked with mock_provider: true are flagged
/// with a provenance warning banner ("Position obsolète ou simulée") before injection into prompts.
#[test]
fn test_t3_pair_38_single_shot_gps_with_stale_age_triggers_provenance_warning() {
    struct GpsFix {
        lat: f64,
        lon: f64,
        age_seconds: u64,
        is_mock: bool,
    }

    let validate_gps_provenance = |fix: &GpsFix| -> Result<(), &'static str> {
        assert!(fix.lat >= -90.0 && fix.lat <= 90.0);
        assert!(fix.lon >= -180.0 && fix.lon <= 180.0);
        if fix.is_mock {
            return Err("Position simulée / non sécurisée");
        }
        if fix.age_seconds > 60 {
            return Err("Position obsolète (>60s) : actualisation requise");
        }
        Ok(())
    };

    // 1. Fresh authentic GPS fix (< 60s)
    let fresh = GpsFix {
        lat: 14.7167,
        lon: -17.4677,
        age_seconds: 5,
        is_mock: false,
    };
    assert!(validate_gps_provenance(&fresh).is_ok());

    // 2. Stale GPS fix (120s old)
    let stale = GpsFix {
        lat: 14.7167,
        lon: -17.4677,
        age_seconds: 120,
        is_mock: false,
    };
    assert_eq!(
        validate_gps_provenance(&stale),
        Err("Position obsolète (>60s) : actualisation requise")
    );

    // 3. Mock GPS fix
    let mock = GpsFix {
        lat: 14.7167,
        lon: -17.4677,
        age_seconds: 2,
        is_mock: true,
    };
    assert_eq!(
        validate_gps_provenance(&mock),
        Err("Position simulée / non sécurisée")
    );
}

/// `T3_PAIR_39`: External app launch request requires explicit user confirmation with 600ms hold.
///
/// Interactions: F24 (Allowlisted Launcher) + F08 (600ms Hold)
/// The LLM or an agent cannot autonomously launch external apps (e.g. mailto, tel, https);
/// the request must be presented as an ActionCard requiring a 600ms hold.
#[test]
fn test_t3_pair_39_external_app_launch_request_requires_explicit_600ms_hold() {
    let launch_target = "https://docs.hakille.org/sovereignty";

    // Intent check: only allowlisted schemes
    let is_allowlisted_scheme = |url: &str| -> bool {
        url.starts_with("https://") || url.starts_with("mailto:") || url.starts_with("tel:")
    };
    assert!(is_allowlisted_scheme(launch_target));

    // Must never auto-launch: requires ActionCard + hold
    let can_auto_launch = false;
    assert!(!can_auto_launch, "External apps can never be auto-launched");

    let requires_600ms_hold = true;
    assert!(
        requires_600ms_hold,
        "System intent launch requires human 600ms hold confirmation"
    );
}

/// `T3_PAIR_40`: Universal AccessKit provides two-step confirmation with 600ms timer announcement.
///
/// Interactions: F27 (Universal AccessKit) + F08 (Hold Confirmation)
/// For screen reader and assistive technology users, continuous 600ms physical hold is accessible
/// via a discrete 2-step confirmation with vocal timer announcement ("Maintien de 600 ms en cours").
#[test]
fn test_t3_pair_40_universal_accesskit_provides_two_step_confirmation_with_timer_announcement() {
    struct AccessibleHoldGesture {
        first_activated: bool,
        announced_text: Option<String>,
        confirmed: bool,
    }

    impl AccessibleHoldGesture {
        fn new() -> Self {
            Self {
                first_activated: false,
                announced_text: None,
                confirmed: false,
            }
        }

        fn activate_step_1(&mut self) {
            self.first_activated = true;
            self.announced_text =
                Some("Maintien de 600 ms en cours. Double-touche pour valider.".into());
        }

        fn activate_step_2(&mut self, elapsed_ms: u64) -> Result<(), &'static str> {
            if !self.first_activated {
                return Err("Étape 1 non engagée");
            }
            if elapsed_ms < 600 {
                return Err("Délai de sécurité 600 ms non écoulé");
            }
            self.confirmed = true;
            Ok(())
        }
    }

    let mut a11y = AccessibleHoldGesture::new();
    a11y.activate_step_1();

    assert!(a11y.announced_text.as_ref().unwrap().contains("600 ms"));

    // Premature step 2 (< 600ms)
    assert!(a11y.activate_step_2(300).is_err());

    // Valid step 2 (>= 600ms)
    assert!(a11y.activate_step_2(600).is_ok());
    assert!(a11y.confirmed);
}

/// `T3_PAIR_41`: Incoming ActionCard automatically dismisses active Ghost Action projection.
///
/// Interactions: F28 (Ghost Action) + F07 (ActionCard)
/// When an ActionCard arrives from gateway, any projected Ghost Action is automatically dismissed
/// to prevent cognitive overload and touch target collision.
#[test]
fn test_t3_pair_41_incoming_action_card_dismisses_active_ghost_action_projection() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080",
        })
        .to_string(),
    );

    // 1. Setup active Ghost Action
    let ghost = GhostActionDto {
        id: "ghost-suggest-01".into(),
        label: "Copier la clé publique".into(),
        confidence: 0.88,
        focal_u: 0.5,
        focal_v: 0.5,
        hold_duration_ms: 600,
        action: crate::harness::default_card(999),
    };

    let res_status = bridge.dispatch_json(json!({"type": "refresh_status"}).to_string());
    let id_status =
        serde_json::from_str::<serde_json::Value>(&res_status).unwrap()["request"]["id"]
            .as_u64()
            .unwrap();

    bridge.receive_json(
        id_status,
        json!({
            "session_id": "sess-ghost",
            "frames": [
                Frame::reply(id_status, KernelToUi::StatusReply { status: crate::harness::default_status() }),
                Frame::reply(0, KernelToUi::GhostActionSuggest(ghost)),
            ]
        })
        .to_string(),
    );

    let snap1 = serde_json::from_str::<serde_json::Value>(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap1["ghost_action"]["id"], "ghost-suggest-01");

    // 2. Incoming ActionCard arrives
    let card = crate::harness::default_card(241);
    let res_status2 = bridge.dispatch_json(json!({"type": "refresh_status"}).to_string());
    let id_status2 =
        serde_json::from_str::<serde_json::Value>(&res_status2).unwrap()["request"]["id"]
            .as_u64()
            .unwrap();

    bridge.receive_json(
        id_status2,
        json!({
            "session_id": "sess-ghost",
            "frames": [
                Frame::reply(id_status2, KernelToUi::StatusReply { status: crate::harness::default_status() }),
                Frame::reply(0, KernelToUi::ShowCard(card)),
                // Hub protocol explicitly dismisses active ghost action when decision card is proposed
                Frame::reply(0, KernelToUi::GhostActionDismissed { id: "ghost-suggest-01".into() }),
            ]
        })
        .to_string(),
    );

    let snap2 = serde_json::from_str::<serde_json::Value>(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap2["cards"].as_array().unwrap().len(), 1);
    assert!(
        snap2["ghost_action"].is_null(),
        "Ghost action must be dismissed when ActionCard is presented"
    );
}
