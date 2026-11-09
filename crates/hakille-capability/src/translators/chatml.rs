use crate::translators::{ProviderTranslator, TranslatorError};
use crate::types::{
    CanonicalContentBlock, CanonicalToolCall, CanonicalToolDefinition, CanonicalToolResult,
    ExecutionMetadata,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMlToolCallBlock {
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMlToolResponseBlock {
    pub name: String,
    pub content: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

pub struct ChatMlTranslator;

impl ChatMlTranslator {
    /// Formats the complete <tools> block for injection into ChatML system prompt.
    pub fn format_system_tools_prompt(
        tools: &[CanonicalToolDefinition],
    ) -> Result<String, TranslatorError> {
        let serialized = serde_json::to_string_pretty(tools)
            .map_err(|e| TranslatorError::SerializationError(e.to_string()))?;
        Ok(format!(
            "# Tools\nYou have access to the following capabilities:\n<tools>\n{serialized}\n</tools>\n\n\
            To invoke a capability, format your response as:\n\
            <tool_call>\n{{\"name\": \"capability_name\", \"arguments\": {{...}}}}\n</tool_call>\n"
        ))
    }

    /// Extracts all tool calls from a raw local LLM generation string.
    pub fn parse_generation_stream(text: &str) -> Result<Vec<CanonicalToolCall>, TranslatorError> {
        let mut calls = Vec::new();
        let mut search_idx = 0;

        while let Some(start_tag) = text[search_idx..].find("<tool_call>") {
            let actual_start = search_idx + start_tag + "<tool_call>".len();
            if let Some(end_tag) = text[actual_start..].find("</tool_call>") {
                let actual_end = actual_start + end_tag;
                let raw_block = text[actual_start..actual_end].trim();
                let clean_json = Self::clean_codeblock_delimiters(raw_block);

                let parsed: Value = serde_json::from_str(&clean_json).map_err(|e| {
                    TranslatorError::ChatMlDelimiterError {
                        details: format!(
                            "Failed to parse JSON inside <tool_call>: {e} in '{clean_json}'"
                        ),
                    }
                })?;

                let name = parsed
                    .get("name")
                    .or_else(|| parsed.get("tool_name"))
                    .or_else(|| parsed.get("tool"))
                    .or_else(|| parsed.get("function"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| TranslatorError::MissingField {
                        field: "name",
                        context: format!("<tool_call> payload: {clean_json}"),
                    })?;

                let args = parsed
                    .get("arguments")
                    .or_else(|| parsed.get("parameters"))
                    .or_else(|| parsed.get("input"))
                    .or_else(|| parsed.get("args"))
                    .cloned()
                    .unwrap_or_else(|| json!({}));

                // Handle stringified arguments inside ChatML
                let arguments = match args {
                    Value::String(s) => {
                        serde_json::from_str(&s).unwrap_or_else(|_| json!({ "raw": s }))
                    }
                    other => other,
                };

                calls.push(CanonicalToolCall {
                    call_id: format!("call_chatml_{}_{}", name, Uuid::now_v7().simple()),
                    tool_name: name.to_string(),
                    arguments,
                });

                search_idx = actual_end + "</tool_call>".len();
            } else {
                break;
            }
        }

        Ok(calls)
    }

    /// Strips markdown code blocks like ```json ... ``` inside <tool_call> tags case-insensitively,
    /// handling uppercase language tags (e.g. ```JSON) and other language tags (e.g. ```python).
    fn clean_codeblock_delimiters(raw: &str) -> String {
        let mut s = raw.trim();
        if s.starts_with("```") {
            if let Some(pos) = s.find('\n') {
                s = &s[pos + 1..];
            } else {
                s = s.strip_prefix("```").unwrap_or(s);
                if s.to_ascii_lowercase().starts_with("json") {
                    s = &s[4..];
                }
            }
        }
        if s.ends_with("```") {
            s = s.strip_suffix("```").unwrap_or(s);
        }
        s.trim().to_string()
    }

    /// Formats a CanonicalToolResult into the <tool_response> format.
    pub fn format_tool_response(result: &CanonicalToolResult) -> String {
        let mut text_parts = Vec::new();
        for block in &result.content {
            match block {
                CanonicalContentBlock::Text { text } => text_parts.push(text.clone()),
                CanonicalContentBlock::Image {
                    mime_type,
                    data_base64,
                } => {
                    text_parts.push(format!("[Image: {mime_type}, {data_base64}]"));
                }
                CanonicalContentBlock::Resource { uri, text, .. } => {
                    if let Some(t) = text {
                        text_parts.push(format!("[Resource ({uri})]: {t}"));
                    }
                }
            }
        }

        let payload = json!({
            "call_id": result.call_id,
            "content": text_parts.join("\n"),
            "is_error": result.is_error,
        });

        format!(
            "<tool_response>\n{}\n</tool_response>",
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        )
    }
}

impl ProviderTranslator for ChatMlTranslator {
    type ProviderToolDef = CanonicalToolDefinition;
    type ProviderToolCall = ChatMlToolCallBlock;
    type ProviderToolResult = ChatMlToolResponseBlock;

    fn to_provider_tool(
        tool: &CanonicalToolDefinition,
    ) -> Result<Self::ProviderToolDef, TranslatorError> {
        Ok(tool.clone())
    }

    fn from_provider_tool(
        raw: &Self::ProviderToolDef,
    ) -> Result<CanonicalToolDefinition, TranslatorError> {
        Ok(raw.clone())
    }

    fn parse_tool_call(
        call: &Self::ProviderToolCall,
    ) -> Result<CanonicalToolCall, TranslatorError> {
        Ok(CanonicalToolCall {
            call_id: format!("call_chatml_{}_{}", call.name, Uuid::now_v7().simple()),
            tool_name: call.name.clone(),
            arguments: call.arguments.clone(),
        })
    }

    fn encode_tool_call(
        call: &CanonicalToolCall,
    ) -> Result<Self::ProviderToolCall, TranslatorError> {
        Ok(ChatMlToolCallBlock {
            name: call.tool_name.clone(),
            arguments: call.arguments.clone(),
        })
    }

    fn encode_tool_result(
        result: &CanonicalToolResult,
    ) -> Result<Self::ProviderToolResult, TranslatorError> {
        Ok(ChatMlToolResponseBlock {
            name: result.call_id.clone(),
            content: json!({
                "content": result.text_summary(),
            }),
            is_error: if result.is_error { Some(true) } else { None },
        })
    }

    fn parse_tool_result(
        raw: &Self::ProviderToolResult,
    ) -> Result<CanonicalToolResult, TranslatorError> {
        let is_error = raw.is_error.unwrap_or(false);
        let text_content = match &raw.content {
            Value::String(s) => s.clone(),
            Value::Object(map) => {
                if let Some(c) = map.get("content").and_then(|v| v.as_str()) {
                    c.to_string()
                } else {
                    raw.content.to_string()
                }
            }
            other => other.to_string(),
        };

        Ok(CanonicalToolResult {
            call_id: raw.name.clone(),
            content: vec![CanonicalContentBlock::Text { text: text_content }],
            is_error,
            metadata: ExecutionMetadata::default(),
        })
    }
}
