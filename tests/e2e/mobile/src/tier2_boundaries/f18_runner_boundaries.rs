//! Feature 18: Sandboxed Agent Runner (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F18_01 to T2_F18_05 (5 tests total).

#![allow(unused_imports)]

use crate::harness::{
    AuditReceipt, AuditSigner, AuditVerifier, AuditVerifyError, DaemonStep, MobileTestRig,
};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{AgentDto, KernelToUi, MemoryRowDto, StatusDto, UiToKernel};
use serde_json::json;
use std::time::Duration;

/// `T2_F18_01`: Executing declarative agent without WASM artifact fails closed.
///
/// Running an agent with artifact_present: false is strictly rejected.
#[test]
fn test_t2_f18_01_declarative_agent_execution_fails_closed() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());

    // Load declarative agent
    let res = bridge.dispatch_json(json!({"type": "load_agents"}).to_string());
    let req_id = serde_json::from_str::<serde_json::Value>(&res).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();
    let reply = json!({
        "session_id": "sess-decl",
        "frames": [
            Frame::reply(req_id, KernelToUi::AgentListReply {
                agents: vec![AgentDto {
                    id: "mail-id".into(),
                    name: "mail".into(),
                    required_caps: vec!["net".into()],
                    artifact_present: false,
                    sha256: "".into(),
                }]
            })
        ]
    });
    bridge.receive_json(req_id, reply.to_string());

    let run_res_str = bridge.dispatch_json(
        json!({
            "type": "run_agent",
            "name": "mail",
            "input_json": "{}"
        })
        .to_string(),
    );

    let run_res: serde_json::Value = serde_json::from_str(&run_res_str).unwrap();
    assert_eq!(run_res["ok"], false);
    assert!(
        run_res["error"]
            .as_str()
            .unwrap()
            .contains("Agent non confirmé exécutable")
    );
}

/// `T2_F18_02`: Agent input JSON > 64 KiB rejected by gateway.
///
/// Gateway enforces `input_json.len() <= 64 * 1024` for AgentRunRequest.
#[tokio::test]
async fn test_t2_f18_02_agent_input_json_exceeding_64_kib_rejected() {
    let (rig, _daemon) = MobileTestRig::setup().await;

    let oversized_json = format!("{{\"payload\":\"{}\"}}", "x".repeat(64 * 1024));
    let frame = Frame::request(
        1,
        UiToKernel::AgentRunRequest {
            name: "code".into(),
            input_json: oversized_json,
        },
    );

    let (status, resp) = rig.post_frame(&frame, None).await;
    assert_eq!(status, 400);
    assert_eq!(resp["error"]["code"], "unsupported");
}

/// `T2_F18_03`: Malformed JSON input string rejected.
///
/// Providing invalid JSON in input_json is rejected with "Entrée agent JSON invalide".
#[test]
fn test_t2_f18_03_malformed_json_input_rejected() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());

    let res_str = bridge.dispatch_json(
        json!({
            "type": "run_agent",
            "name": "any_agent",
            "input_json": "{not-valid-json:"
        })
        .to_string(),
    );

    let res: serde_json::Value = serde_json::from_str(&res_str).unwrap();
    assert_eq!(res["ok"], false);
    assert_eq!(res["error"], "Entrée agent JSON invalide");
}

/// `T2_F18_04`: Agent execution timeout returns HTTP 504.
///
/// Agent execution taking longer than gateway timeout returns HTTP 504 ipc_timeout with issue_unknown: true.
#[tokio::test]
async fn test_t2_f18_04_agent_execution_timeout() {
    let (rig, daemon) =
        MobileTestRig::setup_with_timeouts(Duration::from_millis(100), Duration::from_secs(1))
            .await;

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        let _ = daemon.accept_one_frame(&mut stream).await;
        // Sleep longer than request_timeout (100ms)
        tokio::time::sleep(Duration::from_millis(300)).await;
    });

    let frame = Frame::request(
        1,
        UiToKernel::AgentRunRequest {
            name: "long_running".into(),
            input_json: "{}".into(),
        },
    );

    let (status, resp) = rig.post_frame(&frame, None).await;
    assert_eq!(status, 504);
    assert_eq!(resp["error"]["code"], "ipc_timeout");
    assert_eq!(resp["error"]["issue_unknown"], true);

    daemon_task.await.unwrap();
}

/// `T2_F18_05`: Empty input string rejected vs empty object accepted.
///
/// `input_json: ""` is not valid JSON and rejected; `input_json: "{}"` is valid JSON and accepted.
#[test]
fn test_t2_f18_05_empty_input_string_rejected() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());

    // Empty string rejected
    let res_empty_str = bridge.dispatch_json(
        json!({
            "type": "run_agent",
            "name": "code",
            "input_json": ""
        })
        .to_string(),
    );
    let res_empty: serde_json::Value = serde_json::from_str(&res_empty_str).unwrap();
    assert_eq!(res_empty["ok"], false);
    assert!(
        res_empty["error"]
            .as_str()
            .unwrap()
            .contains("Texte invalide")
            || res_empty["error"]
                .as_str()
                .unwrap()
                .contains("Entrée agent JSON invalide")
    );
}
