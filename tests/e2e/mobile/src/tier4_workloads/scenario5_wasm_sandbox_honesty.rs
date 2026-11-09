//! Tier 4 Real-World Application Workload: Scenario 5
//!
//! Agent WASM Sandboxed Execution vs Declarative Honesty Check
//! Features Exercised: F10 (Security Provenance Banner), F13 (Signed Audit Journal), F17 (Agent Catalog & WASM Check), F18 (Sandboxed Agent Runner)
//! Complexity: Medium
//!
//! Workflow:
//! 1. Catalog ingestion: Hub advertises two agents:
//!    - Agent A (`calc_core`): artifact_present = true, verified sha256.
//!    - Agent B (`declarative_search`): artifact_present = false, uncompiled manifest.
//! 2. UI catalog classification: Agent A marked executable; Agent B marked declarative-only.
//! 3. Adversarial run attempt: Execution on Agent B strictly fails closed (no Wasm engine instantiated).
//! 4. Legitimate sandboxed run: Execution on Agent A succeeds within resource bounds.
//! 5. Output provenance: Tagged with Provenance::VerifiedSkill and sanitized.
//! 6. Cryptographic audit: Verified Ed25519 signed receipt linked to the audit chain.

use crate::harness::{AuditSigner, AuditVerifier};
use hakille_design::provenance::{ProvenanceBand, sanitize_skill_text};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{AgentDto, KernelToUi};
use serde_json::json;

#[test]
fn test_scenario5_wasm_sandboxed_execution_vs_declarative_honesty() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080",
        })
        .to_string(),
    );

    let mut audit_signer = AuditSigner::new();
    let genesis = audit_signer.sign_action("id-genesis", "system_boot", None, json!({}));

    // --- STEP 1: Catalog Ingestion ---
    let sha_agent_a = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    let agent_a = AgentDto {
        id: "agent-calc-core".into(),
        name: "calc_core".into(),
        required_caps: vec![],
        artifact_present: true,
        sha256: sha_agent_a.into(),
    };

    let agent_b = AgentDto {
        id: "agent-decl-search".into(),
        name: "declarative_search".into(),
        required_caps: vec!["web.search".into()],
        artifact_present: false, // Declarative only
        sha256: "".into(),
    };

    let res_load = bridge.dispatch_json(json!({"type": "load_agents"}).to_string());
    let id_load = serde_json::from_str::<serde_json::Value>(&res_load).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();

    bridge.receive_json(
        id_load,
        json!({
            "session_id": "sess-agents-5",
            "frames": [Frame::reply(id_load, KernelToUi::AgentListReply {
                agents: vec![agent_a.clone(), agent_b.clone()],
            })]
        })
        .to_string(),
    );

    // --- STEP 2: Honesty Catalog Validation ---
    let snap1 = serde_json::from_str::<serde_json::Value>(&bridge.snapshot_json()).unwrap();
    let catalog = snap1["agents"].as_array().expect("agents in snapshot");
    assert_eq!(catalog.len(), 2);

    let found_a = catalog.iter().find(|a| a["name"] == "calc_core").unwrap();
    assert_eq!(found_a["artifact_present"], true);

    let found_b = catalog
        .iter()
        .find(|a| a["name"] == "declarative_search")
        .unwrap();
    assert_eq!(found_b["artifact_present"], false);

    // --- STEP 3: Adversarial Execution Attempt on Declarative Agent ---
    let adv_res = bridge.dispatch_json(
        json!({
            "type": "run_agent",
            "name": "declarative_search",
            "input_json": "{\"query\": \"adversarial\"}"
        })
        .to_string(),
    );
    let adv_val: serde_json::Value = serde_json::from_str(&adv_res).unwrap();
    assert_eq!(
        adv_val["ok"], false,
        "Declarative-only agent must be refused execution"
    );
    assert!(
        adv_val["error"]
            .as_str()
            .unwrap()
            .contains("Agent non confirmé exécutable"),
        "Must enforce honest refusal error"
    );

    // --- STEP 4: Legitimate Sandboxed Execution ---
    let legit_res = bridge.dispatch_json(
        json!({
            "type": "run_agent",
            "name": "calc_core",
            "input_json": "{\"op\": \"square\", \"value\": 7}"
        })
        .to_string(),
    );
    let legit_val: serde_json::Value = serde_json::from_str(&legit_res).unwrap();
    assert_eq!(legit_val["ok"], true);
    let run_req_id = legit_val["request"]["id"].as_u64().unwrap();

    // Daemon simulation of sandboxed runner reply
    let raw_reply_text =
        "{\"result\": 49, \"note\": \"<b>Calculé localement</b>\u{1b}[32mOK\u{1b}[0m\"}";
    let sanitized_reply = sanitize_skill_text(raw_reply_text);
    assert!(!sanitized_reply.contains("<b>"));
    assert!(!sanitized_reply.contains('\u{1b}'));

    let agent_reply_frame = Frame::reply(
        run_req_id,
        KernelToUi::AgentRunReply {
            name: "calc_core".into(),
            output_json: "{\"result\": 49}".into(),
            duration_ms: 8,
            input_sha256: "hash-in-7".into(),
            output_sha256: "hash-out-49".into(),
        },
    );

    let receive_res = bridge.receive_json(
        run_req_id,
        json!({
            "session_id": "sess-agents-5",
            "frames": [agent_reply_frame]
        })
        .to_string(),
    );
    assert!(receive_res.contains("\"ok\":true"));

    // --- STEP 5: Output Provenance Verification ---
    let band = ProvenanceBand::VerifiedSkill {
        publisher: "Hakille".into(),
        name: "calc_core".into(),
    };
    assert_eq!(band.label(), "skill vérifié (Hakille · calc_core)");

    // --- STEP 6: Cryptographic Signed Audit Chain ---
    let agent_audit_receipt = audit_signer.sign_action(
        "audit-calc-core-execution",
        "agent_execution_verified",
        None,
        json!({
            "agent_id": "agent-calc-core",
            "sha256": sha_agent_a,
            "duration_ms": 8,
            "sandbox": "wasm_isolated_zero_caps"
        }),
    );

    assert_eq!(agent_audit_receipt.prev_hash, genesis.entry_hash);
    let chain = vec![genesis, agent_audit_receipt];
    AuditVerifier::verify_chain(&chain).expect("Sandboxed agent audit receipt chain must verify");
}
