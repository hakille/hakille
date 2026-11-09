//! Feature 04: Hub Conversation Mode (Milestone M1 / Spec §2, ADR-011)
//!
//! Specifications: T1_F04_01 to T1_F04_05
//!
//! Verifies:
//! - Voice arming sequence generating fresh monotonic context IDs.
//! - Correlated voice submission initiating HudSubmit with tracked voice_request_id.
//! - Authoritative VoiceReply population matching the exact armed turn.
//! - Pre-condition gating (rejects voice when draft or action cards are pending).
//! - Single-use context ID enforcement and explicit cancel invalidation.

use crate::harness::{DaemonStep, MobileTestRig, default_card};
use hakille_ipc::Frame;
use hakille_ui_protocol::{KernelToUi, UiToKernel};
use serde_json::json;

/// `T1_F04_01`: Hub arming sequence generates fresh monotonic context ID.
///
/// Calling `begin_voice` when connected generates a fresh `voice_context_id`
/// and advances the internal monotonic request ID counter.
#[tokio::test]
async fn test_t1_f04_01_hub_arming_generates_fresh_monotonic_context() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        daemon
            .handle_step(&mut stream, 1, DaemonStep::status())
            .await
            .expect("status");
    });

    let (status, _) = rig
        .roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;
    assert_eq!(status, 200);
    daemon_task.await.unwrap();

    // Now bridge is connected with gateway session
    let arm_res_str = bridge.dispatch_json(json!({"type": "begin_voice"}).to_string());
    let arm_res: serde_json::Value = serde_json::from_str(&arm_res_str).unwrap();
    assert_eq!(
        arm_res["ok"], true,
        "begin_voice must succeed: {arm_res_str}"
    );

    let snapshot = rig.snapshot(&bridge);
    let context_id = snapshot["voice_context_id"]
        .as_u64()
        .expect("voice_context_id must be assigned");
    assert!(
        context_id >= 2,
        "context_id must be monotonic (after status id 1)"
    );
}

/// `T1_F04_02`: Voice submission initiates correlated HudSubmit frame.
///
/// Submitting voice with the armed context_id produces a HudSubmit request frame
/// and tracks `voice_request_id` in the snapshot.
#[tokio::test]
async fn test_t1_f04_02_voice_submission_initiates_correlated_request() {
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

    // Submit voice
    let voice_query = "Allume la lueur du salon";
    let sub_res_str = bridge.dispatch_json(
        json!({
            "type": "submit_voice",
            "context_id": ctx_id,
            "text": voice_query,
        })
        .to_string(),
    );

    let sub_res: serde_json::Value = serde_json::from_str(&sub_res_str).unwrap();
    assert_eq!(sub_res["ok"], true);

    let frame: Frame = serde_json::from_value(sub_res["request"].clone()).unwrap();
    match frame.body {
        hakille_ipc::FrameBody::Request { message } => match message {
            UiToKernel::HudSubmit { query } => assert_eq!(query, voice_query),
            other => panic!("expected HudSubmit, got {other:?}"),
        },
        other => panic!("expected Request body, got {other:?}"),
    }

    let snap2 = rig.snapshot(&bridge);
    assert_eq!(snap2["voice_request_id"], frame.id);
    assert_eq!(snap2["pending"]["request_id"], frame.id);
}

/// `T1_F04_03`: Correlated HudReply populates authoritative VoiceReply.
///
/// When the kernel returns HudReply matching voice_request_id, the bridge populates
/// snapshot.voice_reply with exact context_id, request_id, and response text.
#[tokio::test]
async fn test_t1_f04_03_correlated_hud_reply_populates_voice_reply() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        // Step 1: status
        daemon
            .handle_step(&mut stream, 1, DaemonStep::status())
            .await
            .expect("status");

        // Step 2: voice submit reply
        let reply_step = DaemonStep::new(
            UiToKernel::HudSubmit {
                query: "Quelle est la température ?".into(),
            },
            KernelToUi::HudReply {
                response: "Il fait 21 degrés dans le séjour.".into(),
                validation: None,
            },
        );
        daemon
            .handle_step(&mut stream, 3, reply_step)
            .await
            .expect("voice reply step");
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;

    // Arm voice (ctx_id = 2, next request will be 3)
    bridge.dispatch_json(json!({"type": "begin_voice"}).to_string());
    let snap = rig.snapshot(&bridge);
    let ctx_id = snap["voice_context_id"].as_u64().unwrap();

    let (status, result) = rig
        .roundtrip(
            &bridge,
            json!({
                "type": "submit_voice",
                "context_id": ctx_id,
                "text": "Quelle est la température ?",
            }),
        )
        .await;

    assert_eq!(status, 200);
    assert_eq!(result["ok"], true);

    let snap_after = rig.snapshot(&bridge);
    let voice_reply = &snap_after["voice_reply"];
    assert!(
        !voice_reply.is_null(),
        "voice_reply must be populated on bridge snapshot"
    );
    assert_eq!(voice_reply["context_id"], ctx_id);
    assert_eq!(voice_reply["request_id"], 3);
    assert_eq!(voice_reply["text"], "Il fait 21 degrés dans le séjour.");

    daemon_task.await.unwrap();
}

/// `T1_F04_04`: Pre-condition checks reject voice when draft or cards are present.
///
/// Voice interaction requires an unburdened sovereign screen: non-empty draft
/// or pending decision cards strictly prevent arming.
#[tokio::test]
async fn test_t1_f04_04_preconditions_reject_voice_when_draft_or_cards_present() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        // Status returning a card in after
        let step = DaemonStep::status().after(vec![KernelToUi::ShowCard(default_card(55))]);
        daemon
            .handle_step(&mut stream, 1, step)
            .await
            .expect("status with card");
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;
    daemon_task.await.unwrap();

    // 1. Fails when cards are present
    let res_with_card = bridge.dispatch_json(json!({"type": "begin_voice"}).to_string());
    assert!(
        res_with_card.contains("Tour vocal indisponible"),
        "Must reject when cards present: {res_with_card}"
    );

    // 2. Fails when draft is present
    let (rig2, daemon2) = MobileTestRig::setup().await;
    let bridge2 = rig2.new_bridge();
    let daemon_task2 = tokio::spawn(async move {
        let mut stream = daemon2.accept().await.expect("accept");
        daemon2
            .handle_step(&mut stream, 1, DaemonStep::status())
            .await
            .expect("status");
    });
    rig2.roundtrip(&bridge2, json!({"type": "refresh_status"}))
        .await;
    daemon_task2.await.unwrap();

    bridge2.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Brouillon non envoyé",
        })
        .to_string(),
    );
    let res_with_draft = bridge2.dispatch_json(json!({"type": "begin_voice"}).to_string());
    assert!(
        res_with_draft.contains("Un brouillon existe"),
        "Must reject when draft is non-empty: {res_with_draft}"
    );
}

/// `T1_F04_05`: Single-use context ID enforcement and explicit cancellation.
///
/// Context ID cannot be reused after submission or cancellation; stale IDs are rejected.
#[tokio::test]
async fn test_t1_f04_05_single_use_context_and_cancel_invalidation() {
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
    let snap = rig.snapshot(&bridge);
    let ctx_id = snap["voice_context_id"].as_u64().unwrap();

    // Explicit cancel
    let cancel_res = bridge.dispatch_json(json!({"type": "cancel_voice"}).to_string());
    assert!(cancel_res.contains("\"ok\":true"));

    let snap_cancelled = rig.snapshot(&bridge);
    assert!(
        snap_cancelled["voice_context_id"].is_null(),
        "Cancel must clear voice_context_id"
    );

    // Stale submit with old context_id
    let stale_sub = bridge.dispatch_json(
        json!({
            "type": "submit_voice",
            "context_id": ctx_id,
            "text": "Re-soumission interdite",
        })
        .to_string(),
    );

    assert!(
        stale_sub.contains("Contexte vocal ancien, absent ou déjà soumis"),
        "Must reject stale context ID: {stale_sub}"
    );
}
