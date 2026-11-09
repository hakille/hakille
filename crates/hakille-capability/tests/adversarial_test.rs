use hakille_capability::translators::ProviderTranslator;
use hakille_capability::translators::anthropic::AnthropicTranslator;
use hakille_capability::translators::chatml::ChatMlTranslator;
use hakille_capability::translators::gemini::GeminiTranslator;
use hakille_capability::translators::openai::OpenAiTranslator;
use hakille_capability::{
    CanonicalContentBlock, CanonicalToolCall, CanonicalToolDefinition, CanonicalToolResult,
    CapabilityManifest, CapabilityScope, CapabilityType, ExecutionMetadata, RiskTier,
    ValidationError, validate_capability_id, validate_parameters_schema, validate_tool_name,
};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

// ============================================================================
// 1. Tool Name Boundary and Adversarial Fuzzing
// ============================================================================

#[test]
fn test_tool_name_boundary_lengths() {
    // Empty name: must return ValidationError::EmptyField
    let res_empty = validate_tool_name("");
    assert!(
        matches!(
            res_empty,
            Err(ValidationError::EmptyField { field: "name", .. })
        ),
        "Expected EmptyField error for empty name, got: {:?}",
        res_empty
    );

    // Exact min length (1 char): should succeed for valid chars
    assert!(validate_tool_name("a").is_ok());
    assert!(validate_tool_name("Z").is_ok());
    assert!(validate_tool_name("9").is_ok());
    assert!(validate_tool_name("_").is_ok());
    assert!(validate_tool_name("-").is_ok());

    // Exact max boundary (64 chars): must succeed
    let name_64 = "a".repeat(64);
    assert_eq!(name_64.len(), 64);
    assert!(
        validate_tool_name(&name_64).is_ok(),
        "64-character tool name should be valid"
    );

    // Exceeds max boundary (65 chars): must return ValidationError::InvalidToolName
    let name_65 = "a".repeat(65);
    let res_65 = validate_tool_name(&name_65);
    assert!(
        matches!(res_65, Err(ValidationError::InvalidToolName { ref name, ref reason }) if name.len() == 65 && reason.contains("maximum length")),
        "Expected InvalidToolName for 65-char name, got: {:?}",
        res_65
    );

    // Extreme lengths: 128, 1,000, 100,000 chars without panic
    let name_100k = "x".repeat(100_000);
    let res_100k = std::panic::catch_unwind(|| validate_tool_name(&name_100k));
    assert!(
        res_100k.is_ok(),
        "validate_tool_name panicked on 100k chars!"
    );
    assert!(matches!(
        res_100k.unwrap(),
        Err(ValidationError::InvalidToolName { .. })
    ));
}

#[test]
fn test_tool_name_illegal_characters_and_symbols() {
    // Spaces and whitespace
    let whitespace_cases = [
        " ",
        "  ",
        "\t",
        "\n",
        "\r\n",
        "tool name",
        " tool",
        "tool ",
        "tool\tname",
        "tool\nname",
    ];
    for ws in whitespace_cases {
        let res = validate_tool_name(ws);
        assert!(
            res.is_err(),
            "Tool name '{:?}' with whitespace should be rejected",
            ws
        );
    }

    // Punctuation and symbols disallowed by [a-zA-Z0-9_-]
    let symbol_cases = [
        "tool.name",
        "tool/name",
        "tool:name",
        "tool@name",
        "tool#name",
        "tool$name",
        "tool%name",
        "tool^name",
        "tool&name",
        "tool*name",
        "tool(name)",
        "tool+name",
        "tool=name",
        "tool[name]",
        "tool{name}",
        "tool|name",
        "tool\\name",
        "tool<name>",
        "tool?name",
        "tool~name",
        "tool`name",
        "tool!name",
        "tool'name",
        "tool\"name",
        "tool;name",
        "tool,name",
    ];
    for sym in symbol_cases {
        let res = validate_tool_name(sym);
        assert!(
            matches!(res, Err(ValidationError::InvalidToolName { .. })),
            "Symbolic tool name '{}' should be rejected with InvalidToolName",
            sym
        );
    }

    // Unicode / non-ASCII characters
    let unicode_cases = [
        "tööl",
        "outil_français",
        "инструмент",
        "工具",
        "εργαλείο",
        "أداة",
        "כלי",
        "tool_🦀",
        "🔧_tool",
        "🚀launch",
        "tool\u{200B}zero_width", // zero width space
        "tool\u{FEFF}bom",        // byte order mark
    ];
    for uc in unicode_cases {
        let res = validate_tool_name(uc);
        assert!(
            matches!(res, Err(ValidationError::InvalidToolName { .. })),
            "Unicode tool name '{}' should be rejected",
            uc
        );
    }

    // Null bytes and control characters
    let control_cases = [
        "tool\0name",
        "\0tool",
        "tool\0",
        "\x01\x02\x03",
        "tool\x1b[31m",
        "tool\x7f",
    ];
    for ctrl in control_cases {
        let res = validate_tool_name(ctrl);
        assert!(
            res.is_err(),
            "Control char tool name '{:?}' should be rejected",
            ctrl
        );
    }
}

// ============================================================================
// 2. Capability ID Boundary and Adversarial Fuzzing
// ============================================================================

#[test]
fn test_capability_id_boundaries() {
    // Empty id
    let res_empty = validate_capability_id("");
    assert!(
        matches!(
            res_empty,
            Err(ValidationError::EmptyField { field: "id", .. })
        ),
        "Expected EmptyField error for empty capability id"
    );

    // Exactly 128 chars (valid limit)
    let valid_128 = "a".repeat(128);
    assert!(
        validate_capability_id(&valid_128).is_ok(),
        "128-char capability id should be valid"
    );

    // 129 chars (exceeds limit)
    let invalid_129 = "a".repeat(129);
    let res_129 = validate_capability_id(&invalid_129);
    assert!(
        matches!(res_129, Err(ValidationError::InvalidIdentifier { ref reason, .. }) if reason.contains("maximum length")),
        "Expected InvalidIdentifier for 129-char id"
    );

    // Allowed characters: [a-zA-Z0-9._-:/]
    let valid_complex = "mcp:github.com/org-name/repo_v1.0:tool";
    assert!(validate_capability_id(valid_complex).is_ok());

    // Disallowed characters: spaces, @, $, #, !, ?, quotes
    let invalid_ids = [
        "hakille fs",
        "hakille@vfs",
        "mcp#repo",
        "native$tool",
        "tool!danger",
        "mcp?query",
        "tool\"quote",
        "tool'quote",
        "id_with_\0_null",
        "capa_français",
        "capa_🚀",
    ];
    for id in invalid_ids {
        let res = validate_capability_id(id);
        assert!(
            matches!(res, Err(ValidationError::InvalidIdentifier { .. })),
            "Disallowed capability id '{:?}' should be rejected",
            id
        );
    }
}

// ============================================================================
// 3. Parameters Schema Boundary and Invalid Schema Testing
// ============================================================================

#[test]
fn test_parameters_schema_non_object_roots() {
    let non_objects = vec![
        ("null", json!(null)),
        ("boolean_true", json!(true)),
        ("boolean_false", json!(false)),
        ("integer", json!(42)),
        ("negative_integer", json!(-1)),
        ("float", json!(std::f64::consts::PI)),
        ("string", json!("object")),
        ("array_empty", json!([])),
        ("array_with_obj", json!([{"type": "object"}])),
    ];

    for (desc, val) in non_objects {
        let res = validate_parameters_schema("test_tool", &val, false);
        assert!(
            matches!(res, Err(ValidationError::InvalidParametersSchema { ref reason, .. }) if reason.contains("JSON Object")),
            "Root parameters of type {} should fail with 'must be a JSON Object'",
            desc
        );
    }
}

#[test]
fn test_parameters_schema_missing_or_invalid_type() {
    // Missing 'type' field
    let missing_type = json!({
        "properties": {}
    });
    let res = validate_parameters_schema("test_tool", &missing_type, false);
    assert!(
        matches!(res, Err(ValidationError::InvalidParametersSchema { ref reason, .. }) if reason.contains("missing required 'type'")),
        "Schema missing 'type' should be rejected"
    );

    // Invalid 'type' values
    let invalid_types = vec![
        ("string", json!({"type": "string"})),
        ("number", json!({"type": "number"})),
        ("integer", json!({"type": "integer"})),
        ("boolean", json!({"type": "boolean"})),
        ("array", json!({"type": "array"})),
        ("null", json!({"type": null})),
        ("int_type", json!({"type": 123})),
        ("type_array_single", json!({"type": ["object"]})),
        ("type_array_union", json!({"type": ["object", "null"]})),
        ("empty_string", json!({"type": ""})),
        ("uppercase_object", json!({"type": "OBJECT"})),
    ];

    for (name, schema) in invalid_types {
        let res = validate_parameters_schema("test_tool", &schema, false);
        assert!(
            matches!(res, Err(ValidationError::InvalidParametersSchema { ref reason, .. }) if reason.contains("Root 'type' must be \"object\"")),
            "Schema with type {} should be rejected as not root 'object'",
            name
        );
    }
}

#[test]
fn test_parameters_schema_malformed_properties_and_required() {
    // Malformed 'properties'
    let bad_properties = vec![
        json!({"type": "object", "properties": null}),
        json!({"type": "object", "properties": 123}),
        json!({"type": "object", "properties": "not-an-object"}),
        json!({"type": "object", "properties": []}),
        json!({"type": "object", "properties": true}),
    ];
    for bad_prop in bad_properties {
        let res = validate_parameters_schema("test_tool", &bad_prop, false);
        assert!(
            matches!(res, Err(ValidationError::InvalidParametersSchema { ref reason, .. }) if reason.contains("'properties' must be a JSON Object")),
            "Schema with malformed properties {:?} should be rejected",
            bad_prop
        );
    }

    // Malformed 'required'
    let bad_required = vec![
        json!({"type": "object", "required": null}),
        json!({"type": "object", "required": 123}),
        json!({"type": "object", "required": "field"}),
        json!({"type": "object", "required": {}}),
        json!({"type": "object", "required": true}),
        json!({"type": "object", "required": [123]}),
        json!({"type": "object", "required": ["valid", null]}),
        json!({"type": "object", "required": ["valid", true]}),
        json!({"type": "object", "required": ["valid", {}]}),
        json!({"type": "object", "required": ["valid", ["nested"]]}),
    ];
    for bad_req in bad_required {
        let res = validate_parameters_schema("test_tool", &bad_req, false);
        assert!(
            matches!(res, Err(ValidationError::InvalidParametersSchema { .. })),
            "Schema with malformed required {:?} should be rejected",
            bad_req
        );
    }
}

#[test]
fn test_parameters_schema_strict_mode_violations() {
    // 1. Missing additionalProperties in strict mode
    let schema_no_add_props = json!({
        "type": "object",
        "properties": { "foo": { "type": "string" } },
        "required": ["foo"]
    });
    let res1 = validate_parameters_schema("test_tool", &schema_no_add_props, true);
    assert!(
        matches!(res1, Err(ValidationError::StrictModeViolation { ref reason, .. }) if reason.contains("additionalProperties: false")),
        "Missing additionalProperties: false must fail in strict mode"
    );

    // 2. additionalProperties: true in strict mode
    let schema_add_props_true = json!({
        "type": "object",
        "properties": { "foo": { "type": "string" } },
        "required": ["foo"],
        "additionalProperties": true
    });
    let res2 = validate_parameters_schema("test_tool", &schema_add_props_true, true);
    assert!(
        matches!(res2, Err(ValidationError::StrictModeViolation { ref reason, .. }) if reason.contains("additionalProperties: false")),
        "additionalProperties: true must fail in strict mode"
    );

    // 3. additionalProperties: "false" (string instead of bool)
    let schema_add_props_str = json!({
        "type": "object",
        "properties": { "foo": { "type": "string" } },
        "required": ["foo"],
        "additionalProperties": "false"
    });
    let res3 = validate_parameters_schema("test_tool", &schema_add_props_str, true);
    assert!(
        matches!(res3, Err(ValidationError::StrictModeViolation { ref reason, .. }) if reason.contains("additionalProperties: false")),
        "additionalProperties as string must fail in strict mode"
    );

    // 4. Missing property in 'required'
    let schema_missing_required = json!({
        "type": "object",
        "properties": {
            "foo": { "type": "string" },
            "bar": { "type": "number" }
        },
        "required": ["foo"],
        "additionalProperties": false
    });
    let res4 = validate_parameters_schema("test_tool", &schema_missing_required, true);
    assert!(
        matches!(res4, Err(ValidationError::StrictModeViolation { ref reason, .. }) if reason.contains("'bar'")),
        "Property missing from required array must fail in strict mode"
    );

    // 5. Nested object missing additionalProperties: false
    let schema_nested_missing_add = json!({
        "type": "object",
        "properties": {
            "config": {
                "type": "object",
                "properties": { "key": { "type": "string" } },
                "required": ["key"]
            }
        },
        "required": ["config"],
        "additionalProperties": false
    });
    let res5 = validate_parameters_schema("test_tool", &schema_nested_missing_add, true);
    assert!(
        matches!(res5, Err(ValidationError::StrictModeViolation { ref reason, .. }) if reason.contains("nested object 'config'")),
        "Nested object missing additionalProperties: false must fail in strict mode"
    );

    // 6. Nested object missing property in nested required
    let schema_nested_missing_req = json!({
        "type": "object",
        "properties": {
            "config": {
                "type": "object",
                "properties": {
                    "key": { "type": "string" },
                    "val": { "type": "string" }
                },
                "required": ["key"],
                "additionalProperties": false
            }
        },
        "required": ["config"],
        "additionalProperties": false
    });
    let res6 = validate_parameters_schema("test_tool", &schema_nested_missing_req, true);
    assert!(
        matches!(res6, Err(ValidationError::StrictModeViolation { ref reason, .. }) if reason.contains("config.val")),
        "Nested object with unlisted required property must fail in strict mode"
    );
}

#[test]
fn test_strict_mode_deep_nesting_exploration() {
    let deep_schema = json!({
        "type": "object",
        "properties": {
            "level1": {
                "type": "object",
                "properties": {
                    "level2": {
                        "type": "object",
                        "properties": {
                            "level3_key": { "type": "string" }
                        },
                        "required": [],
                        "additionalProperties": true // VIOLATION at depth 2!
                    }
                },
                "required": ["level2"],
                "additionalProperties": false
            }
        },
        "required": ["level1"],
        "additionalProperties": false
    });

    let result = validate_parameters_schema("deep_tool", &deep_schema, true);
    assert!(
        matches!(result, Err(ValidationError::StrictModeViolation { ref reason, .. }) if reason.contains("level1.level2")),
        "Fixed: deep nesting beyond level 1 is recursively traversed and violations are caught"
    );
}

#[test]
fn test_strict_mode_deep_nesting_level4_violation() {
    let deep4_schema = json!({
        "type": "object",
        "properties": {
            "l1": {
                "type": "object",
                "properties": {
                    "l2": {
                        "type": "object",
                        "properties": {
                            "l3": {
                                "type": "object",
                                "properties": {
                                    "leaf": { "type": "string" }
                                },
                                "required": [], // missing "leaf"!
                                "additionalProperties": false
                            }
                        },
                        "required": ["l3"],
                        "additionalProperties": false
                    }
                },
                "required": ["l2"],
                "additionalProperties": false
            }
        },
        "required": ["l1"],
        "additionalProperties": false
    });

    let res = validate_parameters_schema("deep4_tool", &deep4_schema, true);
    assert!(
        matches!(res, Err(ValidationError::StrictModeViolation { ref reason, .. }) if reason.contains("l1.l2.l3.leaf"))
    );
}

#[test]
fn test_strict_mode_deep_nesting_valid() {
    let valid_deep_schema = json!({
        "type": "object",
        "properties": {
            "l1": {
                "type": "object",
                "properties": {
                    "l2": {
                        "type": "object",
                        "properties": {
                            "val": { "type": "string" }
                        },
                        "required": ["val"],
                        "additionalProperties": false
                    }
                },
                "required": ["l2"],
                "additionalProperties": false
            }
        },
        "required": ["l1"],
        "additionalProperties": false
    });

    let res = validate_parameters_schema("valid_deep", &valid_deep_schema, true);
    assert!(res.is_ok());
}

#[test]
fn test_strict_mode_depth3_missing_additional_properties() {
    let deep3_schema = json!({
        "type": "object",
        "properties": {
            "l1": {
                "type": "object",
                "properties": {
                    "l2": {
                        "type": "object",
                        "properties": {
                            "l3": {
                                "type": "object",
                                "properties": {
                                    "val": { "type": "string" }
                                },
                                "required": ["val"]
                                // Missing additionalProperties: false at depth 3!
                            }
                        },
                        "required": ["l3"],
                        "additionalProperties": false
                    }
                },
                "required": ["l2"],
                "additionalProperties": false
            }
        },
        "required": ["l1"],
        "additionalProperties": false
    });

    let res = validate_parameters_schema("deep3_tool", &deep3_schema, true);
    assert!(
        matches!(res, Err(ValidationError::StrictModeViolation { ref reason, .. }) if reason.contains("l1.l2.l3")),
        "Expected StrictModeViolation for missing additionalProperties at depth 3, got: {:?}",
        res
    );
}

#[test]
fn test_strict_mode_depth3_additional_properties_true() {
    let deep3_schema = json!({
        "type": "object",
        "properties": {
            "l1": {
                "type": "object",
                "properties": {
                    "l2": {
                        "type": "object",
                        "properties": {
                            "l3": {
                                "type": "object",
                                "properties": {
                                    "val": { "type": "string" }
                                },
                                "required": ["val"],
                                "additionalProperties": true // VIOLATION at depth 3!
                            }
                        },
                        "required": ["l3"],
                        "additionalProperties": false
                    }
                },
                "required": ["l2"],
                "additionalProperties": false
            }
        },
        "required": ["l1"],
        "additionalProperties": false
    });

    let res = validate_parameters_schema("deep3_tool_true", &deep3_schema, true);
    assert!(
        matches!(res, Err(ValidationError::StrictModeViolation { ref reason, .. }) if reason.contains("l1.l2.l3")),
        "Expected StrictModeViolation for additionalProperties: true at depth 3, got: {:?}",
        res
    );
}

#[test]
fn test_strict_mode_depth3_unlisted_required_property() {
    let deep3_schema = json!({
        "type": "object",
        "properties": {
            "l1": {
                "type": "object",
                "properties": {
                    "l2": {
                        "type": "object",
                        "properties": {
                            "l3": {
                                "type": "object",
                                "properties": {
                                    "p1": { "type": "string" },
                                    "p2": { "type": "number" }
                                },
                                "required": ["p1"], // p2 is missing from required!
                                "additionalProperties": false
                            }
                        },
                        "required": ["l3"],
                        "additionalProperties": false
                    }
                },
                "required": ["l2"],
                "additionalProperties": false
            }
        },
        "required": ["l1"],
        "additionalProperties": false
    });

    let res = validate_parameters_schema("deep3_unlisted", &deep3_schema, true);
    assert!(
        matches!(res, Err(ValidationError::StrictModeViolation { ref reason, .. }) if reason.contains("l1.l2.l3.p2")),
        "Expected StrictModeViolation for unlisted property at depth 3, got: {:?}",
        res
    );
}

#[test]
fn test_strict_mode_depth3_valid() {
    let deep3_valid = json!({
        "type": "object",
        "properties": {
            "l1": {
                "type": "object",
                "properties": {
                    "l2": {
                        "type": "object",
                        "properties": {
                            "l3": {
                                "type": "object",
                                "properties": {
                                    "p1": { "type": "string" },
                                    "p2": { "type": "number" }
                                },
                                "required": ["p1", "p2"],
                                "additionalProperties": false
                            }
                        },
                        "required": ["l3"],
                        "additionalProperties": false
                    }
                },
                "required": ["l2"],
                "additionalProperties": false
            }
        },
        "required": ["l1"],
        "additionalProperties": false
    });

    let res = validate_parameters_schema("deep3_valid", &deep3_valid, true);
    assert!(
        res.is_ok(),
        "Valid depth 3 schema should pass, got: {:?}",
        res
    );
}

fn build_deep_schema(depth: usize, violate_at_depth: Option<usize>) -> Value {
    let mut current = if violate_at_depth == Some(depth) {
        json!({
            "type": "object",
            "properties": {
                "leaf_str": { "type": "string" }
            },
            "required": ["leaf_str"],
            "additionalProperties": true
        })
    } else {
        json!({
            "type": "object",
            "properties": {
                "leaf_str": { "type": "string" }
            },
            "required": ["leaf_str"],
            "additionalProperties": false
        })
    };

    for d in (1..depth).rev() {
        let is_violating = violate_at_depth == Some(d);
        current = json!({
            "type": "object",
            "properties": {
                format!("level_{d}"): current
            },
            "required": [format!("level_{d}")],
            "additionalProperties": if is_violating { json!(true) } else { json!(false) }
        });
    }

    json!({
        "type": "object",
        "properties": {
            "level_0": current
        },
        "required": ["level_0"],
        "additionalProperties": false
    })
}

#[test]
fn test_strict_mode_arbitrary_depth_n_valid() {
    for depth in [3, 4, 5, 8, 12, 16] {
        let valid_schema = build_deep_schema(depth, None);
        let res = validate_parameters_schema(&format!("valid_depth_{depth}"), &valid_schema, true);
        assert!(
            res.is_ok(),
            "Valid schema at depth {depth} failed validation: {:?}",
            res
        );
    }
}

#[test]
fn test_strict_mode_arbitrary_depth_n_violation() {
    for depth in [3, 4, 5, 8, 12, 16] {
        let invalid_schema = build_deep_schema(depth, Some(depth));
        let res =
            validate_parameters_schema(&format!("invalid_depth_{depth}"), &invalid_schema, true);
        assert!(
            matches!(res, Err(ValidationError::StrictModeViolation { .. })),
            "Expected failure for violation at depth {depth}, got: {:?}",
            res
        );
    }
}

#[test]
fn test_strict_mode_deep_nesting_array_items_depth3_violation() {
    let schema = json!({
        "type": "object",
        "properties": {
            "l1": {
                "type": "object",
                "properties": {
                    "l2_array": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "l3_array_item_obj": {
                                    "type": "object",
                                    "properties": {
                                        "name": { "type": "string" }
                                    },
                                    "required": [],
                                    "additionalProperties": false
                                }
                            },
                            "required": ["l3_array_item_obj"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["l2_array"],
                "additionalProperties": false
            }
        },
        "required": ["l1"],
        "additionalProperties": false
    });

    let res = validate_parameters_schema("array_deep_tool", &schema, true);
    assert!(
        matches!(res, Err(ValidationError::StrictModeViolation { ref reason, .. }) if reason.contains("l1.l2_array.items.l3_array_item_obj.name")),
        "Violation inside array item object at depth >= 3 must be rejected, got: {:?}",
        res
    );
}

#[test]
fn test_strict_mode_deep_nesting_array_items_depth3_valid() {
    let schema = json!({
        "type": "object",
        "properties": {
            "l1": {
                "type": "object",
                "properties": {
                    "l2_array": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "l3_array_item_obj": {
                                    "type": "object",
                                    "properties": {
                                        "name": { "type": "string" }
                                    },
                                    "required": ["name"],
                                    "additionalProperties": false
                                }
                            },
                            "required": ["l3_array_item_obj"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["l2_array"],
                "additionalProperties": false
            }
        },
        "required": ["l1"],
        "additionalProperties": false
    });

    let res = validate_parameters_schema("array_deep_valid", &schema, true);
    assert!(
        res.is_ok(),
        "Valid array item object at depth >= 3 should pass, got: {:?}",
        res
    );
}

#[test]
fn test_canonical_tool_definition_description_boundaries() {
    let schema = CanonicalToolDefinition::empty_parameters_schema();

    // Empty description
    let def_empty_desc = CanonicalToolDefinition::new("my_tool", "", schema.clone());
    assert!(matches!(
        def_empty_desc.validate(),
        Err(ValidationError::EmptyField {
            field: "description",
            ..
        })
    ));

    // Whitespace-only description
    let def_ws_desc = CanonicalToolDefinition::new("my_tool", "   \t\n\r  ", schema.clone());
    assert!(matches!(
        def_ws_desc.validate(),
        Err(ValidationError::EmptyField {
            field: "description",
            ..
        })
    ));

    // Very large description (500,000 characters)
    let large_desc = "A".repeat(500_000);
    let def_large = CanonicalToolDefinition::new("large_tool", large_desc, schema.clone());
    let validate_res = std::panic::catch_unwind(|| def_large.validate());
    assert!(validate_res.is_ok(), "Validation panicked on 500k desc!");
    assert!(validate_res.unwrap().is_ok());

    // Unicode in description
    let unicode_desc = "Outil de recherche 🔍 pour système souverain 🦀 (日本語 / 中文 / العربية)";
    let def_unicode = CanonicalToolDefinition::new("search_tool", unicode_desc, schema);
    assert!(def_unicode.validate().is_ok());
}

// ============================================================================
// 4. Canonical Tool Call Boundary & Parsing Tests
// ============================================================================

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct StronglyTypedParams {
    id: u64,
    name: String,
    optional_flag: Option<bool>,
}

#[test]
fn test_canonical_tool_call_json_string_parsing() {
    // Normal JSON
    let call =
        CanonicalToolCall::from_json_str("call_1", "test_tool", r#"{"id": 42, "name": "alice"}"#)
            .unwrap();
    assert_eq!(call.arguments["id"], 42);
    assert_eq!(call.arguments["name"], "alice");

    let parsed: StronglyTypedParams = call.parse_arguments().unwrap();
    assert_eq!(parsed.id, 42);
    assert_eq!(parsed.name, "alice");
    assert_eq!(parsed.optional_flag, None);

    // Empty string and whitespace -> returns empty object {}
    let empty_call = CanonicalToolCall::from_json_str("call_2", "test_tool", "").unwrap();
    assert_eq!(empty_call.arguments, json!({}));

    let ws_call = CanonicalToolCall::from_json_str("call_3", "test_tool", "   \n\t  ").unwrap();
    assert_eq!(ws_call.arguments, json!({}));

    // Malformed JSON -> returns Err(serde_json::Error) without panic
    let bad_json_cases = [
        "{broken",
        "{'single_quotes': 1}",
        "{\"unclosed\": ",
        "{\"trailing\": 1,}",
        "[1, 2,",
        "undefined",
        "<<xml>>",
        "\0",
    ];
    for bad in bad_json_cases {
        let res = std::panic::catch_unwind(|| {
            CanonicalToolCall::from_json_str("call_err", "test_tool", bad)
        });
        assert!(
            res.is_ok(),
            "from_json_str panicked on malformed input: {}",
            bad
        );
        assert!(
            res.unwrap().is_err(),
            "Malformed JSON should return Err: {}",
            bad
        );
    }

    // JSON primitive values in from_json_str
    let primitives = [
        "null",
        "12345",
        "-99.9",
        "true",
        "false",
        "\"just a string\"",
    ];
    for prim in primitives {
        let call = CanonicalToolCall::from_json_str("call_prim", "test", prim).unwrap();
        // Strongly typed parse into struct should cleanly fail without panicking
        let typed_res: Result<StronglyTypedParams, _> = call.parse_arguments();
        assert!(
            typed_res.is_err(),
            "Primitive argument should not deserialize into struct"
        );
    }
}

#[test]
fn test_canonical_tool_call_extreme_numbers_and_nesting() {
    // Extreme integers
    let extreme_json = json!({
        "u64_max": u64::MAX,
        "i64_min": i64::MIN,
        "i64_max": i64::MAX,
        "zero": 0,
        "neg_zero": -0.0,
        "float_min_pos": f64::MIN_POSITIVE,
        "float_max": 1.7976931348623157e+308,
        "very_small_float": 1e-300,
        "null_val": null,
        "nested_nulls": [null, [null, {"a": null}]]
    });

    let call = CanonicalToolCall::new("call_extreme", "extreme_tool", extreme_json.clone());

    // Serde roundtrip
    let ser = serde_json::to_string(&call).unwrap();
    let de: CanonicalToolCall = serde_json::from_str(&ser).unwrap();
    assert_eq!(de.call_id, "call_extreme");
    assert_eq!(de.arguments["u64_max"], u64::MAX);
    assert_eq!(de.arguments["i64_min"], i64::MIN);
    assert_eq!(de.arguments["null_val"], Value::Null);

    // Deep nesting (80 levels)
    let mut deep_value = json!({"leaf": "value"});
    for i in 0..80 {
        deep_value = json!({ format!("depth_{i}"): deep_value });
    }
    let deep_call = CanonicalToolCall::new("call_deep", "deep_tool", deep_value);
    let deep_ser = serde_json::to_string(&deep_call).unwrap();
    let deep_de: CanonicalToolCall = serde_json::from_str(&deep_ser).unwrap();
    assert_eq!(deep_de.call_id, "call_deep");
}

// ============================================================================
// 5. Canonical Content Block & Tool Result Serde & Edge Cases
// ============================================================================

#[test]
fn test_canonical_content_block_variants_and_serde() {
    // 1. Text block
    let text = CanonicalContentBlock::text("Hello, Hakille!");
    assert!(text.is_text());
    assert!(!text.is_image());
    assert!(!text.is_resource());
    assert_eq!(text.as_text(), Some("Hello, Hakille!"));

    // 2. Image block
    let image = CanonicalContentBlock::image(
        "image/png",
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
    );
    assert!(!image.is_text());
    assert!(image.is_image());
    assert!(!image.is_resource());
    assert_eq!(image.as_text(), None);

    // 3. Resource text block
    let res_text = CanonicalContentBlock::resource_text(
        "file:///etc/hosts",
        Some("text/plain".into()),
        "127.0.0.1 localhost",
    );
    assert!(!res_text.is_text());
    assert!(!res_text.is_image());
    assert!(res_text.is_resource());
    // Note: as_text() specifically checks Self::Text, so it returns None for Resource
    assert_eq!(res_text.as_text(), None);

    // 4. Resource blob block
    let res_blob = CanonicalContentBlock::resource_blob(
        "file:///bin/kernel",
        Some("application/octet-stream".into()),
        "DEADBEEF==",
    );
    assert!(res_blob.is_resource());
    assert_eq!(res_blob.as_text(), None);

    // Deserialization with unknown or missing type tags
    let bad_blocks = [
        r#"{"type": "unknown_type", "data": "123"}"#,
        r#"{"text": "missing type tag"}"#,
        r#"{"type": "image"}"#,    // missing fields
        r#"{"type": "text"}"#,     // missing text field
        r#"{"type": "resource"}"#, // missing uri field
        r#"{}"#,
    ];

    for bad in bad_blocks {
        let res: Result<CanonicalContentBlock, _> = serde_json::from_str(bad);
        assert!(
            res.is_err(),
            "Malformed content block '{}' should fail deserialization",
            bad
        );
    }
}

#[test]
fn test_canonical_tool_result_aggregations_and_telemetry() {
    let metadata = ExecutionMetadata::new(150, RiskTier::Tier2ExternalNetwork)
        .with_fuel(5000)
        .with_memory(1024 * 1024)
        .with_audit_record_id("audit_record_98765");

    // Success with multiple text blocks
    let multi_content = vec![
        CanonicalContentBlock::text("Line 1: system initialized"),
        CanonicalContentBlock::image("image/png", "base64data"),
        CanonicalContentBlock::text("Line 2: records scanned"),
    ];
    let res = CanonicalToolResult::success("call_multi", multi_content, metadata.clone());
    assert!(res.is_success());
    assert!(!res.is_error);
    // text_summary should extract and join only the Text blocks with \n
    assert_eq!(
        res.text_summary(),
        "Line 1: system initialized\nLine 2: records scanned"
    );

    // Empty content tool result
    let empty_res = CanonicalToolResult::success("call_empty", vec![], metadata.clone());
    assert_eq!(empty_res.text_summary(), "");

    // Error tool result
    let err_res = CanonicalToolResult::error("call_err", "Connection refused: port 8080", metadata);
    assert!(!err_res.is_success());
    assert!(err_res.is_error);
    assert_eq!(err_res.text_summary(), "Connection refused: port 8080");

    // Serde roundtrip of full CanonicalToolResult
    let ser = serde_json::to_string(&res).unwrap();
    let de: CanonicalToolResult = serde_json::from_str(&ser).unwrap();
    assert_eq!(de.call_id, "call_multi");
    assert_eq!(de.content.len(), 3);
    assert_eq!(de.metadata.fuel_consumed, Some(5000));
    assert_eq!(de.metadata.risk_tier, RiskTier::Tier2ExternalNetwork);
}

// ============================================================================
// 6. Execution Metadata Numerical Boundaries
// ============================================================================

#[test]
fn test_execution_metadata_extremes() {
    let default_meta = ExecutionMetadata::default();
    assert_eq!(default_meta.duration_ms, 0);
    assert_eq!(default_meta.risk_tier, RiskTier::Tier0ReadOnly);
    assert_eq!(default_meta.fuel_consumed, None);
    assert_eq!(default_meta.memory_bytes, None);
    assert_eq!(default_meta.audit_record_id, None);

    // Maximum telemetry numbers
    let max_meta = ExecutionMetadata::new(u64::MAX, RiskTier::Tier3CriticalExecution)
        .with_fuel(u64::MAX)
        .with_memory(usize::MAX)
        .with_audit_record_id("audit_max");

    let ser = serde_json::to_string(&max_meta).unwrap();
    let de: ExecutionMetadata = serde_json::from_str(&ser).unwrap();
    assert_eq!(de.duration_ms, u64::MAX);
    assert_eq!(de.fuel_consumed, Some(u64::MAX));
    assert_eq!(de.memory_bytes, Some(usize::MAX));
    assert_eq!(de.risk_tier, RiskTier::Tier3CriticalExecution);

    // Skip serializing if None check:
    let min_ser = serde_json::to_string(&default_meta).unwrap();
    assert!(!min_ser.contains("fuel_consumed"));
    assert!(!min_ser.contains("memory_bytes"));
    assert!(!min_ser.contains("audit_record_id"));
}

// ============================================================================
// 7. CapabilityManifest & CapabilityDescriptor Boundary Tests
// ============================================================================

#[test]
fn test_manifest_validation_failures() {
    let version = semver::Version::parse("1.0.0").unwrap();

    // 1. Empty ID
    let m_empty_id = CapabilityManifest::new(
        "",
        "Name",
        version.clone(),
        "Desc",
        "Author",
        CapabilityType::Native,
    );
    assert!(matches!(
        m_empty_id.validate(),
        Err(ValidationError::EmptyField { field: "id", .. })
    ));

    // 2. Empty Name
    let m_empty_name = CapabilityManifest::new(
        "hakille.fs",
        "   ",
        version.clone(),
        "Desc",
        "Author",
        CapabilityType::Native,
    );
    assert!(matches!(
        m_empty_name.validate(),
        Err(ValidationError::EmptyField { field: "name", .. })
    ));

    // 3. Empty Description
    let m_empty_desc = CapabilityManifest::new(
        "hakille.fs",
        "Name",
        version.clone(),
        " \n ",
        "Author",
        CapabilityType::Native,
    );
    assert!(matches!(
        m_empty_desc.validate(),
        Err(ValidationError::EmptyField {
            field: "description",
            ..
        })
    ));

    // 4. Duplicate Tool Names
    let mut m_dup = CapabilityManifest::new(
        "hakille.fs",
        "Name",
        version.clone(),
        "Desc",
        "Author",
        CapabilityType::Native,
    );
    let t1 = CanonicalToolDefinition::new(
        "read_file",
        "Read file",
        CanonicalToolDefinition::empty_parameters_schema(),
    );
    let t2 = CanonicalToolDefinition::new(
        "read_file",
        "Duplicate read",
        CanonicalToolDefinition::empty_parameters_schema(),
    );
    m_dup.tools.push(t1);
    m_dup.tools.push(t2);
    assert!(
        matches!(m_dup.validate(), Err(ValidationError::DuplicateToolName { ref name, .. }) if name == "read_file")
    );

    // 5. Invalid Tool inside Manifest
    let mut m_bad_tool = CapabilityManifest::new(
        "hakille.fs",
        "Name",
        version,
        "Desc",
        "Author",
        CapabilityType::Native,
    );
    let t_bad = CanonicalToolDefinition::new(
        "invalid tool name!",
        "Desc",
        CanonicalToolDefinition::empty_parameters_schema(),
    );
    m_bad_tool.tools.push(t_bad);
    assert!(matches!(
        m_bad_tool.validate(),
        Err(ValidationError::InvalidToolName { .. })
    ));
}

#[test]
fn test_descriptor_validation_and_lookup() {
    let version = semver::Version::parse("1.0.0").unwrap();
    let mut manifest = CapabilityManifest::new(
        "hakille.vfs",
        "Virtual Filesystem",
        version,
        "Sovereign filesystem capability",
        "Hakille",
        CapabilityType::Native,
    );
    manifest =
        manifest
            .with_scope(CapabilityScope::Filesystem)
            .with_tool(CanonicalToolDefinition::new(
                "vfs_read",
                "Read file",
                CanonicalToolDefinition::empty_parameters_schema(),
            ));

    let descriptor = manifest.to_descriptor();
    assert!(descriptor.validate().is_ok());
    assert!(descriptor.requires_scope(&CapabilityScope::Filesystem));
    assert!(!descriptor.requires_scope(&CapabilityScope::Network));
    assert!(descriptor.find_tool("vfs_read").is_some());
    assert!(descriptor.find_tool("unknown").is_none());

    // Empty name in descriptor validation
    let mut bad_desc = descriptor.clone();
    bad_desc.name = "   ".to_string();
    assert!(matches!(
        bad_desc.validate(),
        Err(ValidationError::EmptyField { field: "name", .. })
    ));

    // Duplicate tools in descriptor validation
    let mut dup_desc = descriptor.clone();
    dup_desc.tools.push(CanonicalToolDefinition::new(
        "vfs_read",
        "Duplicate",
        CanonicalToolDefinition::empty_parameters_schema(),
    ));
    assert!(matches!(
        dup_desc.validate(),
        Err(ValidationError::DuplicateToolName { .. })
    ));
}

#[test]
fn test_manifest_large_scale_tools() {
    let version = semver::Version::parse("2.0.0").unwrap();
    let mut manifest = CapabilityManifest::new(
        "hakille.massive",
        "Massive Capability Package",
        version,
        "Manifest containing 500 distinct tools for stress testing",
        "Hakille Bench",
        CapabilityType::Wasm,
    );

    for i in 0..500 {
        let name = format!("tool_operation_{i:04}");
        let tool = CanonicalToolDefinition::new(
            name,
            format!("Automated tool description {i}"),
            CanonicalToolDefinition::empty_parameters_schema(),
        );
        manifest.tools.push(tool);
    }

    assert_eq!(manifest.tools.len(), 500);
    let validate_res = std::panic::catch_unwind(|| manifest.validate());
    assert!(
        validate_res.is_ok(),
        "Manifest validation panicked on 500 tools!"
    );
    assert!(validate_res.unwrap().is_ok());

    let descriptor = manifest.to_descriptor();
    assert_eq!(descriptor.tools.len(), 500);
    assert!(descriptor.find_tool("tool_operation_0250").is_some());
    assert!(descriptor.find_tool("tool_operation_9999").is_none());
}

// ============================================================================
// 8. RiskTier & CapabilityType Deserialization Edge Cases
// ============================================================================

#[test]
fn test_risk_tier_invalid_inputs() {
    let invalid_tiers = [
        "\"tier4\"",
        "\"tier_unknown\"",
        "\"\"",
        "4",
        "-1",
        "null",
        "true",
        "{}",
    ];

    for inv in invalid_tiers {
        let res: Result<RiskTier, _> = serde_json::from_str(inv);
        assert!(
            res.is_err(),
            "Invalid risk tier '{}' should fail deserialization",
            inv
        );
    }

    assert_eq!(RiskTier::from_level(255), None);
    assert_eq!(RiskTier::from_level(4), None);
}

#[test]
fn test_capability_type_invalid_inputs() {
    let invalid_types = ["\"unknown\"", "\"docker\"", "\"vm\"", "1", "null", "{}"];

    for inv in invalid_types {
        let res: Result<CapabilityType, _> = serde_json::from_str(inv);
        assert!(
            res.is_err(),
            "Invalid capability type '{}' should fail deserialization",
            inv
        );
    }
}

// ============================================================================
// 9. Provider Translators Adversarial Inputs
// ============================================================================

#[test]
fn test_translators_adversarial_name_and_arguments() {
    // 1. Anthropic tool name sanitization on adversarial names
    assert_eq!(AnthropicTranslator::sanitize_tool_name(""), "unnamed_tool");
    assert_eq!(AnthropicTranslator::sanitize_tool_name("   "), "___");
    assert_eq!(
        AnthropicTranslator::sanitize_tool_name("search docs.v1!"),
        "search_docs_v1_"
    );
    let name_100 = "x".repeat(100);
    assert_eq!(AnthropicTranslator::sanitize_tool_name(&name_100).len(), 64);

    // 2. OpenAI translator handling of malformed arguments
    let malformed_openai_call = hakille_capability::translators::openai::OpenAiToolCall {
        id: "call_bad".to_string(),
        call_type: "function".to_string(),
        function: hakille_capability::translators::openai::OpenAiFunctionCallPayload {
            name: "fs_read".to_string(),
            arguments: "{unclosed:".to_string(),
        },
    };
    let parse_res = OpenAiTranslator::parse_tool_call(&malformed_openai_call);
    assert!(
        matches!(
            parse_res,
            Err(hakille_capability::translators::TranslatorError::MalformedArguments { .. })
        ),
        "OpenAI translator must return MalformedArguments error on broken JSON"
    );

    // 3. Gemini synthetic ID generation when function call has no id
    let gemini_call_no_id = hakille_capability::translators::gemini::GeminiFunctionCallPart {
        function_call: hakille_capability::translators::gemini::GeminiFunctionCall {
            id: None,
            name: "calculate".to_string(),
            args: json!({"expr": "2+2"}),
        },
    };
    let parsed_gemini = GeminiTranslator::parse_tool_call(&gemini_call_no_id).unwrap();
    assert!(
        !parsed_gemini.call_id.is_empty(),
        "Gemini translator must synthesize call_id"
    );
    assert_eq!(parsed_gemini.tool_name, "calculate");

    // 4. ChatML parse generation stream on empty or adversarial streams
    let empty_stream = "";
    assert!(
        ChatMlTranslator::parse_generation_stream(empty_stream)
            .unwrap()
            .is_empty()
    );

    let garbage_stream = "<tool_call>not json at all</tool_call>";
    let garbage_res = ChatMlTranslator::parse_generation_stream(garbage_stream);
    assert!(
        garbage_res.is_err(),
        "ChatML translator should return Delimiter/Parse error on unparseable tool_call JSON"
    );
}

// ============================================================================
// 10. Proptest Randomized Property-Based Adversarial Fuzzing
// ============================================================================

proptest! {
    #[test]
    fn prop_validate_tool_name_does_not_panic(s in "\\PC*") {
        let _ = std::panic::catch_unwind(|| {
            let res = validate_tool_name(&s);
            if res.is_ok() {
                // If it passes, assert length <= 64 and all chars are ascii_alphanumeric, _, -
                prop_assert!(!s.is_empty());
                prop_assert!(s.len() <= 64);
                prop_assert!(s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
            }
            Ok(())
        });
    }

    #[test]
    fn prop_validate_capability_id_does_not_panic(s in "\\PC*") {
        let _ = std::panic::catch_unwind(|| {
            let res = validate_capability_id(&s);
            if res.is_ok() {
                // If it passes, assert length <= 128 and chars in [a-zA-Z0-9._-:/]
                prop_assert!(!s.is_empty());
                prop_assert!(s.len() <= 128);
                prop_assert!(s.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '/' || c == '_' || c == '-' || c == ':'));
            }
            Ok(())
        });
    }

    #[test]
    fn prop_canonical_tool_call_json_str_never_panics(s in "\\PC*") {
        let _ = std::panic::catch_unwind(|| {
            let _ = CanonicalToolCall::from_json_str("call_prop", "tool_prop", &s);
        });
    }

    #[test]
    fn prop_validate_parameters_schema_never_panics(
        strict in any::<bool>(),
        tool_name in "\\PC*",
        has_type in any::<bool>(),
        has_add_props in any::<bool>(),
        add_props_val in any::<bool>(),
        prop_key in "\\PC*",
        req_key in "\\PC*"
    ) {
        let mut map = serde_json::Map::new();
        if has_type {
            map.insert("type".to_string(), json!("object"));
        }
        if has_add_props {
            map.insert("additionalProperties".to_string(), json!(add_props_val));
        }
        let mut props = serde_json::Map::new();
        if !prop_key.is_empty() {
            props.insert(prop_key.clone(), json!({"type": "string"}));
        }
        map.insert("properties".to_string(), Value::Object(props));
        if !req_key.is_empty() {
            map.insert("required".to_string(), json!([req_key]));
        }
        let schema = Value::Object(map);
        let _ = validate_parameters_schema(&tool_name, &schema, strict);
    }

    #[test]
    fn prop_canonical_tool_definition_validate_never_panics(
        name in "\\PC*",
        desc in "\\PC*",
        strict in any::<bool>()
    ) {
        let tool = CanonicalToolDefinition::new(name, desc, CanonicalToolDefinition::empty_parameters_schema())
            .with_strict(strict);
        let _ = tool.validate();
    }

    #[test]
    fn prop_risk_tier_roundtrip_and_invariants(level in any::<u8>()) {
        let tier_opt = RiskTier::from_level(level);
        if let Some(tier) = tier_opt {
            prop_assert_eq!(tier.level(), level);
            prop_assert!(level <= 3);
            let ser = serde_json::to_string(&tier).unwrap();
            let de: RiskTier = serde_json::from_str(&ser).unwrap();
            prop_assert_eq!(de, tier);
            prop_assert_eq!(tier.requires_user_confirmation(), level == 3);
            prop_assert_eq!(tier.has_network_egress(), level == 2 || level == 3);
            prop_assert_eq!(tier.is_mutating(), level >= 1);
        } else {
            prop_assert!(level > 3);
        }
    }

    #[test]
    fn prop_capability_type_roundtrip_and_display(raw in "\\PC*") {
        if let Ok(cap_type) = serde_json::from_str::<CapabilityType>(&format!("\"{raw}\"")) {
            let s = cap_type.as_str();
            prop_assert_eq!(format!("{cap_type}"), s);
            let ser = serde_json::to_string(&cap_type).unwrap();
            let de: CapabilityType = serde_json::from_str(&ser).unwrap();
            prop_assert_eq!(de, cap_type);
            if cap_type.is_mcp() {
                prop_assert!(matches!(cap_type, CapabilityType::McpStdio | CapabilityType::McpSse));
            }
        }
    }

    #[test]
    fn prop_canonical_content_block_serde_roundtrip(text in "\\PC*", mime in "\\PC*", uri in "\\PC*") {
        // Text block
        let b_text = CanonicalContentBlock::text(&text);
        prop_assert!(b_text.is_text());
        prop_assert_eq!(b_text.as_text(), Some(text.as_str()));
        let ser_t = serde_json::to_string(&b_text).unwrap();
        let de_t: CanonicalContentBlock = serde_json::from_str(&ser_t).unwrap();
        prop_assert_eq!(de_t, b_text);

        // Image block
        let b_img = CanonicalContentBlock::image(&mime, "base64data");
        prop_assert!(b_img.is_image());
        let ser_i = serde_json::to_string(&b_img).unwrap();
        let de_i: CanonicalContentBlock = serde_json::from_str(&ser_i).unwrap();
        prop_assert_eq!(de_i, b_img);

        // Resource block
        let b_res = CanonicalContentBlock::resource_text(&uri, Some(mime.clone()), &text);
        prop_assert!(b_res.is_resource());
        let ser_r = serde_json::to_string(&b_res).unwrap();
        let de_r: CanonicalContentBlock = serde_json::from_str(&ser_r).unwrap();
        prop_assert_eq!(de_r, b_res);
    }

    #[test]
    fn prop_execution_metadata_serde_and_fields(
        duration in any::<u64>(),
        level in 0u8..=3u8,
        fuel in proptest::option::of(any::<u64>()),
        mem in proptest::option::of(any::<usize>()),
        audit_id in proptest::option::of("\\PC*")
    ) {
        let tier = RiskTier::from_level(level).unwrap();
        let mut meta = ExecutionMetadata::new(duration, tier);
        if let Some(f) = fuel {
            meta = meta.with_fuel(f);
        }
        if let Some(m) = mem {
            meta = meta.with_memory(m);
        }
        if let Some(ref a) = audit_id {
            meta = meta.with_audit_record_id(a);
        }
        let ser = serde_json::to_string(&meta).unwrap();
        let de: ExecutionMetadata = serde_json::from_str(&ser).unwrap();
        prop_assert_eq!(de, meta);
    }

    #[test]
    fn prop_canonical_tool_result_serde_and_invariants(
        call_id in "\\PC*",
        text in "\\PC*",
        is_error in any::<bool>()
    ) {
        let meta = ExecutionMetadata::default();
        let result = if is_error {
            CanonicalToolResult::error(&call_id, &text, meta)
        } else {
            CanonicalToolResult::success_text(&call_id, &text, meta)
        };
        prop_assert_eq!(result.is_success(), !is_error);
        prop_assert_eq!(result.is_error, is_error);
        prop_assert_eq!(result.text_summary(), text);
        let ser = serde_json::to_string(&result).unwrap();
        let de: CanonicalToolResult = serde_json::from_str(&ser).unwrap();
        prop_assert_eq!(de, result);
    }

    #[test]
    fn prop_capability_manifest_and_descriptor_validation_never_panics(
        id in "\\PC*",
        name in "\\PC*",
        desc in "\\PC*",
        author in "\\PC*"
    ) {
        let version = semver::Version::new(1, 0, 0);
        let manifest = CapabilityManifest::new(
            &id,
            &name,
            version,
            &desc,
            &author,
            CapabilityType::Native,
        );
        let val_res = manifest.validate();
        if val_res.is_ok() {
            let descriptor = manifest.to_descriptor();
            prop_assert!(descriptor.validate().is_ok());
            let ser = serde_json::to_string(&manifest).unwrap();
            let de: CapabilityManifest = serde_json::from_str(&ser).unwrap();
            prop_assert_eq!(de, manifest);
        }
    }
}
