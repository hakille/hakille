//! Adversarial Stress and Fuzzing Test Suite for Bidirectional Translators.
//!
//! Specifically targets:
//! - crates/hakille-capability/src/translators/openai.rs
//! - crates/hakille-capability/src/translators/anthropic.rs
//! - crates/hakille-capability/src/translators/gemini.rs
//! - crates/hakille-capability/src/translators/chatml.rs
//! - crates/hakille-capability/src/translators/mod.rs

use hakille_capability::translators::anthropic::AnthropicTranslator;
use hakille_capability::translators::chatml::ChatMlTranslator;
use hakille_capability::translators::gemini::GeminiTranslator;
use hakille_capability::translators::openai::{
    OpenAiFunctionCallPayload, OpenAiToolCall, OpenAiTranslator, enforce_openai_strict_schema,
};
use hakille_capability::translators::{ProviderTranslator, TranslatorError};
use hakille_capability::{
    CanonicalContentBlock, CanonicalToolCall, CanonicalToolResult, ExecutionMetadata,
};
use proptest::prelude::*;
use serde_json::{Value, json};

// ============================================================================
// 1. Heavily Nested Schemas & Recursion Limits
// ============================================================================

fn generate_deeply_nested_schema(depth: usize) -> Value {
    let mut current = json!({
        "type": "string",
        "description": "leaf property"
    });

    for i in (0..depth).rev() {
        current = json!({
            "type": "object",
            "properties": {
                format!("level_{i}"): current
            },
            "required": [format!("level_{i}")]
        });
    }
    current
}

#[test]
fn test_adversarial_deeply_nested_schema_openai_strict() {
    // Test 100 levels of nested objects
    let mut schema = generate_deeply_nested_schema(100);
    let res = enforce_openai_strict_schema(&mut schema, "deep_tool");
    assert!(
        res.is_ok(),
        "100-level schema normalization must succeed without panic"
    );
    assert_eq!(schema["additionalProperties"], json!(false));

    // Verify first 10 levels have additionalProperties: false
    let mut curr = &schema;
    for i in 0..10 {
        curr = &curr["properties"][format!("level_{i}")];
        assert_eq!(curr["additionalProperties"], json!(false));
    }
}

#[test]
fn test_adversarial_deeply_nested_schema_gemini() {
    let mut schema = generate_deeply_nested_schema(100);
    GeminiTranslator::normalize_types_to_uppercase(&mut schema);
    assert_eq!(schema["type"], "OBJECT");

    let mut curr = &schema;
    for i in 0..10 {
        curr = &curr["properties"][format!("level_{i}")];
        assert_eq!(curr["type"], "OBJECT");
    }

    GeminiTranslator::normalize_types_to_lowercase(&mut schema);
    assert_eq!(schema["type"], "object");
}

#[test]
fn test_adversarial_deeply_nested_arrays() {
    let mut current = json!({ "type": "string" });
    for _ in 0..50 {
        current = json!({
            "type": "array",
            "items": current
        });
    }

    let mut root = json!({
        "type": "object",
        "properties": {
            "matrix": current
        }
    });

    let res = enforce_openai_strict_schema(&mut root, "matrix_tool");
    assert!(
        res.is_ok(),
        "Deep array schema should normalize without panic"
    );
}

#[test]
fn test_adversarial_schema_recursive_defs_handling() {
    let mut schema = json!({
        "type": "object",
        "$defs": {
            "tree_node": {
                "type": "object",
                "properties": {
                    "value": { "type": "string" },
                    "left": { "$ref": "#/$defs/tree_node" },
                    "right": { "$ref": "#/$defs/tree_node" }
                }
            }
        },
        "properties": {
            "root": { "$ref": "#/$defs/tree_node" }
        }
    });

    let res = enforce_openai_strict_schema(&mut schema, "binary_tree");
    assert!(
        res.is_ok(),
        "Schemas with recursive $ref must not loop infinitely"
    );
    assert_eq!(
        schema["$defs"]["tree_node"]["additionalProperties"],
        json!(false)
    );
    assert!(
        schema["$defs"]["tree_node"]["required"]
            .as_array()
            .is_some()
    );
}

// ============================================================================
// 2. OpenAI Malformed JSON & Adversarial Payloads
// ============================================================================

#[test]
fn test_adversarial_openai_malformed_json_variants() {
    let malformed_cases = [
        ("{ missing_quotes: 123 }", "unquoted key"),
        ("{\"key\": undefined}", "undefined token"),
        ("{\"trailing\": 123,}", "trailing comma"),
        ("{\"incomplete\": ", "truncated JSON"),
        ("{{{{}}}}", "invalid braces"),
        ("\0\0\0", "null bytes"),
        ("{\"a\": 1} extra text", "trailing garbage"),
        ("{\"unclosed_string\": \"hello", "unclosed string"),
        ("NaN", "NaN token"),
        ("Infinity", "Infinity token"),
    ];

    for (raw_json, label) in malformed_cases {
        let call = OpenAiToolCall {
            id: "call_malformed".to_string(),
            call_type: "function".to_string(),
            function: OpenAiFunctionCallPayload {
                name: "test_tool".to_string(),
                arguments: raw_json.to_string(),
            },
        };

        let res = OpenAiTranslator::parse_tool_call(&call);
        assert!(
            matches!(res, Err(TranslatorError::MalformedArguments { ref tool_name, .. }) if tool_name == "test_tool"),
            "Payload '{label}' ({raw_json}) should return TranslatorError::MalformedArguments without panicking, got: {:?}",
            res
        );
    }
}

#[test]
fn test_adversarial_openai_empty_and_whitespace_arguments() {
    let empty_cases = ["", "   ", "\t\t", "\n\r\n  ", "   \t \n "];
    for empty_str in empty_cases {
        let call = OpenAiToolCall {
            id: "call_empty".to_string(),
            call_type: "function".to_string(),
            function: OpenAiFunctionCallPayload {
                name: "test_tool".to_string(),
                arguments: empty_str.to_string(),
            },
        };

        let res = OpenAiTranslator::parse_tool_call(&call);
        assert!(
            res.is_ok(),
            "Empty/whitespace stringified arguments must parse to empty object"
        );
        assert_eq!(res.unwrap().arguments, json!({}));
    }
}

#[test]
fn test_adversarial_openai_non_object_valid_json() {
    // Valid JSON strings that are NOT JSON objects
    let non_objects = [
        ("12345", json!(12345)),
        ("true", json!(true)),
        ("false", json!(false)),
        ("null", json!(null)),
        ("\"just a string\"", json!("just a string")),
        ("[1, 2, 3]", json!([1, 2, 3])),
    ];

    for (raw, expected) in non_objects {
        let call = OpenAiToolCall {
            id: "call_non_obj".to_string(),
            call_type: "function".to_string(),
            function: OpenAiFunctionCallPayload {
                name: "test_tool".to_string(),
                arguments: raw.to_string(),
            },
        };

        let res = OpenAiTranslator::parse_tool_call(&call);
        assert!(
            res.is_ok(),
            "Valid JSON literal '{raw}' parses into arguments value without panic"
        );
        assert_eq!(res.unwrap().arguments, expected);
    }
}

#[test]
fn test_adversarial_openai_1mb_arguments() {
    let large_str = "x".repeat(1024 * 1024); // 1 MB
    let args = json!({ "content": large_str });
    let stringified = serde_json::to_string(&args).unwrap();

    let call = OpenAiToolCall {
        id: "call_1mb".to_string(),
        call_type: "function".to_string(),
        function: OpenAiFunctionCallPayload {
            name: "fs_write".to_string(),
            arguments: stringified,
        },
    };

    let res = OpenAiTranslator::parse_tool_call(&call);
    assert!(
        res.is_ok(),
        "1MB stringified JSON arguments must be parsed without panic"
    );
    assert_eq!(
        res.unwrap().arguments["content"].as_str().unwrap().len(),
        1024 * 1024
    );
}

// ============================================================================
// 3. ChatML Code Fences, Multiple Languages & Malformed Delimiters
// ============================================================================

#[test]
fn test_adversarial_chatml_code_fences_variations() {
    // Standard ```json block
    let standard =
        "<tool_call>\n```json\n{\"name\": \"run\", \"arguments\": {\"x\": 1}}\n```\n</tool_call>";
    let calls = ChatMlTranslator::parse_generation_stream(standard).unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].tool_name, "run");

    // Bare ``` block (no language tag)
    let bare =
        "<tool_call>\n```\n{\"name\": \"run\", \"arguments\": {\"x\": 2}}\n```\n</tool_call>";
    let calls_bare = ChatMlTranslator::parse_generation_stream(bare).unwrap();
    assert_eq!(calls_bare.len(), 1);
    assert_eq!(calls_bare[0].tool_name, "run");
    assert_eq!(calls_bare[0].arguments["x"], 2);

    // Unfenced raw JSON
    let raw = "<tool_call>\n{\"name\": \"run\", \"arguments\": {\"x\": 3}}\n</tool_call>";
    let calls_raw = ChatMlTranslator::parse_generation_stream(raw).unwrap();
    assert_eq!(calls_raw.len(), 1);
    assert_eq!(calls_raw[0].tool_name, "run");
    assert_eq!(calls_raw[0].arguments["x"], 3);
}

#[test]
fn test_adversarial_chatml_uppercase_json_fence_defect() {
    let text =
        "<tool_call>\n```JSON\n{\"name\": \"run\", \"arguments\": {\"x\": 1}}\n```\n</tool_call>";
    let res = ChatMlTranslator::parse_generation_stream(text);
    assert!(
        res.is_ok(),
        "Fixed: uppercase ```JSON fence is cleaned and parsed successfully"
    );
    let calls = res.unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].tool_name, "run");
    assert_eq!(calls[0].arguments["x"], 1);
}

#[test]
fn test_adversarial_chatml_other_language_fence_defect() {
    let text_py =
        "<tool_call>\n```python\n{\"name\": \"run\", \"arguments\": {}}\n```\n</tool_call>";
    let res = ChatMlTranslator::parse_generation_stream(text_py);
    assert!(
        res.is_ok(),
        "Fixed: ```python code fence is stripped cleanly without syntax error"
    );
    let calls = res.unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].tool_name, "run");
}

#[test]
fn test_adversarial_chatml_embedded_code_fences_in_args() {
    // Code snippet inside arguments containing nested code fences
    let payload = r#"
<tool_call>
```json
{
  "name": "write_script",
  "arguments": {
    "code": "```python\nprint('hello')\n```",
    "lang": "python"
  }
}
```
</tool_call>
"#;
    let calls = ChatMlTranslator::parse_generation_stream(payload).unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].tool_name, "write_script");
    assert_eq!(calls[0].arguments["code"], "```python\nprint('hello')\n```");
}

#[test]
fn test_adversarial_chatml_stream_with_unclosed_tags() {
    // Unclosed <tool_call>
    let unclosed = "Here is a call:\n<tool_call>\n{\"name\": \"incomplete\"";
    let calls = ChatMlTranslator::parse_generation_stream(unclosed).unwrap();
    assert!(
        calls.is_empty(),
        "Unclosed <tool_call> should safely return empty calls without panic"
    );
}

#[test]
fn test_adversarial_chatml_stream_error_halts_multi_calls() {
    // Stream with 2 calls: first is invalid JSON, second is valid
    let mixed = r#"
<tool_call>
{ invalid json }
</tool_call>
<tool_call>
{"name": "valid", "arguments": {}}
</tool_call>
"#;
    let res = ChatMlTranslator::parse_generation_stream(mixed);
    assert!(
        res.is_err(),
        "First malformed call halts stream parsing and returns DelimiterError"
    );
}

// ============================================================================
// 4. Multimodal Content Blocks: Corrupted Base64 & Missing URIs
// ============================================================================

#[test]
fn test_adversarial_corrupted_base64_multimodal_blocks() {
    let corrupted_payloads = [
        "not_valid_base64!!!",
        "===",
        "abc", // invalid length not multiple of 4
        "ab\0cd",
        "a b c d",
        "!!!@@@###$$$%%%",
    ];

    for b64 in corrupted_payloads {
        let result = CanonicalToolResult::success(
            "call_b64",
            vec![CanonicalContentBlock::image("image/jpeg", b64)],
            ExecutionMetadata::default(),
        );

        // 1. Anthropic: passes through data_base64
        let anthropic_res = AnthropicTranslator::encode_tool_result(&result);
        assert!(
            anthropic_res.is_ok(),
            "Anthropic encoding should not panic on corrupted base64"
        );
        let decoded_anthropic =
            AnthropicTranslator::parse_tool_result(&anthropic_res.unwrap()).unwrap();
        assert_eq!(decoded_anthropic.content.len(), 1);
        if let CanonicalContentBlock::Image { data_base64, .. } = &decoded_anthropic.content[0] {
            assert_eq!(data_base64, b64);
        } else {
            panic!("Expected image block");
        }

        // 2. OpenAI: formats as text representation without panic
        let openai_res = OpenAiTranslator::encode_tool_result(&result);
        assert!(
            openai_res.is_ok(),
            "OpenAI encoding should not panic on corrupted base64"
        );

        // 3. Gemini: formats in response JSON without panic
        let gemini_res = GeminiTranslator::encode_tool_result(&result);
        assert!(
            gemini_res.is_ok(),
            "Gemini encoding should not panic on corrupted base64"
        );

        // 4. ChatML: formats as text representation without panic
        let chatml_res = ChatMlTranslator::encode_tool_result(&result);
        assert!(
            chatml_res.is_ok(),
            "ChatML encoding should not panic on corrupted base64"
        );
    }
}

#[test]
fn test_adversarial_resource_empty_uri_and_payload_drop_defect() {
    let resource_empty = CanonicalContentBlock::Resource {
        uri: "".to_string(),
        mime_type: None,
        text: None,
        blob_base64: None,
    };

    let result = CanonicalToolResult::success(
        "call_res",
        vec![resource_empty],
        ExecutionMetadata::default(),
    );

    let anthropic_res = AnthropicTranslator::encode_tool_result(&result).unwrap();
    let decoded_anthropic = AnthropicTranslator::parse_tool_result(&anthropic_res).unwrap();
    assert_eq!(
        decoded_anthropic.content.len(),
        1,
        "Fixed: Anthropic preserves Resource blocks when text and blob_base64 are both None"
    );
    assert_eq!(decoded_anthropic.text_summary(), "[Resource ()]");

    // OpenAI preserves resource representation even with no text
    let openai_res = OpenAiTranslator::encode_tool_result(&result).unwrap();
    assert!(openai_res.content.contains("[Resource ()]"));

    // Gemini drops resource representation if text is None
    let gemini_res = GeminiTranslator::encode_tool_result(&result).unwrap();
    let decoded_gemini = GeminiTranslator::parse_tool_result(&gemini_res).unwrap();
    assert_eq!(decoded_gemini.text_summary(), "");
}

#[test]
fn test_adversarial_empty_content_result_roundtrip() {
    let empty_res =
        CanonicalToolResult::success("call_empty_content", vec![], ExecutionMetadata::default());

    // OpenAI fallback
    let openai_res = OpenAiTranslator::encode_tool_result(&empty_res).unwrap();
    assert_eq!(openai_res.content, "{\"status\": \"success\"}");
    let decoded_openai = OpenAiTranslator::parse_tool_result(&openai_res).unwrap();
    assert_eq!(decoded_openai.text_summary(), "{\"status\": \"success\"}");

    // Anthropic
    let anthropic_res = AnthropicTranslator::encode_tool_result(&empty_res).unwrap();
    let decoded_anthropic = AnthropicTranslator::parse_tool_result(&anthropic_res).unwrap();
    assert_eq!(decoded_anthropic.content.len(), 0);

    // Gemini
    let gemini_res = GeminiTranslator::encode_tool_result(&empty_res).unwrap();
    let decoded_gemini = GeminiTranslator::parse_tool_result(&gemini_res).unwrap();
    assert_eq!(decoded_gemini.text_summary(), "");
}

// ============================================================================
// 5. Unicode, Control Chars & Edge Strings
// ============================================================================

#[test]
fn test_adversarial_unicode_and_special_characters_handling() {
    let edge_strings = [
        "tool_with_\0_null_byte",
        "tool_with_\u{202E}_rtl_override",
        "tool_with_emoji_🚀_fire_🔥",
        "tool_with_cjk_测试_工具",
        "tool_with_combining_a\u{0300}\u{0301}\u{0302}",
        "tool_with_whitespace_ \t \n ",
    ];

    for s in edge_strings {
        // Anthropic tool name sanitizer must produce valid [a-zA-Z0-9_-]{1,64}
        let sanitized = AnthropicTranslator::sanitize_tool_name(s);
        assert!(!sanitized.is_empty());
        assert!(sanitized.len() <= 64);
        assert!(
            sanitized
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        );

        // Tool call roundtrip with unicode values in arguments
        let call = CanonicalToolCall::new("call_unicode", "unicode_tool", json!({ "payload": s }));
        let openai_call = OpenAiTranslator::encode_tool_call(&call).unwrap();
        let decoded = OpenAiTranslator::parse_tool_call(&openai_call).unwrap();
        assert_eq!(decoded.arguments["payload"], s);
    }
}

// ============================================================================
// 6. Roundtrip Fidelity & Defect Quantification
// ============================================================================

#[test]
fn test_adversarial_defect_openai_error_prefix_collision() {
    let output = "[ERROR] Database cluster failover completed successfully.";
    let success = CanonicalToolResult::success(
        "call_db_check",
        vec![CanonicalContentBlock::text(output)],
        ExecutionMetadata::default(),
    );

    let encoded = OpenAiTranslator::encode_tool_result(&success).unwrap();
    let decoded = OpenAiTranslator::parse_tool_result(&encoded).unwrap();

    // Verify fixed behavior: is_error is preserved as false and prefix is not stripped
    assert!(
        !decoded.is_error,
        "Fixed: OpenAiTranslator preserves is_error as false when output starts with '[ERROR]'"
    );
    assert_eq!(
        decoded.text_summary(),
        output,
        "Fixed: OpenAiTranslator does not strip '[ERROR]' from legitimate output"
    );
}

#[test]
fn test_adversarial_defect_gemini_call_id_expansion() {
    let original_id = "invoke_1001";
    let res = CanonicalToolResult::success_text(original_id, "done", ExecutionMetadata::default());

    let encoded_1 = GeminiTranslator::encode_tool_result(&res).unwrap();
    let decoded_1 = GeminiTranslator::parse_tool_result(&encoded_1).unwrap();
    assert_eq!(
        decoded_1.call_id, original_id,
        "Fixed: Gemini roundtrip preserves call_id symmetrically"
    );

    let encoded_2 = GeminiTranslator::encode_tool_result(&decoded_1).unwrap();
    let decoded_2 = GeminiTranslator::parse_tool_result(&encoded_2).unwrap();
    assert_eq!(
        decoded_2.call_id, original_id,
        "Fixed: Consecutive Gemini roundtrips do not accumulate 'call_' prefixes"
    );
}

#[test]
fn test_adversarial_defect_chatml_call_id_expansion() {
    let original_id = "chat_call_007";
    let res = CanonicalToolResult::success_text(original_id, "ok", ExecutionMetadata::default());

    let encoded_1 = ChatMlTranslator::encode_tool_result(&res).unwrap();
    let decoded_1 = ChatMlTranslator::parse_tool_result(&encoded_1).unwrap();
    assert_eq!(
        decoded_1.call_id, original_id,
        "Fixed: ChatML roundtrip preserves call_id symmetrically"
    );
}

#[test]
fn test_adversarial_defect_gemini_unsupported_nested_types() {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "val": {
                "anyOf": [
                    { "type": "string" },
                    { "type": "integer" }
                ]
            }
        }
    });

    GeminiTranslator::normalize_types_to_uppercase(&mut schema);
    let anyof_0_type = schema["properties"]["val"]["anyOf"][0]["type"]
        .as_str()
        .unwrap();
    assert_eq!(
        anyof_0_type, "STRING",
        "Fixed: Gemini type normalization converts types inside anyOf to uppercase"
    );
}

// ============================================================================
// 7. Property-Based Fuzzing & Stress Invariants
// ============================================================================

proptest! {
    #[test]
    fn prop_anthropic_sanitize_tool_name_invariants(raw_name in "\\PC*") {
        let sanitized = AnthropicTranslator::sanitize_tool_name(&raw_name);
        // Invariant 1: never empty
        prop_assert!(!sanitized.is_empty(), "Sanitized name must never be empty");
        // Invariant 2: length <= 64
        prop_assert!(sanitized.len() <= 64, "Sanitized name must not exceed 64 chars");
        // Invariant 3: valid characters [a-zA-Z0-9_-]
        prop_assert!(
            sanitized.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
            "Sanitized name must strictly contain [a-zA-Z0-9_-], got: {}",
            sanitized
        );
    }

    #[test]
    fn prop_gemini_schema_normalization_idempotency(
        key in "[a-z_]{1,16}",
        val_type in "string|integer|number|boolean|array|object"
    ) {
        let mut schema = json!({
            "type": "object",
            "properties": {
                key: { "type": val_type }
            }
        });

        // First pass: uppercase
        GeminiTranslator::normalize_types_to_uppercase(&mut schema);
        prop_assert_eq!(&schema["type"], "OBJECT");

        // Second pass: idempotent
        let schema_clone = schema.clone();
        GeminiTranslator::normalize_types_to_uppercase(&mut schema);
        prop_assert_eq!(&schema, &schema_clone);

        // Third pass: lowercase recovery
        GeminiTranslator::normalize_types_to_lowercase(&mut schema);
        prop_assert_eq!(&schema["type"], "object");
    }

    #[test]
    fn prop_chatml_stream_parser_never_panics(stream_fuzz in "\\PC*") {
        // Stream parser must never panic on arbitrary unicode strings
        let _ = ChatMlTranslator::parse_generation_stream(&stream_fuzz);
    }
}

// ============================================================================
// 8. Multi-Block Ordering and Heterogeneous Multimodal Tests
// ============================================================================

#[test]
fn test_adversarial_anthropic_heterogeneous_multimodal_ordering() {
    let blocks = vec![
        CanonicalContentBlock::text("Step 1: initiated"),
        CanonicalContentBlock::image("image/png", "iVBORw0KGgoAAAANSUhEUg=="),
        CanonicalContentBlock::text("Step 2: in progress"),
        CanonicalContentBlock::resource_blob(
            "file:///data.bin",
            Some("application/pdf".to_string()),
            "JVBERi0xLjcK",
        ),
        CanonicalContentBlock::text("Step 3: completed"),
    ];

    let result =
        CanonicalToolResult::success("call_multi_block", blocks, ExecutionMetadata::default());
    let anthropic_res = AnthropicTranslator::encode_tool_result(&result).unwrap();
    let decoded = AnthropicTranslator::parse_tool_result(&anthropic_res).unwrap();

    // 5 input blocks: 3 text, 1 image, 1 resource (encoded as image)
    assert_eq!(decoded.content.len(), 5);
    assert!(decoded.content[0].is_text());
    assert!(decoded.content[1].is_image());
    assert!(decoded.content[2].is_text());
    assert!(decoded.content[3].is_image()); // resource converted to image
    assert!(decoded.content[4].is_text());
}

#[test]
fn test_gemini_nullable_union_type_normalization() {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "name": {
                "type": ["string", "null"]
            },
            "alt": {
                "oneOf": [
                    { "type": ["integer", "null"] }
                ]
            }
        },
        "$defs": {
            "NestedDef": {
                "type": "object",
                "properties": {
                    "count": { "type": ["number", "null"] }
                }
            }
        }
    });

    GeminiTranslator::normalize_types_to_uppercase(&mut schema);

    assert_eq!(schema["properties"]["name"]["type"], "STRING");
    assert_eq!(schema["properties"]["name"]["nullable"], true);

    assert_eq!(schema["properties"]["alt"]["oneOf"][0]["type"], "INTEGER");
    assert_eq!(schema["properties"]["alt"]["oneOf"][0]["nullable"], true);

    assert_eq!(
        schema["$defs"]["NestedDef"]["properties"]["count"]["type"],
        "NUMBER"
    );
    assert_eq!(
        schema["$defs"]["NestedDef"]["properties"]["count"]["nullable"],
        true
    );

    // Also verify denormalization to lowercase
    GeminiTranslator::normalize_types_to_lowercase(&mut schema);
    assert_eq!(schema["properties"]["name"]["type"], "string");
    assert_eq!(schema["properties"]["alt"]["oneOf"][0]["type"], "integer");
    assert_eq!(
        schema["$defs"]["NestedDef"]["properties"]["count"]["type"],
        "number"
    );
}

#[test]
fn test_openai_strict_schema_recursion_combinators() {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "query": {
                "anyOf": [
                    {
                        "type": "object",
                        "properties": {
                            "text": { "type": "string" }
                        }
                    },
                    {
                        "type": "object",
                        "properties": {
                            "id": { "type": "integer" }
                        }
                    }
                ]
            }
        }
    });

    enforce_openai_strict_schema(&mut schema, "search_tool").unwrap();

    assert_eq!(schema["additionalProperties"], false);
    let any_of = &schema["properties"]["query"]["anyOf"];
    assert_eq!(any_of[0]["additionalProperties"], false);
    assert_eq!(any_of[0]["required"], json!(["text"]));
    assert_eq!(any_of[1]["additionalProperties"], false);
    assert_eq!(any_of[1]["required"], json!(["id"]));
}

#[test]
fn test_chatml_fence_edge_cases() {
    // Single-line ```JSON fence
    let single_line =
        "<tool_call>```JSON{\"name\": \"exec\", \"arguments\": {\"code\": 42}}```</tool_call>";
    let parsed = ChatMlTranslator::parse_generation_stream(single_line).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].tool_name, "exec");
    assert_eq!(parsed[0].arguments["code"], 42);

    // Multiline ```BASH fence
    let bash_fence = "<tool_call>\n```BASH\n{\"name\": \"exec\", \"arguments\": {\"cmd\": \"ls\"}}\n```\n</tool_call>";
    let parsed_bash = ChatMlTranslator::parse_generation_stream(bash_fence).unwrap();
    assert_eq!(parsed_bash.len(), 1);
    assert_eq!(parsed_bash[0].tool_name, "exec");
    assert_eq!(parsed_bash[0].arguments["cmd"], "ls");
}

#[test]
fn test_anthropic_resource_empty_payload_custom_uri() {
    let res_block = CanonicalContentBlock::Resource {
        uri: "s3://corp-data/manifest.json".to_string(),
        mime_type: Some("application/json".to_string()),
        text: None,
        blob_base64: None,
    };
    let res =
        CanonicalToolResult::success("call_s3", vec![res_block], ExecutionMetadata::default());
    let encoded = AnthropicTranslator::encode_tool_result(&res).unwrap();
    let decoded = AnthropicTranslator::parse_tool_result(&encoded).unwrap();

    assert_eq!(decoded.content.len(), 1);
    assert_eq!(
        decoded.text_summary(),
        "[Resource (s3://corp-data/manifest.json)]"
    );
}

#[test]
fn test_challenger1_it2_exhaustive_verification_of_five_defects() {
    // ------------------------------------------------------------------------
    // Defect 1: OpenAI Tool Results Error Disambiguation
    // ------------------------------------------------------------------------
    let test_cases_openai_success = [
        "[ERROR] DB failover ok",
        "[ERROR]   double space prefix",
        "[ERROR]\tTab separated log",
        "[ERROR] 404 Not Found handled gracefully",
        "Normal output without error prefix",
    ];

    for text_val in test_cases_openai_success {
        let success = CanonicalToolResult::success(
            "call_oai_test",
            vec![CanonicalContentBlock::text(text_val)],
            ExecutionMetadata::default(),
        );
        let encoded = OpenAiTranslator::encode_tool_result(&success).unwrap();
        let decoded = OpenAiTranslator::parse_tool_result(&encoded).unwrap();

        assert!(
            !decoded.is_error,
            "OpenAI tool result with text '{text_val}' must NOT be inverted to is_error=true"
        );
        assert_eq!(
            decoded.text_summary(),
            text_val,
            "OpenAI tool result with text '{text_val}' must NOT have its prefix stripped"
        );
    }

    // Verify true errors are properly preserved as is_error = true
    let error_result = CanonicalToolResult::error(
        "call_oai_err",
        "[ERROR] Real critical failure",
        ExecutionMetadata::default(),
    );
    let enc_err = OpenAiTranslator::encode_tool_result(&error_result).unwrap();
    let dec_err = OpenAiTranslator::parse_tool_result(&enc_err).unwrap();
    assert!(dec_err.is_error, "True error must retain is_error=true");
    assert_eq!(
        dec_err.text_summary(),
        "[ERROR] Real critical failure",
        "True error content must be preserved"
    );

    // ------------------------------------------------------------------------
    // Defect 2: Gemini & ChatML call_id Roundtripping
    // ------------------------------------------------------------------------
    let test_call_ids = [
        "call_12345",
        "my_custom_id",
        "call_call_accumulated_test",
        "uuid-v7-1234",
    ];
    for &initial_id in &test_call_ids {
        // Gemini 10 consecutive roundtrips
        let mut gemini_curr =
            CanonicalToolResult::success_text(initial_id, "data", ExecutionMetadata::default());
        for round in 1..=10 {
            let enc = GeminiTranslator::encode_tool_result(&gemini_curr).unwrap();
            let dec = GeminiTranslator::parse_tool_result(&enc).unwrap();
            assert_eq!(
                dec.call_id, initial_id,
                "Gemini call_id must remain '{initial_id}' on round {round}, got '{}'",
                dec.call_id
            );
            gemini_curr = dec;
        }

        // ChatML 10 consecutive roundtrips
        let mut chatml_curr =
            CanonicalToolResult::success_text(initial_id, "data", ExecutionMetadata::default());
        for round in 1..=10 {
            let enc = ChatMlTranslator::encode_tool_result(&chatml_curr).unwrap();
            let dec = ChatMlTranslator::parse_tool_result(&enc).unwrap();
            assert_eq!(
                dec.call_id, initial_id,
                "ChatML call_id must remain '{initial_id}' on round {round}, got '{}'",
                dec.call_id
            );
            chatml_curr = dec;
        }
    }

    // ------------------------------------------------------------------------
    // Defect 3: ChatML Case-Insensitive Codeblock Fences
    // ------------------------------------------------------------------------
    let fence_variants = [
        "<tool_call>\n```JSON\n{\"name\": \"test_tool\", \"arguments\": {\"k\": 1}}\n```\n</tool_call>",
        "<tool_call>\n```Json\n{\"name\": \"test_tool\", \"arguments\": {\"k\": 2}}\n```\n</tool_call>",
        "<tool_call>\n```PYTHON\n{\"name\": \"test_tool\", \"arguments\": {\"k\": 3}}\n```\n</tool_call>",
        "<tool_call>\n```bash\n{\"name\": \"test_tool\", \"arguments\": {\"k\": 4}}\n```\n</tool_call>",
        "<tool_call>\n```sh\n{\"name\": \"test_tool\", \"arguments\": {\"k\": 5}}\n```\n</tool_call>",
        "<tool_call>```JSON{\"name\": \"test_tool\", \"arguments\": {\"k\": 6}}```</tool_call>",
        "<tool_call>  ```json\n{\"name\": \"test_tool\", \"arguments\": {\"k\": 7}}\n```  </tool_call>",
    ];

    for (idx, stream_text) in fence_variants.iter().enumerate() {
        let parsed = ChatMlTranslator::parse_generation_stream(stream_text)
            .unwrap_or_else(|e| panic!("Failed to parse fence variant {idx}: {e:?}"));
        assert_eq!(
            parsed.len(),
            1,
            "Variant {idx} must yield exactly 1 tool call"
        );
        assert_eq!(parsed[0].tool_name, "test_tool");
        assert_eq!(parsed[0].arguments["k"], (idx as i64) + 1);
    }

    // ------------------------------------------------------------------------
    // Defect 4: Anthropic Resource Block Preservation
    // ------------------------------------------------------------------------
    let resource_no_payload = CanonicalContentBlock::Resource {
        uri: "https://example.com/schema.json".to_string(),
        mime_type: Some("application/json".to_string()),
        text: None,
        blob_base64: None,
    };
    let ant_result = CanonicalToolResult::success(
        "call_ant_res",
        vec![resource_no_payload],
        ExecutionMetadata::default(),
    );
    let ant_enc = AnthropicTranslator::encode_tool_result(&ant_result).unwrap();
    let ant_dec = AnthropicTranslator::parse_tool_result(&ant_enc).unwrap();

    assert_eq!(ant_dec.content.len(), 1, "Resource block must be preserved");
    assert_eq!(
        ant_dec.text_summary(),
        "[Resource (https://example.com/schema.json)]",
        "Resource URI must be preserved in text block"
    );

    // Multi-block with text and empty-payload resource
    let multi_blocks = vec![
        CanonicalContentBlock::text("Header info"),
        CanonicalContentBlock::Resource {
            uri: "file:///config.yaml".to_string(),
            mime_type: None,
            text: None,
            blob_base64: None,
        },
    ];
    let ant_multi =
        CanonicalToolResult::success("call_ant_multi", multi_blocks, ExecutionMetadata::default());
    let ant_multi_enc = AnthropicTranslator::encode_tool_result(&ant_multi).unwrap();
    let ant_multi_dec = AnthropicTranslator::parse_tool_result(&ant_multi_enc).unwrap();
    assert_eq!(ant_multi_dec.content.len(), 2);
    assert_eq!(ant_multi_dec.content[0].as_text().unwrap(), "Header info");
    assert_eq!(
        ant_multi_dec.content[1].as_text().unwrap(),
        "[Resource (file:///config.yaml)]"
    );

    // ------------------------------------------------------------------------
    // Defect 5: Gemini Uppercase Normalization Recurses Combinators and $defs
    // ------------------------------------------------------------------------
    let mut schema = json!({
        "type": "object",
        "properties": {
            "all_of_prop": {
                "allOf": [
                    { "type": "string" },
                    { "type": ["integer", "null"] }
                ]
            },
            "any_of_prop": {
                "anyOf": [
                    { "type": "boolean" },
                    { "type": ["number", "null"] }
                ]
            },
            "one_of_prop": {
                "oneOf": [
                    { "type": "string" }
                ]
            }
        },
        "$defs": {
            "CustomType": {
                "type": "object",
                "properties": {
                    "nested_str": { "type": ["string", "null"] },
                    "nested_arr": {
                        "type": "array",
                        "items": { "type": "integer" }
                    }
                }
            }
        },
        "definitions": {
            "LegacyDef": {
                "type": "object",
                "properties": {
                    "flag": { "type": "boolean" }
                }
            }
        }
    });

    // Uppercase pass
    GeminiTranslator::normalize_types_to_uppercase(&mut schema);

    assert_eq!(schema["type"], "OBJECT");
    // allOf
    assert_eq!(
        schema["properties"]["all_of_prop"]["allOf"][0]["type"],
        "STRING"
    );
    assert_eq!(
        schema["properties"]["all_of_prop"]["allOf"][1]["type"],
        "INTEGER"
    );
    assert_eq!(
        schema["properties"]["all_of_prop"]["allOf"][1]["nullable"],
        true
    );
    // anyOf
    assert_eq!(
        schema["properties"]["any_of_prop"]["anyOf"][0]["type"],
        "BOOLEAN"
    );
    assert_eq!(
        schema["properties"]["any_of_prop"]["anyOf"][1]["type"],
        "NUMBER"
    );
    assert_eq!(
        schema["properties"]["any_of_prop"]["anyOf"][1]["nullable"],
        true
    );
    // oneOf
    assert_eq!(
        schema["properties"]["one_of_prop"]["oneOf"][0]["type"],
        "STRING"
    );
    // $defs
    assert_eq!(schema["$defs"]["CustomType"]["type"], "OBJECT");
    assert_eq!(
        schema["$defs"]["CustomType"]["properties"]["nested_str"]["type"],
        "STRING"
    );
    assert_eq!(
        schema["$defs"]["CustomType"]["properties"]["nested_str"]["nullable"],
        true
    );
    assert_eq!(
        schema["$defs"]["CustomType"]["properties"]["nested_arr"]["type"],
        "ARRAY"
    );
    assert_eq!(
        schema["$defs"]["CustomType"]["properties"]["nested_arr"]["items"]["type"],
        "INTEGER"
    );
    // definitions
    assert_eq!(schema["definitions"]["LegacyDef"]["type"], "OBJECT");
    assert_eq!(
        schema["definitions"]["LegacyDef"]["properties"]["flag"]["type"],
        "BOOLEAN"
    );

    // Lowercase pass
    GeminiTranslator::normalize_types_to_lowercase(&mut schema);
    assert_eq!(schema["type"], "object");
    assert_eq!(
        schema["properties"]["all_of_prop"]["allOf"][0]["type"],
        "string"
    );
    assert_eq!(
        schema["properties"]["all_of_prop"]["allOf"][1]["type"],
        "integer"
    );
    assert_eq!(
        schema["properties"]["any_of_prop"]["anyOf"][0]["type"],
        "boolean"
    );
    assert_eq!(
        schema["properties"]["any_of_prop"]["anyOf"][1]["type"],
        "number"
    );
    assert_eq!(
        schema["properties"]["one_of_prop"]["oneOf"][0]["type"],
        "string"
    );
    assert_eq!(schema["$defs"]["CustomType"]["type"], "object");
    assert_eq!(
        schema["$defs"]["CustomType"]["properties"]["nested_str"]["type"],
        "string"
    );
    assert_eq!(
        schema["$defs"]["CustomType"]["properties"]["nested_arr"]["type"],
        "array"
    );
    assert_eq!(
        schema["$defs"]["CustomType"]["properties"]["nested_arr"]["items"]["type"],
        "integer"
    );
    assert_eq!(schema["definitions"]["LegacyDef"]["type"], "object");
    assert_eq!(
        schema["definitions"]["LegacyDef"]["properties"]["flag"]["type"],
        "boolean"
    );
}
