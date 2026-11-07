use hakille_capability::translators::anthropic::AnthropicTranslator;
use hakille_capability::translators::chatml::ChatMlTranslator;
use hakille_capability::translators::gemini::GeminiTranslator;
use hakille_capability::translators::openai::{OpenAiTranslator, enforce_openai_strict_schema};
use hakille_capability::translators::{ProviderTranslator, SchemaNormalizer, TranslatorError};
use hakille_capability::{
    CanonicalContentBlock, CanonicalToolCall, CanonicalToolDefinition, CanonicalToolResult,
    ExecutionMetadata, RiskTier,
};
use proptest::prelude::*;
use serde_json::json;

#[test]
fn test_schema_normalizer_trait() {
    let mut schema = json!({
        "type": "object",
        "properties": { "name": { "type": "string" } }
    });
    let normalizer = OpenAiTranslator;
    normalizer
        .normalize_schema(&mut schema, "normalizer_test")
        .unwrap();
    assert_eq!(schema["additionalProperties"], json!(false));
}

// ============================================================================
// 1. OpenAI Strict Mode & Translators Tests
// ============================================================================

#[test]
fn test_openai_tool_definition_roundtrip() {
    let schema = json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "Absolute file path" },
            "encoding": { "type": "string", "description": "Optional file encoding" }
        },
        "required": ["path"]
    });

    let canonical = CanonicalToolDefinition::new("fs_read_file", "Read file from disk", schema)
        .with_strict(true);

    let openai_def = OpenAiTranslator::to_provider_tool(&canonical).unwrap();
    assert_eq!(openai_def.tool_type, "function");
    assert_eq!(openai_def.function.name, "fs_read_file");
    assert_eq!(openai_def.function.strict, Some(true));

    // Strict mode mutations verification:
    // 1. additionalProperties: false must be present at root
    assert_eq!(
        openai_def.function.parameters.get("additionalProperties"),
        Some(&serde_json::Value::Bool(false))
    );

    // 2. 'encoding' was optional, so it must be converted to null union: ["string", "null"]
    let encoding_prop = &openai_def.function.parameters["properties"]["encoding"];
    assert_eq!(encoding_prop["type"], json!(["string", "null"]));

    // 3. 'required' array must contain both "encoding" and "path"
    let req_arr = openai_def.function.parameters["required"]
        .as_array()
        .unwrap();
    let req_strings: Vec<&str> = req_arr.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(req_strings, vec!["encoding", "path"]);

    // Inverse conversion back to canonical
    let recovered = OpenAiTranslator::from_provider_tool(&openai_def).unwrap();
    assert_eq!(recovered.name, canonical.name);
    assert_eq!(recovered.description, canonical.description);
    assert!(recovered.strict);
}

#[test]
fn test_openai_strict_schema_recursion_and_keyword_stripping() {
    let mut schema = json!({
        "type": "object",
        "patternProperties": { "^s_": { "type": "string" } },
        "properties": {
            "user": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "age": { "type": "integer" }
                },
                "required": ["name"]
            }
        },
        "required": ["user"]
    });

    enforce_openai_strict_schema(&mut schema, "test_tool").unwrap();

    // Verify patternProperties stripped
    assert!(schema.get("patternProperties").is_none());

    // Root additionalProperties is false
    assert_eq!(schema["additionalProperties"], json!(false));

    // Nested user object has additionalProperties: false
    let user_schema = &schema["properties"]["user"];
    assert_eq!(user_schema["additionalProperties"], json!(false));

    // Nested age was optional, so must be nullable
    assert_eq!(
        user_schema["properties"]["age"]["type"],
        json!(["integer", "null"])
    );

    // Nested required must contain age and name
    let nested_req: Vec<&str> = user_schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(nested_req, vec!["age", "name"]);
}

#[test]
fn test_openai_tool_call_and_result_roundtrip() {
    let call = CanonicalToolCall::new(
        "call_12345",
        "fs_read_file",
        json!({
            "path": "/home/antigravity/Cargo.toml",
            "lines": 50
        }),
    );

    let openai_call = OpenAiTranslator::encode_tool_call(&call).unwrap();
    assert_eq!(openai_call.id, "call_12345");
    assert_eq!(openai_call.call_type, "function");
    assert_eq!(openai_call.function.name, "fs_read_file");

    // Arguments must be stringified JSON
    let parsed_args: serde_json::Value =
        serde_json::from_str(&openai_call.function.arguments).unwrap();
    assert_eq!(parsed_args["path"], "/home/antigravity/Cargo.toml");
    assert_eq!(parsed_args["lines"], 50);

    // Parse back
    let decoded_call = OpenAiTranslator::parse_tool_call(&openai_call).unwrap();
    assert_eq!(decoded_call, call);

    // Result roundtrip (success with text and image)
    let meta = ExecutionMetadata::new(32, RiskTier::Tier0ReadOnly);
    let result = CanonicalToolResult::success(
        "call_12345",
        vec![
            CanonicalContentBlock::text("File content line 1"),
            CanonicalContentBlock::image("image/png", "iVBORw0KGgoAAAANSUhEUg=="),
        ],
        meta,
    );

    let openai_result = OpenAiTranslator::encode_tool_result(&result).unwrap();
    assert_eq!(openai_result.role, "tool");
    assert_eq!(openai_result.tool_call_id, "call_12345");
    assert!(openai_result.content.contains("File content line 1"));
    assert!(
        openai_result
            .content
            .contains("[Multimodal Image (image/png)")
    );

    let decoded_result = OpenAiTranslator::parse_tool_result(&openai_result).unwrap();
    assert!(!decoded_result.is_error);
    assert_eq!(decoded_result.call_id, "call_12345");

    // Result with error
    let err_result = CanonicalToolResult::error(
        "call_12345",
        "Permission denied: access restricted",
        ExecutionMetadata::default(),
    );
    let openai_err = OpenAiTranslator::encode_tool_result(&err_result).unwrap();
    assert!(openai_err.content.starts_with("[ERROR]"));

    let decoded_err = OpenAiTranslator::parse_tool_result(&openai_err).unwrap();
    assert!(decoded_err.is_error);
    assert!(decoded_err.text_summary().contains("Permission denied"));
}

#[test]
fn test_openai_malformed_arguments_handling() {
    let bad_call = hakille_capability::translators::openai::OpenAiToolCall {
        id: "call_bad".to_string(),
        call_type: "function".to_string(),
        function: hakille_capability::translators::openai::OpenAiFunctionCallPayload {
            name: "fs_read_file".to_string(),
            arguments: "{ bad json: missing quotes }".to_string(),
        },
    };

    let result = OpenAiTranslator::parse_tool_call(&bad_call);
    assert!(matches!(
        result,
        Err(TranslatorError::MalformedArguments { .. })
    ));
}

// ============================================================================
// 2. Anthropic Claude Translators Tests
// ============================================================================

#[test]
fn test_anthropic_tool_definition_and_sanitization() {
    let schema = json!({
        "type": "object",
        "properties": {
            "path": { "type": "string" }
        },
        "required": ["path"]
    });

    let tool =
        CanonicalToolDefinition::new("mcp:github/get_repo", "Fetch repo info", schema.clone());
    let anthropic_def = AnthropicTranslator::to_provider_tool(&tool).unwrap();

    // Anthropic names must be sanitized from : and / to _
    assert_eq!(anthropic_def.name, "mcp_github_get_repo");
    assert_eq!(anthropic_def.description, "Fetch repo info");
    assert_eq!(anthropic_def.input_schema, schema);

    let canonical = AnthropicTranslator::from_provider_tool(&anthropic_def).unwrap();
    assert_eq!(canonical.name, "mcp_github_get_repo");
}

#[test]
fn test_anthropic_tool_call_and_multimodal_result_roundtrip() {
    let call = CanonicalToolCall::new("toolu_01A09q90", "get_weather", json!({"city": "Paris"}));
    let anthropic_call = AnthropicTranslator::encode_tool_call(&call).unwrap();
    assert_eq!(anthropic_call.block_type, "tool_use");
    assert_eq!(anthropic_call.id, "toolu_01A09q90");
    assert_eq!(anthropic_call.input, json!({"city": "Paris"}));

    let decoded_call = AnthropicTranslator::parse_tool_call(&anthropic_call).unwrap();
    assert_eq!(decoded_call, call);

    // Multimodal result roundtrip
    let result = CanonicalToolResult::success(
        "toolu_01A09q90",
        vec![
            CanonicalContentBlock::text("Weather in Paris: 18°C"),
            CanonicalContentBlock::image("image/jpeg", "/9j/4AAQSkZJRg=="),
        ],
        ExecutionMetadata::default(),
    );

    let anthropic_result = AnthropicTranslator::encode_tool_result(&result).unwrap();
    assert_eq!(anthropic_result.block_type, "tool_result");
    assert_eq!(anthropic_result.tool_use_id, "toolu_01A09q90");
    assert_eq!(anthropic_result.is_error, None);

    let decoded_result = AnthropicTranslator::parse_tool_result(&anthropic_result).unwrap();
    assert!(!decoded_result.is_error);
    assert_eq!(decoded_result.content.len(), 2);
    assert!(decoded_result.content[0].is_text());
    assert!(decoded_result.content[1].is_image());

    // Error result
    let err_result = CanonicalToolResult::error(
        "toolu_01A09q90",
        "API limit exceeded",
        ExecutionMetadata::default(),
    );
    let anthropic_err = AnthropicTranslator::encode_tool_result(&err_result).unwrap();
    assert_eq!(anthropic_err.is_error, Some(true));

    let decoded_err = AnthropicTranslator::parse_tool_result(&anthropic_err).unwrap();
    assert!(decoded_err.is_error);
    assert_eq!(decoded_err.text_summary(), "API limit exceeded");
}

// ============================================================================
// 3. Google Gemini Translators Tests
// ============================================================================

#[test]
fn test_gemini_type_normalization_and_declaration() {
    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://hakille.io/schemas/tool.json",
        "additionalProperties": false,
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "count": { "type": "integer" },
            "tags": {
                "type": "array",
                "items": { "type": "string" }
            }
        },
        "required": ["name"]
    });

    let tool = CanonicalToolDefinition::new("gemini_tool", "Tool description", schema);
    let gemini_decl = GeminiTranslator::to_provider_tool(&tool).unwrap();

    // Verify Draft 2020-12 tags stripped
    assert!(gemini_decl.parameters.get("$schema").is_none());
    assert!(gemini_decl.parameters.get("$id").is_none());
    assert!(gemini_decl.parameters.get("additionalProperties").is_none());

    // Verify uppercase types
    assert_eq!(gemini_decl.parameters["type"], "OBJECT");
    assert_eq!(
        gemini_decl.parameters["properties"]["name"]["type"],
        "STRING"
    );
    assert_eq!(
        gemini_decl.parameters["properties"]["count"]["type"],
        "INTEGER"
    );
    assert_eq!(
        gemini_decl.parameters["properties"]["tags"]["type"],
        "ARRAY"
    );
    assert_eq!(
        gemini_decl.parameters["properties"]["tags"]["items"]["type"],
        "STRING"
    );

    // Inverse conversion recovers lowercase types
    let recovered = GeminiTranslator::from_provider_tool(&gemini_decl).unwrap();
    assert_eq!(recovered.parameters["type"], "object");
    assert_eq!(recovered.parameters["properties"]["name"]["type"], "string");
}

#[test]
fn test_gemini_tool_call_and_response_roundtrip() {
    let call = CanonicalToolCall::new(
        "call_gemini_001",
        "get_stock_quote",
        json!({"ticker": "RUST"}),
    );
    let part = GeminiTranslator::encode_tool_call(&call).unwrap();
    assert_eq!(part.function_call.id, Some("call_gemini_001".to_string()));
    assert_eq!(part.function_call.name, "get_stock_quote");
    assert_eq!(part.function_call.args["ticker"], "RUST");

    let decoded_call = GeminiTranslator::parse_tool_call(&part).unwrap();
    assert_eq!(decoded_call, call);

    // Synthetic call ID when Gemini omits ID
    let part_no_id = hakille_capability::translators::gemini::GeminiFunctionCallPart {
        function_call: hakille_capability::translators::gemini::GeminiFunctionCall {
            id: None,
            name: "test_tool".to_string(),
            args: json!({}),
        },
    };
    let parsed_synthetic = GeminiTranslator::parse_tool_call(&part_no_id).unwrap();
    assert!(
        parsed_synthetic
            .call_id
            .starts_with("call_gemini_test_tool_")
    );

    // Tool Result response envelope
    let result =
        CanonicalToolResult::success_text("001", "Price: $150.00", ExecutionMetadata::default());
    let resp_part = GeminiTranslator::encode_tool_result(&result).unwrap();
    assert_eq!(resp_part.function_response.response["success"], json!(true));
    assert_eq!(
        resp_part.function_response.response["result"],
        json!("Price: $150.00")
    );

    let decoded_result = GeminiTranslator::parse_tool_result(&resp_part).unwrap();
    assert!(!decoded_result.is_error);
    assert_eq!(decoded_result.text_summary(), "Price: $150.00");

    // Error result response envelope
    let err_result =
        CanonicalToolResult::error("001", "Symbol not found", ExecutionMetadata::default());
    let err_part = GeminiTranslator::encode_tool_result(&err_result).unwrap();
    assert_eq!(err_part.function_response.response["success"], json!(false));
    assert_eq!(err_part.function_response.response["is_error"], json!(true));

    let decoded_err = GeminiTranslator::parse_tool_result(&err_part).unwrap();
    assert!(decoded_err.is_error);
    assert_eq!(decoded_err.text_summary(), "Symbol not found");
}

// ============================================================================
// 4. ChatML Local GGUF Stream Parser Tests
// ============================================================================

#[test]
fn test_chatml_stream_multi_call_parsing() {
    let stream_text = r#"
I will check the configuration file and execute a query.
<tool_call>
```json
{
  "name": "fs_read_file",
  "arguments": {
    "path": "/home/antigravity/hakille/Cargo.toml"
  }
}
```
</tool_call>
Here is the next step:
<tool_call>
{"tool_name": "db_query", "parameters": {"sql": "SELECT 1;"}}
</tool_call>
Finished preparing requests.
"#;

    let calls = ChatMlTranslator::parse_generation_stream(stream_text).unwrap();
    assert_eq!(calls.len(), 2);

    assert_eq!(calls[0].tool_name, "fs_read_file");
    assert_eq!(
        calls[0].arguments["path"],
        "/home/antigravity/hakille/Cargo.toml"
    );
    assert!(calls[0].call_id.starts_with("call_chatml_fs_read_file_"));

    assert_eq!(calls[1].tool_name, "db_query");
    assert_eq!(calls[1].arguments["sql"], "SELECT 1;");
    assert!(calls[1].call_id.starts_with("call_chatml_db_query_"));
}

#[test]
fn test_chatml_system_tools_prompt_and_response_format() {
    let tools = vec![CanonicalToolDefinition::new(
        "echo",
        "Echo input back",
        json!({"type": "object"}),
    )];

    let prompt = ChatMlTranslator::format_system_tools_prompt(&tools).unwrap();
    assert!(prompt.contains("<tools>"));
    assert!(prompt.contains("</tools>"));
    assert!(prompt.contains("\"name\": \"echo\""));

    let result =
        CanonicalToolResult::success_text("call_01", "Output text", ExecutionMetadata::default());
    let response_str = ChatMlTranslator::format_tool_response(&result);
    assert!(response_str.contains("<tool_response>"));
    assert!(response_str.contains("</tool_response>"));
    assert!(response_str.contains("\"content\": \"Output text\""));
}

// ============================================================================
// 5. Property-Based Fuzz Tests (Proptest)
// ============================================================================

proptest! {
    #[test]
    fn fuzz_openai_arguments_roundtrip(
        name in "[a-zA-Z0-9_-]{1,32}",
        random_key in "[a-zA-Z0-9]{1,16}",
        random_str in "\\PC*",
        random_int in any::<i64>(),
        random_bool in any::<bool>()
    ) {
        let original_args = json!({
            random_key: random_str,
            "count": random_int,
            "flag": random_bool,
        });

        let call = CanonicalToolCall {
            call_id: "fuzz_call".to_string(),
            tool_name: name,
            arguments: original_args.clone(),
        };

        let encoded = OpenAiTranslator::encode_tool_call(&call).expect("encoding should never fail");
        let decoded = OpenAiTranslator::parse_tool_call(&encoded).expect("decoding should succeed");

        prop_assert_eq!(decoded.arguments, original_args);
    }
}
