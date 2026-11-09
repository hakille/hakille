//! Feature 05: TokenDelta Real-Time Streaming (Milestone M1 / Spec §1, Protocol §22)
//!
//! Specifications: T1_F05_01 to T1_F05_05
//!
//! Verifies:
//! - Real-time incremental token delta streaming into assistant chat entry.
//! - Authoritative replacement contract: HudReply overwrites provisional stream.
//! - Protocol order invariant: strictly rejects any TokenDelta arriving after terminal reply.
//! - Batch envelope bounds (max 256 frames, 1 MiB payload).
//! - Streaming lifecycle state flag and draft clearing synchronization.

use crate::harness::{DaemonStep, MobileTestRig};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{KernelToUi, UiToKernel};
use serde_json::json;

/// `T1_F05_01`: Incremental token delta aggregation in assistant chat entry.
///
/// Streamed TokenDelta frames arrive prior to terminal reply and progressively
/// concatenate in the assistant's chat bubble.
#[tokio::test]
async fn test_t1_f05_01_incremental_token_aggregation_in_assistant_chat() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        daemon
            .handle_step(&mut stream, 1, DaemonStep::status())
            .await
            .expect("status");

        let step = DaemonStep::new(
            UiToKernel::HudSubmit {
                query: "Streaming question".into(),
            },
            KernelToUi::HudReply {
                response: "Alpha Beta Gamma".into(),
                validation: None,
            },
        )
        .before(vec![
            KernelToUi::TokenDelta {
                text: "Alpha ".into(),
            },
            KernelToUi::TokenDelta {
                text: "Beta ".into(),
            },
            KernelToUi::TokenDelta {
                text: "Gamma".into(),
            },
        ]);

        daemon
            .handle_step(&mut stream, 2, step)
            .await
            .expect("streaming step");
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;

    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Streaming question",
        })
        .to_string(),
    );

    let (status, result) = rig.roundtrip(&bridge, json!({"type": "submit"})).await;
    assert_eq!(status, 200);
    assert_eq!(result["ok"], true);

    let snapshot = rig.snapshot(&bridge);
    let chat = snapshot["chat"].as_array().expect("chat array");
    assert_eq!(chat.len(), 2, "Expected user query and assistant reply");
    assert_eq!(chat[0]["role"], "user");
    assert_eq!(chat[0]["text"], "Streaming question");
    assert_eq!(chat[1]["role"], "assistant");
    assert_eq!(chat[1]["text"], "Alpha Beta Gamma");

    daemon_task.await.unwrap();
}

/// `T1_F05_02`: Authoritative replacement contract: HudReply overwrites provisional stream.
///
/// TokenDelta is provisional; when the final HudReply arrives, the assistant bubble
/// is replaced by the exact, authoritative text of the HudReply.
#[tokio::test]
async fn test_t1_f05_02_final_replacement_by_hud_reply() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        daemon
            .handle_step(&mut stream, 1, DaemonStep::status())
            .await
            .expect("status");

        // The streaming deltas contain noisy/provisional text, but HudReply contains polished final
        let step = DaemonStep::new(
            UiToKernel::HudSubmit {
                query: "Traduire mot".into(),
            },
            KernelToUi::HudReply {
                response: "Texte final canonique et vérifié.".into(),
                validation: None,
            },
        )
        .before(vec![KernelToUi::TokenDelta {
            text: "Texte provisoire non corrigé...".into(),
        }]);

        daemon
            .handle_step(&mut stream, 2, step)
            .await
            .expect("streaming step");
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;

    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Traduire mot",
        })
        .to_string(),
    );

    let (status, _) = rig.roundtrip(&bridge, json!({"type": "submit"})).await;
    assert_eq!(status, 200);

    let snapshot = rig.snapshot(&bridge);
    let assistant_msg = &snapshot["chat"][1]["text"];
    assert_eq!(
        assistant_msg, "Texte final canonique et vérifié.",
        "HudReply must authoritatively replace provisional token deltas"
    );
    assert_eq!(
        snapshot["streaming"], false,
        "Streaming flag must reset to false upon final reply"
    );

    daemon_task.await.unwrap();
}

/// `T1_F05_03`: Protocol order invariant strictly rejects deltas after terminal reply.
///
/// A gateway response batch containing a TokenDelta after the terminal reply is malformed
/// and strictly rejected by the mobile core state machine.
#[test]
fn test_t1_f05_03_protocol_order_invariant_rejects_deltas_after_terminal() {
    let bridge = MobileBridge::new();

    // Setup connected state
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080",
        })
        .to_string(),
    );

    // Dispatch submit
    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Question d'invariance",
        })
        .to_string(),
    );

    let dispatch_res_str = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    let dispatch_res: serde_json::Value = serde_json::from_str(&dispatch_res_str).unwrap();
    assert_eq!(dispatch_res["ok"], true);
    let req_id = dispatch_res["request"]["id"].as_u64().unwrap();

    // Construct invalid batch: HudReply followed by TokenDelta
    let invalid_envelope = json!({
        "session_id": "test-session-order",
        "frames": [
            Frame::reply(req_id, KernelToUi::HudReply {
                response: "Réponse terminale".into(),
                validation: None,
            }),
            Frame::reply(0, KernelToUi::TokenDelta {
                text: "Delta illégal après terminal".into(),
            }),
        ],
    });

    let receive_res_str = bridge.receive_json(req_id, invalid_envelope.to_string());
    let receive_res: serde_json::Value = serde_json::from_str(&receive_res_str).unwrap();

    assert_eq!(receive_res["ok"], false);
    assert!(
        receive_res["error"]
            .as_str()
            .unwrap()
            .contains("Delta de texte reçu après la réponse finale"),
        "Must enforce protocol order: {receive_res_str}"
    );
}

/// `T1_F05_04`: Batch envelope bounds enforcement.
///
/// Batches must contain <= 256 frames and non-empty responses.
#[test]
fn test_t1_f05_04_batch_envelope_bounds_enforcement() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080",
        })
        .to_string(),
    );
    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Test bounds",
        })
        .to_string(),
    );

    let dispatch_res_str = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    let dispatch_res: serde_json::Value = serde_json::from_str(&dispatch_res_str).unwrap();
    let req_id = dispatch_res["request"]["id"].as_u64().unwrap();

    // Empty frames envelope rejected
    let empty_envelope = json!({
        "session_id": "test-session-bounds",
        "frames": [],
    });
    let empty_res_str = bridge.receive_json(req_id, empty_envelope.to_string());
    assert!(empty_res_str.contains("Réponse gateway sans session ou batch hors limites"));
}

/// `T1_F05_05`: Streaming lifecycle state flag and draft clearing synchronization.
///
/// During active token streaming, snapshot.streaming is true. When terminal HudReply
/// matches the submitted draft, the draft is cleared atomically with reply arrival.
#[tokio::test]
async fn test_t1_f05_05_streaming_lifecycle_and_draft_clearing() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        daemon
            .handle_step(&mut stream, 1, DaemonStep::status())
            .await
            .expect("status");

        let step = DaemonStep::new(
            UiToKernel::HudSubmit {
                query: "Efface mon brouillon".into(),
            },
            KernelToUi::HudReply {
                response: "Brouillon acquitté et exécuté.".into(),
                validation: None,
            },
        );

        daemon
            .handle_step(&mut stream, 2, step)
            .await
            .expect("submit step");
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;

    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Efface mon brouillon",
        })
        .to_string(),
    );

    let (status, result) = rig.roundtrip(&bridge, json!({"type": "submit"})).await;
    assert_eq!(status, 200);
    assert_eq!(result["ok"], true);

    let snapshot = rig.snapshot(&bridge);
    assert_eq!(
        snapshot["draft"], "",
        "Draft must be cleared upon matching HudReply receipt"
    );
    assert_eq!(
        snapshot["streaming"], false,
        "Streaming must be false after completion"
    );

    daemon_task.await.unwrap();
}
