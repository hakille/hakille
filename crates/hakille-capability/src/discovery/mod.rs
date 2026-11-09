//! Hybrid Capability Discovery Engine.
//!
//! Executes a two-phase search pipeline:
//! 1. Phase 1: Structured Pre-Filtering (Active lifecycle, scope authorization, risk tier ceiling, tags, capability types).
//! 2. Phase 2: Semantic Vector Ranking (384-dimensional dense cosine similarity with deterministic tie-breaking).
//! 3. Dynamic Prompt Budget Optimizer: Greedy packing of discovered definitions within `token_budget`.

use crate::registry::UniversalCapabilityRegistry;
use crate::types::{CanonicalToolDefinition, CapabilityType, RiskTier};
use hakille_core::CapabilityToken;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

/// Discovery errors.
#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("Registry error during discovery: {0}")]
    Registry(String),

    #[error("Embedding generation failed: {0}")]
    Embedding(String),

    #[error("Invalid query: {0}")]
    InvalidQuery(String),
}

/// Incoming discovery query parameters.
#[derive(Clone, Debug)]
pub struct DiscoveryQuery {
    /// Natural language user request or task description.
    pub query_text: String,
    /// Optional 384-dimensional query embedding vector.
    pub query_embedding: Option<Vec<f32>>,
    /// Caller Biscuit capability token for scope pre-filtering.
    pub caller_token: Option<CapabilityToken>,
    /// Upper bound risk tier allowed for this discovery request.
    pub max_risk_tier: Option<RiskTier>,
    /// Allowed capability runtime types.
    pub allowed_types: Option<HashSet<CapabilityType>>,
    /// Required capability metadata tags (all must match).
    pub required_tags: Option<HashSet<String>>,
    /// Maximum number of tools to return.
    pub top_k: usize,
    /// Minimum cosine similarity score threshold (e.g., 0.65).
    pub min_similarity: f32,
    /// Maximum LLM prompt token budget for injected tool definitions.
    pub token_budget: usize,
}

impl Default for DiscoveryQuery {
    fn default() -> Self {
        Self {
            query_text: String::new(),
            query_embedding: None,
            caller_token: None,
            max_risk_tier: None,
            allowed_types: None,
            required_tags: None,
            top_k: 5,
            min_similarity: 0.65,
            token_budget: 1000,
        }
    }
}

impl DiscoveryQuery {
    /// Creates a new query with query text.
    pub fn new(query_text: impl Into<String>) -> Self {
        Self {
            query_text: query_text.into(),
            ..Default::default()
        }
    }

    /// Sets query embedding.
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.query_embedding = Some(embedding);
        self
    }

    /// Sets maximum risk tier ceiling.
    pub fn with_max_risk_tier(mut self, tier: RiskTier) -> Self {
        self.max_risk_tier = Some(tier);
        self
    }

    /// Sets token budget.
    pub fn with_token_budget(mut self, budget: usize) -> Self {
        self.token_budget = budget;
        self
    }

    /// Sets top_k tools limit.
    pub fn with_top_k(mut self, top_k: usize) -> Self {
        self.top_k = top_k;
        self
    }

    /// Sets minimum similarity threshold.
    pub fn with_min_similarity(mut self, min_similarity: f32) -> Self {
        self.min_similarity = min_similarity;
        self
    }
}

/// Discovered tool with ranking telemetry and token footprint.
#[derive(Clone, Debug, PartialEq)]
pub struct DiscoveredTool {
    /// Canonical tool definition.
    pub tool: CanonicalToolDefinition,
    /// Parent capability ID exposing this tool.
    pub capability_id: String,
    /// Evaluated cosine similarity score (0.0 to 1.0).
    pub similarity_score: f32,
    /// Estimated prompt token consumption.
    pub estimated_tokens: usize,
    /// Evaluated risk tier.
    pub evaluated_risk: RiskTier,
}

/// Hybrid discovery engine combining structured security filters with semantic vector ranking.
pub struct CapabilityDiscoveryEngine {
    registry: UniversalCapabilityRegistry,
}

impl CapabilityDiscoveryEngine {
    /// Creates a new discovery engine over the universal capability registry.
    pub fn new(registry: UniversalCapabilityRegistry) -> Self {
        Self { registry }
    }

    /// Returns a reference to the underlying capability registry.
    pub fn registry(&self) -> &UniversalCapabilityRegistry {
        &self.registry
    }

    /// Executes the two-phase discovery and budget optimization pipeline.
    pub async fn discover(
        &self,
        query: &DiscoveryQuery,
        tool_embeddings: &HashMap<String, Vec<f32>>,
    ) -> Result<Vec<DiscoveredTool>, DiscoveryError> {
        let active_capabilities = self.registry.list_active().await;

        // Phase 1: Structured Pre-Filtering
        let mut candidates = Vec::new();
        for cap in active_capabilities {
            // Scope check: If caller token provided, verify validity
            if let Some(token) = &query.caller_token
                && token.is_expired()
            {
                continue;
            }

            // CapabilityType filter
            if let Some(types) = &query.allowed_types
                && !types.contains(&cap.capability_type)
            {
                continue;
            }

            // Required tags filter
            if let Some(req_tags) = &query.required_tags {
                let cap_tags: HashSet<String> = cap
                    .metadata
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|t| t.as_str().map(|s| s.to_ascii_lowercase()))
                            .collect()
                    })
                    .unwrap_or_default();

                if !req_tags
                    .iter()
                    .all(|t| cap_tags.contains(&t.to_ascii_lowercase()))
                {
                    continue;
                }
            }

            for tool in cap.tools {
                // RiskTier ceiling filter
                let estimated_risk = infer_tool_risk(&tool);
                if let Some(max_risk) = query.max_risk_tier
                    && estimated_risk > max_risk
                {
                    continue;
                }

                candidates.push((cap.id.clone(), tool, estimated_risk));
            }
        }

        // Phase 2: Semantic Vector Ranking
        let mut scored_candidates = Vec::new();
        if let Some(q_vec) = &query.query_embedding {
            for (cap_id, tool, risk) in candidates {
                if let Some(doc_vec) = tool_embeddings.get(&tool.name) {
                    let sim = cosine_similarity(q_vec, doc_vec);
                    if sim >= query.min_similarity {
                        scored_candidates.push((sim, cap_id, tool, risk));
                    }
                }
            }
            // Deterministic sort: similarity descending, then tool name ascending
            scored_candidates.sort_by(|a, b| {
                b.0.partial_cmp(&a.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.2.name.cmp(&b.2.name))
            });
        } else {
            // Lexical fallback when query embeddings are omitted
            let query_words: Vec<String> = query
                .query_text
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_ascii_lowercase())
                .collect();

            if query_words.is_empty() {
                for (cap_id, tool, risk) in candidates {
                    scored_candidates.push((1.0, cap_id, tool, risk));
                }
                scored_candidates.sort_by(|a, b| a.2.name.cmp(&b.2.name));
            } else {
                for (cap_id, tool, risk) in candidates {
                    let text = format!("{} {}", tool.name, tool.description).to_ascii_lowercase();
                    let matches = query_words
                        .iter()
                        .filter(|w| text.contains(w.as_str()))
                        .count();
                    let score = (matches as f32) / (query_words.len() as f32);
                    if score >= query.min_similarity {
                        scored_candidates.push((score, cap_id, tool, risk));
                    }
                }
                scored_candidates.sort_by(|a, b| {
                    b.0.partial_cmp(&a.0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.2.name.cmp(&b.2.name))
                });
            }
        }

        // Dynamic Prompt Budget Optimizer (Greedy Packing)
        let mut accumulated_tokens = 0;
        let mut selected_tools = Vec::new();

        for (score, cap_id, tool, risk) in scored_candidates {
            if selected_tools.len() >= query.top_k {
                break;
            }
            let tool_tokens = estimate_tool_tokens(&tool);
            if accumulated_tokens + tool_tokens > query.token_budget {
                continue; // Skip tool if it exceeds remaining budget
            }

            accumulated_tokens += tool_tokens;
            selected_tools.push(DiscoveredTool {
                tool,
                capability_id: cap_id,
                similarity_score: score,
                estimated_tokens: tool_tokens,
                evaluated_risk: risk,
            });
        }

        Ok(selected_tools)
    }
}

/// Computes cosine similarity between two dense embedding vectors.
///
/// Returns 0.0 if vector lengths mismatch, vectors are empty, or any dot product or norm
/// is non-finite or zero.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if !dot.is_finite() || !norm_a.is_finite() || !norm_b.is_finite() {
        return 0.0;
    }
    let denom = norm_a * norm_b;
    if denom <= 0.0 || !denom.is_finite() {
        return 0.0;
    }
    let sim = dot / denom;
    if !sim.is_finite() {
        return 0.0;
    }
    sim.clamp(-1.0, 1.0)
}

/// Heuristically estimates the LLM prompt token consumption of a tool definition.
///
/// Formulation:
/// - 18 tokens baseline tool declaration overhead.
/// - ceil(name_len / 3.8) + ceil(desc_len / 3.8)
/// - ceil(schema_json_len / 3.2)
pub fn estimate_tool_tokens(tool: &CanonicalToolDefinition) -> usize {
    let overhead = 18;
    let name_tokens = (tool.name.len() as f32 / 3.8).ceil() as usize;
    let desc_tokens = (tool.description.len() as f32 / 3.8).ceil() as usize;
    let schema_json = serde_json::to_string(&tool.parameters).unwrap_or_default();
    let schema_tokens = (schema_json.len() as f32 / 3.2).ceil() as usize;
    overhead + name_tokens + desc_tokens + schema_tokens
}

/// Infers the default risk tier of a canonical tool based on semantic keywords in its name and description.
pub fn infer_tool_risk(tool: &CanonicalToolDefinition) -> RiskTier {
    let name = tool.name.to_lowercase();
    let desc = tool.description.to_lowercase();

    if name.contains("exec")
        || name.contains("shell")
        || name.contains("bash")
        || name.contains("delete")
        || name.contains("destroy")
        || name.contains("rm")
        || name.contains("sudo")
        || desc.contains("financial")
        || desc.contains("transfer")
        || desc.contains("biometric")
    {
        RiskTier::Tier3CriticalExecution
    } else if name.contains("http")
        || name.contains("fetch")
        || name.contains("download")
        || name.contains("curl")
        || name.contains("socket")
        || name.contains("api")
        || desc.contains("network")
        || desc.contains("outbound")
    {
        RiskTier::Tier2ExternalNetwork
    } else if name.contains("write")
        || name.contains("create")
        || name.contains("update")
        || name.contains("set")
        || name.contains("modify")
        || name.contains("save")
        || desc.contains("mutate")
        || desc.contains("state")
    {
        RiskTier::Tier1StateMutating
    } else {
        RiskTier::Tier0ReadOnly
    }
}
