use crate::translators::{ProviderTranslator, SchemaNormalizer, TranslatorError};
use crate::types::{
    CanonicalContentBlock, CanonicalToolCall, CanonicalToolDefinition, CanonicalToolResult,
    ExecutionMetadata,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenAiToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String, // Always "function"
    pub function: OpenAiFunctionDefinition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenAiFunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenAiToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String, // Always "function"
    pub function: OpenAiFunctionCallPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenAiFunctionCallPayload {
    pub name: String,
    pub arguments: String, // Stringified JSON
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenAiToolResult {
    pub role: String, // Always "tool"
    pub tool_call_id: String,
    pub content: String,
}

pub struct OpenAiTranslator;

impl SchemaNormalizer for OpenAiTranslator {
    fn normalize_schema(&self, schema: &mut Value, tool_name: &str) -> Result<(), TranslatorError> {
        enforce_openai_strict_schema(schema, tool_name)
    }

    fn denormalize_schema(&self, _schema: &mut Value) -> Result<(), TranslatorError> {
        Ok(())
    }
}

impl ProviderTranslator for OpenAiTranslator {
    type ProviderToolDef = OpenAiToolDefinition;
    type ProviderToolCall = OpenAiToolCall;
    type ProviderToolResult = OpenAiToolResult;

    fn to_provider_tool(
        tool: &CanonicalToolDefinition,
    ) -> Result<Self::ProviderToolDef, TranslatorError> {
        let mut parameters = tool.parameters.clone();
        if tool.strict {
            enforce_openai_strict_schema(&mut parameters, &tool.name)?;
        }

        Ok(OpenAiToolDefinition {
            tool_type: "function".to_string(),
            function: OpenAiFunctionDefinition {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters,
                strict: if tool.strict { Some(true) } else { None },
            },
        })
    }

    fn from_provider_tool(
        raw: &Self::ProviderToolDef,
    ) -> Result<CanonicalToolDefinition, TranslatorError> {
        if raw.tool_type != "function" {
            return Err(TranslatorError::SchemaTransformationError {
                tool_name: raw.function.name.clone(),
                reason: format!("Expected tool type 'function', got '{}'", raw.tool_type),
            });
        }
        Ok(CanonicalToolDefinition {
            name: raw.function.name.clone(),
            description: raw.function.description.clone(),
            parameters: raw.function.parameters.clone(),
            strict: raw.function.strict.unwrap_or(false),
        })
    }

    fn parse_tool_call(
        call: &Self::ProviderToolCall,
    ) -> Result<CanonicalToolCall, TranslatorError> {
        let trimmed = call.function.arguments.trim();
        let arguments: Value = if trimmed.is_empty() {
            json!({})
        } else {
            serde_json::from_str(trimmed).map_err(|e| TranslatorError::MalformedArguments {
                tool_name: call.function.name.clone(),
                raw_arguments: call.function.arguments.clone(),
                details: e.to_string(),
            })?
        };

        Ok(CanonicalToolCall {
            call_id: call.id.clone(),
            tool_name: call.function.name.clone(),
            arguments,
        })
    }

    fn encode_tool_call(
        call: &CanonicalToolCall,
    ) -> Result<Self::ProviderToolCall, TranslatorError> {
        let stringified_args = serde_json::to_string(&call.arguments)
            .map_err(|e| TranslatorError::SerializationError(e.to_string()))?;

        Ok(OpenAiToolCall {
            id: call.call_id.clone(),
            call_type: "function".to_string(),
            function: OpenAiFunctionCallPayload {
                name: call.tool_name.clone(),
                arguments: stringified_args,
            },
        })
    }

    fn encode_tool_result(
        result: &CanonicalToolResult,
    ) -> Result<Self::ProviderToolResult, TranslatorError> {
        let mut text_parts = Vec::new();

        if result.is_error {
            text_parts.push("[ERROR]".to_string());
        }

        for block in &result.content {
            match block {
                CanonicalContentBlock::Text { text } => text_parts.push(text.clone()),
                CanonicalContentBlock::Image {
                    mime_type,
                    data_base64,
                } => {
                    text_parts.push(format!(
                        "[Multimodal Image ({mime_type}): data:{mime_type};base64,{data_base64}]"
                    ));
                }
                CanonicalContentBlock::Resource { uri, text, .. } => {
                    if let Some(t) = text {
                        text_parts.push(format!("[Resource ({uri})]: {t}"));
                    } else {
                        text_parts.push(format!("[Resource ({uri})]"));
                    }
                }
            }
        }

        let content = if text_parts.is_empty() {
            if result.is_error {
                "{\"error\": \"Execution failed with no message\"}".to_string()
            } else {
                "{\"status\": \"success\"}".to_string()
            }
        } else {
            text_parts.join("\n")
        };

        Ok(OpenAiToolResult {
            role: "tool".to_string(),
            tool_call_id: result.call_id.clone(),
            content,
        })
    }

    fn parse_tool_result(
        raw: &Self::ProviderToolResult,
    ) -> Result<CanonicalToolResult, TranslatorError> {
        let (is_error, clean_content) = if raw.content == "[ERROR]" {
            (true, String::new())
        } else if let Some(stripped) = raw.content.strip_prefix("[ERROR]\n") {
            (true, stripped.to_string())
        } else if raw.content.starts_with("{\"error\":") && raw.content.ends_with('}') {
            (true, raw.content.clone())
        } else {
            (false, raw.content.clone())
        };

        Ok(CanonicalToolResult {
            call_id: raw.tool_call_id.clone(),
            content: vec![CanonicalContentBlock::Text {
                text: clean_content,
            }],
            is_error,
            metadata: ExecutionMetadata::default(),
        })
    }
}

/// Recursively enforces OpenAI Strict Mode requirements on JSON Schema.
pub fn enforce_openai_strict_schema(
    schema: &mut Value,
    _tool_name: &str,
) -> Result<(), TranslatorError> {
    if let Value::Object(map) = schema {
        // Strip prohibited strict mode keywords
        map.remove("patternProperties");
        map.remove("minProperties");
        map.remove("maxProperties");

        let is_object = map.get("type").and_then(|t| t.as_str()) == Some("object")
            || map.contains_key("properties");

        if is_object {
            map.insert("additionalProperties".to_string(), Value::Bool(false));

            let mut current_required: Vec<String> =
                if let Some(Value::Array(reqs)) = map.get("required") {
                    reqs.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                } else {
                    Vec::new()
                };

            if let Some(Value::Object(props)) = map.get_mut("properties") {
                let declared_keys: Vec<String> = props.keys().cloned().collect();
                for prop_name in &declared_keys {
                    if let Some(prop_val) = props.get_mut(prop_name) {
                        if !current_required.contains(prop_name) {
                            make_nullable(prop_val);
                            current_required.push(prop_name.clone());
                        }
                        enforce_openai_strict_schema(prop_val, _tool_name)?;
                    }
                }
            }

            current_required.sort();
            current_required.dedup();
            map.insert(
                "required".to_string(),
                Value::Array(current_required.into_iter().map(Value::String).collect()),
            );
        }

        // Recurse into array items
        if let Some(items) = map.get_mut("items") {
            match items {
                Value::Array(arr) => {
                    for item in arr {
                        enforce_openai_strict_schema(item, _tool_name)?;
                    }
                }
                _ => {
                    enforce_openai_strict_schema(items, _tool_name)?;
                }
            }
        }

        // Recurse into schema combinators (anyOf, oneOf, allOf)
        for combinator in ["anyOf", "oneOf", "allOf"] {
            if let Some(Value::Array(variants)) = map.get_mut(combinator) {
                for variant in variants {
                    enforce_openai_strict_schema(variant, _tool_name)?;
                }
            }
        }

        // Recurse into defs
        if let Some(Value::Object(defs_map)) = map.get_mut("$defs") {
            for def_schema in defs_map.values_mut() {
                enforce_openai_strict_schema(def_schema, _tool_name)?;
            }
        } else if let Some(Value::Object(defs_map)) = map.get_mut("definitions") {
            for def_schema in defs_map.values_mut() {
                enforce_openai_strict_schema(def_schema, _tool_name)?;
            }
        }
    }
    Ok(())
}

fn make_nullable(schema: &mut Value) {
    if let Value::Object(map) = schema {
        if let Some(type_val) = map.get_mut("type") {
            match type_val {
                Value::String(s) => {
                    if s != "null" {
                        *type_val = json!([s.clone(), "null"]);
                    }
                }
                Value::Array(arr) if !arr.iter().any(|v| v.as_str() == Some("null")) => {
                    arr.push(Value::String("null".to_string()));
                }
                _ => {}
            }
        } else if let Some(Value::Array(arr)) = map.get_mut("anyOf") {
            if !arr
                .iter()
                .any(|v| v.get("type").and_then(|t| t.as_str()) == Some("null"))
            {
                arr.push(json!({"type": "null"}));
            }
        } else {
            map.insert("type".to_string(), json!(["string", "null"]));
        }
    }
}
