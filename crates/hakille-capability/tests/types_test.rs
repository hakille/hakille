use hakille_capability::{
    CanonicalContentBlock, CanonicalToolCall, CanonicalToolDefinition, CanonicalToolResult,
    CapabilityDescriptor, CapabilityManifest, CapabilityScope, CapabilityType, ExecutionMetadata,
    RiskTier, ValidationError,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TestArgs {
    query: String,
    limit: u32,
}

#[test]
fn test_risk_tier_hierarchy_and_properties() {
    assert_eq!(RiskTier::Tier0ReadOnly.level(), 0);
    assert_eq!(RiskTier::Tier1StateMutating.level(), 1);
    assert_eq!(RiskTier::Tier2ExternalNetwork.level(), 2);
    assert_eq!(RiskTier::Tier3CriticalExecution.level(), 3);

    assert_eq!(RiskTier::from_level(0), Some(RiskTier::Tier0ReadOnly));
    assert_eq!(RiskTier::from_level(1), Some(RiskTier::Tier1StateMutating));
    assert_eq!(
        RiskTier::from_level(2),
        Some(RiskTier::Tier2ExternalNetwork)
    );
    assert_eq!(
        RiskTier::from_level(3),
        Some(RiskTier::Tier3CriticalExecution)
    );
    assert_eq!(RiskTier::from_level(4), None);

    assert!(!RiskTier::Tier0ReadOnly.requires_user_confirmation());
    assert!(!RiskTier::Tier1StateMutating.requires_user_confirmation());
    assert!(!RiskTier::Tier2ExternalNetwork.requires_user_confirmation());
    assert!(RiskTier::Tier3CriticalExecution.requires_user_confirmation());

    assert!(!RiskTier::Tier0ReadOnly.has_network_egress());
    assert!(!RiskTier::Tier1StateMutating.has_network_egress());
    assert!(RiskTier::Tier2ExternalNetwork.has_network_egress());
    assert!(RiskTier::Tier3CriticalExecution.has_network_egress());

    assert!(!RiskTier::Tier0ReadOnly.is_mutating());
    assert!(RiskTier::Tier1StateMutating.is_mutating());
    assert!(RiskTier::Tier2ExternalNetwork.is_mutating());
    assert!(RiskTier::Tier3CriticalExecution.is_mutating());

    // Serde roundtrip
    let serialized = serde_json::to_string(&RiskTier::Tier3CriticalExecution).unwrap();
    assert_eq!(serialized, "\"tier3_critical_execution\"");
    let deserialized: RiskTier = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, RiskTier::Tier3CriticalExecution);

    // Aliases
    let alias1: RiskTier = serde_json::from_str("\"tier3\"").unwrap();
    assert_eq!(alias1, RiskTier::Tier3CriticalExecution);
    let alias2: RiskTier = serde_json::from_str("\"Tier3CriticalExecution\"").unwrap();
    assert_eq!(alias2, RiskTier::Tier3CriticalExecution);
}

#[test]
fn test_capability_type_properties() {
    assert!(!CapabilityType::Native.is_mcp());
    assert!(CapabilityType::McpStdio.is_mcp());
    assert!(CapabilityType::McpSse.is_mcp());
    assert!(!CapabilityType::Wasm.is_mcp());
    assert!(!CapabilityType::Skill.is_mcp());
    assert!(!CapabilityType::Rest.is_mcp());

    assert!(!CapabilityType::Native.is_sandboxed());
    assert!(CapabilityType::McpStdio.is_sandboxed());
    assert!(CapabilityType::Wasm.is_sandboxed());
    assert!(!CapabilityType::Rest.is_sandboxed());

    let ser = serde_json::to_string(&CapabilityType::McpStdio).unwrap();
    assert_eq!(ser, "\"mcp_stdio\"");
    let de: CapabilityType = serde_json::from_str(&ser).unwrap();
    assert_eq!(de, CapabilityType::McpStdio);
}

#[test]
fn test_canonical_tool_definition_validation() {
    let schema = json!({
        "type": "object",
        "properties": {
            "query": { "type": "string" },
            "limit": { "type": "integer" }
        },
        "required": ["query"]
    });

    let tool =
        CanonicalToolDefinition::new("search_docs", "Search documentation index", schema.clone());
    assert!(tool.validate().is_ok());

    // Empty tool name
    let bad_name = CanonicalToolDefinition::new("", "desc", schema.clone());
    assert!(matches!(
        bad_name.validate(),
        Err(ValidationError::EmptyField { field: "name", .. })
    ));

    // Invalid characters in name
    let invalid_chars = CanonicalToolDefinition::new("search docs!", "desc", schema.clone());
    assert!(matches!(
        invalid_chars.validate(),
        Err(ValidationError::InvalidToolName { .. })
    ));

    // Empty description
    let bad_desc = CanonicalToolDefinition::new("search", "   ", schema.clone());
    assert!(matches!(
        bad_desc.validate(),
        Err(ValidationError::EmptyField {
            field: "description",
            ..
        })
    ));

    // Non-object parameters schema
    let bad_schema = CanonicalToolDefinition::new("search", "desc", json!("string"));
    assert!(matches!(
        bad_schema.validate(),
        Err(ValidationError::InvalidParametersSchema { .. })
    ));

    // Strict mode validation failure when additionalProperties: false is missing
    let strict_tool = tool.clone().with_strict(true);
    assert!(matches!(
        strict_tool.validate(),
        Err(ValidationError::StrictModeViolation { .. })
    ));

    // Valid strict mode schema
    let valid_strict_schema = json!({
        "type": "object",
        "properties": {
            "query": { "type": "string" },
            "limit": { "type": ["integer", "null"] }
        },
        "required": ["limit", "query"],
        "additionalProperties": false
    });
    let valid_strict_tool =
        CanonicalToolDefinition::new("search_docs", "desc", valid_strict_schema).with_strict(true);
    assert!(valid_strict_tool.validate().is_ok());
}

#[test]
fn test_canonical_tool_call_and_result() {
    let args = json!({ "query": "rust 2024", "limit": 10 });
    let call = CanonicalToolCall::new("call_001", "search_docs", args);
    assert_eq!(call.call_id, "call_001");
    assert_eq!(call.tool_name, "search_docs");

    let parsed_args: TestArgs = call.parse_arguments().unwrap();
    assert_eq!(
        parsed_args,
        TestArgs {
            query: "rust 2024".to_string(),
            limit: 10
        }
    );

    // Call from stringified JSON
    let from_str =
        CanonicalToolCall::from_json_str("call_002", "search", "{\"query\":\"test\",\"limit\":5}")
            .unwrap();
    assert_eq!(from_str.arguments["query"], "test");
    assert_eq!(from_str.arguments["limit"], 5);

    // Call from empty string
    let from_empty = CanonicalToolCall::from_json_str("call_003", "empty", "").unwrap();
    assert_eq!(from_empty.arguments, json!({}));

    // Tool Result Success
    let meta = ExecutionMetadata::new(45, RiskTier::Tier0ReadOnly)
        .with_fuel(1200)
        .with_memory(4096)
        .with_audit_record_id("audit_123");
    let result = CanonicalToolResult::success_text("call_001", "Found 10 results", meta.clone());
    assert!(result.is_success());
    assert!(!result.is_error);
    assert_eq!(result.text_summary(), "Found 10 results");
    assert_eq!(result.metadata.fuel_consumed, Some(1200));

    // Tool Result Error
    let err_result = CanonicalToolResult::error("call_001", "File not found", meta);
    assert!(!err_result.is_success());
    assert!(err_result.is_error);
    assert_eq!(err_result.text_summary(), "File not found");
}

#[test]
fn test_content_block_serde_tagging() {
    let text_block = CanonicalContentBlock::text("Hello, world!");
    assert!(text_block.is_text());
    assert_eq!(text_block.as_text(), Some("Hello, world!"));

    let img_block = CanonicalContentBlock::image("image/png", "iVBORw0KGgoAAAANSUhEUg==");
    assert!(img_block.is_image());

    let res_block = CanonicalContentBlock::resource_text(
        "file:///doc.txt",
        Some("text/plain".into()),
        "Document text",
    );
    assert!(res_block.is_resource());

    // Serde JSON tag check
    let serialized_text = serde_json::to_string(&text_block).unwrap();
    assert!(serialized_text.contains("\"type\":\"text\""));

    let serialized_img = serde_json::to_string(&img_block).unwrap();
    assert!(serialized_img.contains("\"type\":\"image\""));

    let deserialized_img: CanonicalContentBlock = serde_json::from_str(&serialized_img).unwrap();
    assert_eq!(deserialized_img, img_block);
}

#[test]
fn test_manifest_and_descriptor_lifecycle() {
    let version = semver::Version::parse("1.2.3").unwrap();
    let mut manifest = CapabilityManifest::new(
        "hakille.fs",
        "Filesystem Capability",
        version.clone(),
        "Provides read and write access to sovereign virtual filesystem.",
        "Hakille Systems",
        CapabilityType::Native,
    );

    manifest = manifest
        .with_scope(CapabilityScope::Filesystem)
        .with_tool(CanonicalToolDefinition::new(
            "fs_read",
            "Read file contents",
            json!({"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}),
        ))
        .with_metadata("license", json!("Apache-2.0"));

    assert!(manifest.validate().is_ok());
    assert_eq!(manifest.find_tool("fs_read").unwrap().name, "fs_read");
    assert!(manifest.find_tool("unknown").is_none());

    // Convert to descriptor
    let descriptor: CapabilityDescriptor = manifest.to_descriptor();
    assert!(descriptor.validate().is_ok());
    assert!(descriptor.requires_scope(&CapabilityScope::Filesystem));
    assert!(!descriptor.requires_scope(&CapabilityScope::Network));

    // Duplicate tool name rejection
    let mut dup_manifest = manifest.clone();
    dup_manifest.tools.push(CanonicalToolDefinition::new(
        "fs_read",
        "Duplicate tool",
        json!({"type": "object"}),
    ));
    assert!(matches!(
        dup_manifest.validate(),
        Err(ValidationError::DuplicateToolName { .. })
    ));

    // TOML roundtrip
    let toml_str = toml::to_string(&manifest).unwrap();
    let from_toml = CapabilityManifest::from_toml(&toml_str).unwrap();
    assert_eq!(from_toml.id, "hakille.fs");
    assert_eq!(from_toml.version, version);

    // JSON roundtrip
    let json_str = serde_json::to_string(&manifest).unwrap();
    let from_json = CapabilityManifest::from_json(&json_str).unwrap();
    assert_eq!(from_json.id, "hakille.fs");
}
