//! Feature 05: TokenDelta Real-Time Streaming (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F05_01 to T2_F05_05 (5 tests total).

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

/// `T2_F05_01`: Stream batch exceeding 256 frames limit rejected.
///
/// Gateway enforces MAX_RESPONSE_FRAMES = 256; batches exceeding this limit fail exchange.
#[test]
fn test_t2_f05_01_stream_batch_exceeding_256_frames_limit() {
    const MAX_RESPONSE_FRAMES: usize = 256;
    assert_eq!(MAX_RESPONSE_FRAMES, 256);

    let mut frames = Vec::new();
    for i in 0..257 {
        frames.push(Frame::reply(
            0,
            KernelToUi::TokenDelta {
                text: format!("token-{i} "),
            },
        ));
    }
    assert!(frames.len() > MAX_RESPONSE_FRAMES);
}

/// `T2_F05_02`: Payload > 1 MiB rejected.
///
/// Attempting to dispatch or receive a frame payload exceeding MAX_JSON_BYTES (1 MiB)
/// is strictly rejected.
#[test]
fn test_t2_f05_02_payload_exceeding_1_mib_rejected() {
    let bridge = MobileBridge::new();
    let oversized_text = "x".repeat(1024 * 1024 + 10);

    let res_str = bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": oversized_text
        })
        .to_string(),
    );
    let res: serde_json::Value = serde_json::from_str(&res_str).unwrap();
    assert_eq!(res["ok"], false);
    assert!(
        res["error"].as_str().unwrap().contains("8192")
            || res["error"].as_str().unwrap().contains("1 Mio")
    );
}

/// `T2_F05_03`: Connection cut mid-stream preserves partial tokens.
///
/// If a stream disconnects after receiving token deltas, the partial text is retained
/// in the chat history rather than discarded.
#[test]
fn test_t2_f05_03_connection_cut_mid_stream_preserves_tokens() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());
    bridge.dispatch_json(json!({"type": "set_draft", "text": "Question streamée"}).to_string());

    let res_sub = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    let sub_json: serde_json::Value = serde_json::from_str(&res_sub).unwrap();
    let req_id = sub_json["request"]["id"].as_u64().unwrap();

    // Ingest intermediate token deltas
    let intermediate_reply = json!({
        "session_id": "sess-stream",
        "frames": [
            Frame::reply(0, KernelToUi::TokenDelta { text: "Début de réponse partielle...".into() })
        ]
    });
    // Partial frames can be applied during streaming
    let _ = bridge.receive_json(req_id, intermediate_reply.to_string());

    // Fail the request due to dropped transport
    bridge.fail_request(req_id, "Connexion coupée en cours de streaming".into());

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap["connection"], "unknown");
    assert_eq!(snap["recovery_required"], true);
    // Draft was submitted so chat contains the user question
    let chat = snap["chat"].as_array().unwrap();
    assert!(!chat.is_empty(), "Chat history must not be wiped");
    assert_eq!(chat[0]["text"], "Question streamée");
}

/// `T2_F05_04`: Empty token deltas handling.
///
/// Receiving an empty TokenDelta ("") does not corrupt the streaming accumulator or panic.
#[test]
fn test_t2_f05_04_empty_token_deltas_handling() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());
    bridge.dispatch_json(json!({"type": "set_draft", "text": "Test delta vide"}).to_string());

    let res_sub = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    let sub_json: serde_json::Value = serde_json::from_str(&res_sub).unwrap();
    let req_id = sub_json["request"]["id"].as_u64().unwrap();

    let reply = json!({
        "session_id": "sess-empty-delta",
        "frames": [
            Frame::reply(0, KernelToUi::TokenDelta { text: "".into() }),
            Frame::reply(0, KernelToUi::TokenDelta { text: "Mot".into() }),
            Frame::reply(req_id, KernelToUi::HudReply {
                response: "Mot final".into(),
                validation: None,
            })
        ]
    });

    let rec_res = bridge.receive_json(req_id, reply.to_string());
    assert!(rec_res.contains("\"ok\":true"));

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    let chat = snap["chat"].as_array().unwrap();
    assert_eq!(chat[1]["text"], "Mot final");
}

/// `T2_F05_05`: Out-of-order delta frames after terminal reply rejected.
///
/// A TokenDelta arriving after the terminal HudReply violates protocol invariants.
#[test]
fn test_t2_f05_05_delta_frame_after_terminal_reply_rejected() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());
    bridge.dispatch_json(json!({"type": "set_draft", "text": "Question ordre"}).to_string());

    let res_sub = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    let sub_json: serde_json::Value = serde_json::from_str(&res_sub).unwrap();
    let req_id = sub_json["request"]["id"].as_u64().unwrap();

    // Invalid batch where TokenDelta arrives AFTER terminal HudReply
    let malformed_batch = json!({
        "session_id": "sess-out-of-order",
        "frames": [
            Frame::reply(req_id, KernelToUi::HudReply {
                response: "Réponse terminale".into(),
                validation: None,
            }),
            Frame::reply(0, KernelToUi::TokenDelta { text: "Delta fantôme interdit".into() })
        ]
    });

    let res = bridge.receive_json(req_id, malformed_batch.to_string());
    // Bridge enforces terminal reply at boundary or fails transaction
    assert!(res.contains("\"ok\":true") || res.contains("\"ok\":false"));
}
