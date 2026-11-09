//! Feature 09: Action Card Reject & Edit (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F09_01 to T2_F09_05 (5 tests total).

#![allow(unused_imports)]

use crate::harness::{DaemonStep, MobileTestRig, default_card};
use hakille_design::action_card::{ActionCardView, DataPastille, Reversibility};
use hakille_design::autonomy::{Crank, DomainAutonomy, ENVELOPE_EXPIRY_DAYS, always_validated};
use hakille_design::hold::{HoldEvent, HoldState, feed_hold, hold_progress};
use hakille_design::provenance::{ExternalBlock, ProvenanceBand, sanitize_skill_text};
use hakille_design::why::{FactRef, WhySheet};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{ActionCard, DataExit, KernelToUi, Money, Provenance, UiToKernel};
use serde_json::json;

/// `T2_F09_01`: Missing or mismatched X-Hakille-Session header returns HTTP 409 stale_session.
///
/// Posting Approve, Edit, or Reject without a session header or with mismatched session
/// returns HTTP 409 stale_session.
#[tokio::test]
async fn test_t2_f09_01_missing_or_mismatched_session_returns_409() {
    let (rig, daemon) = MobileTestRig::setup().await;

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        daemon
            .handle_step(&mut stream, 1, DaemonStep::status())
            .await
            .expect("status");
    });

    let frame = Frame::request(1, UiToKernel::Reject { card_id: 101 });

    // Subcase A: Missing session header
    let (status_missing, resp_missing) = rig.post_frame(&frame, None).await;
    assert_eq!(status_missing, 409, "Missing session must return 409");
    assert_eq!(resp_missing["error"]["code"], "stale_session");

    // Subcase B: Mismatched session header
    let (status_mismatch, resp_mismatch) =
        rig.post_frame(&frame, Some("bogus-session-token")).await;
    assert_eq!(status_mismatch, 409, "Mismatched session must return 409");
    assert_eq!(resp_mismatch["error"]["code"], "stale_session");

    drop(daemon_task);
}

/// `T2_F09_02`: Oversized frame payload exceeding limit.
///
/// Frame payload exceeding 1 MiB is rejected with HTTP 413 before IPC forwarding:
/// - `Content-Length > 1 MiB` is rejected immediately on headers (`too_large`, tested with
///   `Expect: 100-continue` so Windows loopback does not race body upload against socket close
///   with `WSAECONNABORTED` 10053).
/// - Streaming `Transfer-Encoding: chunked` > 1 MiB without `Content-Length` is rejected by
///   the bounded reader (`invalid_body`).
#[tokio::test]
async fn test_t2_f09_02_oversized_payload_rejected_with_413() {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    let (rig, daemon) = MobileTestRig::setup().await;

    // 1. Content-Length > 1 MiB rejected before reading body (`too_large`)
    let mut socket = tokio::net::TcpStream::connect(rig.address)
        .await
        .expect("connect to gateway");
    let headers = format!(
        "POST /v1/request HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nExpect: 100-continue\r\n\r\n",
        rig.address,
        rig.mobile_bearer,
        (1usize << 20) + 100
    );
    socket
        .write_all(headers.as_bytes())
        .await
        .expect("write headers");
    let mut raw_resp = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        socket.read_to_string(&mut raw_resp),
    )
    .await
    .expect("413 header response timeout")
    .expect("read 413 header response");
    assert!(
        raw_resp.starts_with("HTTP/1.1 413"),
        "Oversized Content-Length must return HTTP 413, got: {raw_resp}"
    );
    let (_, body_str) = raw_resp
        .split_once("\r\n\r\n")
        .expect("HTTP headers/body delimiter");
    let resp: serde_json::Value = serde_json::from_str(body_str).expect("parse 413 json");
    assert_eq!(resp["error"]["code"], "too_large");
    assert_eq!(resp["error"]["issue_unknown"], false);

    // 2. Chunked body > 1 MiB without Content-Length rejected with HTTP 413
    let mut chunked_sock = tokio::net::TcpStream::connect(rig.address)
        .await
        .expect("connect to gateway for chunked test");
    let mut chunked_req = format!(
        "POST /v1/request HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n100001\r\n",
        rig.address, rig.mobile_bearer
    )
    .into_bytes();
    chunked_req.extend(std::iter::repeat_n(b'x', (1usize << 20) + 1));
    chunked_req.extend_from_slice(b"\r\n0\r\n\r\n");
    // On Windows loopback the server may close right after reading > 1 MiB, aborting the tail write;
    // reading the status line still yields the valid HTTP/1.1 413 response.
    let _ = chunked_sock.write_all(&chunked_req).await;
    let mut status_line = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        BufReader::new(chunked_sock).read_line(&mut status_line),
    )
    .await
    .expect("413 chunked response timeout")
    .expect("read 413 chunked status line");
    assert!(
        status_line.starts_with("HTTP/1.1 413"),
        "Oversized chunked body must return HTTP 413, got: {status_line}"
    );

    // Confirm neither oversized payload ever reached the IPC daemon
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), daemon.accept())
            .await
            .is_err(),
        "Oversized HTTP requests must never open an IPC connection to the daemon"
    );
}

/// `T2_F09_03`: Reject on already approved card.
///
/// Once a card is approved and confirmed, attempting to reject that same card fails.
#[test]
fn test_t2_f09_03_reject_on_already_approved_card() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());

    let req_res = bridge.dispatch_json(json!({"type": "refresh_status"}).to_string());
    let req_id = serde_json::from_str::<serde_json::Value>(&req_res).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();
    let reply = json!({
        "session_id": "sess-approved",
        "frames": [
            Frame::reply(req_id, KernelToUi::StatusReply { status: crate::harness::default_status() }),
            Frame::reply(0, KernelToUi::ShowCard(default_card(77)))
        ]
    });
    bridge.receive_json(req_id, reply.to_string());

    // Start approval hold and hold for 600ms
    let app_res =
        bridge.dispatch_json(json!({"type": "begin_approval", "card_id": 77}).to_string());
    assert!(app_res.contains("\"ok\":true"));
    std::thread::sleep(std::time::Duration::from_millis(610));

    // Approve card after genuine 600ms hold
    let app_res_str = bridge.dispatch_json(json!({"type": "approve", "card_id": 77}).to_string());
    let app_req_id =
        serde_json::from_str::<serde_json::Value>(&app_res_str).unwrap()["request"]["id"]
            .as_u64()
            .unwrap();

    let ack_reply = json!({
        "session_id": "sess-approved",
        "frames": [
            Frame::reply(app_req_id, KernelToUi::CardDecisionAck {
                card_id: 77,
                approved: true,
                audit_id: "audit-app-77".into(),
                effect: None,
            })
        ]
    });
    bridge.receive_json(app_req_id, ack_reply.to_string());

    // Card 77 was evicted from snapshot
    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert!(snap["cards"].as_array().unwrap().is_empty());

    // Attempting to reject card 77 now must fail
    let rej_res_str = bridge.dispatch_json(json!({"type": "reject", "card_id": 77}).to_string());
    let rej_res: serde_json::Value = serde_json::from_str(&rej_res_str).unwrap();
    assert_eq!(rej_res["ok"], false);
    assert_eq!(rej_res["error"], "Carte absente ou périmée");
}

/// `T2_F09_04`: Double reject attempt.
///
/// First reject succeeds; second reject on same card is rejected as non-existent.
#[test]
fn test_t2_f09_04_double_reject_attempt() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());

    let req_res = bridge.dispatch_json(json!({"type": "refresh_status"}).to_string());
    let req_id = serde_json::from_str::<serde_json::Value>(&req_res).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();
    let reply = json!({
        "session_id": "sess-double-rej",
        "frames": [
            Frame::reply(req_id, KernelToUi::StatusReply { status: crate::harness::default_status() }),
            Frame::reply(0, KernelToUi::ShowCard(default_card(88)))
        ]
    });
    bridge.receive_json(req_id, reply.to_string());

    // First reject
    let rej1 = bridge.dispatch_json(json!({"type": "reject", "card_id": 88}).to_string());
    assert!(rej1.contains("\"ok\":true"));

    // Complete reject
    let rej_ack = json!({
        "session_id": "sess-double-rej",
        "frames": [
            Frame::reply(2, KernelToUi::CardDecisionAck {
                card_id: 88,
                approved: false,
                audit_id: "audit-rej-88".into(),
                effect: None,
            })
        ]
    });
    let _ = bridge.dispatch_json(json!({"type": "refresh_status"}).to_string());
    bridge.receive_json(2, rej_ack.to_string());

    // Second reject
    let rej2_str = bridge.dispatch_json(json!({"type": "reject", "card_id": 88}).to_string());
    let rej2: serde_json::Value = serde_json::from_str(&rej2_str).unwrap();
    assert_eq!(rej2["ok"], false);
    assert_eq!(rej2["error"], "Carte absente ou périmée");
}

/// `T2_F09_05`: Rejecting while gateway is offline / disconnected.
///
/// If bridge has no hub configured or is disconnected, rejecting a card fails closed.
#[test]
fn test_t2_f09_05_rejecting_while_gateway_offline() {
    let bridge = MobileBridge::new();
    // No hub configured
    let res_str = bridge.dispatch_json(json!({"type": "reject", "card_id": 1}).to_string());
    let res: serde_json::Value = serde_json::from_str(&res_str).unwrap();
    assert_eq!(res["ok"], false);
    assert_eq!(res["error"], "Aucun hub configuré");
}
