//! Universal Capability Architecture Demonstration Showcase.
//!
//! Demonstrates the complete unified extensible AI ecosystem:
//! 1. Native in-process pure Rust tools
//! 2. Model Context Protocol (MCP 2024-11-05) client/server session over duplex stream
//! 3. Declarative Agent Skills (`SKILL.md`) with 3-tier progressive context loading
//! 4. Intelligent Tool Orchestration: hybrid discovery, Biscuit security gate,
//!    Tier 3 physical confirmation (600ms hold), timeout preemption, and Merkle audit logging.

use chrono::Utc;
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use serde_json::json;
use std::sync::Arc;
use tokio::io::duplex;

use hakille_audit::AuditJournal;
use hakille_capability::adapters::{
    IntelligentToolOrchestrator, McpClientAdapter, NativeToolsAdapter, OrchestrationRequest,
    ProgressiveSkillsLoader, run_mock_mcp_server,
};
use hakille_capability::audit::CapabilityAuditAdapter;
use hakille_capability::registry::UniversalCapabilityRegistry;
use hakille_capability::security::ActionConfirmationProof;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("===============================================================================");
    println!("  HAKILLE UNIVERSAL CAPABILITY, TOOLS & EXTENSIBILITY SHOWCASE");
    println!("  Sovereign Pure-Rust Extensible Cognitive Architecture");
    println!("===============================================================================\n");

    // ------------------------------------------------------------------------
    // Step 1: Initialize Core Infrastructure (Registry & Audit Journal)
    // ------------------------------------------------------------------------
    println!("[1/5] Initializing Universal Capability Registry & Cryptographic Audit Ledger...");
    let registry = UniversalCapabilityRegistry::new();

    let temp_dir = tempfile::tempdir()?;
    let audit_db_path = temp_dir.path().join("showcase_audit.redb");
    let mut rng = OsRng;
    let signing_key = SigningKey::generate(&mut rng);
    let journal = Arc::new(AuditJournal::new(
        audit_db_path.to_str().unwrap(),
        signing_key,
    )?);
    let audit_adapter = CapabilityAuditAdapter::new(journal);
    println!("      -> Registry initialized (thread-safe, versioned, multi-dimensional index)");
    println!("      -> Audit ledger initialized in redb (SHA-256 Merkle hash chain)\n");

    // ------------------------------------------------------------------------
    // Step 2: Register Native Capability Provider
    // ------------------------------------------------------------------------
    println!("[2/5] Registering Pure-Rust Native Tools...");
    let native_adapter = Arc::new(NativeToolsAdapter::with_standard_tools());
    let native_manifest = native_adapter.to_manifest("hakille.system", "System Native Tools");
    let native_tools_count = native_manifest.tools.len();
    registry.register(native_manifest).await?;
    registry.activate("hakille.system", None).await?;
    println!(
        "      -> Registered and activated 'hakille.system' ({} native tools available)\n",
        native_tools_count
    );

    // ------------------------------------------------------------------------
    // Step 3: Establish MCP Session (Model Context Protocol 2024-11-05)
    // ------------------------------------------------------------------------
    println!("[3/5] Launching Sovereign Tokio MCP Session (JSON-RPC 2.0)...");
    let (client_stream, server_stream) = duplex(65536);
    tokio::spawn(async move {
        run_mock_mcp_server(server_stream).await;
    });

    let mut mcp_client = McpClientAdapter::from_duplex(client_stream);
    let init_res = mcp_client.initialize().await?;
    println!(
        "      -> MCP Handshake complete: protocolVersion={}, server={}",
        init_res.protocol_version, init_res.server_info.name
    );

    let mcp_tools = mcp_client.list_tools().await?;
    println!(
        "      -> Discovered {} MCP tools via tools/list:",
        mcp_tools.len()
    );
    for t in &mcp_tools {
        println!("         * [{}] {}", t.name, t.description);
    }

    let mcp_manifest = mcp_client
        .to_manifest("mcp.weather", "Live Weather & Echo MCP Server")
        .await?;
    registry.register(mcp_manifest).await?;
    registry.activate("mcp.weather", None).await?;
    let mcp_client_arc = Arc::new(mcp_client);
    println!("      -> Integrated and activated 'mcp.weather' in Universal Registry\n");

    // ------------------------------------------------------------------------
    // Step 4: Load Declarative Agent Skill (SKILL.md)
    // ------------------------------------------------------------------------
    println!("[4/5] Loading Declarative Cognitive Skill (3-Tier Progressive Loading)...");
    let skills_loader = Arc::new(ProgressiveSkillsLoader::new());
    let sample_skill_md = r#"---
name: database_architect
displayName: Database Sovereign Architect
version: 1.0.0
description: Cognitive heuristics for relational schema inspection, normalization, and EXPLAIN query plan tuning.
scope: filesystem
---

# Database Sovereign Architect Heuristics

1. Identify tables exceeding 10M rows lacking partition keys.
2. Flag columns with HIGH cardinality for composite B-Tree indexing.
3. Propose connection pool sizing (PgBouncer) matching CPU core count * 2.
4. Enforce strict isolation: NEVER recommend dropping production tables.
"#;

    let skill_name = skills_loader.load_skill_from_str(sample_skill_md).await?;
    println!("      -> Parsed SKILL.md: '{}'", skill_name);

    // Stage 1: Catalog
    let catalog = skills_loader.catalog_definitions().await;
    println!(
        "      -> Stage 1 (Catalog Index): ~{} tokens footprint",
        catalog[0].description.len() / 4
    );

    // Stage 2: Knowledge Injection
    let instructions = skills_loader
        .inject_instructions(&skill_name)
        .await
        .unwrap();
    println!(
        "      -> Stage 2 (Knowledge Injection): {} bytes of cognitive directives available",
        instructions.len()
    );

    // Stage 3: Bundled Resource
    skills_loader
        .attach_resource(
            &skill_name,
            "schemas/reference.sql",
            b"CREATE TABLE users (id UUID PRIMARY KEY);".to_vec(),
        )
        .await?;
    println!(
        "      -> Stage 3 (Resource Fetching): Attached bundled reference schema with path-traversal guard\n"
    );

    // ------------------------------------------------------------------------
    // Step 5: Multi-Tool Intelligent Orchestration Workflow
    // ------------------------------------------------------------------------
    println!("[5/5] Executing Intelligent Tool Orchestrator Scenarios...");
    let mut orchestrator =
        IntelligentToolOrchestrator::new(registry.clone(), None, Some(audit_adapter));

    // Wire executors
    orchestrator.register_executor("system_info", native_adapter.clone());
    orchestrator.register_executor("calculate_expression", native_adapter.clone());
    orchestrator.register_executor("emergency_shred", native_adapter.clone());
    orchestrator.register_executor("mcp_echo", mcp_client_arc.clone());
    orchestrator.register_executor("mcp_get_weather", mcp_client_arc.clone());
    orchestrator.register_executor("database_architect", skills_loader.clone());

    // Scenario A: Safe Native Tool Execution (Tier 0 ReadOnly)
    println!("   --- Scenario A: Safe Native Tool Execution (Tier 0 ReadOnly) ---");
    let req_a = OrchestrationRequest {
        intent: "Evaluate arithmetic formula 125 * 8".to_string(),
        caller_token: None,
        explicit_tool_name: Some("calculate_expression".to_string()),
        arguments: json!({ "op": "mul", "a": 125.0, "b": 8.0 }),
        confirmation_proof: None,
        timeout_ms: 2000,
    };
    let res_a = orchestrator.execute(req_a).await?;
    println!("       Target Tool   : {}", res_a.selected_tool);
    println!("       Risk Tier     : {:?}", res_a.evaluated_risk);
    println!(
        "       Output Result : {}",
        res_a.tool_result.text_summary().replace('\n', " ")
    );
    println!(
        "       Audit Record  : {}\n",
        res_a.audit_tx_id.as_deref().unwrap_or("none")
    );

    // Scenario B: Remote MCP Tool Execution (Tier 2 ExternalNetwork)
    println!("   --- Scenario B: Remote MCP Tool Execution (Tier 2 ExternalNetwork) ---");
    let req_b = OrchestrationRequest {
        intent: "Check weather in Tokyo via MCP server".to_string(),
        caller_token: None,
        explicit_tool_name: Some("mcp_get_weather".to_string()),
        arguments: json!({ "city": "Tokyo" }),
        confirmation_proof: None,
        timeout_ms: 3000,
    };
    let res_b = orchestrator.execute(req_b).await?;
    println!("       Target Tool   : {}", res_b.selected_tool);
    println!("       Risk Tier     : {:?}", res_b.evaluated_risk);
    println!(
        "       Output Result : {}",
        res_b.tool_result.text_summary().replace('\n', " ")
    );
    println!(
        "       Audit Record  : {}\n",
        res_b.audit_tx_id.as_deref().unwrap_or("none")
    );

    // Scenario C: Critical Action Protection (Tier 3 CriticalExecution Gate)
    println!("   --- Scenario C: Security Gate on Critical Tier 3 Action ---");
    let req_c_unauthorized = OrchestrationRequest {
        intent: "Trigger emergency memory shred".to_string(),
        caller_token: None,
        explicit_tool_name: Some("emergency_shred".to_string()),
        arguments: json!({ "vault_id": "ephemeral_vault_01", "confirm": true }),
        confirmation_proof: None, // Missing 600ms hold proof!
        timeout_ms: 1000,
    };
    let err_c = orchestrator.execute(req_c_unauthorized).await;
    match err_c {
        Err(e) => println!(
            "       [SUCCESSFULLY BLOCKED] Security Engine caught unauthorized Tier 3 attempt:\n       -> {}",
            e
        ),
        Ok(_) => panic!("Security failure: Tier 3 action executed without confirmation!"),
    }

    // Now supply valid 600ms physical confirmation proof
    let shred_args = json!({ "vault_id": "ephemeral_vault_01", "confirm": true });
    let action_hash = ActionConfirmationProof::compute_action_hash("emergency_shred", &shred_args);
    let valid_proof = ActionConfirmationProof::new(
        "card_shred_ui_99",
        action_hash,
        "sovereign_operator",
        650, // Physical hold duration >= 600ms
        Utc::now(),
    );
    let req_c_authorized = OrchestrationRequest {
        intent: "Trigger emergency memory shred".to_string(),
        caller_token: None,
        explicit_tool_name: Some("emergency_shred".to_string()),
        arguments: shred_args,
        confirmation_proof: Some(valid_proof),
        timeout_ms: 2000,
    };
    let res_c = orchestrator.execute(req_c_authorized).await?;
    println!("       [AUTHORIZED WITH 600MS HOLD PROOF] Critical action confirmed by user:");
    println!("       Target Tool   : {}", res_c.selected_tool);
    println!("       Risk Tier     : {:?}", res_c.evaluated_risk);
    println!(
        "       Output Result : {}",
        res_c.tool_result.text_summary().replace('\n', " ")
    );
    println!(
        "       Audit Record  : {}\n",
        res_c.audit_tx_id.as_deref().unwrap_or("none")
    );

    println!("===============================================================================");
    println!("  ALL SHOWCASE SCENARIOS COMPLETED SUCCESSFULLY");
    println!("  Universal Extensibility, MCP, Skills, Security & Audit 100% OPERATIONAL");
    println!("===============================================================================");

    Ok(())
}
