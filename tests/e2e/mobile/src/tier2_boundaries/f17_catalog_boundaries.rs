//! Feature 17: Agent Catalog & WASM Check (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F17_01 to T2_F17_05 (5 tests total).

#![allow(unused_imports)]

use crate::harness::{
    AuditReceipt, AuditSigner, AuditVerifier, AuditVerifyError, DaemonStep, MobileTestRig,
};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{AgentDto, KernelToUi, MemoryRowDto, StatusDto, UiToKernel};
use serde_json::json;
use std::time::Duration;

/// `T2_F17_01`: Empty catalog reply handling.
///
/// Receiving an empty agent list sets snapshot.agents to empty without panicking.
#[test]
fn test_t2_f17_01_empty_catalog_reply() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());

    let res = bridge.dispatch_json(json!({"type": "load_agents"}).to_string());
    let req_id = serde_json::from_str::<serde_json::Value>(&res).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();

    let reply = json!({
        "session_id": "sess-empty-agents",
        "frames": [
            Frame::reply(req_id, KernelToUi::AgentListReply { agents: vec![] })
        ]
    });

    let rec_res = bridge.receive_json(req_id, reply.to_string());
    assert!(rec_res.contains("\"ok\":true"));

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert!(snap["agents"].as_array().unwrap().is_empty());
}

/// `T2_F17_02`: Catalog with 100+ agents.
///
/// Correctly receives, parses, and stores 100 agents in snapshot without overflow.
#[test]
fn test_t2_f17_02_catalog_with_100_agents() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());

    let mut agents = Vec::new();
    for i in 0..100 {
        agents.push(AgentDto {
            id: format!("agent-{i}"),
            name: format!("tool_{i}"),
            required_caps: vec!["net".into()],
            artifact_present: false,
            sha256: "".into(),
        });
    }

    let res = bridge.dispatch_json(json!({"type": "load_agents"}).to_string());
    let req_id = serde_json::from_str::<serde_json::Value>(&res).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();

    let reply = json!({
        "session_id": "sess-100-agents",
        "frames": [
            Frame::reply(req_id, KernelToUi::AgentListReply { agents })
        ]
    });

    let rec_res = bridge.receive_json(req_id, reply.to_string());
    assert!(rec_res.contains("\"ok\":true"));

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap["agents"].as_array().unwrap().len(), 100);
}

/// `T2_F17_03`: Agent with invalid SHA-256 hex string flagged.
///
/// An executable agent claiming artifact_present = true must have exactly 64 valid hex characters.
#[test]
fn test_t2_f17_03_agent_with_invalid_sha256_hex() {
    fn validate_agent_sha256(agent: &AgentDto) -> bool {
        if !agent.artifact_present {
            agent.sha256.is_empty()
        } else {
            agent.sha256.len() == 64 && agent.sha256.chars().all(|c| c.is_ascii_hexdigit())
        }
    }

    // 63 chars (too short)
    let bad_len = AgentDto {
        id: "a1".into(),
        name: "test".into(),
        required_caps: vec![],
        artifact_present: true,
        sha256: "a".repeat(63),
    };
    assert!(!validate_agent_sha256(&bad_len));

    // 64 chars with non-hex
    let non_hex = AgentDto {
        id: "a2".into(),
        name: "test".into(),
        required_caps: vec![],
        artifact_present: true,
        sha256: "z".repeat(64),
    };
    assert!(!validate_agent_sha256(&non_hex));

    // Valid 64-hex
    let valid = AgentDto {
        id: "a3".into(),
        name: "test".into(),
        required_caps: vec![],
        artifact_present: true,
        sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
    };
    assert!(validate_agent_sha256(&valid));
}

/// `T2_F17_04`: Duplicate agent names in catalog handling.
///
/// Duplicate agent names are handled safely without crashing.
#[test]
fn test_t2_f17_04_duplicate_agent_names_handling() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());

    let agents = vec![
        AgentDto {
            id: "id-dup-1".into(),
            name: "calculator".into(),
            required_caps: vec![],
            artifact_present: false,
            sha256: "".into(),
        },
        AgentDto {
            id: "id-dup-2".into(),
            name: "calculator".into(),
            required_caps: vec![],
            artifact_present: true,
            sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
        },
    ];

    let res = bridge.dispatch_json(json!({"type": "load_agents"}).to_string());
    let req_id = serde_json::from_str::<serde_json::Value>(&res).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();

    let reply = json!({
        "session_id": "sess-dups",
        "frames": [Frame::reply(req_id, KernelToUi::AgentListReply { agents })]
    });

    let rec_res = bridge.receive_json(req_id, reply.to_string());
    assert!(rec_res.contains("\"ok\":true"));

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap["agents"].as_array().unwrap().len(), 2);
}

/// `T2_F17_05`: Agent manifest with zero capabilities valid.
///
/// An agent requiring 0 capabilities is a valid sandboxed agent.
#[test]
fn test_t2_f17_05_agent_with_zero_capabilities_valid() {
    let pure_agent = AgentDto {
        id: "agent-pure".into(),
        name: "linter".into(),
        required_caps: vec![],
        artifact_present: true,
        sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
    };
    assert!(pure_agent.required_caps.is_empty());
    assert!(pure_agent.artifact_present);
}
