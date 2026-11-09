//! Comprehensive test suite for Universal Capability Registry.

use hakille_capability::types::{
    CanonicalToolDefinition, CapabilityManifest, CapabilityScope, CapabilityType,
};
use hakille_capability::{CapabilityLifecycle, RegistryError, UniversalCapabilityRegistry};
use semver::{Version, VersionReq};
use std::sync::Arc;

fn create_test_manifest(
    id: &str,
    version: &str,
    tool_names: &[&str],
    scopes: &[CapabilityScope],
    cap_type: CapabilityType,
    tags: &[&str],
) -> CapabilityManifest {
    let mut manifest = CapabilityManifest::new(
        id,
        format!("Display Name for {id}"),
        Version::parse(version).unwrap(),
        format!("Description for capability {id}"),
        "Hakille Test Suite",
        cap_type,
    );
    for scope in scopes {
        manifest = manifest.with_scope(scope.clone());
    }
    for tool_name in tool_names {
        let tool = CanonicalToolDefinition::new(
            *tool_name,
            format!("Documentation for tool {tool_name}"),
            CanonicalToolDefinition::empty_parameters_schema(),
        );
        manifest = manifest.with_tool(tool);
    }
    let tag_values: Vec<serde_json::Value> = tags
        .iter()
        .map(|t| serde_json::Value::String(t.to_string()))
        .collect();
    manifest = manifest.with_metadata("tags", serde_json::Value::Array(tag_values));
    manifest
}

#[tokio::test]
async fn test_registration_and_initial_lifecycle() {
    let registry = UniversalCapabilityRegistry::new();
    let manifest = create_test_manifest(
        "hakille.fs",
        "1.0.0",
        &["read_file"],
        &[CapabilityScope::Filesystem],
        CapabilityType::Native,
        &["storage", "io"],
    );

    // Register capability
    let reg_res = registry.register(manifest.clone()).await;
    assert!(reg_res.is_ok(), "Registration should succeed");

    // Duplicate registration should fail
    let dup_res = registry.register(manifest).await;
    match dup_res {
        Err(RegistryError::AlreadyExists { id, version }) => {
            assert_eq!(id, "hakille.fs");
            assert_eq!(version, Version::parse("1.0.0").unwrap());
        }
        other => panic!("Expected AlreadyExists error, got {other:?}"),
    }

    // Newly registered capability should be in Registered lifecycle state
    let ver = Version::parse("1.0.0").unwrap();
    let lifecycle = registry.get_lifecycle("hakille.fs", &ver).await;
    assert!(matches!(
        lifecycle,
        Some(CapabilityLifecycle::Registered { .. })
    ));

    // Not yet active
    assert!(registry.list_active().await.is_empty());
    assert!(registry.find_tool("read_file").await.is_none());
}

#[tokio::test]
async fn test_activation_and_lifecycle_transitions() {
    let registry = UniversalCapabilityRegistry::new();
    let manifest_v1 = create_test_manifest(
        "hakille.net",
        "1.0.0",
        &["http_get"],
        &[CapabilityScope::Network],
        CapabilityType::Native,
        &["network"],
    );
    let manifest_v2 = create_test_manifest(
        "hakille.net",
        "1.1.0",
        &["http_get", "http_post"],
        &[CapabilityScope::Network],
        CapabilityType::Native,
        &["network"],
    );

    registry.register(manifest_v1).await.unwrap();
    registry.register(manifest_v2).await.unwrap();

    // Activate with requirement ^1.0
    let req = VersionReq::parse("^1.0").unwrap();
    let activated_ver = registry.activate("hakille.net", Some(&req)).await.unwrap();
    assert_eq!(activated_ver, Version::parse("1.1.0").unwrap());

    // Check active lifecycle
    let active_list = registry.list_active().await;
    assert_eq!(active_list.len(), 1);
    assert_eq!(active_list[0].id, "hakille.net");
    assert_eq!(active_list[0].version, Version::parse("1.1.0").unwrap());

    // Tools exposed
    let tool_res = registry.find_tool("http_post").await;
    assert!(tool_res.is_some());
    let (desc, tool) = tool_res.unwrap();
    assert_eq!(desc.id, "hakille.net");
    assert_eq!(tool.name, "http_post");

    // Deprecate active capability
    let dep_res = registry
        .deprecate(
            "hakille.net",
            Some("Use v2.0 in future".to_string()),
            Some("hakille.net.v2".to_string()),
        )
        .await;
    assert!(dep_res.is_ok());

    let lifecycle = registry
        .get_lifecycle("hakille.net", &Version::parse("1.1.0").unwrap())
        .await;
    assert!(matches!(
        lifecycle,
        Some(CapabilityLifecycle::Deprecated { .. })
    ));

    // Deprecated tools are still callable
    assert!(registry.find_tool("http_get").await.is_some());

    // Disable capability
    let dis_res = registry.disable("hakille.net").await;
    assert!(dis_res.is_ok());
    assert!(registry.list_active().await.is_empty());
    assert!(registry.find_tool("http_get").await.is_none());

    // Unregister disabled version
    let unreg_res = registry
        .unregister("hakille.net", &Version::parse("1.1.0").unwrap())
        .await;
    assert!(unreg_res.is_ok());

    // V1.0.0 still registered
    assert!(
        registry
            .get_lifecycle("hakille.net", &Version::parse("1.0.0").unwrap())
            .await
            .is_some()
    );
}

#[tokio::test]
async fn test_active_unregister_blocked() {
    let registry = UniversalCapabilityRegistry::new();
    let manifest = create_test_manifest(
        "hakille.db",
        "1.0.0",
        &["sql_query"],
        &[CapabilityScope::Filesystem],
        CapabilityType::Native,
        &["db"],
    );
    registry.register(manifest).await.unwrap();
    registry.activate("hakille.db", None).await.unwrap();

    let ver = Version::parse("1.0.0").unwrap();
    let unreg_res = registry.unregister("hakille.db", &ver).await;
    match unreg_res {
        Err(RegistryError::ActiveUnregisterBlocked { id }) => {
            assert_eq!(id, "hakille.db");
        }
        other => panic!("Expected ActiveUnregisterBlocked error, got {other:?}"),
    }
}

#[tokio::test]
async fn test_tool_collision_detection() {
    let registry = UniversalCapabilityRegistry::new();
    let cap_a = create_test_manifest(
        "hakille.provider_a",
        "1.0.0",
        &["search_docs"],
        &[],
        CapabilityType::Native,
        &[],
    );
    let cap_b = create_test_manifest(
        "hakille.provider_b",
        "1.0.0",
        &["search_docs"],
        &[],
        CapabilityType::Native,
        &[],
    );

    registry.register(cap_a).await.unwrap();
    registry.register(cap_b).await.unwrap();

    // Activate provider A
    registry.activate("hakille.provider_a", None).await.unwrap();

    // Activating provider B with colliding tool 'search_docs' must fail
    let act_b = registry.activate("hakille.provider_b", None).await;
    match act_b {
        Err(RegistryError::ToolCollision {
            tool_name,
            existing_capability_id,
        }) => {
            assert_eq!(tool_name, "search_docs");
            assert!(existing_capability_id.contains("hakille.provider_a@1.0.0"));
        }
        other => panic!("Expected ToolCollision error, got {other:?}"),
    }
}

#[tokio::test]
async fn test_multidimensional_indices() {
    let registry = UniversalCapabilityRegistry::new();

    let fs_cap = create_test_manifest(
        "hakille.fs",
        "1.0.0",
        &["read_file"],
        &[CapabilityScope::Filesystem],
        CapabilityType::Native,
        &["storage", "io"],
    );
    let net_cap = create_test_manifest(
        "hakille.mcp.github",
        "1.0.0",
        &["list_issues"],
        &[CapabilityScope::Network],
        CapabilityType::McpStdio,
        &["vcs", "github"],
    );

    registry.register(fs_cap).await.unwrap();
    registry.register(net_cap).await.unwrap();

    registry.activate("hakille.fs", None).await.unwrap();
    registry.activate("hakille.mcp.github", None).await.unwrap();

    // Scope index query
    let fs_scoped = registry.list_by_scope(&CapabilityScope::Filesystem).await;
    assert_eq!(fs_scoped.len(), 1);
    assert_eq!(fs_scoped[0].id, "hakille.fs");

    // Type index query
    let mcp_typed = registry.list_by_type(CapabilityType::McpStdio).await;
    assert_eq!(mcp_typed.len(), 1);
    assert_eq!(mcp_typed[0].id, "hakille.mcp.github");

    // Tag index query
    let tag_results = registry.list_by_tag("storage").await;
    assert_eq!(tag_results.len(), 1);
    assert_eq!(tag_results[0].id, "hakille.fs");
}

#[tokio::test]
async fn test_concurrent_registry_access() {
    let registry = Arc::new(UniversalCapabilityRegistry::new());
    let mut handles = Vec::new();

    for i in 0..10 {
        let reg = Arc::clone(&registry);
        handles.push(tokio::spawn(async move {
            let id = format!("hakille.cap_{i}");
            let tool = format!("tool_{i}");
            let manifest = create_test_manifest(
                &id,
                "1.0.0",
                &[&tool],
                &[CapabilityScope::ModelInference],
                CapabilityType::Native,
                &["test"],
            );
            reg.register(manifest).await.unwrap();
            reg.activate(&id, None).await.unwrap();

            // Read path immediately
            let found = reg.find_tool(&tool).await;
            assert!(found.is_some());
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let active = registry.list_active().await;
    assert_eq!(active.len(), 10);
}
