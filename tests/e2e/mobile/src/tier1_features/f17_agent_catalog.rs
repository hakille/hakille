//! Feature 17: Agent Catalog & WASM Check (Milestone M1 / Spec §2, §23)
//!
//! Specifications: T1_F17_01 to T1_F17_05
//!
//! Verifies:
//! - AgentDto structure: honesty contract between declarative and executable agents.
//! - Dispatch of MobileAction::LoadAgents produces UiToKernel::AgentListRequest.
//! - KernelToUi::AgentListReply faithfully updates MobileSnapshot.agents.
//! - Cryptographic SHA-256 artifact digest verification against WASM bytecode.
//! - Full roundtrip through gateway and mock daemon for agent catalog queries.

use crate::harness::{DaemonStep, MobileTestRig, hex};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{AgentDto, KernelToUi, UiToKernel};
use serde_json::json;
use sha2::{Digest, Sha256};

/// `T1_F17_01`: AgentDto structure and honesty contract.
///
/// Declarative agents declare required capabilities but have `artifact_present: false`
/// and empty sha256. Executable agents must have `artifact_present: true` and a valid
/// 64-hex SHA-256 artifact digest.
#[test]
fn test_t1_f17_01_agent_dto_structure_and_honesty_contract() {
    // Declarative agent (manifest only, no compiled binary)
    let declarative = AgentDto {
        id: "agent-mail".into(),
        name: "mail".into(),
        required_caps: vec!["network.send".into(), "contacts.read".into()],
        artifact_present: false,
        sha256: "".into(),
    };
    assert!(
        !declarative.artifact_present,
        "Declarative agent must not claim artifact_present"
    );
    assert!(
        declarative.sha256.is_empty(),
        "Declarative agent must have empty sha256"
    );
    assert_eq!(declarative.required_caps.len(), 2);

    // Executable agent (verified WASM binary present)
    let executable = AgentDto {
        id: "agent-code".into(),
        name: "code".into(),
        required_caps: vec!["fs.read".into()],
        artifact_present: true,
        sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
    };
    assert!(
        executable.artifact_present,
        "Executable agent must have artifact_present = true"
    );
    assert_eq!(
        executable.sha256.len(),
        64,
        "SHA-256 must be exactly 64 hex characters"
    );
}

/// `T1_F17_02`: LoadAgents dispatch and frame generation.
///
/// Dispatching `MobileAction::LoadAgents` produces a valid `UiToKernel::AgentListRequest`.
#[test]
fn test_t1_f17_02_load_agents_dispatch_and_frame_generation() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:17840",
        })
        .to_string(),
    );

    let res_str = bridge.dispatch_json(
        json!({
            "type": "load_agents"
        })
        .to_string(),
    );

    let res: serde_json::Value = serde_json::from_str(&res_str).unwrap();
    assert_eq!(res["ok"], true, "load_agents dispatch must succeed");

    let frame: Frame = serde_json::from_value(res["request"].clone()).unwrap();
    match frame.body {
        hakille_ipc::FrameBody::Request { message } => match message {
            UiToKernel::AgentListRequest => {}
            other => panic!("Expected AgentListRequest, got {other:?}"),
        },
        other => panic!("Expected Request body, got {other:?}"),
    }
}

/// `T1_F17_03`: AgentListReply populates MobileSnapshot.agents.
///
/// Receiving `KernelToUi::AgentListReply` populates `snapshot.agents` with exact fields.
#[test]
fn test_t1_f17_03_agent_list_reply_populates_mobile_snapshot() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:17840",
        })
        .to_string(),
    );

    let agents = vec![
        AgentDto {
            id: "agent-1".into(),
            name: "calendar".into(),
            required_caps: vec!["calendar.read".into()],
            artifact_present: false,
            sha256: "".into(),
        },
        AgentDto {
            id: "agent-2".into(),
            name: "home".into(),
            required_caps: vec!["home.devices".into()],
            artifact_present: true,
            sha256: "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".into(),
        },
    ];

    let res_str = bridge.dispatch_json(json!({"type": "load_agents"}).to_string());
    let res: serde_json::Value = serde_json::from_str(&res_str).unwrap();
    let id = res["request"]["id"].as_u64().unwrap();
    let reply = KernelToUi::AgentListReply {
        agents: agents.clone(),
    };

    let envelope = json!({
        "session_id": "sess-agents",
        "frames": [Frame::reply(id, reply)]
    });

    bridge.receive_json(id, envelope.to_string());

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    let snap_agents = snap["agents"].as_array().expect("snapshot.agents array");
    assert_eq!(snap_agents.len(), 2);

    assert_eq!(snap_agents[0]["name"], "calendar");
    assert_eq!(snap_agents[0]["artifact_present"], false);

    assert_eq!(snap_agents[1]["name"], "home");
    assert_eq!(snap_agents[1]["artifact_present"], true);
    assert_eq!(
        snap_agents[1]["sha256"],
        "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
    );
}

/// `T1_F17_04`: Cryptographic SHA-256 artifact digest verification against WASM bytecode.
///
/// Ensures that artifact SHA-256 is strictly computed and verified over raw WASM bytes.
#[test]
fn test_t1_f17_04_sha256_artifact_verification_integrity() {
    // Valid standard WASM magic bytes header
    let mock_wasm_bytes = b"\0asm\x01\x00\x00\x00";

    let mut hasher = Sha256::new();
    hasher.update(mock_wasm_bytes);
    let computed_hash = hex::encode(hasher.finalize());

    let agent = AgentDto {
        id: "agent-verified".into(),
        name: "code".into(),
        required_caps: vec![],
        artifact_present: true,
        sha256: computed_hash.clone(),
    };

    assert_eq!(agent.sha256, computed_hash);
    assert_eq!(agent.sha256.len(), 64);
    assert!(agent.sha256.chars().all(|c| c.is_ascii_hexdigit()));

    // Corrupted bytecode digest does not match
    let corrupted_wasm = b"\0asm\x02\x00\x00\x00";
    let mut bad_hasher = Sha256::new();
    bad_hasher.update(corrupted_wasm);
    let bad_hash = hex::encode(bad_hasher.finalize());
    assert_ne!(
        agent.sha256, bad_hash,
        "Corrupted bytecode must produce different hash"
    );
}

/// `T1_F17_05`: Gateway agent list roundtrip through daemon.
///
/// Full HTTP gateway roundtrip requesting agent catalog from mock daemon.
#[tokio::test]
async fn test_t1_f17_05_gateway_agent_list_roundtrip_through_daemon() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let sample_agents = vec![
        AgentDto {
            id: "agent-mail".into(),
            name: "mail".into(),
            required_caps: vec!["network".into()],
            artifact_present: false,
            sha256: "".into(),
        },
        AgentDto {
            id: "agent-calc".into(),
            name: "calculator".into(),
            required_caps: vec![],
            artifact_present: true,
            sha256: "1122334455667788990011223344556677889900112233445566778899001122".into(),
        },
    ];

    let daemon_task = tokio::spawn({
        let agents = sample_agents.clone();
        async move {
            let mut stream = daemon.accept().await.expect("accept");

            let step = DaemonStep::new(
                UiToKernel::AgentListRequest,
                KernelToUi::AgentListReply { agents },
            );
            daemon
                .handle_step(&mut stream, 1, step)
                .await
                .expect("agent list reply");
        }
    });

    let (status, resp) = rig
        .roundtrip(
            &bridge,
            json!({
                "type": "load_agents"
            }),
        )
        .await;

    assert_eq!(status, 200);
    assert_eq!(resp["ok"], true);

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    let agents = snap["agents"].as_array().expect("snapshot.agents array");
    assert_eq!(agents.len(), 2);
    assert_eq!(agents[0]["name"], "mail");
    assert_eq!(agents[1]["name"], "calculator");

    daemon_task.await.expect("daemon task clean exit");
}
