//! Declarative Agent Skills Adapter & Progressive Context Loader.
//!
//! Implements parsing of `SKILL.md` specifications with YAML frontmatter,
//! 3-tier progressive loading (Catalog -> Instruction Injection -> Resource Fetching),
//! and path traversal security guards.

use async_trait::async_trait;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::CapabilityError;
use crate::execution::{CapabilityExecutor, ExecutionContext};
use crate::types::{
    CanonicalContentBlock, CanonicalToolCall, CanonicalToolDefinition, CanonicalToolResult,
    CapabilityManifest, CapabilityScope, CapabilityType, ExecutionMetadata, RiskTier,
};

/// Represents an inspected, validated declarative skill.
#[derive(Debug, Clone)]
pub struct SkillDescriptor {
    /// Canonical skill identifier (e.g. "postgres_dba").
    pub name: String,
    /// Human-friendly display title.
    pub display_name: String,
    /// SemVer version.
    pub version: semver::Version,
    /// Short overview for vector/keyword catalog discovery (~40 tokens).
    pub description: String,
    /// JSON Schema for skill input parameters.
    pub parameters: Value,
    /// Required capability scopes (e.g. Filesystem, Network).
    pub scopes_required: Vec<CapabilityScope>,
    /// Full procedural markdown instructions.
    pub instructions_markdown: String,
    /// Optional embedded or bundled resource files (scripts, SQL schemas, templates).
    pub resources: HashMap<String, Vec<u8>>,
}

impl SkillDescriptor {
    /// Generates a `CanonicalToolDefinition` representing this skill in model prompts.
    pub fn to_canonical_tool(&self) -> CanonicalToolDefinition {
        CanonicalToolDefinition::new(
            self.name.clone(),
            format!("{}: {}", self.display_name, self.description),
            self.parameters.clone(),
        )
    }

    /// Generates a full `CapabilityManifest` for registration in the Universal Registry.
    pub fn to_manifest(&self) -> CapabilityManifest {
        let mut manifest = CapabilityManifest::new(
            format!("skill.{}", self.name),
            self.display_name.clone(),
            self.version.clone(),
            self.description.clone(),
            "Agent Skills Ecosystem",
            CapabilityType::Skill,
        );

        manifest = manifest.with_tool(self.to_canonical_tool());
        for scope in &self.scopes_required {
            manifest = manifest.with_scope(scope.clone());
        }

        manifest.with_metadata("tags", json!(["skill", "heuristics", self.name]))
    }
}

/// Robust parser for `SKILL.md` format.
pub struct SkillParser;

impl SkillParser {
    /// Parses a complete `SKILL.md` file content with YAML frontmatter.
    pub fn parse_skill_markdown(content: &str) -> Result<SkillDescriptor, CapabilityError> {
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            return Err(CapabilityError::Custom(
                "SKILL.md must begin with '---' YAML frontmatter delimiter".to_string(),
            ));
        }

        let after_first_delimiter = &trimmed[3..];
        let end_delimiter_idx = after_first_delimiter.find("\n---").ok_or_else(|| {
            CapabilityError::Custom("Unclosed YAML frontmatter in SKILL.md".to_string())
        })?;

        let frontmatter_str = &after_first_delimiter[..end_delimiter_idx];
        let body_markdown = &after_first_delimiter[end_delimiter_idx + 4..].trim();

        let mut name = String::new();
        let mut display_name = String::new();
        let mut version_str = "1.0.0".to_string();
        let mut description = String::new();
        let mut scopes = Vec::new();

        // Line-based YAML key-value parsing
        for line in frontmatter_str.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, val)) = line.split_once(':') {
                let key = key.trim();
                let val = val.trim().trim_matches('"').trim_matches('\'');

                match key {
                    "name" => name = val.to_string(),
                    "displayName" | "display_name" => display_name = val.to_string(),
                    "version" => version_str = val.to_string(),
                    "description" => description = val.to_string(),
                    "scope" => match val.to_ascii_lowercase().as_str() {
                        "filesystem" => scopes.push(CapabilityScope::Filesystem),
                        "network" => scopes.push(CapabilityScope::Network),
                        "gui_input" => scopes.push(CapabilityScope::GuiInput),
                        "wallet" => scopes.push(CapabilityScope::Wallet),
                        "audio" => scopes.push(CapabilityScope::Audio),
                        "camera" => scopes.push(CapabilityScope::Camera),
                        "fullsystem" | "full_system" => scopes.push(CapabilityScope::FullSystem),
                        unknown => {
                            return Err(CapabilityError::Custom(format!(
                                "Unknown capability scope '{unknown}' in SKILL.md"
                            )));
                        }
                    },
                    _ => {}
                }
            }
        }

        if name.is_empty() {
            return Err(CapabilityError::Custom(
                "SKILL.md frontmatter missing 'name' field".to_string(),
            ));
        }

        if display_name.is_empty() {
            display_name = name.clone();
        }

        let version = semver::Version::parse(&version_str).map_err(|e| {
            CapabilityError::Custom(format!("Invalid version '{version_str}' in SKILL.md: {e}"))
        })?;

        let default_parameters = json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Specific action requested under this skill"
                },
                "input": {
                    "type": "string",
                    "description": "Context or input query"
                }
            },
            "required": ["action"]
        });

        Ok(SkillDescriptor {
            name,
            display_name,
            version,
            description,
            parameters: default_parameters,
            scopes_required: scopes,
            instructions_markdown: body_markdown.to_string(),
            resources: HashMap::new(),
        })
    }
}

/// 3-Tier Progressive Skills Loader.
#[derive(Clone, Default)]
pub struct ProgressiveSkillsLoader {
    skills: Arc<RwLock<HashMap<String, SkillDescriptor>>>,
}

impl ProgressiveSkillsLoader {
    /// Creates a new skills loader.
    pub fn new() -> Self {
        Self {
            skills: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Registers a parsed skill descriptor into the loader.
    pub async fn add_skill(&self, skill: SkillDescriptor) {
        let mut lock = self.skills.write().await;
        lock.insert(skill.name.clone(), skill);
    }

    /// Loads and registers a skill directly from markdown content.
    pub async fn load_skill_from_str(&self, content: &str) -> Result<String, CapabilityError> {
        let skill = SkillParser::parse_skill_markdown(content)?;
        let name = skill.name.clone();
        self.add_skill(skill).await;
        Ok(name)
    }

    /// Adds a bundled resource to an existing skill with path traversal protection.
    pub async fn attach_resource(
        &self,
        skill_name: &str,
        resource_path: &str,
        data: Vec<u8>,
    ) -> Result<(), CapabilityError> {
        if resource_path.contains("..")
            || resource_path.starts_with('/')
            || resource_path.starts_with('\\')
        {
            return Err(CapabilityError::SecurityViolation {
                reason: format!("Path traversal rejected in skill resource: '{resource_path}'"),
            });
        }

        let mut lock = self.skills.write().await;
        let skill = lock
            .get_mut(skill_name)
            .ok_or_else(|| CapabilityError::ToolNotFound(skill_name.to_string()))?;

        skill.resources.insert(resource_path.to_string(), data);
        Ok(())
    }

    // ========================================================================
    // Stage 1: Catalog Discovery (~40 tokens / skill)
    // ========================================================================
    /// Returns lightweight catalog definitions for all loaded skills.
    pub async fn catalog_definitions(&self) -> Vec<CanonicalToolDefinition> {
        let lock = self.skills.read().await;
        lock.values().map(|s| s.to_canonical_tool()).collect()
    }

    // ========================================================================
    // Stage 2: Runtime Knowledge Injection (Heuristic prompt injection)
    // ========================================================================
    /// Injects full Markdown instructions when a skill is activated.
    pub async fn inject_instructions(&self, skill_name: &str) -> Option<String> {
        let lock = self.skills.read().await;
        lock.get(skill_name).map(|s| {
            format!(
                "## Skill: {} (v{})\n\n{}\n",
                s.display_name, s.version, s.instructions_markdown
            )
        })
    }

    // ========================================================================
    // Stage 3: Lazy Resource Fetching (Bounded sandbox access)
    // ========================================================================
    /// Fetches a bundled resource file on demand with path traversal protection.
    pub async fn fetch_resource(
        &self,
        skill_name: &str,
        resource_path: &str,
    ) -> Result<Vec<u8>, CapabilityError> {
        if resource_path.contains("..")
            || resource_path.starts_with('/')
            || resource_path.starts_with('\\')
            || resource_path.contains('\\')
        {
            return Err(CapabilityError::SecurityViolation {
                reason: format!("Path traversal in resource fetch: '{resource_path}'"),
            });
        }

        let lock = self.skills.read().await;
        let skill = lock
            .get(skill_name)
            .ok_or_else(|| CapabilityError::ToolNotFound(skill_name.to_string()))?;

        skill.resources.get(resource_path).cloned().ok_or_else(|| {
            CapabilityError::ExecutionFailed {
                tool_name: skill_name.to_string(),
                message: format!("Resource '{resource_path}' not found in skill '{skill_name}'"),
            }
        })
    }
}

#[async_trait]
impl CapabilityExecutor for ProgressiveSkillsLoader {
    async fn execute(
        &self,
        call: &CanonicalToolCall,
        _context: &ExecutionContext,
    ) -> Result<CanonicalToolResult, CapabilityError> {
        let instructions = self
            .inject_instructions(&call.tool_name)
            .await
            .ok_or_else(|| CapabilityError::ToolNotFound(call.tool_name.clone()))?;

        let action = call
            .arguments
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("consult");

        let response_text = format!(
            "Executed skill '{}' with action '{}'. Applied cognitive directives:\n\n{}",
            call.tool_name, action, instructions
        );

        let metadata = ExecutionMetadata::new(0, RiskTier::Tier0ReadOnly);

        Ok(CanonicalToolResult::success(
            call.call_id.clone(),
            vec![CanonicalContentBlock::text(response_text)],
            metadata,
        ))
    }
}
