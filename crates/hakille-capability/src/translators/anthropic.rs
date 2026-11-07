use crate::translators::{ProviderTranslator, TranslatorError};
use crate::types::{
    CanonicalContentBlock, CanonicalToolCall, CanonicalToolDefinition, CanonicalToolResult,
    ExecutionMetadata,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnthropicToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnthropicToolCall {
    #[serde(rename = "type")]
    pub block_type: String, // Always "tool_use"
    pub id: String,
    pub name: String,
    pub input: Value, // Directly a JSON object
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnthropicToolResult {
    #[serde(rename = "type")]
    pub block_type: String, // Always "tool_result"
    pub tool_use_id: String,
    pub content: AnthropicToolResultContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnthropicToolResultContent {
    Text(String),
    Blocks(Vec<AnthropicContentBlock>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: AnthropicImageSource },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnthropicImageSource {
    #[serde(rename = "type")]
    pub source_type: String, // Always "base64"
    pub media_type: String,
    pub data: String,
}

pub struct AnthropicTranslator;

impl AnthropicTranslator {
    pub fn sanitize_tool_name(name: &str) -> String {
        let sanitized: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .take(64)
            .collect();
        if sanitized.is_empty() {
            "unnamed_tool".to_string()
        } else {
            sanitized
        }
    }
}

impl ProviderTranslator for AnthropicTranslator {
    type ProviderToolDef = AnthropicToolDefinition;
    type ProviderToolCall = AnthropicToolCall;
    type ProviderToolResult = AnthropicToolResult;

    fn to_provider_tool(
        tool: &CanonicalToolDefinition,
    ) -> Result<Self::ProviderToolDef, TranslatorError> {
        let name = Self::sanitize_tool_name(&tool.name);
        Ok(AnthropicToolDefinition {
            name,
            description: tool.description.clone(),
            input_schema: tool.parameters.clone(),
        })
    }

    fn from_provider_tool(
        raw: &Self::ProviderToolDef,
    ) -> Result<CanonicalToolDefinition, TranslatorError> {
        Ok(CanonicalToolDefinition {
            name: raw.name.clone(),
            description: raw.description.clone(),
            parameters: raw.input_schema.clone(),
            strict: false,
        })
    }

    fn parse_tool_call(
        call: &Self::ProviderToolCall,
    ) -> Result<CanonicalToolCall, TranslatorError> {
        if call.block_type != "tool_use" {
            return Err(TranslatorError::MissingField {
                field: "type == tool_use",
                context: format!("Anthropic tool call block '{}'", call.name),
            });
        }

        // Defensive normalization: if input is delivered as a JSON string, parse it
        let arguments = match &call.input {
            Value::String(s) => serde_json::from_str(s).unwrap_or_else(|_| json!({ "raw": s })),
            other => other.clone(),
        };

        Ok(CanonicalToolCall {
            call_id: call.id.clone(),
            tool_name: call.name.clone(),
            arguments,
        })
    }

    fn encode_tool_call(
        call: &CanonicalToolCall,
    ) -> Result<Self::ProviderToolCall, TranslatorError> {
        Ok(AnthropicToolCall {
            block_type: "tool_use".to_string(),
            id: call.call_id.clone(),
            name: Self::sanitize_tool_name(&call.tool_name),
            input: call.arguments.clone(),
        })
    }

    fn encode_tool_result(
        result: &CanonicalToolResult,
    ) -> Result<Self::ProviderToolResult, TranslatorError> {
        let mut blocks = Vec::new();

        for block in &result.content {
            match block {
                CanonicalContentBlock::Text { text } => {
                    blocks.push(AnthropicContentBlock::Text { text: text.clone() });
                }
                CanonicalContentBlock::Image {
                    mime_type,
                    data_base64,
                } => {
                    blocks.push(AnthropicContentBlock::Image {
                        source: AnthropicImageSource {
                            source_type: "base64".to_string(),
                            media_type: mime_type.clone(),
                            data: data_base64.clone(),
                        },
                    });
                }
                CanonicalContentBlock::Resource {
                    uri,
                    text,
                    blob_base64,
                    mime_type,
                } => {
                    if let Some(data) = blob_base64 {
                        let media = mime_type
                            .clone()
                            .unwrap_or_else(|| "application/octet-stream".to_string());
                        blocks.push(AnthropicContentBlock::Image {
                            source: AnthropicImageSource {
                                source_type: "base64".to_string(),
                                media_type: media,
                                data: data.clone(),
                            },
                        });
                    } else if let Some(t) = text {
                        blocks.push(AnthropicContentBlock::Text {
                            text: format!("[Resource ({uri})]: {t}"),
                        });
                    } else {
                        blocks.push(AnthropicContentBlock::Text {
                            text: format!("[Resource ({uri})]"),
                        });
                    }
                }
            }
        }

        let content = if blocks.len() == 1 {
            if let AnthropicContentBlock::Text { ref text } = blocks[0] {
                AnthropicToolResultContent::Text(text.clone())
            } else {
                AnthropicToolResultContent::Blocks(blocks)
            }
        } else {
            AnthropicToolResultContent::Blocks(blocks)
        };

        Ok(AnthropicToolResult {
            block_type: "tool_result".to_string(),
            tool_use_id: result.call_id.clone(),
            content,
            is_error: if result.is_error { Some(true) } else { None },
        })
    }

    fn parse_tool_result(
        raw: &Self::ProviderToolResult,
    ) -> Result<CanonicalToolResult, TranslatorError> {
        let is_error = raw.is_error.unwrap_or(false);
        let mut content = Vec::new();

        match &raw.content {
            AnthropicToolResultContent::Text(t) => {
                content.push(CanonicalContentBlock::Text { text: t.clone() });
            }
            AnthropicToolResultContent::Blocks(blocks) => {
                for b in blocks {
                    match b {
                        AnthropicContentBlock::Text { text } => {
                            content.push(CanonicalContentBlock::Text { text: text.clone() });
                        }
                        AnthropicContentBlock::Image { source } => {
                            content.push(CanonicalContentBlock::Image {
                                mime_type: source.media_type.clone(),
                                data_base64: source.data.clone(),
                            });
                        }
                    }
                }
            }
        }

        Ok(CanonicalToolResult {
            call_id: raw.tool_use_id.clone(),
            content,
            is_error,
            metadata: ExecutionMetadata::default(),
        })
    }
}
