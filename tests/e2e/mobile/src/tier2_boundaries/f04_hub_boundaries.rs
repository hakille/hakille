//! Feature 04: Hub Conversation Mode (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F04_01 to T2_F04_05 (5 tests total).

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

/// `T2_F04_01`: Concurrent voice requests rejected.
///
/// Attempting to submit voice while a voice request is already in flight is rejected.
#[tokio::test]
async fn test_t2_f04_01_concurrent_voice_requests_rejected() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        daemon
            .handle_step(&mut stream, 1, DaemonStep::status())
            .await
            .expect("status");
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;
    daemon_task.await.unwrap();

    // Arm voice
    bridge.dispatch_json(json!({"type": "begin_voice"}).to_string());
    let snap1 = rig.snapshot(&bridge);
    let ctx_id = snap1["voice_context_id"].as_u64().unwrap();

    // Submit first voice request
    let sub1_str = bridge.dispatch_json(
        json!({
            "type": "submit_voice",
            "context_id": ctx_id,
            "text": "Première requête vocale"
        })
        .to_string(),
    );
    assert!(sub1_str.contains("\"ok\":true"));

    // Attempt second submit_voice while first is pending
    let sub2_str = bridge.dispatch_json(
        json!({
            "type": "submit_voice",
            "context_id": ctx_id,
            "text": "Seconde requête concurrente"
        })
        .to_string(),
    );
    let sub2: serde_json::Value = serde_json::from_str(&sub2_str).unwrap();
    assert_eq!(sub2["ok"], false);
    assert!(
        sub2["error"].as_str().unwrap().contains("déjà en cours")
            || sub2["error"].as_str().unwrap().contains("déjà soumis")
    );
}

/// `T2_F04_02`: Timeout on hub response (simulated drop).
///
/// When the daemon drops the connection during a voice request, gateway returns HTTP 502/504
/// with issue_unknown: true, and bridge transitions to ConnectionState::Unknown with recovery_required.
#[tokio::test]
async fn test_t2_f04_02_timeout_on_hub_response_simulated_drop() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        daemon
            .handle_step(&mut stream, 1, DaemonStep::status())
            .await
            .expect("status");

        // On step 2, daemon immediately closes the connection without replying
        let _ = daemon.accept_one_frame(&mut stream).await;
        drop(stream);
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;

    bridge.dispatch_json(json!({"type": "begin_voice"}).to_string());
    let snap = rig.snapshot(&bridge);
    let ctx_id = snap["voice_context_id"].as_u64().unwrap();

    let (frame, session) = rig.prepare_frame(
        &bridge,
        json!({
            "type": "submit_voice",
            "context_id": ctx_id,
            "text": "Requête avec coupure réseau"
        }),
    );

    let (status, resp) = rig.post_frame(&frame, session.as_deref()).await;
    assert!(status == 502 || status == 504);
    assert_eq!(resp["error"]["issue_unknown"], true);

    // Tell bridge about the failure
    bridge.receive_json(frame.id, serde_json::to_string(&resp).unwrap());
    let final_snap = rig.snapshot(&bridge);
    assert_eq!(final_snap["connection"], "unknown");
    assert_eq!(final_snap["recovery_required"], true);

    daemon_task.await.unwrap();
}

/// `T2_F04_03`: Voice context ID overflow / boundary exhaustion.
///
/// When next_id is at u64::MAX - 1, begin_voice rejects with insufficient IDs.
#[test]
fn test_t2_f04_03_voice_context_id_overflow_boundary() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());

    // Inject state with next_id = u64::MAX - 1 via checkpoint restore
    let checkpoint = json!({
        "schema_version": 2,
        "draft": "",
        "next_request_id": u64::MAX - 1,
        "uncertain_operations": [],
        "acknowledged_operations": []
    });
    bridge.restore_draft_json(serde_json::to_string(&checkpoint).unwrap());

    // Fake connection state to Connected
    let status_reply = json!({
        "session_id": "sess-limit",
        "frames": [
            Frame::reply(u64::MAX - 1, KernelToUi::StatusReply {
                status: crate::harness::default_status(),
            })
        ]
    });
    // Set pending request so receive_json can complete and connect
    let _ = bridge.dispatch_json(json!({"type": "refresh_status"}).to_string());
    bridge.receive_json(u64::MAX - 1, status_reply.to_string());

    // Now attempt begin_voice at exhaustion limit
    let res_str = bridge.dispatch_json(json!({"type": "begin_voice"}).to_string());
    let res: serde_json::Value = serde_json::from_str(&res_str).unwrap();
    assert_eq!(res["ok"], false);
    assert!(
        res["error"]
            .as_str()
            .unwrap()
            .contains("Identifiants insuffisants")
            || res["error"].as_str().unwrap().contains("épuisés")
    );
}

/// `T2_F04_04`: Cancelling during pending voice request.
///
/// Calling cancel_voice while voice is pending interrupts the flight and clears pending.
#[tokio::test]
async fn test_t2_f04_04_cancelling_during_pending_voice_request() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        daemon
            .handle_step(&mut stream, 1, DaemonStep::status())
            .await
            .expect("status");
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;
    daemon_task.await.unwrap();

    bridge.dispatch_json(json!({"type": "begin_voice"}).to_string());
    let snap = rig.snapshot(&bridge);
    let ctx_id = snap["voice_context_id"].as_u64().unwrap();

    let sub_str = bridge.dispatch_json(
        json!({
            "type": "submit_voice",
            "context_id": ctx_id,
            "text": "Requête vocale à annuler"
        })
        .to_string(),
    );
    assert!(sub_str.contains("\"ok\":true"));

    let snap_pending = rig.snapshot(&bridge);
    assert!(snap_pending["pending"].is_object());

    // Cancel voice while pending
    let cancel_res = bridge.dispatch_json(json!({"type": "cancel_voice"}).to_string());
    assert!(cancel_res.contains("\"ok\":true"));

    let snap_after = rig.snapshot(&bridge);
    assert!(
        snap_after["pending"].is_null(),
        "Pending must be cleared upon cancel"
    );
    assert!(snap_after["voice_context_id"].is_null());
}

/// `T2_F04_05`: UTF-8 surrogate and emoji fuzzing in voice transcript.
///
/// Complex multilingual characters, ZWJ sequences, and emojis in voice transcripts
/// are preserved without data loss or corruption.
#[tokio::test]
async fn test_t2_f04_05_utf8_emoji_and_multilingual_voice_fuzzing() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        daemon
            .handle_step(&mut stream, 1, DaemonStep::status())
            .await
            .expect("status");
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;
    daemon_task.await.unwrap();

    let complex_transcripts = [
        "Nangadef ak diam! 🇸🇳 Allume la lueur du salon ✨💡🔥",
        "مرحبا بك في نظام هاكيلي السيادي 🌟",
        "Événement à l'échéance : café et croissant 🥐☕️",
        "Family ZWJ: 👨‍👩‍👧‍👦 and flags: 🇫🇷 🇸🇳 🇺🇳",
    ];

    for transcript in complex_transcripts {
        bridge.dispatch_json(json!({"type": "begin_voice"}).to_string());
        let snap = rig.snapshot(&bridge);
        let ctx_id = snap["voice_context_id"].as_u64().unwrap();

        let sub_res_str = bridge.dispatch_json(
            json!({
                "type": "submit_voice",
                "context_id": ctx_id,
                "text": transcript
            })
            .to_string(),
        );
        let sub_res: serde_json::Value = serde_json::from_str(&sub_res_str).unwrap();
        assert_eq!(sub_res["ok"], true);

        let req_frame: Frame = serde_json::from_value(sub_res["request"].clone()).unwrap();
        match req_frame.body {
            hakille_ipc::FrameBody::Request { message } => match message {
                UiToKernel::HudSubmit { query } => assert_eq!(query, transcript),
                other => panic!("expected HudSubmit, got {other:?}"),
            },
            other => panic!("expected Request body, got {other:?}"),
        }

        // Clean up pending and clear draft so next iteration can proceed
        bridge.dispatch_json(
            json!({
                "type": "fail_before_send",
                "request_id": req_frame.id,
                "message": "test iteration reset"
            })
            .to_string(),
        );
        bridge.dispatch_json(json!({"type": "set_draft", "text": ""}).to_string());
    }
}
