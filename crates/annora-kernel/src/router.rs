use annora_core::{Cost, TrustLevel};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveInput {
    pub raw_prompt: String,
    pub trust_level: TrustLevel,
    pub has_image: bool,
    pub has_audio: bool,
    pub estimated_token_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RoutingDecision {
    System1,
    System2 {
        estimated_cost: Cost,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexityScore {
    pub score: f64, // 0.0 (trivial) to 1.0 (extremely complex)
    pub explanation: String,
}

#[async_trait]
pub trait ComplexityClassifier: Send + Sync {
    async fn classify(&self, input: &CognitiveInput) -> ComplexityScore;
}

pub struct DefaultComplexityClassifier;

#[async_trait]
impl ComplexityClassifier for DefaultComplexityClassifier {
    async fn classify(&self, input: &CognitiveInput) -> ComplexityScore {
        let length_factor = (input.raw_prompt.len() as f64 / 500.0).clamp(0.0, 0.4);
        let multi_modal_factor = if input.has_image { 0.3 } else { 0.0 };
        
        // Keywords indicating complex multi-step reasoning, coding or analysis
        let lower = input.raw_prompt.to_lowercase();
        let reasoning_words = [
            "analyse", "analyze", "explain why", "prove", "contract", "legal",
            "refactor", "architect", "synthesize", "compare and contrast", "diagnose"
        ];
        let has_complex_word = reasoning_words.iter().any(|word| lower.contains(word));
        let keyword_factor = if has_complex_word { 0.4 } else { 0.0 };

        let score = (length_factor + multi_modal_factor + keyword_factor).clamp(0.0, 1.0);
        let explanation = if score > 0.5 {
            "Complex multi-step reasoning or high-depth task detected".to_string()
        } else {
            "Standard conversational or fast-path request".to_string()
        };

        ComplexityScore { score, explanation }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingPolicy {
    pub s1_threshold: f64,
    pub entropy_threshold: f64,
}

impl Default for RoutingPolicy {
    fn default() -> Self {
        Self {
            s1_threshold: 0.5,
            entropy_threshold: 0.75,
        }
    }
}

pub struct CognitiveRouter {
    classifier: Box<dyn ComplexityClassifier>,
    policy: RoutingPolicy,
}

impl CognitiveRouter {
    pub fn new(classifier: Box<dyn ComplexityClassifier>, policy: RoutingPolicy) -> Self {
        Self { classifier, policy }
    }

    pub fn with_default_classifier() -> Self {
        Self::new(Box::new(DefaultComplexityClassifier), RoutingPolicy::default())
    }

    pub async fn route(&self, input: &CognitiveInput, system1_entropy: f64) -> RoutingDecision {
        let complexity = self.classifier.classify(input).await;

        if complexity.score < self.policy.s1_threshold
            && system1_entropy < self.policy.entropy_threshold
        {
            RoutingDecision::System1
        } else {
            let tokens = input.estimated_token_count.max(input.raw_prompt.len() / 4);
            let cost_cents = (tokens as u64 / 1000).max(1) * 2; // rough estimate
            RoutingDecision::System2 {
                estimated_cost: Cost {
                    amount_cents: cost_cents,
                    currency: "EUR".to_string(),
                    description: format!("System 2 routing: {}", complexity.explanation),
                },
                reason: complexity.explanation,
            }
        }
    }
}
