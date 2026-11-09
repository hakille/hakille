//! Feature 15: SPO Knowledge Graph Explorer (Milestone M1 / Spec §2, §22)
//!
//! Specifications: T1_F15_01 to T1_F15_05
//!
//! Verifies:
//! - MemoryRowDto Subject-Predicate-Object triple structure and categorical tags.
//! - Dispatch of MemoryRequest queries with optional filter and row limit.
//! - Faithful population of MobileSnapshot.memory upon MemoryReply arrival.
//! - Sovereign fact insertion via Remember action tracking mutating state.
//! - Strict validation of query limit bounds (1 <= limit <= 200).

use crate::harness::{DaemonStep, MobileTestRig};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{KernelToUi, MemoryRowDto, UiToKernel};
use serde_json::json;

/// `T1_F15_01`: MemoryRowDto Subject-Predicate-Object structure.
///
/// Each sovereign memory row exposes a unique row id, natural language fact text,
/// category tags, and provenance source (e.g. User, System, Inferred).
#[test]
fn test_t1_f15_01_memory_row_dto_spo_structure() {
    let row = MemoryRowDto {
        id: "mem-uuid-001".into(),
        text: "Alice habite à Dakar depuis 2022.".into(),
        tags: vec!["contact".into(), "location".into(), "high_trust".into()],
        source: "User".into(),
    };

    assert_eq!(row.id, "mem-uuid-001");
    assert_eq!(row.text, "Alice habite à Dakar depuis 2022.");
    assert_eq!(row.tags.len(), 3);
    assert!(row.tags.contains(&"location".to_string()));
    assert_eq!(row.source, "User");
}

/// `T1_F15_02`: Load memory query dispatch via MobileBridge.
///
/// Dispatching MobileAction::LoadMemory produces a valid UiToKernel::MemoryRequest
/// containing the query string and row limit.
#[test]
fn test_t1_f15_02_load_memory_dispatch_via_bridge() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080",
        })
        .to_string(),
    );

    let res_str = bridge.dispatch_json(
        json!({
            "type": "load_memory",
            "query": "Dakar",
            "limit": 50,
        })
        .to_string(),
    );

    let res: serde_json::Value = serde_json::from_str(&res_str).unwrap();
    assert_eq!(res["ok"], true);

    let frame: Frame = serde_json::from_value(res["request"].clone()).unwrap();
    match frame.body {
        hakille_ipc::FrameBody::Request { message } => match message {
            UiToKernel::MemoryRequest { query, limit } => {
                assert_eq!(query, Some("Dakar".to_string()));
                assert_eq!(limit, 50);
            }
            other => panic!("Expected MemoryRequest, got {other:?}"),
        },
        other => panic!("Expected Request body, got {other:?}"),
    }
}

/// `T1_F15_03`: MemoryReply arrival populates snapshot.memory.
///
/// Gateway receiving MemoryReply updates snapshot.memory with verbatim rows.
#[tokio::test]
async fn test_t1_f15_03_memory_reply_populates_snapshot_rows() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let sample_rows = vec![
        MemoryRowDto {
            id: "row-1".into(),
            text: "Serveur DNS local: 192.168.1.1".into(),
            tags: vec!["network".into()],
            source: "System".into(),
        },
        MemoryRowDto {
            id: "row-2".into(),
            text: "Clé SSH souveraine générée le 2026-01-01".into(),
            tags: vec!["security".into()],
            source: "User".into(),
        },
    ];

    let daemon_task = tokio::spawn({
        let rows = sample_rows.clone();
        async move {
            let mut stream = daemon.accept().await.expect("accept");
            // Step 1: status
            daemon
                .handle_step(&mut stream, 1, DaemonStep::status())
                .await
                .expect("step 1");

            // Step 2: memory request
            let mem_step = DaemonStep::new(
                UiToKernel::MemoryRequest {
                    query: None,
                    limit: 10,
                },
                KernelToUi::MemoryReply { rows },
            );
            daemon
                .handle_step(&mut stream, 2, mem_step)
                .await
                .expect("step 2 memory");
        }
    });

    rig.roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;

    let (status, result) = rig
        .roundtrip(
            &bridge,
            json!({
                "type": "load_memory",
                "query": null,
                "limit": 10,
            }),
        )
        .await;

    assert_eq!(status, 200);
    assert_eq!(result["ok"], true);

    let snapshot = rig.snapshot(&bridge);
    let mem = snapshot["memory"].as_array().expect("memory array");
    assert_eq!(mem.len(), 2);
    assert_eq!(mem[0]["id"], "row-1");
    assert_eq!(mem[0]["text"], "Serveur DNS local: 192.168.1.1");
    assert_eq!(mem[1]["id"], "row-2");

    daemon_task.await.unwrap();
}

/// `T1_F15_04`: Memory insertion via Remember action tracks mutating state.
///
/// Dispatching Remember creates a mutating UiToKernel::MemoryRemember request
/// and records an in-flight UncertainOperation in the snapshot.
#[test]
fn test_t1_f15_04_memory_remember_action_tracks_mutation() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080",
        })
        .to_string(),
    );

    let fact = "Hakille protège la souveraineté cognitive.";
    let res_str = bridge.dispatch_json(
        json!({
            "type": "remember",
            "text": fact,
        })
        .to_string(),
    );

    let res: serde_json::Value = serde_json::from_str(&res_str).unwrap();
    assert_eq!(res["ok"], true);

    let frame: Frame = serde_json::from_value(res["request"].clone()).unwrap();
    match frame.body {
        hakille_ipc::FrameBody::Request { message } => match message {
            UiToKernel::MemoryRemember { text } => assert_eq!(text, fact),
            other => panic!("Expected MemoryRemember, got {other:?}"),
        },
        other => panic!("Expected Request body, got {other:?}"),
    }

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap["pending"]["kind"], "remember");
    assert_eq!(snap["pending"]["mutating"], true);

    let uncertain = snap["uncertain_operations"].as_array().unwrap();
    assert_eq!(uncertain.len(), 1);
    assert_eq!(uncertain[0]["kind"], "remember");
    assert_eq!(uncertain[0]["phase"], "in_flight");
}

/// `T1_F15_05`: Memory query limit bounds validation (1 <= limit <= 200).
///
/// Attempting to query with limit 0 or > 200 strictly returns an error:
/// "Limite de liste attendue entre 1 et 200".
#[test]
fn test_t1_f15_05_memory_row_limit_bounds_validation() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080",
        })
        .to_string(),
    );

    // 1. Underflow: limit = 0
    let under_res = bridge.dispatch_json(
        json!({
            "type": "load_memory",
            "query": null,
            "limit": 0,
        })
        .to_string(),
    );
    assert!(
        under_res.contains("Limite de liste attendue entre 1 et 200"),
        "Limit 0 must be rejected: {under_res}"
    );

    // 2. Overflow: limit = 201
    let over_res = bridge.dispatch_json(
        json!({
            "type": "load_memory",
            "query": null,
            "limit": 201,
        })
        .to_string(),
    );
    assert!(
        over_res.contains("Limite de liste attendue entre 1 et 200"),
        "Limit 201 must be rejected: {over_res}"
    );

    // 3. Valid limits: 1 and 200
    let valid_1 = bridge.dispatch_json(
        json!({
            "type": "load_memory",
            "query": null,
            "limit": 1,
        })
        .to_string(),
    );
    assert!(valid_1.contains("\"ok\":true"));

    // Reset pending by failing previous
    let req_id = serde_json::from_str::<serde_json::Value>(&valid_1).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();
    bridge.fail_request(req_id, "test reset".into());

    let valid_200 = bridge.dispatch_json(
        json!({
            "type": "load_memory",
            "query": null,
            "limit": 200,
        })
        .to_string(),
    );
    assert!(valid_200.contains("\"ok\":true"));
}
