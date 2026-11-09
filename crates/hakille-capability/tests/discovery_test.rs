//! Integration tests for Hybrid Capability Discovery Engine.

use hakille_capability::types::{
    CanonicalToolDefinition, CapabilityManifest, CapabilityType, RiskTier,
};
use hakille_capability::{
    CapabilityDiscoveryEngine, DiscoveryQuery, UniversalCapabilityRegistry, cosine_similarity,
    estimate_tool_tokens,
};
use semver::Version;
use std::collections::{HashMap, HashSet};

fn create_capability(
    id: &str,
    tools: Vec<(&str, &str, serde_json::Value)>,
    cap_type: CapabilityType,
    tags: &[&str],
) -> CapabilityManifest {
    let mut manifest = CapabilityManifest::new(
        id,
        format!("Display {id}"),
        Version::parse("1.0.0").unwrap(),
        format!("Description for {id}"),
        "Hakille Discovery Tester",
        cap_type,
    );
    for (name, desc, schema) in tools {
        manifest = manifest.with_tool(CanonicalToolDefinition::new(name, desc, schema));
    }
    let tag_values: Vec<serde_json::Value> = tags
        .iter()
        .map(|t| serde_json::Value::String(t.to_string()))
        .collect();
    manifest = manifest.with_metadata("tags", serde_json::Value::Array(tag_values));
    manifest
}

#[tokio::test]
async fn test_structured_pre_filtering() {
    let registry = UniversalCapabilityRegistry::new();

    // 1. Filesystem capability (Active)
    let fs_cap = create_capability(
        "hakille.fs",
        vec![
            (
                "read_file",
                "Read contents of a file",
                serde_json::json!({"type": "object", "properties": {"path": {"type": "string"}}}),
            ),
            (
                "delete_file",
                "Delete a file permanently",
                serde_json::json!({"type": "object", "properties": {"path": {"type": "string"}}}),
            ),
        ],
        CapabilityType::Native,
        &["storage", "fs"],
    );
    registry.register(fs_cap).await.unwrap();
    registry.activate("hakille.fs", None).await.unwrap();

    // 2. Network capability (Active)
    let net_cap = create_capability(
        "hakille.net",
        vec![(
            "fetch_url",
            "Download content from remote HTTP endpoint",
            serde_json::json!({"type": "object", "properties": {"url": {"type": "string"}}}),
        )],
        CapabilityType::McpStdio,
        &["network", "web"],
    );
    registry.register(net_cap).await.unwrap();
    registry.activate("hakille.net", None).await.unwrap();

    // 3. Database capability (Registered but Disabled)
    let db_cap = create_capability(
        "hakille.db",
        vec![(
            "query_sql",
            "Execute read-only SQL query",
            serde_json::json!({"type": "object", "properties": {"sql": {"type": "string"}}}),
        )],
        CapabilityType::Native,
        &["database"],
    );
    registry.register(db_cap).await.unwrap(); // Not activated!

    let engine = CapabilityDiscoveryEngine::new(registry);
    let embeddings = HashMap::new();

    // Test A: Discovery with no filters -> returns all active tools (read_file, delete_file, fetch_url), excluding db
    let query_all = DiscoveryQuery::default();
    let discovered = engine.discover(&query_all, &embeddings).await.unwrap();
    let tool_names: Vec<String> = discovered.into_iter().map(|d| d.tool.name).collect();
    assert!(tool_names.contains(&"read_file".to_string()));
    assert!(tool_names.contains(&"delete_file".to_string()));
    assert!(tool_names.contains(&"fetch_url".to_string()));
    assert!(!tool_names.contains(&"query_sql".to_string()));

    // Test B: Filter by CapabilityType::Native -> only read_file and delete_file
    let mut allowed_types = HashSet::new();
    allowed_types.insert(CapabilityType::Native);
    let query_native = DiscoveryQuery {
        allowed_types: Some(allowed_types),
        ..Default::default()
    };

    let discovered_native = engine.discover(&query_native, &embeddings).await.unwrap();
    let native_names: Vec<String> = discovered_native.into_iter().map(|d| d.tool.name).collect();
    assert!(native_names.contains(&"read_file".to_string()));
    assert!(native_names.contains(&"delete_file".to_string()));
    assert!(!native_names.contains(&"fetch_url".to_string()));

    // Test C: Filter by RiskTier ceiling (Tier 1 max) -> excludes delete_file (Tier 3) and fetch_url (Tier 2)
    let query_tier1 = DiscoveryQuery::default().with_max_risk_tier(RiskTier::Tier1StateMutating);

    let discovered_safe = engine.discover(&query_tier1, &embeddings).await.unwrap();
    let safe_names: Vec<String> = discovered_safe.into_iter().map(|d| d.tool.name).collect();
    assert!(safe_names.contains(&"read_file".to_string()));
    assert!(!safe_names.contains(&"delete_file".to_string()));
    assert!(!safe_names.contains(&"fetch_url".to_string()));
}

#[tokio::test]
async fn test_cosine_similarity_and_ranking() {
    let registry = UniversalCapabilityRegistry::new();
    let cap = create_capability(
        "hakille.tools",
        vec![
            (
                "search_code",
                "Search source code in workspace",
                serde_json::json!({"type": "object", "properties": {}}),
            ),
            (
                "analyze_ast",
                "Analyze AST of Rust code",
                serde_json::json!({"type": "object", "properties": {}}),
            ),
            (
                "play_music",
                "Play an audio file",
                serde_json::json!({"type": "object", "properties": {}}),
            ),
        ],
        CapabilityType::Native,
        &[],
    );
    registry.register(cap).await.unwrap();
    registry.activate("hakille.tools", None).await.unwrap();

    let engine = CapabilityDiscoveryEngine::new(registry);

    // Synthetic 384-dimensional vector embeddings
    let mut q_vec = vec![0.0f32; 384];
    q_vec[0] = 1.0;

    let mut doc_code = vec![0.0f32; 384];
    doc_code[0] = 0.95;
    doc_code[1] = 0.05;

    let mut doc_ast = vec![0.0f32; 384];
    doc_ast[0] = 0.80;
    doc_ast[1] = 0.20;

    let mut doc_music = vec![0.0f32; 384];
    doc_music[10] = 1.0; // Orthogonal, similarity = 0.0

    let mut embeddings = HashMap::new();
    embeddings.insert("search_code".to_string(), doc_code);
    embeddings.insert("analyze_ast".to_string(), doc_ast);
    embeddings.insert("play_music".to_string(), doc_music);

    let query = DiscoveryQuery::default()
        .with_embedding(q_vec)
        .with_min_similarity(0.50);

    let discovered = engine.discover(&query, &embeddings).await.unwrap();
    assert_eq!(discovered.len(), 2);
    assert_eq!(discovered[0].tool.name, "search_code");
    assert!(discovered[0].similarity_score > 0.9);
    assert_eq!(discovered[1].tool.name, "analyze_ast");
    assert!(discovered[1].similarity_score > 0.75);
}

#[test]
fn test_token_estimation_and_knapsack_budget() {
    let tool = CanonicalToolDefinition::new(
        "generate_report",
        "Generate a comprehensive financial analysis report with charts and markdown summary",
        serde_json::json!({
            "type": "object",
            "properties": {
                "tenant_id": { "type": "string" },
                "quarter": { "type": "integer" }
            },
            "required": ["tenant_id"]
        }),
    );

    let tokens = estimate_tool_tokens(&tool);
    assert!(
        tokens > 40 && tokens < 150,
        "Estimated tokens should be realistic: {tokens}"
    );

    // Test cosine similarity edge cases
    let v1 = vec![1.0, 0.0, 0.0];
    let v2 = vec![1.0, 0.0, 0.0];
    assert!((cosine_similarity(&v1, &v2) - 1.0).abs() < 1e-5);

    let v_orth = vec![0.0, 1.0, 0.0];
    assert_eq!(cosine_similarity(&v1, &v_orth), 0.0);

    let v_empty: Vec<f32> = vec![];
    assert_eq!(cosine_similarity(&v_empty, &v_empty), 0.0);
}

#[tokio::test]
async fn test_prompt_budget_optimizer() {
    let registry = UniversalCapabilityRegistry::new();
    let mut tools = Vec::new();
    for i in 0..10 {
        tools.push((
            format!("tool_{i:02}"),
            format!("Long description explaining functionality for tool {i} in exhaustive detail to consume prompt tokens"),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "param1": { "type": "string" },
                    "param2": { "type": "integer" }
                }
            }),
        ));
    }

    let tool_refs: Vec<(&str, &str, serde_json::Value)> = tools
        .iter()
        .map(|(n, d, s)| (n.as_str(), d.as_str(), s.clone()))
        .collect();

    let cap = create_capability("hakille.bulk", tool_refs, CapabilityType::Native, &[]);
    registry.register(cap).await.unwrap();
    registry.activate("hakille.bulk", None).await.unwrap();

    let engine = CapabilityDiscoveryEngine::new(registry);
    let embeddings = HashMap::new();

    // Set tight token budget allowing at most 2 tools
    let query = DiscoveryQuery::default()
        .with_token_budget(120)
        .with_top_k(10);

    let discovered = engine.discover(&query, &embeddings).await.unwrap();
    let total_tokens: usize = discovered.iter().map(|d| d.estimated_tokens).sum();

    assert!(
        total_tokens <= 120,
        "Total tokens {total_tokens} must not exceed budget of 120"
    );
    assert!(!discovered.is_empty(), "At least one tool should be packed");
    assert!(
        discovered.len() < 5,
        "Tight budget should constrain tool count"
    );
}
