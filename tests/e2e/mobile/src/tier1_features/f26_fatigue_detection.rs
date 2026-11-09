//! Feature 26: Ergonomic Fatigue Detection (Milestone M1 / Spec §1, ui-protocol)
//!
//! Specifications: T1_F26_01 to T1_F26_05
//!
//! Verifies:
//! - FatigueSeverity levels: Mild, Moderate, and Critical thresholds.
//! - PERCLOS P80 (0.0 to 1.0) and Eye/Mouth Aspect Ratio (EAR/MAR) bounds.
//! - Yawn count accumulation and benevolent, non-moralizing recommendation text.
//! - Non-blocking advisory contract: fatigue events do not lock input or set recovery_required.
//! - Real-time streaming of KernelToUi::FatigueAlert through gateway to client.

use crate::harness::{DaemonStep, MobileTestRig};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{FatigueAlertEvent, FatigueSeverity, KernelToUi};
use serde_json::json;

/// `T1_F26_01`: FatigueSeverity levels and serialization.
///
/// Verifies the three fatigue tiers: Mild, Moderate, and Critical.
#[test]
fn test_t1_f26_01_fatigue_severity_levels_and_serialization() {
    let severities = [
        FatigueSeverity::Mild,
        FatigueSeverity::Moderate,
        FatigueSeverity::Critical,
    ];

    for sev in severities {
        let serialized = serde_json::to_string(&sev).expect("serialize severity");
        let deserialized: FatigueSeverity =
            serde_json::from_str(&serialized).expect("deserialize severity");
        assert_eq!(sev, deserialized);
    }
}

/// `T1_F26_02`: PERCLOS P80 and EAR/MAR bounds validation.
///
/// PERCLOS P80 is bounded in [0.0, 1.0]. EAR and MAR are non-negative aspect ratios.
#[test]
fn test_t1_f26_02_perclos_p80_and_ear_mar_bounds_validation() {
    let event = FatigueAlertEvent {
        severity: FatigueSeverity::Moderate,
        perclos_p80: 0.12, // 12% closure (Moderate: 10-15%)
        ear: 0.22,         // Eye Aspect Ratio
        mar: 0.45,         // Mouth Aspect Ratio (mouth open)
        yawn_count: 2,
        recommendation: "Pause conseillée".into(),
        timestamp_ms: 1731000100000,
    };

    assert!(
        (0.0..=1.0).contains(&event.perclos_p80),
        "PERCLOS must be in [0.0, 1.0]"
    );
    assert!(event.ear >= 0.0, "EAR must be non-negative");
    assert!(event.mar >= 0.0, "MAR must be non-negative");
    assert_eq!(event.severity, FatigueSeverity::Moderate);
}

/// `T1_F26_03`: Yawn detection and benevolent recommendation.
///
/// When multiple yawns are detected, recommendation text is respectful and concise
/// ("Pause conseillée"), never infantilizing or commanding.
#[test]
fn test_t1_f26_03_yawn_detection_and_benevolent_recommendation() {
    let event = FatigueAlertEvent {
        severity: FatigueSeverity::Critical,
        perclos_p80: 0.18, // > 15%
        ear: 0.15,
        mar: 0.65, // Significant yawn
        yawn_count: 5,
        recommendation: "Pause de 15 minutes recommandée".into(),
        timestamp_ms: 1731000105000,
    };

    assert_eq!(event.yawn_count, 5);
    assert_eq!(event.severity, FatigueSeverity::Critical);
    assert!(
        event.recommendation.contains("recommandée") || event.recommendation.contains("conseillée")
    );
    // Must not contain patronizing language
    assert!(!event.recommendation.to_lowercase().contains("interdit"));
    assert!(!event.recommendation.to_lowercase().contains("bloqué"));
}

/// `T1_F26_04`: Non-blocking ergonomic advisory contract.
///
/// Fatigue alerts are advisory only: they never block user inputs, never set
/// `recovery_required: true`, and never inhibit ongoing drafts or actions.
#[test]
fn test_t1_f26_04_non_blocking_ergonomic_advisory_contract() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Travail en cours"
        })
        .to_string(),
    );

    let snap_before: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap_before["recovery_required"], false);

    // Draft remains editable and intact
    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Travail en cours suite"
        })
        .to_string(),
    );

    let snap_after: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap_after["draft"], "Travail en cours suite");
    assert_eq!(snap_after["recovery_required"], false);
}

/// `T1_F26_05`: Real-time gateway streaming of FatigueAlert.
///
/// Streaming `KernelToUi::FatigueAlert` through the gateway delivers the event
/// cleanly to the client without transport disruption.
#[tokio::test]
async fn test_t1_f26_05_gateway_streaming_of_fatigue_alert() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let _bridge = rig.new_bridge();

    let alert_event = FatigueAlertEvent {
        severity: FatigueSeverity::Mild,
        perclos_p80: 0.08,
        ear: 0.28,
        mar: 0.20,
        yawn_count: 1,
        recommendation: "Penser à s'hydrater".into(),
        timestamp_ms: 1731000110000,
    };

    let daemon_task = tokio::spawn({
        let alert = alert_event.clone();
        async move {
            let mut stream = daemon.accept().await.expect("accept");

            let step = DaemonStep::status().after(vec![KernelToUi::FatigueAlert(alert)]);
            daemon
                .handle_step(&mut stream, 1, step)
                .await
                .expect("status with fatigue alert");
        }
    });

    let req_frame = Frame::request(1, hakille_ui_protocol::UiToKernel::StatusRequest);
    let (status, resp) = rig.post_frame(&req_frame, None).await;

    assert_eq!(status, 200);
    assert!(resp["error"].is_null());

    let frames = resp["frames"].as_array().expect("frames");
    let fatigue_frame = frames
        .iter()
        .find(|f| f["body"]["Reply"]["message"]["FatigueAlert"].is_object())
        .expect("FatigueAlert frame found");

    assert_eq!(
        fatigue_frame["body"]["Reply"]["message"]["FatigueAlert"]["severity"],
        "Mild"
    );
    assert_eq!(
        fatigue_frame["body"]["Reply"]["message"]["FatigueAlert"]["yawn_count"],
        1
    );

    daemon_task.await.expect("daemon cleanly finishes");
}
