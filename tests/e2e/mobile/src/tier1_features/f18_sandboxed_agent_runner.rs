//! Feature 18: Sandboxed Agent Runner (Milestone M1 / Spec §2, §23)
//!
//! Specifications: T1_F18_01 to T1_F18_05
//!
//! Verifies:
//! - Strict refusal to execute declarative-only agents (`artifact_present: false`).
//! - Validation of agent input JSON payload format.
//! - Dispatch of `UiToKernel::AgentRunRequest` for verified executable agents.
//! - Snapshot outcomes recording upon receiving `AgentRunReply`.
//! - Gateway payload bounds enforcement for agent execution requests.

use crate::harness::{DaemonStep, MobileTestRig};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{AgentDto, KernelToUi, UiToKernel};
use serde_json::json;

/// `T1_F18_01`: Refusal when agent is declarative only.
///
/// If an agent is in the catalog but has `artifact_present: false`, `MobileBridge`
/// strictly refuses execution with "Agent non confirmé exécutable par le hub ; actualiser sa liste".
#[test]
fn test_t1_f18_01_refusal_when_agent_is_declarative_only() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:17840",
        })
        .to_string(),
    );

    // Inject declarative-only agent into snapshot via load_agents
    let res = bridge.dispatch_json(json!({"type": "load_agents"}).to_string());
    let id = serde_json::from_str::<serde_json::Value>(&res).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();

    bridge.receive_json(
        id,
        json!({
            "session_id": "sess-agents",
            "frames": [Frame::reply(id, KernelToUi::AgentListReply {
                agents: vec![AgentDto {
                    id: "agent-mail".into(),
                    name: "mail".into(),
                    required_caps: vec!["network.send".into()],
                    artifact_present: false,
                    sha256: "".into(),
                }],
            })]
        })
        .to_string(),
    );

    // Attempting to run the declarative agent must fail-closed
    let res_str = bridge.dispatch_json(
        json!({
            "type": "run_agent",
            "name": "mail",
            "input_json": "{}"
        })
        .to_string(),
    );

    let res: serde_json::Value = serde_json::from_str(&res_str).unwrap();
    assert_eq!(res["ok"], false, "Must refuse running declarative agent");
    assert!(
        res["error"]
            .as_str()
            .unwrap()
            .contains("Agent non confirmé exécutable"),
        "Must return explicit unconfirmed executable error"
    );
}

/// `T1_F18_02`: Refusal when input JSON is malformed.
///
/// An executable agent dispatch requires valid JSON in `input_json`.
/// Non-JSON string is rejected with "Entrée agent JSON invalide".
#[test]
fn test_t1_f18_02_refusal_when_input_json_is_malformed() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:17840",
        })
        .to_string(),
    );

    // Inject verified executable agent
    let res = bridge.dispatch_json(json!({"type": "load_agents"}).to_string());
    let id = serde_json::from_str::<serde_json::Value>(&res).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();

    bridge.receive_json(
        id,
        json!({
            "session_id": "sess-agents",
            "frames": [Frame::reply(id, KernelToUi::AgentListReply {
                agents: vec![AgentDto {
                    id: "agent-code".into(),
                    name: "code".into(),
                    required_caps: vec![],
                    artifact_present: true,
                    sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
                }],
            })]
        }).to_string(),
    );

    // Malformed JSON input
    let res_str = bridge.dispatch_json(
        json!({
            "type": "run_agent",
            "name": "code",
            "input_json": "this is not json {"
        })
        .to_string(),
    );

    let res: serde_json::Value = serde_json::from_str(&res_str).unwrap();
    assert_eq!(res["ok"], false);
    assert!(
        res["error"]
            .as_str()
            .unwrap()
            .contains("Entrée agent JSON invalide"),
        "Must reject non-JSON input"
    );
}

/// `T1_F18_03`: Executable agent dispatches AgentRunRequest.
///
/// For an executable agent with valid JSON input, dispatch produces
/// `UiToKernel::AgentRunRequest { name, input_json }`.
#[test]
fn test_t1_f18_03_executable_agent_dispatches_agent_run_request() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:17840",
        })
        .to_string(),
    );

    let res = bridge.dispatch_json(json!({"type": "load_agents"}).to_string());
    let id = serde_json::from_str::<serde_json::Value>(&res).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();

    bridge.receive_json(
        id,
        json!({
            "session_id": "sess-agents",
            "frames": [Frame::reply(id, KernelToUi::AgentListReply {
                agents: vec![AgentDto {
                    id: "agent-code".into(),
                    name: "code".into(),
                    required_caps: vec![],
                    artifact_present: true,
                    sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
                }],
            })]
        }).to_string(),
    );

    let input = json!({"command": "format", "target": "lib.rs"}).to_string();
    let res_str = bridge.dispatch_json(
        json!({
            "type": "run_agent",
            "name": "code",
            "input_json": input
        })
        .to_string(),
    );

    let res: serde_json::Value = serde_json::from_str(&res_str).unwrap();
    assert_eq!(res["ok"], true);

    let frame: Frame = serde_json::from_value(res["request"].clone()).unwrap();
    match frame.body {
        hakille_ipc::FrameBody::Request { message } => match message {
            UiToKernel::AgentRunRequest { name, input_json } => {
                assert_eq!(name, "code");
                assert_eq!(input_json, input);
            }
            other => panic!("Expected AgentRunRequest, got {other:?}"),
        },
        other => panic!("Expected Request body, got {other:?}"),
    }
}

/// `T1_F18_04`: AgentRunReply updates outcomes in MobileSnapshot.
///
/// When the daemon replies with `AgentRunReply`, the bridge records
/// the confirmed outcome in `snapshot.outcomes`.
#[test]
fn test_t1_f18_04_agent_run_reply_updates_outcomes_in_snapshot() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:17840",
        })
        .to_string(),
    );

    // Register executable agent
    let res_load = bridge.dispatch_json(json!({"type": "load_agents"}).to_string());
    let id_load = serde_json::from_str::<serde_json::Value>(&res_load).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();

    bridge.receive_json(
        id_load,
        json!({
            "session_id": "sess-agents",
            "frames": [Frame::reply(id_load, KernelToUi::AgentListReply {
                agents: vec![AgentDto {
                    id: "agent-calc".into(),
                    name: "calc".into(),
                    required_caps: vec![],
                    artifact_present: true,
                    sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
                }],
            })]
        }).to_string(),
    );

    // Dispatch run
    let res_str = bridge.dispatch_json(
        json!({
            "type": "run_agent",
            "name": "calc",
            "input_json": "{\"expr\": \"2+2\"}"
        })
        .to_string(),
    );

    let res: serde_json::Value = serde_json::from_str(&res_str).unwrap();
    let req_id = res["request"]["id"].as_u64().unwrap();

    // Reply from daemon
    let reply = KernelToUi::AgentRunReply {
        name: "calc".into(),
        output_json: "{\"result\": 4}".into(),
        duration_ms: 12,
        input_sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
        output_sha256: "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb".into(),
    };

    let envelope = json!({
        "session_id": "sess-agents",
        "frames": [Frame::reply(req_id, reply)]
    });

    let receive_res = bridge.receive_json(req_id, envelope.to_string());
    let receive_val: serde_json::Value = serde_json::from_str(&receive_res).unwrap();
    assert_eq!(receive_val["ok"], true);

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    let outcomes = snap["outcomes"].as_array().expect("outcomes array");
    let calc_outcome = outcomes
        .iter()
        .find(|o| o["request_id"] == req_id)
        .expect("outcome found");

    assert_eq!(calc_outcome["kind"], "run_agent");
    assert_eq!(calc_outcome["state"], "confirmed");
}

/// `T1_F18_05`: Gateway enforces agent run payload bounds.
///
/// Gateway checks `!name.is_empty() && name.len() <= 128 && input_json.len() <= 64 * 1024`.
/// Empty name or input exceeding 64 KiB is rejected with HTTP 400.
#[tokio::test]
async fn test_t1_f18_05_gateway_enforces_agent_run_payload_bounds() {
    let (rig, daemon) = MobileTestRig::setup().await;

    // Subcase A: Empty name rejected
    let empty_name_frame = Frame::request(
        1,
        UiToKernel::AgentRunRequest {
            name: "".into(),
            input_json: "{}".into(),
        },
    );
    let (status1, resp1) = rig.post_frame(&empty_name_frame, None).await;
    assert_eq!(status1, 400);
    assert_eq!(resp1["error"]["code"], "unsupported");

    // Subcase B: Oversized input (> 64 KiB) rejected
    let oversized_json = format!("{{\"pad\":\"{}\"}}", "x".repeat(65 * 1024));
    let over_frame = Frame::request(
        2,
        UiToKernel::AgentRunRequest {
            name: "test".into(),
            input_json: oversized_json,
        },
    );
    let (status2, resp2) = rig.post_frame(&over_frame, None).await;
    assert_eq!(status2, 400);
    assert_eq!(resp2["error"]["code"], "unsupported");

    // Subcase C: Boundary valid name (128 chars) and small JSON passes gateway check
    let boundary_name = "a".repeat(128);
    let daemon_task = tokio::spawn({
        let bname = boundary_name.clone();
        async move {
            let mut stream = daemon.accept().await.expect("accept");
            let step = DaemonStep::new(
                UiToKernel::AgentRunRequest {
                    name: bname.clone(),
                    input_json: "{}".into(),
                },
                KernelToUi::AgentRunReply {
                    name: bname,
                    output_json: "{}".into(),
                    duration_ms: 5,
                    input_sha256: "hash1".into(),
                    output_sha256: "hash2".into(),
                },
            );
            daemon
                .handle_step(&mut stream, 3, step)
                .await
                .expect("step 3");
        }
    });

    let valid_frame = Frame::request(
        3,
        UiToKernel::AgentRunRequest {
            name: boundary_name,
            input_json: "{}".into(),
        },
    );
    let (status3, resp3) = rig.post_frame(&valid_frame, None).await;
    assert_eq!(
        status3, 200,
        "128-char name and valid JSON must pass gateway check"
    );
    assert!(resp3["error"].is_null());
    daemon_task.await.expect("daemon exit");
}
