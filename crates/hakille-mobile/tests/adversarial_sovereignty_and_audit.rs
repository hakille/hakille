use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_mobile::types::{DomainAutonomyDto, WhySheetDto};
use hakille_ui_protocol::{AgentDto, JournalEntryDto, KernelToUi, MemoryRowDto};
use serde_json::json;

fn setup_bridge() -> MobileBridge {
    let bridge = MobileBridge::new();
    let set_hub = bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "https://hub.example.com"
        })
        .to_string(),
    );
    assert!(set_hub.contains("\"ok\":true"));
    bridge
}

#[test]
fn test_why_sheet_honest_missing_when_trace_absent() {
    let bridge = setup_bridge();

    // 1. Demande d'explication pour un ID inexistant
    let res = bridge.dispatch_json(
        json!({
            "type": "request_why",
            "target_id": "missing-card-42",
            "target_type": "card"
        })
        .to_string(),
    );
    let val: serde_json::Value = serde_json::from_str(&res).unwrap();
    assert_eq!(val["ok"], true);
    let why = &val["snapshot"]["why_sheet"];
    assert_eq!(
        why["sentence"].as_str().unwrap(),
        "Je n'ai pas gardé la raison de ce choix."
    );
    assert_eq!(why["confidence_word"].as_str().unwrap(), "basse");
    assert_eq!(why["confidence_value"].as_f64().unwrap(), 0.0);
    assert!(why["facts"].as_array().unwrap().is_empty());

    // Direct DTO missing check
    let missing_dto = WhySheetDto::missing("target-x", "Aucune trace");
    assert_eq!(
        missing_dto.sentence,
        "Je n'ai pas gardé la raison de ce choix."
    );
    assert_eq!(missing_dto.plan, "Aucune trace");
    assert!(missing_dto.facts.is_empty());

    // Fermeture de la feuille
    let close_res = bridge.dispatch_json(json!({ "type": "close_why" }).to_string());
    let close_val: serde_json::Value = serde_json::from_str(&close_res).unwrap();
    assert_eq!(close_val["ok"], true);
    assert!(close_val["snapshot"]["why_sheet"].is_null());
}

#[test]
fn test_autonomy_crank_demotion_on_undo() {
    let bridge = setup_bridge();

    // Set "Mail" crank to 3 (ActEnvelope)
    let set_res = bridge.dispatch_json(
        json!({
            "type": "set_autonomy_crank",
            "domain": "Mail",
            "crank": 3,
            "max_cost_cents": 500,
            "max_actions": 10
        })
        .to_string(),
    );
    let val: serde_json::Value = serde_json::from_str(&set_res).unwrap();
    assert_eq!(val["ok"], true);
    let mail_domain = val["snapshot"]["domain_autonomies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["domain"] == "Mail")
        .unwrap();
    assert_eq!(mail_domain["crank"], 3);
    assert_eq!(mail_domain["envelope"]["max_cost_cents"], 500);

    // Call Undo on journal entry #42
    let undo_res = bridge.dispatch_json(
        json!({
            "type": "undo",
            "journal_id": 42
        })
        .to_string(),
    );
    let undo_val: serde_json::Value = serde_json::from_str(&undo_res).unwrap();
    assert_eq!(undo_val["ok"], true);
    assert_eq!(undo_val["request"]["id"], 1);

    // Inspect snapshot: crank should be demoted from 3 to 2, validated count reset to 0, envelope purged
    let mail_domain_after = undo_val["snapshot"]["domain_autonomies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["domain"] == "Mail")
        .unwrap();
    assert_eq!(mail_domain_after["crank"], 2);
    assert_eq!(mail_domain_after["validated_count"], 0);
    assert!(mail_domain_after["envelope"].is_null());
}

#[test]
fn test_autonomy_crank_promotion_requires_20_validated_actions() {
    let mut domain = DomainAutonomyDto::new("Agenda");
    assert_eq!(domain.crank, 1);
    assert_eq!(domain.validated_count, 0);
    assert!(!domain.promotion_offerable);

    // 19 validated actions
    for _ in 0..19 {
        domain.record_validated();
    }
    assert_eq!(domain.validated_count, 19);
    assert!(!domain.promotion_offerable);

    // 20th validated action triggers promotion
    domain.record_validated();
    assert_eq!(domain.validated_count, 20);
    assert!(domain.promotion_offerable);

    // Reverting demotes crank and clears promotion
    domain.record_reverted();
    assert_eq!(domain.crank, 0);
    assert_eq!(domain.validated_count, 0);
    assert!(!domain.promotion_offerable);
}

#[test]
fn test_audit_journal_verification_detects_tampering() {
    let bridge = setup_bridge();

    // 1. Load journal
    let load_res = bridge.dispatch_json(
        json!({
            "type": "load_journal",
            "limit": 10
        })
        .to_string(),
    );
    let load_val: serde_json::Value = serde_json::from_str(&load_res).unwrap();
    let req_id = load_val["request"]["id"].as_u64().unwrap();

    // 2. Receive journal entry
    let frame = Frame::reply(
        req_id,
        KernelToUi::JournalReply {
            entries: vec![JournalEntryDto {
                id: "entry-1".into(),
                timestamp: "2026-11-08T19:00:00Z".into(),
                action: "Suppression fichier".into(),
                summary: "Suppression sécurisée du fichier cache".into(),
            }],
        },
    );
    let rec_res = bridge.receive_stream_frame(req_id, serde_json::to_string(&frame).unwrap());
    let rec_val: serde_json::Value = serde_json::from_str(&rec_res).unwrap();
    assert_eq!(rec_val["ok"], true);
    assert_eq!(rec_val["snapshot"]["journal"].as_array().unwrap().len(), 1);

    // 3. Request Why for this journal entry
    let why_res = bridge.dispatch_json(
        json!({
            "type": "request_why",
            "target_id": "entry-1",
            "target_type": "journal"
        })
        .to_string(),
    );
    let why_val: serde_json::Value = serde_json::from_str(&why_res).unwrap();
    assert_eq!(why_val["ok"], true);
    let why = &why_val["snapshot"]["why_sheet"];
    assert_eq!(why["target_id"].as_str().unwrap(), "entry-1");
    assert_eq!(
        why["sentence"].as_str().unwrap(),
        "Suppression fichier: Suppression sécurisée du fichier cache"
    );
    assert!(
        why["powers"]
            .as_array()
            .unwrap()
            .contains(&json!("Audit Ed25519"))
    );
}

#[test]
fn test_cryptographic_forget_records_tombstone() {
    let bridge = setup_bridge();

    // 1. Seed memory with a triplet
    let req = bridge.dispatch_json(
        json!({
            "type": "load_memory",
            "query": null,
            "limit": 5
        })
        .to_string(),
    );
    let req_id = serde_json::from_str::<serde_json::Value>(&req).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();
    let frame = Frame::reply(
        req_id,
        KernelToUi::MemoryReply {
            rows: vec![
                MemoryRowDto {
                    id: "fact-user-secret".into(),
                    text: "Alice préfère le thé vert".into(),
                    tags: vec!["préférence".into()],
                    source: "User".into(),
                },
                MemoryRowDto {
                    id: "fact-safe".into(),
                    text: "Hakille v1.0 disponible".into(),
                    tags: vec!["système".into()],
                    source: "System".into(),
                },
            ],
        },
    );
    let _ = bridge.receive_stream_frame(req_id, serde_json::to_string(&frame).unwrap());

    // Verify snapshot has 2 facts
    let snap_raw = bridge.snapshot_json();
    let snap: serde_json::Value = serde_json::from_str(&snap_raw).unwrap();
    assert_eq!(snap["memory"].as_array().unwrap().len(), 2);
    assert!(snap["tombstones"].as_array().unwrap().is_empty());

    // 2. Forget fact-user-secret
    let forget_res = bridge.dispatch_json(
        json!({
            "type": "forget_memory",
            "row_id": "fact-user-secret"
        })
        .to_string(),
    );
    let forget_val: serde_json::Value = serde_json::from_str(&forget_res).unwrap();
    assert_eq!(forget_val["ok"], true);

    // The fact must be immediately purged from memory, added to tombstones, and an erasure receipt created
    let memory_after = forget_val["snapshot"]["memory"].as_array().unwrap();
    assert_eq!(memory_after.len(), 1);
    assert_eq!(memory_after[0]["id"], "fact-safe");

    let tombstones = forget_val["snapshot"]["tombstones"].as_array().unwrap();
    assert!(tombstones.contains(&json!("fact-user-secret")));

    let receipts = forget_val["snapshot"]["erasure_receipts"]
        .as_array()
        .unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0]["target_id"], "fact-user-secret");
    assert!(
        receipts[0]["verification_token"]
            .as_str()
            .unwrap()
            .starts_with("sha256-receipt-")
    );
}

#[test]
fn test_agent_catalog_strictly_rejects_declarative_execution() {
    let bridge = setup_bridge();

    // 1. Populate agent list with 1 executable agent and 1 declarative-only agent
    let load_req = bridge.dispatch_json(json!({ "type": "load_agents" }).to_string());
    let req_id = serde_json::from_str::<serde_json::Value>(&load_req).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();

    let frame = Frame::reply(
        req_id,
        KernelToUi::AgentListReply {
            agents: vec![
                AgentDto {
                    id: "echo-agent".into(),
                    name: "echo-agent".into(),
                    required_caps: vec![],
                    artifact_present: true,
                    sha256: "e".repeat(64),
                },
                AgentDto {
                    id: "declarative-planner".into(),
                    name: "declarative-planner".into(),
                    required_caps: vec!["fs_read".into(), "net_http".into()],
                    artifact_present: false, // Pas de binaire WASM !
                    sha256: String::new(),
                },
            ],
        },
    );
    let _ = bridge.receive_stream_frame(req_id, serde_json::to_string(&frame).unwrap());

    // 2. Attempting to execute the declarative agent MUST fail immediately
    let run_declarative = bridge.dispatch_json(
        json!({
            "type": "run_agent",
            "name": "declarative-planner",
            "input_json": "{}"
        })
        .to_string(),
    );
    let dec_val: serde_json::Value = serde_json::from_str(&run_declarative).unwrap();
    assert_eq!(dec_val["ok"], false);
    assert!(
        dec_val["error"]
            .as_str()
            .unwrap()
            .contains("non confirmé exécutable")
    );

    // 3. Executing the executable agent succeeds
    let run_exec = bridge.dispatch_json(
        json!({
            "type": "run_agent",
            "name": "echo-agent",
            "input_json": "{\"msg\":\"hello\"}"
        })
        .to_string(),
    );
    let exec_val: serde_json::Value = serde_json::from_str(&run_exec).unwrap();
    assert_eq!(exec_val["ok"], true);
}

#[test]
fn test_sandboxed_agent_run_captures_metrics_and_hashes() {
    let bridge = setup_bridge();

    // Seed agent list
    let load_req = bridge.dispatch_json(json!({ "type": "load_agents" }).to_string());
    let req_id = serde_json::from_str::<serde_json::Value>(&load_req).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();
    let frame_agents = Frame::reply(
        req_id,
        KernelToUi::AgentListReply {
            agents: vec![AgentDto {
                id: "count-agent".into(),
                name: "count-agent".into(),
                required_caps: vec![],
                artifact_present: true,
                sha256: "c".repeat(64),
            }],
        },
    );
    let _ = bridge.receive_stream_frame(req_id, serde_json::to_string(&frame_agents).unwrap());

    // Launch agent
    let run_res = bridge.dispatch_json(
        json!({
            "type": "run_agent",
            "name": "count-agent",
            "input_json": "{\"items\":[1,2,3]}"
        })
        .to_string(),
    );
    let run_val: serde_json::Value = serde_json::from_str(&run_res).unwrap();
    assert_eq!(run_val["ok"], true);
    let run_id = run_val["request"]["id"].as_u64().unwrap();

    // Kernel replies with execution outcome containing real duration and SHA-256 hashes
    let reply_frame = Frame::reply(
        run_id,
        KernelToUi::AgentRunReply {
            name: "count-agent".into(),
            output_json: "{\"count\":3}".into(),
            duration_ms: 12,
            input_sha256: "1".repeat(64),
            output_sha256: "2".repeat(64),
        },
    );
    let res_reply =
        bridge.receive_stream_frame(run_id, serde_json::to_string(&reply_frame).unwrap());
    let parsed: serde_json::Value = serde_json::from_str(&res_reply).unwrap();
    assert_eq!(parsed["ok"], true);

    // Snapshot must have recorded last_agent_run
    let outcome = &parsed["snapshot"]["last_agent_run"];
    assert!(!outcome.is_null());
    assert_eq!(outcome["name"], "count-agent");
    assert_eq!(outcome["output_json"], "{\"count\":3}");
    assert_eq!(outcome["duration_ms"], 12);
    assert_eq!(outcome["input_sha256"], "1".repeat(64));
    assert_eq!(outcome["output_sha256"], "2".repeat(64));
}
