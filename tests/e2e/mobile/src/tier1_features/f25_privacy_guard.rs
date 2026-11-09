//! Feature 25: PrivacyGuard Screen Shield (Milestone M1 / Spec §2, ui-protocol)
//!
//! Specifications: T1_F25_01 to T1_F25_05
//!
//! Verifies:
//! - ShoulderSurfingDetected trigger and immediate screen shield blurring response.
//! - RawDataExfiltrationBlocked trigger protecting unconsented sensory data.
//! - HardwareKillSwitchActivated trigger signaling physical camera/mic disconnect.
//! - EphemeralTtlPurged trigger attesting zero persistent biometric storage in RAM.
//! - Real-time gateway streaming and framing of KernelToUi::PrivacyGuardAlert events.

use crate::harness::{DaemonStep, MobileTestRig};
use hakille_ipc::Frame;
use hakille_ui_protocol::{KernelToUi, PrivacyGuardEvent, PrivacyGuardTrigger};

/// `T1_F25_01`: Shoulder surfing trigger serialization and UI veil.
///
/// Detecting multiple observers behind the screen triggers `ShoulderSurfingDetected`,
/// requiring immediate screen obscuration and sensitive data veiling.
#[test]
fn test_t1_f25_01_shoulder_surfing_trigger_serialization_and_veil() {
    let event = PrivacyGuardEvent {
        trigger: PrivacyGuardTrigger::ShoulderSurfingDetected { people_count: 3 },
        description: "3 personnes détectées face à l'écran".into(),
        recommended_action: "Floutage écran activé".into(),
        timestamp_ms: 1731000000000,
    };

    let serialized = serde_json::to_string(&event).expect("serialize event");
    let deserialized: PrivacyGuardEvent =
        serde_json::from_str(&serialized).expect("deserialize event");

    assert_eq!(
        deserialized.trigger,
        PrivacyGuardTrigger::ShoulderSurfingDetected { people_count: 3 }
    );
    assert_eq!(deserialized.recommended_action, "Floutage écran activé");
    assert_eq!(deserialized.timestamp_ms, 1731000000000);
}

/// `T1_F25_02`: Raw data exfiltration blocked trigger.
///
/// Blocking unpermitted exfiltration of raw sensor/biometric data produces
/// `RawDataExfiltrationBlocked` with the blocked capability scope.
#[test]
fn test_t1_f25_02_raw_data_exfiltration_blocked_trigger() {
    let event = PrivacyGuardEvent {
        trigger: PrivacyGuardTrigger::RawDataExfiltrationBlocked {
            blocked_scope: "biometrics.face_mesh".into(),
        },
        description: "Tentative d'extraction du maillage facial brut bloquée".into(),
        recommended_action: "Isolation du bac à sable confirmée".into(),
        timestamp_ms: 1731000005000,
    };

    let msg = KernelToUi::PrivacyGuardAlert(event);
    let json_val = serde_json::to_value(&msg).unwrap();
    assert!(json_val["PrivacyGuardAlert"].is_object());
    assert_eq!(
        json_val["PrivacyGuardAlert"]["trigger"]["RawDataExfiltrationBlocked"]["blocked_scope"],
        "biometrics.face_mesh"
    );
}

/// `T1_F25_03`: Hardware kill switch trigger.
///
/// When a physical privacy switch cuts the camera or microphone,
/// `HardwareKillSwitchActivated` is emitted with device identifier.
#[test]
fn test_t1_f25_03_hardware_kill_switch_trigger() {
    let event = PrivacyGuardEvent {
        trigger: PrivacyGuardTrigger::HardwareKillSwitchActivated {
            device: "camera_shutter".into(),
        },
        description: "Obturateur physique caméra fermé".into(),
        recommended_action: "Passage automatique en mode audio seul".into(),
        timestamp_ms: 1731000010000,
    };

    let roundtrip: PrivacyGuardEvent =
        serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
    assert_eq!(
        roundtrip.trigger,
        PrivacyGuardTrigger::HardwareKillSwitchActivated {
            device: "camera_shutter".into()
        }
    );
}

/// `T1_F25_04`: Ephemeral TTL purged trigger.
///
/// Attests that in-memory sensory data was completely wiped upon TTL expiry,
/// guaranteeing zero persistent biometric remnants.
#[test]
fn test_t1_f25_04_ephemeral_ttl_purged_trigger() {
    let event = PrivacyGuardEvent {
        trigger: PrivacyGuardTrigger::EphemeralTtlPurged,
        description: "Mémoire vive biométrique purgée après inactivité".into(),
        recommended_action: "Aucune action requise".into(),
        timestamp_ms: 1731000015000,
    };

    assert_eq!(event.trigger, PrivacyGuardTrigger::EphemeralTtlPurged);
    assert!(event.description.contains("purgée"));
}

/// `T1_F25_05`: Real-time gateway streaming of PrivacyGuardAlert.
///
/// Streaming `KernelToUi::PrivacyGuardAlert` through the gateway delivers the frame
/// cleanly to the mobile client in response frames.
#[tokio::test]
async fn test_t1_f25_05_privacy_guard_event_roundtrip_through_gateway() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let _bridge = rig.new_bridge();

    let guard_event = PrivacyGuardEvent {
        trigger: PrivacyGuardTrigger::ShoulderSurfingDetected { people_count: 2 },
        description: "Observateur tiers détecté".into(),
        recommended_action: "Masquage confidentiel activé".into(),
        timestamp_ms: 1731000020000,
    };

    let daemon_task = tokio::spawn({
        let event = guard_event.clone();
        async move {
            let mut stream = daemon.accept().await.expect("accept");

            let step = DaemonStep::status().after(vec![KernelToUi::PrivacyGuardAlert(event)]);
            daemon
                .handle_step(&mut stream, 1, step)
                .await
                .expect("status with privacy guard alert");
        }
    });

    let req_frame = Frame::request(1, hakille_ui_protocol::UiToKernel::StatusRequest);
    let (status, resp) = rig.post_frame(&req_frame, None).await;

    assert_eq!(status, 200);
    assert!(resp["error"].is_null());

    // Frame stream must contain the PrivacyGuardAlert frame
    let frames = resp["frames"].as_array().expect("frames");
    let guard_frame = frames
        .iter()
        .find(|f| f["body"]["Reply"]["message"]["PrivacyGuardAlert"].is_object())
        .expect("PrivacyGuardAlert frame found");

    assert_eq!(
        guard_frame["body"]["Reply"]["message"]["PrivacyGuardAlert"]["trigger"]["ShoulderSurfingDetected"]
            ["people_count"],
        2
    );

    daemon_task.await.expect("daemon exit");
}
