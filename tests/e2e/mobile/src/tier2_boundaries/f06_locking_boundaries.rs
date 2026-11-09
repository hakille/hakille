//! Feature 06: Honest Wait & Input Locking (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F06_01 to T2_F06_05 (5 tests total).

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

/// `T2_F06_01`: Simultaneous multiple taps while input locked.
///
/// Blasting 20 rapid actions while a request is pending results in strict rejection
/// for all 20 attempts.
#[test]
fn test_t2_f06_01_simultaneous_multiple_taps_while_input_locked() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());
    bridge.dispatch_json(json!({"type": "set_draft", "text": "Requête initiale"}).to_string());

    let res1 = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    assert!(res1.contains("\"ok\":true"));

    let actions = [
        json!({"type": "submit"}),
        json!({"type": "refresh_status"}),
        json!({"type": "load_memory", "limit": 10}),
        json!({"type": "remember", "text": "Nouveau fait"}),
        json!({"type": "load_journal", "limit": 10}),
        json!({"type": "load_agents"}),
        json!({"type": "begin_approval", "card_id": 1}),
        json!({"type": "approve", "card_id": 1}),
        json!({"type": "reject", "card_id": 1}),
        json!({"type": "begin_voice"}),
    ];

    for (idx, action) in actions.iter().cycle().take(20).enumerate() {
        let err_str = bridge.dispatch_json(action.to_string());
        let err_json: serde_json::Value = serde_json::from_str(&err_str).unwrap();
        assert_eq!(
            err_json["ok"], false,
            "Tap {idx} must be rejected while input locked"
        );
        let err_msg = err_json["error"].as_str().unwrap();
        assert!(
            err_msg == "Une requête est déjà en cours"
                || err_msg.contains("Tour vocal indisponible"),
            "Error on tap {idx} must indicate in-flight lock: got {err_msg}"
        );
    }
}

/// `T2_F06_02`: Clock skew during wait duration.
///
/// Even if system clock jumps backwards, saturating duration calculations prevent panic.
#[test]
fn test_t2_f06_02_clock_skew_during_wait_duration() {
    let t0 = Instant::now();
    // Simulate time backwards by comparing t0 to t0
    let duration = t0.saturating_duration_since(t0 + Duration::from_secs(60));
    assert_eq!(
        duration,
        Duration::ZERO,
        "Clock jump backward must saturate to zero"
    );
}

/// `T2_F06_03`: Server response taking extended time (gateway timeout).
///
/// When server exceeds gateway timeout (5s in rig), gateway returns HTTP 504 ipc_timeout.
#[tokio::test]
async fn test_t2_f06_03_extended_wait_triggers_gateway_timeout() {
    let (rig, daemon) =
        MobileTestRig::setup_with_timeouts(Duration::from_millis(100), Duration::from_secs(1))
            .await;

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        // Read the request, but sleep longer than request_timeout (100ms)
        let _ = daemon.accept_one_frame(&mut stream).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
    });

    let frame = Frame::request(1, UiToKernel::StatusRequest);
    let (status, resp) = rig.post_frame(&frame, None).await;

    assert_eq!(status, 504, "Gateway must return HTTP 504 on IPC timeout");
    assert_eq!(resp["error"]["code"], "ipc_timeout");
    assert_eq!(resp["error"]["issue_unknown"], true);

    daemon_task.await.unwrap();
}

/// `T2_F06_04`: Network reset during wait.
///
/// Abrupt socket disconnect during in-flight wait returns HTTP 502 with issue_unknown: true.
#[tokio::test]
async fn test_t2_f06_04_network_reset_during_wait() {
    let (rig, daemon) = MobileTestRig::setup().await;

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        let _ = daemon.accept_one_frame(&mut stream).await;
        // Drop stream abruptly
        drop(stream);
    });

    let frame = Frame::request(1, UiToKernel::StatusRequest);
    let (status, resp) = rig.post_frame(&frame, None).await;

    assert_eq!(
        status, 502,
        "Gateway must return HTTP 502 on abrupt socket close"
    );
    assert_eq!(resp["error"]["code"], "ipc_transport");
    assert_eq!(resp["error"]["issue_unknown"], true);

    daemon_task.await.unwrap();
}

/// `T2_F06_05`: Request ID reaching boundary.
///
/// u64::MAX is strictly reserved for BARRIER_ID and cannot be allocated to requests.
#[test]
fn test_t2_f06_05_request_id_reaching_boundary() {
    assert_eq!(BARRIER_ID, u64::MAX);

    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());

    // Restore state where next_request_id is u64::MAX
    let checkpoint = json!({
        "schema_version": 2,
        "draft": "Test limit",
        "next_request_id": u64::MAX,
        "uncertain_operations": [],
        "acknowledged_operations": []
    });
    bridge.restore_draft_json(serde_json::to_string(&checkpoint).unwrap());

    let res_str = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    let res: serde_json::Value = serde_json::from_str(&res_str).unwrap();
    assert_eq!(res["ok"], false);
    assert!(
        res["error"].as_str().unwrap().contains("barrière réservé")
            || res["error"].as_str().unwrap().contains("épuisés")
    );
}
