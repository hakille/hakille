use crate::error::ValidationError;
use serde_json::Value;

pub const MAX_IDENTIFIER_LEN: usize = 128;
pub const MAX_TOOL_NAME_LEN: usize = 64;

/// Validates a capability identifier (e.g. "hakille.fs", "mcp:github/repos", "native:system").
pub fn validate_capability_id(id: &str) -> Result<(), ValidationError> {
    if id.is_empty() {
        return Err(ValidationError::EmptyField {
            field: "id",
            context: "capability identifier".to_string(),
        });
    }

    if id.len() > MAX_IDENTIFIER_LEN {
        return Err(ValidationError::InvalidIdentifier {
            id: id.to_string(),
            reason: format!("Identifier exceeds maximum length of {MAX_IDENTIFIER_LEN} chars"),
        });
    }

    let is_valid = id.chars().all(|c| {
        c.is_ascii_alphanumeric() || c == '.' || c == '/' || c == '_' || c == '-' || c == ':'
    });

    if !is_valid {
        return Err(ValidationError::InvalidIdentifier {
            id: id.to_string(),
            reason: "Identifier contains invalid characters (allowed: [a-zA-Z0-9._-:/])"
                .to_string(),
        });
    }

    Ok(())
}

/// Validates a tool name (e.g. "read_file", "search_web", "query_database").
pub fn validate_tool_name(name: &str) -> Result<(), ValidationError> {
    if name.is_empty() {
        return Err(ValidationError::EmptyField {
            field: "name",
            context: "tool name".to_string(),
        });
    }

    if name.len() > MAX_TOOL_NAME_LEN {
        return Err(ValidationError::InvalidToolName {
            name: name.to_string(),
            reason: format!("Tool name exceeds maximum length of {MAX_TOOL_NAME_LEN} chars"),
        });
    }

    let is_valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');

    if !is_valid {
        return Err(ValidationError::InvalidToolName {
            name: name.to_string(),
            reason: "Tool name contains invalid characters (allowed: [a-zA-Z0-9_-])".to_string(),
        });
    }

    Ok(())
}

/// Validates a JSON Schema draft 2020-12 parameters definition.
pub fn validate_parameters_schema(
    tool_name: &str,
    parameters: &Value,
    strict: bool,
) -> Result<(), ValidationError> {
    let schema_obj = match parameters {
        Value::Object(map) => map,
        _ => {
            return Err(ValidationError::InvalidParametersSchema {
                tool_name: tool_name.to_string(),
                reason: "Parameters schema must be a JSON Object".to_string(),
            });
        }
    };

    // Verify root type is "object"
    match schema_obj.get("type") {
        Some(Value::String(t)) if t == "object" => {}
        Some(other) => {
            return Err(ValidationError::InvalidParametersSchema {
                tool_name: tool_name.to_string(),
                reason: format!("Root 'type' must be \"object\", found: {other}"),
            });
        }
        None => {
            return Err(ValidationError::InvalidParametersSchema {
                tool_name: tool_name.to_string(),
                reason: "Root schema is missing required 'type': \"object\" declaration"
                    .to_string(),
            });
        }
    }

    // Check properties if declared
    let properties_map = if let Some(props) = schema_obj.get("properties") {
        match props {
            Value::Object(map) => Some(map),
            _ => {
                return Err(ValidationError::InvalidParametersSchema {
                    tool_name: tool_name.to_string(),
                    reason: "'properties' must be a JSON Object".to_string(),
                });
            }
        }
    } else {
        None
    };

    // Check required array if declared
    let mut required_set = std::collections::HashSet::new();
    if let Some(req) = schema_obj.get("required") {
        match req {
            Value::Array(arr) => {
                for item in arr {
                    match item {
                        Value::String(s) => {
                            required_set.insert(s.clone());
                        }
                        _ => {
                            return Err(ValidationError::InvalidParametersSchema {
                                tool_name: tool_name.to_string(),
                                reason: "'required' array must contain only strings".to_string(),
                            });
                        }
                    }
                }
            }
            _ => {
                return Err(ValidationError::InvalidParametersSchema {
                    tool_name: tool_name.to_string(),
                    reason: "'required' must be a JSON Array".to_string(),
                });
            }
        }
    }

    // Strict mode validation rules (OpenAI strict mode compatibility)
    if strict {
        // Rule 1: additionalProperties MUST be boolean false
        match schema_obj.get("additionalProperties") {
            Some(Value::Bool(false)) => {}
            _ => {
                return Err(ValidationError::StrictModeViolation {
                    tool_name: tool_name.to_string(),
                    reason: "Strict mode requires 'additionalProperties: false' at root level"
                        .to_string(),
                });
            }
        }

        // Rule 2: All properties in 'properties' must be in 'required'
        if let Some(props) = properties_map {
            for prop_name in props.keys() {
                if !required_set.contains(prop_name) {
                    return Err(ValidationError::StrictModeViolation {
                        tool_name: tool_name.to_string(),
                        reason: format!(
                            "Strict mode requires property '{prop_name}' to be listed in 'required'"
                        ),
                    });
                }
            }

            // Recurse into nested objects for strict mode
            for (prop_name, prop_val) in props {
                if let Value::Object(nested_map) = prop_val {
                    let is_nested_obj = nested_map.get("type").and_then(|t| t.as_str())
                        == Some("object")
                        || (nested_map
                            .get("type")
                            .and_then(|t| t.as_array())
                            .map(|arr| arr.iter().any(|v| v.as_str() == Some("object")))
                            .unwrap_or(false))
                        || nested_map.contains_key("properties");
                    if is_nested_obj && nested_map.contains_key("properties") {
                        validate_nested_strict_object(tool_name, prop_name, nested_map)?;
                    } else if let Some(Value::Object(items_map)) = nested_map.get("items") {
                        let is_items_obj = items_map.get("type").and_then(|t| t.as_str())
                            == Some("object")
                            || (items_map
                                .get("type")
                                .and_then(|t| t.as_array())
                                .map(|arr| arr.iter().any(|v| v.as_str() == Some("object")))
                                .unwrap_or(false))
                            || items_map.contains_key("properties");
                        if is_items_obj && items_map.contains_key("properties") {
                            validate_nested_strict_object(
                                tool_name,
                                &format!("{prop_name}.items"),
                                items_map,
                            )?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn validate_nested_strict_object(
    tool_name: &str,
    field_name: &str,
    map: &serde_json::Map<String, Value>,
) -> Result<(), ValidationError> {
    match map.get("additionalProperties") {
        Some(Value::Bool(false)) => {}
        _ => {
            return Err(ValidationError::StrictModeViolation {
                tool_name: tool_name.to_string(),
                reason: format!(
                    "Strict mode requires 'additionalProperties: false' in nested object '{field_name}'"
                ),
            });
        }
    }

    let mut nested_req = std::collections::HashSet::new();
    if let Some(Value::Array(reqs)) = map.get("required") {
        for r in reqs {
            if let Some(s) = r.as_str() {
                nested_req.insert(s);
            }
        }
    }

    if let Some(Value::Object(props)) = map.get("properties") {
        for prop_key in props.keys() {
            if !nested_req.contains(prop_key.as_str()) {
                return Err(ValidationError::StrictModeViolation {
                    tool_name: tool_name.to_string(),
                    reason: format!(
                        "Strict mode requires nested property '{field_name}.{prop_key}' to be listed in 'required'"
                    ),
                });
            }
        }

        // Recursively validate all child objects (depth >= 3)
        for (prop_key, prop_val) in props {
            if let Value::Object(child_map) = prop_val {
                let is_child_obj = child_map.get("type").and_then(|t| t.as_str()) == Some("object")
                    || (child_map
                        .get("type")
                        .and_then(|t| t.as_array())
                        .map(|arr| arr.iter().any(|v| v.as_str() == Some("object")))
                        .unwrap_or(false))
                    || child_map.contains_key("properties");
                if is_child_obj && child_map.contains_key("properties") {
                    let child_field = format!("{field_name}.{prop_key}");
                    validate_nested_strict_object(tool_name, &child_field, child_map)?;
                } else if let Some(Value::Object(items_map)) = child_map.get("items") {
                    let is_items_obj = items_map.get("type").and_then(|t| t.as_str())
                        == Some("object")
                        || (items_map
                            .get("type")
                            .and_then(|t| t.as_array())
                            .map(|arr| arr.iter().any(|v| v.as_str() == Some("object")))
                            .unwrap_or(false))
                        || items_map.contains_key("properties");
                    if is_items_obj && items_map.contains_key("properties") {
                        let items_field = format!("{field_name}.{prop_key}.items");
                        validate_nested_strict_object(tool_name, &items_field, items_map)?;
                    }
                }
            }
        }
    }

    // Also validate objects inside array items if declared
    if let Some(Value::Object(items_map)) = map.get("items") {
        let is_items_obj = items_map.get("type").and_then(|t| t.as_str()) == Some("object")
            || (items_map
                .get("type")
                .and_then(|t| t.as_array())
                .map(|arr| arr.iter().any(|v| v.as_str() == Some("object")))
                .unwrap_or(false))
            || items_map.contains_key("properties");
        if is_items_obj && items_map.contains_key("properties") {
            let items_field = format!("{field_name}.items");
            validate_nested_strict_object(tool_name, &items_field, items_map)?;
        }
    }

    Ok(())
}
