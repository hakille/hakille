//! Feature 09: Action Card Reject & Edit (Milestone M1 / Spec §3, §8)
//!
//! Specifications: T1_F09_01 to T1_F09_05
//!
//! Verifies:
//! - Card rejection dispatches UiToKernel::Reject request frame.
//! - CardDecisionAck evicts rejected card from MobileSnapshot.cards immediately.
//! - Gateway card decision guard rejects requests with missing or stale session headers (HTTP 409).
//! - Rejection is immediate and does not require the 600 ms hold-to-confirm gesture.
//! - Attempting to reject a non-existent or expired card ID returns an explicit error.

use crate::harness::{DaemonStep, MobileTestRig, default_card};
use hakille_ipc::Frame;
use hakille_ui_protocol::{KernelToUi, UiToKernel};
use serde_json::json;

/// `T1_F09_01`: Card rejection dispatches UiToKernel::Reject request frame.
///
/// Dispatching MobileAction::Reject for an open card generates an authenticated
/// UiToKernel::Reject request targeting that card ID.
#[tokio::test]
async fn test_t1_f09_01_card_rejection_produces_ui_reject_request() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        let step = DaemonStep::status().after(vec![KernelToUi::ShowCard(default_card(101))]);
        daemon
            .handle_step(&mut stream, 1, step)
            .await
            .expect("status with card 101");
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;
    daemon_task.await.unwrap();

    let (frame, session) = rig.prepare_frame(
        &bridge,
        json!({
            "type": "reject",
            "card_id": 101,
        }),
    );

    assert!(session.is_some(), "Card reject requires gateway session");
    match frame.body {
        hakille_ipc::FrameBody::Request { message } => match message {
            UiToKernel::Reject { card_id } => assert_eq!(card_id, 101),
            other => panic!("expected UiToKernel::Reject, got {other:?}"),
        },
        other => panic!("expected Request body, got {other:?}"),
    }
}

/// `T1_F09_02`: CardDecisionAck evicts rejected card from snapshot.
///
/// When the daemon confirms rejection via CardDecisionAck, the card is immediately
/// evicted from snapshot.cards and approval state is cleared.
#[tokio::test]
async fn test_t1_f09_02_card_decision_ack_evicts_card_from_snapshot() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        // Step 1: Status returns card 101
        daemon
            .handle_step(
                &mut stream,
                1,
                DaemonStep::status().after(vec![KernelToUi::ShowCard(default_card(101))]),
            )
            .await
            .expect("step 1");

        // Step 2: Reject card 101
        let reject_step = DaemonStep::new(
            UiToKernel::Reject { card_id: 101 },
            KernelToUi::CardDecisionAck {
                card_id: 101,
                approved: false,
                audit_id: "audit-reject-101".into(),
                effect: None,
            },
        );
        daemon
            .handle_step(&mut stream, 2, reject_step)
            .await
            .expect("step 2 reject");
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;

    // Verify card is present before rejection
    let snap_before = rig.snapshot(&bridge);
    assert_eq!(snap_before["cards"].as_array().unwrap().len(), 1);

    // Execute rejection roundtrip
    let (status, result) = rig
        .roundtrip(&bridge, json!({"type": "reject", "card_id": 101}))
        .await;

    assert_eq!(status, 200);
    assert_eq!(result["ok"], true);

    // Verify card is evicted
    let snap_after = rig.snapshot(&bridge);
    assert!(
        snap_after["cards"].as_array().unwrap().is_empty(),
        "Rejected card must be evicted from snapshot.cards"
    );

    daemon_task.await.unwrap();
}

/// `T1_F09_03`: Gateway decision guard rejects missing or stale session headers.
///
/// A card decision (Approve, Edit, Reject) posted to /v1/request without the mandatory
/// X-Hakille-Session header or with an invalid session returns HTTP 409 stale_session.
#[tokio::test]
async fn test_t1_f09_03_stale_session_header_rejected_by_gateway() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        daemon
            .handle_step(
                &mut stream,
                1,
                DaemonStep::status().after(vec![KernelToUi::ShowCard(default_card(101))]),
            )
            .await
            .expect("step 1");
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;
    daemon_task.await.unwrap();

    let reject_frame = Frame::request(5, UiToKernel::Reject { card_id: 101 });

    // 1. Missing session header
    let resp_no_sess = rig
        .post_frame_raw(&reject_frame, None)
        .await
        .expect("send request");
    assert_eq!(
        resp_no_sess.status().as_u16(),
        409,
        "Missing session must return 409"
    );
    let body_no_sess: serde_json::Value = resp_no_sess.json().await.unwrap();
    assert_eq!(body_no_sess["error"]["code"], "stale_session");

    // 2. Mismatched session header
    let resp_bad_sess = rig
        .post_frame_raw(&reject_frame, Some("bogus-expired-session-id"))
        .await
        .expect("send request");
    assert_eq!(
        resp_bad_sess.status().as_u16(),
        409,
        "Mismatched session must return 409"
    );
}

/// `T1_F09_04`: Card rejection does not require hold-to-confirm.
///
/// Unlike Approve, Reject takes effect immediately without begin_approval
/// or waiting for the 600 ms threshold.
#[tokio::test]
async fn test_t1_f09_04_card_rejection_does_not_require_hold() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        daemon
            .handle_step(
                &mut stream,
                1,
                DaemonStep::status().after(vec![KernelToUi::ShowCard(default_card(101))]),
            )
            .await
            .expect("step 1");
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;
    daemon_task.await.unwrap();

    // Directly dispatch reject without begin_approval
    let res_str = bridge.dispatch_json(
        json!({
            "type": "reject",
            "card_id": 101,
        })
        .to_string(),
    );

    let res: serde_json::Value = serde_json::from_str(&res_str).unwrap();
    assert_eq!(
        res["ok"], true,
        "Reject must succeed immediately without hold: {res_str}"
    );
    assert_eq!(res["snapshot"]["pending"]["kind"], "reject");
}

/// `T1_F09_05`: Rejection of non-existent card ID fails with explicit error.
///
/// Attempting to reject a card ID not present in snapshot.cards returns
/// "Carte absente ou périmée".
#[tokio::test]
async fn test_t1_f09_05_rejection_of_nonexistent_card_fails() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        daemon
            .handle_step(&mut stream, 1, DaemonStep::status())
            .await
            .expect("step 1");
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;
    daemon_task.await.unwrap();

    let bad_reject_res = bridge.dispatch_json(
        json!({
            "type": "reject",
            "card_id": 99999,
        })
        .to_string(),
    );

    assert!(
        bad_reject_res.contains("Carte absente ou périmée"),
        "Must fail when card not found: {bad_reject_res}"
    );
}
