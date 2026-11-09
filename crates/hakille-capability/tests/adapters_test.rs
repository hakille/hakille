//! Comprehensive integration tests for Universal Capability Adapters:
//! Native, Model Context Protocol (MCP), Agent Skills, and Intelligent Orchestrator.

use chrono::Utc;
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::duplex;

use hakille_audit::AuditJournal;

use hakille_capability::adapters::{
    IntelligentToolOrchestrator, McpClientAdapter, NativeToolsAdapter, OrchestrationRequest,
    ProgressiveSkillsLoader, run_mock_mcp_server,
};
use hakille_capability::audit::CapabilityAuditAdapter;
use hakille_capability::execution::{ExecutionContext, UniversalExecutionEngine};
use hakille_capability::registry::UniversalCapabilityRegistry;
use hakille_capability::security::ActionConfirmationProof;
use hakille_capability::types::{CanonicalToolCall, RiskTier};

#[tokio::test]
async fn test_native_tools_adapter_execution_and_errors() {
    let adapter = Arc::new(NativeToolsAdapter::with_standard_tools());

    // 1. System info tool execution
    let call = CanonicalToolCall::new("call_sys_1", "system_info", json!({}));
    let ctx = ExecutionContext::new("call_sys_1", Duration::from_millis(500));
    let result = UniversalExecutionEngine::execute_guarded(adapter.clone(), &call, &ctx)
        .await
        .expect("System info execution should succeed");

    assert!(!result.is_error);
    let summary = result.text_summary();
    assert!(summary.contains("status"));
    assert!(summary.contains("operational"));

    // 2. Calculator tool execution
    let call_calc = CanonicalToolCall::new(
        "call_calc_1",
        "calculate_expression",
        json!({ "op": "mul", "a": 7.0, "b": 6.0 }),
    );
    let ctx_calc = ExecutionContext::new("call_calc_1", Duration::from_millis(500));
    let res_calc =
        UniversalExecutionEngine::execute_guarded(adapter.clone(), &call_calc, &ctx_calc)
            .await
            .expect("Calculator execution should succeed");

    assert!(!res_calc.is_error);
    assert!(res_calc.text_summary().contains("42"));

    // 3. Calculator division by zero error handling
    let call_div0 = CanonicalToolCall::new(
        "call_div0",
        "calculate_expression",
        json!({ "op": "div", "a": 10.0, "b": 0.0 }),
    );
    let res_div0 =
        UniversalExecutionEngine::execute_guarded(adapter.clone(), &call_div0, &ctx_calc)
            .await
            .expect("Should return tool result with is_error=true");

    assert!(res_div0.is_error);
    assert!(res_div0.text_summary().contains("Division by zero"));

    // 4. VFS path traversal security guard
    let call_vfs = CanonicalToolCall::new(
        "call_vfs_bad",
        "vfs_read",
        json!({ "path": "../../../etc/passwd" }),
    );
    let res_vfs = UniversalExecutionEngine::execute_guarded(adapter.clone(), &call_vfs, &ctx_calc)
        .await
        .expect("Tool should return error result");
    assert!(res_vfs.is_error);
    assert!(res_vfs.text_summary().contains("Path traversal forbidden"));

    // 5. Unknown tool lookup failure
    let call_unknown = CanonicalToolCall::new("call_unk", "nonexistent_tool", json!({}));
    let err =
        UniversalExecutionEngine::execute_guarded(adapter.clone(), &call_unknown, &ctx_calc).await;
    assert!(err.is_err());
}

#[tokio::test]
async fn test_mcp_client_server_full_lifecycle() {
    let (client_stream, server_stream) = duplex(65536);

    // Spawn the embedded mock MCP server
    tokio::spawn(async move {
        run_mock_mcp_server(server_stream).await;
    });

    let mut client = McpClientAdapter::from_duplex(client_stream);

    // 1. Mandatory MCP initialize handshake
    let init_res = client
        .initialize()
        .await
        .expect("MCP initialize must succeed");
    assert_eq!(init_res.protocol_version, "2024-11-05");
    assert_eq!(init_res.server_info.name, "mock-mcp-server");

    // 2. Discover exposed tools
    let tools = client.list_tools().await.expect("tools/list must succeed");
    assert_eq!(tools.len(), 2);
    assert!(tools.iter().any(|t| t.name == "mcp_echo"));
    assert!(tools.iter().any(|t| t.name == "mcp_get_weather"));

    // 3. Call tool mcp_echo
    let res_echo = client
        .call_tool(
            "mcp_echo",
            json!({ "message": "Sovereign Hakille Extensibility" }),
        )
        .await
        .expect("Call to mcp_echo must succeed");
    assert!(!res_echo.is_error);
    assert!(
        res_echo
            .text_summary()
            .contains("Echo from MCP: Sovereign Hakille Extensibility")
    );

    // 4. Call tool mcp_get_weather
    let res_weather = client
        .call_tool("mcp_get_weather", json!({ "city": "Tokyo" }))
        .await
        .expect("Call to mcp_get_weather must succeed");
    assert!(!res_weather.is_error);
    assert!(res_weather.text_summary().contains("Weather in Tokyo"));

    // 5. Ping
    client.ping().await.expect("MCP ping must succeed");

    // 6. Capability Manifest generation & Registry integration
    let manifest = client
        .to_manifest("mcp:weather-service", "Weather Service MCP")
        .await
        .expect("Should generate capability manifest from MCP client");
    assert_eq!(manifest.tools.len(), 2);

    let registry = UniversalCapabilityRegistry::new();
    registry
        .register(manifest)
        .await
        .expect("Should register MCP capability");
    registry
        .activate("mcp:weather-service", None)
        .await
        .expect("Should activate MCP capability");

    let found = registry.find_tool("mcp_echo").await;
    assert!(found.is_some());
    let (desc, tool_def) = found.unwrap();
    assert_eq!(desc.id, "mcp:weather-service");
    assert_eq!(tool_def.name, "mcp_echo");
}

#[tokio::test]
async fn test_progressive_skills_loader_and_security() {
    let loader = Arc::new(ProgressiveSkillsLoader::new());

    let skill_markdown = r#"---
name: sql_optimizer
displayName: SQL Sovereign Optimizer
version: 1.2.0
description: Analyzes PostgreSQL EXPLAIN output and generates optimal index recommendations.
scope: filesystem
---

# SQL Sovereign Optimizer Manual

1. Inspect the provided EXPLAIN query plan for Sequential Scans.
2. Calculate selectivity on filtered columns.
3. Propose B-Tree or BRIN composite indexes with precise ordering.
4. Always verify table write amplification impact.
"#;

    // 1. Load skill from markdown content
    let skill_name = loader
        .load_skill_from_str(skill_markdown)
        .await
        .expect("Skill parsing should succeed");
    assert_eq!(skill_name, "sql_optimizer");

    // 2. Stage 1: Catalog discovery (~40 tokens)
    let catalog = loader.catalog_definitions().await;
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].name, "sql_optimizer");
    assert!(catalog[0].description.contains("PostgreSQL EXPLAIN"));

    // 3. Stage 2: Runtime knowledge injection
    let instructions = loader
        .inject_instructions("sql_optimizer")
        .await
        .expect("Instructions must be available");
    assert!(instructions.contains("SQL Sovereign Optimizer Manual"));
    assert!(instructions.contains("Sequential Scans"));

    // 4. Stage 3: Bundled resource attachment & safe fetching
    loader
        .attach_resource(
            "sql_optimizer",
            "rules/heuristics.sql",
            b"SELECT * FROM pg_stat_activity;".to_vec(),
        )
        .await
        .expect("Attaching safe resource must succeed");

    let res_bytes = loader
        .fetch_resource("sql_optimizer", "rules/heuristics.sql")
        .await
        .expect("Fetching resource must succeed");
    assert_eq!(res_bytes, b"SELECT * FROM pg_stat_activity;");

    // Security: Path traversal rejected
    let bad_traversal = loader
        .attach_resource("sql_optimizer", "../etc/passwd", b"evil".to_vec())
        .await;
    assert!(bad_traversal.is_err());

    let bad_fetch = loader
        .fetch_resource("sql_optimizer", "../../../secret.key")
        .await;
    assert!(bad_fetch.is_err());

    // 5. Skill execution as CapabilityExecutor
    let call = CanonicalToolCall::new(
        "call_skill_1",
        "sql_optimizer",
        json!({ "action": "analyze", "input": "EXPLAIN ANALYZE SELECT * FROM users WHERE email = 'test@example.com';" }),
    );
    let ctx = ExecutionContext::new("call_skill_1", Duration::from_millis(500));
    let result = UniversalExecutionEngine::execute_guarded(loader.clone(), &call, &ctx)
        .await
        .expect("Skill execution should succeed");

    assert!(!result.is_error);
    assert!(
        result
            .text_summary()
            .contains("Executed skill 'sql_optimizer'")
    );
}

#[tokio::test]
async fn test_intelligent_tool_orchestrator_end_to_end() {
    let registry = UniversalCapabilityRegistry::new();
    let native_adapter = Arc::new(NativeToolsAdapter::with_standard_tools());

    // Register native capability
    let native_manifest = native_adapter.to_manifest("hakille.system", "System Native Tools");
    registry
        .register(native_manifest)
        .await
        .expect("Register native capability");
    registry
        .activate("hakille.system", None)
        .await
        .expect("Activate native capability");

    // Setup temporary Audit journal in memory/tempfile
    let temp_dir = tempfile::tempdir().unwrap();
    let audit_db_path = temp_dir.path().join("audit.redb");
    let mut rng = OsRng;
    let signing_key = SigningKey::generate(&mut rng);
    let journal =
        Arc::new(AuditJournal::new(audit_db_path.to_str().unwrap(), signing_key).unwrap());
    let audit_adapter = CapabilityAuditAdapter::new(journal);

    let mut orchestrator =
        IntelligentToolOrchestrator::new(registry.clone(), None, Some(audit_adapter));

    // Register executors for tools
    orchestrator.register_executor("calculate_expression", native_adapter.clone());
    orchestrator.register_executor("emergency_shred", native_adapter.clone());

    // 1. Safe tool execution via intent discovery
    let req = OrchestrationRequest {
        intent: "Calculate arithmetic sum".to_string(),
        caller_token: None,
        explicit_tool_name: Some("calculate_expression".to_string()),
        arguments: json!({ "op": "add", "a": 100.0, "b": 250.0 }),
        confirmation_proof: None,
        timeout_ms: 1000,
    };

    let outcome = orchestrator
        .execute(req)
        .await
        .expect("Execution should succeed");
    assert_eq!(outcome.selected_tool, "calculate_expression");
    assert!(!outcome.tool_result.is_error);
    assert!(outcome.tool_result.text_summary().contains("350"));
    assert!(
        outcome.audit_tx_id.is_some(),
        "Audit transaction ID should be generated"
    );

    // 2. Critical Tier 3 tool WITHOUT physical hold proof -> must be rejected
    let req_crit_unauthorized = OrchestrationRequest {
        intent: "Emergency memory purge".to_string(),
        caller_token: None,
        explicit_tool_name: Some("emergency_shred".to_string()),
        arguments: json!({ "vault_id": "vault_alpha", "confirm": true }),
        confirmation_proof: None,
        timeout_ms: 1000,
    };

    let err_crit = orchestrator.execute(req_crit_unauthorized).await;
    assert!(
        err_crit.is_err(),
        "Tier 3 action must require physical confirmation proof"
    );

    // 3. Critical Tier 3 tool WITH valid 600ms physical confirmation proof -> must succeed
    let action_args = json!({ "vault_id": "vault_alpha", "confirm": true });
    let action_hash = ActionConfirmationProof::compute_action_hash("emergency_shred", &action_args);
    let valid_proof = ActionConfirmationProof::new(
        "card_shred_01",
        action_hash,
        "operator_root",
        650, // >= 600ms hold
        Utc::now(),
    );

    let req_crit_authorized = OrchestrationRequest {
        intent: "Emergency memory purge".to_string(),
        caller_token: None,
        explicit_tool_name: Some("emergency_shred".to_string()),
        arguments: action_args,
        confirmation_proof: Some(valid_proof),
        timeout_ms: 1000,
    };

    let outcome_crit = orchestrator
        .execute(req_crit_authorized)
        .await
        .expect("Authorized critical action with 600ms hold must succeed");

    assert_eq!(outcome_crit.selected_tool, "emergency_shred");
    assert!(!outcome_crit.tool_result.is_error);
    assert!(outcome_crit.tool_result.text_summary().contains("purged"));
    assert_eq!(
        outcome_crit.evaluated_risk,
        RiskTier::Tier3CriticalExecution
    );
}
