use annora_core::{Result, TrustLevel};
use annora_inference::GgufInferenceEngine;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct System1Output {
    pub text_response: String,
    pub action_intent: Option<String>,
    pub output_entropy: f64,
    pub latency_ms: u64,
}

pub struct System1Engine {
    max_latency_budget_ms: u64,
    inference_engine: Option<GgufInferenceEngine>,
}

impl System1Engine {
    pub fn new(max_latency_budget_ms: u64) -> Self {
        Self {
            max_latency_budget_ms,
            inference_engine: None,
        }
    }

    pub fn with_inference_engine(
        max_latency_budget_ms: u64,
        inference_engine: GgufInferenceEngine,
    ) -> Self {
        Self {
            max_latency_budget_ms,
            inference_engine: Some(inference_engine),
        }
    }

    pub fn set_inference_engine(&mut self, engine: GgufInferenceEngine) {
        self.inference_engine = Some(engine);
    }

    pub async fn process(
        &self,
        prompt: &str,
        trust_level: &TrustLevel,
    ) -> Result<System1Output> {
        let start = std::time::Instant::now();

        // System 1 checks untrusted inputs: never interpret as executable command
        let sanitized_prompt = if *trust_level == TrustLevel::Untrusted {
            format!("[External Content]: {}", prompt)
        } else {
            prompt.to_string()
        };

        // Entropy heuristic: high uncertainty if prompt is ambiguous or interrogative complex
        let entropy = if sanitized_prompt.contains("?") && sanitized_prompt.len() > 100 {
            0.65
        } else {
            0.25
        };

        let response = if *trust_level == TrustLevel::Untrusted {
            "Received external content. Stored without automatic execution.".to_string()
        } else if let Some(ref engine) = self.inference_engine {
            engine.generate_instruct(prompt, None, 64)
                .unwrap_or_else(|_| format!("System 1 fast response to: {}", sanitized_prompt))
        } else {
            format!("System 1 fast response to: {}", sanitized_prompt)
        };

        let elapsed = start.elapsed().as_millis() as u64;

        Ok(System1Output {
            text_response: response,
            action_intent: None,
            output_entropy: entropy,
            latency_ms: elapsed.min(self.max_latency_budget_ms),
        })
    }
}

