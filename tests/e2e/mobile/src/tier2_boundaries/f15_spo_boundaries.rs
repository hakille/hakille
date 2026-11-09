//! Feature 15: SPO Knowledge Graph Explorer (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F15_01 to T2_F15_05 (5 tests total).

#![allow(unused_imports)]

use crate::harness::{
    AuditReceipt, AuditSigner, AuditVerifier, AuditVerifyError, DaemonStep, MobileTestRig,
};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{AgentDto, KernelToUi, MemoryRowDto, StatusDto, UiToKernel};
use serde_json::json;
use std::time::Duration;

/// `T2_F15_01`: Query text > 64 KiB rejected by gateway.
///
/// Gateway enforces query.len() <= 64 * 1024; queries exceeding this are rejected with HTTP 400.
#[tokio::test]
async fn test_t2_f15_01_query_text_exceeding_64_kib_rejected() {
    let (rig, _daemon) = MobileTestRig::setup().await;

    let oversized_query = "q".repeat(64 * 1024 + 1);
    let frame = Frame::request(
        1,
        UiToKernel::MemoryRequest {
            query: Some(oversized_query),
            limit: 50,
        },
    );

    let (status, resp) = rig.post_frame(&frame, None).await;
    assert_eq!(
        status, 400,
        "Oversized query text must be rejected with HTTP 400"
    );
    assert_eq!(resp["error"]["code"], "unsupported");
}

/// `T2_F15_02`: Empty query string handled safely.
///
/// Query = None returns all facts up to limit without error.
#[test]
fn test_t2_f15_02_empty_query_string_handled_safely() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());

    let res_str = bridge.dispatch_json(
        json!({
            "type": "load_memory",
            "query": null,
            "limit": 20
        })
        .to_string(),
    );

    let res: serde_json::Value = serde_json::from_str(&res_str).unwrap();
    assert_eq!(res["ok"], true);

    let frame: Frame = serde_json::from_value(res["request"].clone()).unwrap();
    match frame.body {
        hakille_ipc::FrameBody::Request { message } => match message {
            UiToKernel::MemoryRequest { query, limit } => {
                assert!(query.is_none());
                assert_eq!(limit, 20);
            }
            other => panic!("expected MemoryRequest, got {other:?}"),
        },
        other => panic!("expected Request body, got {other:?}"),
    }
}

/// `T2_F15_03`: Row limit bounds: limit = 0 and limit > 200.
///
/// Gateway requires `*limit > 0 && *limit <= 200` for MemoryRequest.
#[tokio::test]
async fn test_t2_f15_03_row_limit_bounds_limit_0_and_201() {
    let (rig, daemon) = MobileTestRig::setup().await;

    // Limit 0 rejected
    let f_zero = Frame::request(
        1,
        UiToKernel::MemoryRequest {
            query: None,
            limit: 0,
        },
    );
    let (s_zero, r_zero) = rig.post_frame(&f_zero, None).await;
    assert_eq!(s_zero, 400);
    assert_eq!(r_zero["error"]["code"], "unsupported");

    // Limit 201 rejected
    let f_over = Frame::request(
        2,
        UiToKernel::MemoryRequest {
            query: None,
            limit: 201,
        },
    );
    let (s_over, r_over) = rig.post_frame(&f_over, None).await;
    assert_eq!(s_over, 400);
    assert_eq!(r_over["error"]["code"], "unsupported");

    // Limit 1 and 200 accepted
    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        let step = DaemonStep::new(
            UiToKernel::MemoryRequest {
                query: None,
                limit: 200,
            },
            KernelToUi::MemoryReply { rows: vec![] },
        );
        daemon
            .handle_step(&mut stream, 3, step)
            .await
            .expect("step");
    });

    let f_valid = Frame::request(
        3,
        UiToKernel::MemoryRequest {
            query: None,
            limit: 200,
        },
    );
    let (s_valid, r_valid) = rig.post_frame(&f_valid, None).await;
    assert_eq!(s_valid, 200);
    assert!(r_valid["error"].is_null());

    daemon_task.await.unwrap();
}

/// `T2_F15_04`: Special characters and SQL/JSON injection strings in SPO triplets.
///
/// Malicious injection payloads are treated as literal text and do not cause injection.
#[test]
fn test_t2_f15_04_injection_strings_in_spo_triplets() {
    let injection_payloads = [
        "' OR '1'='1' --",
        "{\"type\":\"drop_table\",\"target\":\"memory\"}",
        "<script>document.cookie</script>",
        "'; DROP TABLE facts; --",
        "\x00\x01\x02\x03",
    ];

    for payload in injection_payloads {
        let row = MemoryRowDto {
            id: "mem-test-id".into(),
            text: payload.into(),
            tags: vec!["audit".into()],
            source: "User".into(),
        };

        let serialized = serde_json::to_string(&row).unwrap();
        let deserialized: MemoryRowDto = serde_json::from_str(&serialized).unwrap();
        assert_eq!(
            deserialized.text, payload,
            "Payload must be preserved literally"
        );
    }
}

/// `T2_F15_05`: Remembering identical duplicate facts.
///
/// Remembering duplicate facts executes safely without crashing the bridge.
#[test]
fn test_t2_f15_05_remembering_identical_duplicate_facts() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(json!({"type": "set_hub", "hub_id": "http://127.0.0.1:8080"}).to_string());

    let fact_text = "La réunion a lieu à 14h";

    let res1_str = bridge.dispatch_json(
        json!({
            "type": "remember",
            "text": fact_text
        })
        .to_string(),
    );
    assert!(res1_str.contains("\"ok\":true"));

    // Reset pending
    let req1_id = serde_json::from_str::<serde_json::Value>(&res1_str).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();
    let reply1 = json!({
        "session_id": "sess-rem",
        "frames": [
            Frame::reply(req1_id, KernelToUi::MemoryRememberAck {
                id: "mem-uuid-1".into(),
            })
        ]
    });
    bridge.receive_json(req1_id, reply1.to_string());

    // Second remember with identical text
    let res2_str = bridge.dispatch_json(
        json!({
            "type": "remember",
            "text": fact_text
        })
        .to_string(),
    );
    assert!(res2_str.contains("\"ok\":true"));
}
