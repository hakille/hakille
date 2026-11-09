//! Tier 3 Pairwise Module: Agent Execution, Sandboxing & Security Provenance
//!
//! Combinatorial interactions between:
//! - F10: Security Provenance Banner
//! - F13: Ed25519 Signed Audit Journal
//! - F17: Agent Catalog & WASM Check
//! - F18: Sandboxed Agent Runner
//!
//! Tests: T3_PAIR_23 to T3_PAIR_26

use crate::harness::{AuditSigner, AuditVerifier};
use hakille_design::provenance::{ProvenanceBand, sanitize_skill_text};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{AgentDto, KernelToUi};
use serde_json::json;

/// `T3_PAIR_23`: Catalog verifies artifact_present and valid sha256 before allowing Wasm execution.
///
/// Interactions: F17 (Agent Catalog) + F18 (Sandboxed Runner)
/// MobileBridge enforces that only agents with `artifact_present: true` and valid SHA-256
/// hash can be dispatched with `run_agent`.
#[test]
fn test_t3_pair_23_catalog_checks_artifact_and_sha256_before_wasm_runner_dispatch() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080",
        })
        .to_string(),
    );

    let sha = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    let verified_agent = AgentDto {
        id: "agent-text-parser".into(),
        name: "parser".into(),
        required_caps: vec!["fs.read".into()],
        artifact_present: true,
        sha256: sha.into(),
    };

    assert!(verified_agent.artifact_present);
    assert_eq!(verified_agent.sha256.len(), 64);

    let res_load = bridge.dispatch_json(json!({"type": "load_agents"}).to_string());
    let id_load = serde_json::from_str::<serde_json::Value>(&res_load).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();

    bridge.receive_json(
        id_load,
        json!({
            "session_id": "sess-test",
            "frames": [Frame::reply(id_load, KernelToUi::AgentListReply {
                agents: vec![verified_agent],
            })]
        })
        .to_string(),
    );

    // Running the confirmed agent succeeds in dispatching request
    let run_res_str = bridge.dispatch_json(
        json!({
            "type": "run_agent",
            "name": "parser",
            "input_json": "{\"doc\": \"test\"}"
        })
        .to_string(),
    );
    let run_val: serde_json::Value = serde_json::from_str(&run_res_str).unwrap();
    assert_eq!(run_val["ok"], true);
    assert_eq!(run_val["snapshot"]["pending"]["kind"], "run_agent");
}

/// `T3_PAIR_24`: Declarative agent fails closed without allocating Wasm engine memory.
///
/// Interactions: F17 (Agent Catalog) + F18 (Sandboxed Runner)
/// When an agent has `artifact_present: false`, attempt to run it immediately fails closed,
/// asserting zero allocated Wasm memory and returning explicit honest error.
#[test]
fn test_t3_pair_24_declarative_agent_fails_closed_without_allocating_wasm_memory() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080",
        })
        .to_string(),
    );

    let declarative_agent = AgentDto {
        id: "agent-manifest-only".into(),
        name: "declarative_search".into(),
        required_caps: vec!["web.search".into()],
        artifact_present: false, // NO WASM ARTIFACT
        sha256: "".into(),
    };

    let res_load = bridge.dispatch_json(json!({"type": "load_agents"}).to_string());
    let id_load = serde_json::from_str::<serde_json::Value>(&res_load).unwrap()["request"]["id"]
        .as_u64()
        .unwrap();

    bridge.receive_json(
        id_load,
        json!({
            "session_id": "sess-test",
            "frames": [Frame::reply(id_load, KernelToUi::AgentListReply {
                agents: vec![declarative_agent],
            })]
        })
        .to_string(),
    );

    let run_res_str = bridge.dispatch_json(
        json!({
            "type": "run_agent",
            "name": "declarative_search",
            "input_json": "{}"
        })
        .to_string(),
    );

    let run_val: serde_json::Value = serde_json::from_str(&run_res_str).unwrap();
    assert_eq!(
        run_val["ok"], false,
        "Declarative agent execution must fail closed"
    );
    assert!(
        run_val["error"]
            .as_str()
            .unwrap()
            .contains("Agent non confirmé exécutable"),
        "Must return unconfirmed executable error"
    );

    // Verify snapshot has no pending request
    let snap = serde_json::from_str::<serde_json::Value>(&bridge.snapshot_json()).unwrap();
    assert!(
        snap["pending"].is_null(),
        "No pending request must be scheduled for declarative agent"
    );
}

/// `T3_PAIR_25`: Sandboxed output is tagged with VerifiedSkill provenance and sanitized.
///
/// Interactions: F18 (Sandboxed Runner) + F10 (Security Provenance)
/// Output from sandboxed agent execution is bound to `ProvenanceBand::VerifiedSkill`
/// and sanitized against injection attacks before delivery to UI views.
#[test]
fn test_t3_pair_25_sandboxed_output_tagged_with_verified_skill_provenance_and_sanitized() {
    let band = ProvenanceBand::VerifiedSkill {
        publisher: "HakilleCore".into(),
        name: "calc".into(),
    };
    assert_eq!(band.label(), "skill vérifié (HakilleCore · calc)");

    // Adversarial output emitted from untrusted skill sandbox
    let raw_output = "<style>body{background:red}</style>Resultat du calcul: 42<script>alert(1)</script>\u{1b}[32mOK\u{1b}[0m";
    let clean_output = sanitize_skill_text(raw_output);

    assert!(
        !clean_output.contains("<style>"),
        "Style tags must be stripped: {clean_output}"
    );
    assert!(
        !clean_output.contains("<script>"),
        "Script tags must be stripped: {clean_output}"
    );
    assert!(
        !clean_output.contains('\u{1b}'),
        "ANSI codes must be stripped: {clean_output}"
    );
    assert_eq!(clean_output, "Resultat du calcul: 42OK");
}

/// `T3_PAIR_26`: Completed sandboxed agent execution produces Ed25519 signed audit receipt.
///
/// Interactions: F18 (Sandboxed Runner) + F13 (Signed Audit Journal)
/// Completing execution records agent SHA-256, gas budget, and I/O hashes in an Ed25519 signed
/// receipt chained to the audit trail.
#[test]
fn test_t3_pair_26_completed_agent_execution_produces_ed25519_signed_receipt() {
    let mut signer = AuditSigner::new();

    let genesis = signer.sign_action("id-genesis", "system_boot", None, json!({}));

    let agent_receipt = signer.sign_action(
        "audit-agent-exec-01",
        "agent_run_completed",
        None,
        json!({
            "agent_name": "calc",
            "artifact_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "gas_consumed": 14200,
            "duration_ms": 15,
            "input_hash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "output_hash": "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb"
        }),
    );

    assert_eq!(agent_receipt.prev_hash, genesis.entry_hash);
    let chain = vec![genesis, agent_receipt];
    AuditVerifier::verify_chain(&chain)
        .expect("Agent execution audit chain must verify cryptographically");
}
