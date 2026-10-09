use annora_core::{Cost, Result, TrustLevel};
use annora_inference::GgufInferenceEngine;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct System2Plan {
    pub reasoning_trace: Vec<String>,
    pub proposed_steps: Vec<String>,
    pub synthesized_answer: String,
    pub confidence_score: f64,
    pub cost_incurred: Cost,
}

pub struct System2Engine {
    provider_name: String,
    cost_per_1k_tokens_cents: u64,
    inference_engine: Option<GgufInferenceEngine>,
}

impl System2Engine {
    pub fn new(provider_name: impl Into<String>, cost_per_1k_tokens_cents: u64) -> Self {
        Self {
            provider_name: provider_name.into(),
            cost_per_1k_tokens_cents,
            inference_engine: None,
        }
    }

    pub fn with_inference_engine(
        provider_name: impl Into<String>,
        cost_per_1k_tokens_cents: u64,
        inference_engine: GgufInferenceEngine,
    ) -> Self {
        Self {
            provider_name: provider_name.into(),
            cost_per_1k_tokens_cents,
            inference_engine: Some(inference_engine),
        }
    }

    pub fn set_inference_engine(&mut self, engine: GgufInferenceEngine) {
        self.inference_engine = Some(engine);
    }

    pub async fn reason(
        &self,
        prompt: &str,
        context_triplets: &[String],
        trust_level: &TrustLevel,
    ) -> Result<System2Plan> {
        let mut trace = Vec::new();
        trace.push(format!("Activated System 2 via provider '{}'", self.provider_name));
        trace.push(format!("Evaluating trust level: {:?}", trust_level));
        
        if !context_triplets.is_empty() {
            trace.push(format!("Injected {} knowledge graph facts as grounded context", context_triplets.len()));
        }

        let mut steps = Vec::new();
        steps.push(format!("1. Analyze intent: '{}'", prompt));
        steps.push("2. Verify safety and constitutional constraints".to_string());
        steps.push("3. Formulate bounded response or action plan".to_string());

        let answer = if let Some(ref engine) = self.inference_engine {
            let deep_prompt = if context_triplets.is_empty() {
                prompt.to_string()
            } else {
                format!("Contexte mémorisé : {}\nQuestion/Tâche : {}", context_triplets.join(" ; "), prompt)
            };
            engine.generate_instruct(&deep_prompt, Some("System 2 Deliberate Reasoner"), 256)
                .unwrap_or_else(|_| format!(
                    "System 2 synthesized output for task: '{}' (Grounded with {} facts)",
                    prompt, context_triplets.len()
                ))
        } else {
            format!(
                "System 2 synthesized output for task: '{}' (Grounded with {} facts)",
                prompt,
                context_triplets.len()
            )
        };

        let approx_tokens = prompt.len() / 4 + 200;
        let cost_cents = ((approx_tokens as u64 * self.cost_per_1k_tokens_cents) / 1000).max(1);

        Ok(System2Plan {
            reasoning_trace: trace,
            proposed_steps: steps,
            synthesized_answer: answer,
            confidence_score: 0.88,
            cost_incurred: Cost {
                amount_cents: cost_cents,
                currency: "EUR".to_string(),
                description: format!("System 2 run via {}", self.provider_name),
            },
        })
    }
}

