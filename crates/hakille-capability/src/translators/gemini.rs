use crate::translators::{ProviderTranslator, SchemaNormalizer, TranslatorError};
use crate::types::{
    CanonicalContentBlock, CanonicalToolCall, CanonicalToolDefinition, CanonicalToolResult,
    ExecutionMetadata,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeminiFunctionDeclaration {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeminiToolContainer {
    #[serde(rename = "functionDeclarations")]
    pub function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeminiFunctionCallPart {
    #[serde(rename = "functionCall")]
    pub function_call: GeminiFunctionCall,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeminiFunctionCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub args: Value, // Directly a JSON object
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeminiFunctionResponsePart {
    #[serde(rename = "functionResponse")]
    pub function_response: GeminiFunctionResponse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeminiFunctionResponse {
    pub name: String,
    pub response: Value, // Must be a JSON object
}

pub struct GeminiTranslator;

impl GeminiTranslator {
    pub fn normalize_types_to_uppercase(schema: &mut Value) {
        if let Value::Object(map) = schema {
            // Strip incompatible Draft 2020-12 tags
            map.remove("$schema");
            map.remove("$id");
            map.remove("additionalProperties");

            let mut set_nullable = false;
            if let Some(type_val) = map.get_mut("type") {
                match type_val {
                    Value::String(t) => {
                        *t = t.to_uppercase();
                    }
                    Value::Array(arr) => {
                        let has_null = arr.iter().any(|v| v.as_str() == Some("null"));
                        let non_null_type = arr
                            .iter()
                            .find_map(|v| v.as_str().filter(|&s| s != "null"))
                            .map(String::from);
                        if let Some(concrete_type) = non_null_type {
                            *type_val = Value::String(concrete_type.to_uppercase());
                            if has_null {
                                set_nullable = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
            if set_nullable {
                map.insert("nullable".to_string(), Value::Bool(true));
            }

            if let Some(Value::Object(props)) = map.get_mut("properties") {
                for prop_val in props.values_mut() {
                    Self::normalize_types_to_uppercase(prop_val);
                }
            }

            if let Some(items) = map.get_mut("items") {
                match items {
                    Value::Array(arr) => {
                        for item in arr {
                            Self::normalize_types_to_uppercase(item);
                        }
                    }
                    _ => {
                        Self::normalize_types_to_uppercase(items);
                    }
                }
            }

            // Recurse into schema combinators (anyOf, oneOf, allOf)
            for combinator in ["anyOf", "oneOf", "allOf"] {
                if let Some(Value::Array(variants)) = map.get_mut(combinator) {
                    for variant in variants {
                        Self::normalize_types_to_uppercase(variant);
                    }
                }
            }

            // Recurse into defs
            if let Some(Value::Object(defs)) = map.get_mut("$defs") {
                for def_val in defs.values_mut() {
                    Self::normalize_types_to_uppercase(def_val);
                }
            }
            if let Some(Value::Object(defs)) = map.get_mut("definitions") {
                for def_val in defs.values_mut() {
                    Self::normalize_types_to_uppercase(def_val);
                }
            }
        }
    }

    pub fn normalize_types_to_lowercase(schema: &mut Value) {
        if let Value::Object(map) = schema {
            if let Some(Value::String(t)) = map.get_mut("type") {
                *t = t.to_lowercase();
            }

            if let Some(Value::Object(props)) = map.get_mut("properties") {
                for prop_val in props.values_mut() {
                    Self::normalize_types_to_lowercase(prop_val);
                }
            }

            if let Some(items) = map.get_mut("items") {
                match items {
                    Value::Array(arr) => {
                        for item in arr {
                            Self::normalize_types_to_lowercase(item);
                        }
                    }
                    _ => {
                        Self::normalize_types_to_lowercase(items);
                    }
                }
            }

            for combinator in ["anyOf", "oneOf", "allOf"] {
                if let Some(Value::Array(variants)) = map.get_mut(combinator) {
                    for variant in variants {
                        Self::normalize_types_to_lowercase(variant);
                    }
                }
            }

            if let Some(Value::Object(defs)) = map.get_mut("$defs") {
                for def_val in defs.values_mut() {
                    Self::normalize_types_to_lowercase(def_val);
                }
            }
            if let Some(Value::Object(defs)) = map.get_mut("definitions") {
                for def_val in defs.values_mut() {
                    Self::normalize_types_to_lowercase(def_val);
                }
            }
        }
    }
}

impl SchemaNormalizer for GeminiTranslator {
    fn normalize_schema(
        &self,
        schema: &mut Value,
        _tool_name: &str,
    ) -> Result<(), TranslatorError> {
        Self::normalize_types_to_uppercase(schema);
        Ok(())
    }

    fn denormalize_schema(&self, schema: &mut Value) -> Result<(), TranslatorError> {
        Self::normalize_types_to_lowercase(schema);
        Ok(())
    }
}

impl ProviderTranslator for GeminiTranslator {
    type ProviderToolDef = GeminiFunctionDeclaration;
    type ProviderToolCall = GeminiFunctionCallPart;
    type ProviderToolResult = GeminiFunctionResponsePart;

    fn to_provider_tool(
        tool: &CanonicalToolDefinition,
    ) -> Result<Self::ProviderToolDef, TranslatorError> {
        let mut parameters = tool.parameters.clone();
        Self::normalize_types_to_uppercase(&mut parameters);

        Ok(GeminiFunctionDeclaration {
            name: tool.name.clone(),
            description: tool.description.clone(),
            parameters,
        })
    }

    fn from_provider_tool(
        raw: &Self::ProviderToolDef,
    ) -> Result<CanonicalToolDefinition, TranslatorError> {
        let mut parameters = raw.parameters.clone();
        Self::normalize_types_to_lowercase(&mut parameters);

        Ok(CanonicalToolDefinition {
            name: raw.name.clone(),
            description: raw.description.clone(),
            parameters,
            strict: false,
        })
    }

    fn parse_tool_call(
        part: &Self::ProviderToolCall,
    ) -> Result<CanonicalToolCall, TranslatorError> {
        let call = &part.function_call;
        let call_id = call
            .id
            .clone()
            .unwrap_or_else(|| format!("call_gemini_{}_{}", call.name, Uuid::now_v7().simple()));

        Ok(CanonicalToolCall {
            call_id,
            tool_name: call.name.clone(),
            arguments: call.args.clone(),
        })
    }

    fn encode_tool_call(
        call: &CanonicalToolCall,
    ) -> Result<Self::ProviderToolCall, TranslatorError> {
        Ok(GeminiFunctionCallPart {
            function_call: GeminiFunctionCall {
                id: Some(call.call_id.clone()),
                name: call.tool_name.clone(),
                args: call.arguments.clone(),
            },
        })
    }

    fn encode_tool_result(
        result: &CanonicalToolResult,
    ) -> Result<Self::ProviderToolResult, TranslatorError> {
        let mut text_parts = Vec::new();

        for block in &result.content {
            match block {
                CanonicalContentBlock::Text { text } => text_parts.push(text.clone()),
                CanonicalContentBlock::Image {
                    mime_type,
                    data_base64,
                } => {
                    text_parts.push(format!(
                        "[Image ({mime_type}): data:{mime_type};base64,{data_base64}]"
                    ));
                }
                CanonicalContentBlock::Resource { uri, text, .. } => {
                    if let Some(t) = text {
                        text_parts.push(format!("[Resource ({uri})]: {t}"));
                    }
                }
            }
        }

        let combined_text = text_parts.join("\n");
        let response_payload = if result.is_error {
            json!({
                "error": combined_text,
                "success": false,
                "is_error": true
            })
        } else {
            json!({
                "result": combined_text,
                "success": true
            })
        };

        Ok(GeminiFunctionResponsePart {
            function_response: GeminiFunctionResponse {
                name: result.call_id.clone(),
                response: response_payload,
            },
        })
    }

    fn parse_tool_result(
        raw: &Self::ProviderToolResult,
    ) -> Result<CanonicalToolResult, TranslatorError> {
        let resp = &raw.function_response.response;
        let is_error = resp
            .get("is_error")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            || resp.get("error").is_some();

        let text_content = if let Some(res) = resp.get("result").and_then(|v| v.as_str()) {
            res.to_string()
        } else if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            err.to_string()
        } else {
            resp.to_string()
        };

        Ok(CanonicalToolResult {
            call_id: raw.function_response.name.clone(),
            content: vec![CanonicalContentBlock::Text { text: text_content }],
            is_error,
            metadata: ExecutionMetadata::default(),
        })
    }
}
