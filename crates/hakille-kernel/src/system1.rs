use hakille_core::{HakilleError, Result, TrustLevel};
use hakille_inference::GgufInferenceEngine;
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
    /// Timeout LLM configurable (ms). Défaut 180s (décodage CPU 0.5B Q4_K_M) ;
    /// override via `HAKILLE_QWEN_TIMEOUT_MS`. Séparé du `max_latency_budget_ms`
    /// (fast-path historique 100ms, conservé pour compatibilité mais le
    /// décodage réel dépasse nécessairement ce budget — latence réelle
    /// rapportée honnêtement, `warn!` si dépassement).
    llm_timeout_ms: u64,
}

/// Timeout LLM par défaut (10 min : chargement 491MB + décodage CPU 0.5B Q4_K_M
/// en debug, 2-5 min en release ; `cfg!(test)` génère 8 tokens mais reste lent
/// en debug — ne pas réduire sans mesurer).
pub const DEFAULT_LLM_TIMEOUT_MS: u64 = 600_000;

fn resolve_llm_timeout_ms(configured: u64) -> u64 {
    std::env::var("HAKILLE_QWEN_TIMEOUT_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(configured)
}

impl System1Engine {
    pub fn new(max_latency_budget_ms: u64) -> Self {
        Self {
            max_latency_budget_ms,
            inference_engine: None,
            llm_timeout_ms: DEFAULT_LLM_TIMEOUT_MS,
        }
    }

    pub fn with_inference_engine(
        max_latency_budget_ms: u64,
        inference_engine: GgufInferenceEngine,
    ) -> Self {
        Self {
            max_latency_budget_ms,
            inference_engine: Some(inference_engine),
            llm_timeout_ms: DEFAULT_LLM_TIMEOUT_MS,
        }
    }

    /// Legs : moteur d'inspection seul (GGUF gabarits). Conservé pour
    /// compatibilité `CognitiveOrchestrator::set_inference_engine` ; la
    /// génération RÉELLE utilise désormais le singleton `QwenEngine`
    /// (lazy, quantifié candle). Le champ legacy n'est plus consulté pour
    /// générer (documenté, pas de double moteur silencieux).
    pub fn set_inference_engine(&mut self, engine: GgufInferenceEngine) {
        self.inference_engine = Some(engine);
    }

    /// Timeout LLM configurable (chaînable).
    pub fn with_llm_timeout(mut self, timeout_ms: u64) -> Self {
        self.llm_timeout_ms = timeout_ms;
        self
    }

    pub fn set_llm_timeout(&mut self, timeout_ms: u64) {
        self.llm_timeout_ms = timeout_ms;
    }

    pub fn llm_timeout_ms(&self) -> u64 {
        resolve_llm_timeout_ms(self.llm_timeout_ms)
    }

    /// Estimation d'entropie System 1, documentée comme `1 − confiance
    /// heuristique` (mission 5) : faute de logits du modèle local, on approxime
    /// l'incertitude par des indices linguistiques observables.
    /// - requête courte affirmative ⇒ 0.25 (confiant, fast-path) ;
    /// - interrogative OU longue (> 100 caractères) ⇒ 0.45 (incertain) ;
    /// - interrogative ET longue ⇒ 0.65 (très incertain, routage System 2).
    ///
    /// Heuristique assumée en attendant les log-probabilités réelles du moteur
    /// GGUF ; seuils alignés sur `RoutingPolicy.entropy_threshold` (0.75).
    pub fn entropy_estimate(&self, prompt: &str) -> f64 {
        let interrogative = prompt.contains('?');
        let long = prompt.len() > 100;
        match (interrogative, long) {
            (true, true) => 0.65,
            (true, false) | (false, true) => 0.45,
            (false, false) => 0.25,
        }
    }

    pub async fn process(&self, prompt: &str, trust_level: &TrustLevel) -> Result<System1Output> {
        let start = std::time::Instant::now();

        // System 1 checks untrusted inputs: never interpret as executable command
        let sanitized_prompt = if *trust_level == TrustLevel::Untrusted {
            format!("[External Content]: {}", prompt)
        } else {
            prompt.to_string()
        };

        // Entropie via l'estimateur unique (1 − confiance heuristique, voir ci-dessus).
        let entropy = self.entropy_estimate(&sanitized_prompt);

        // Garde sécurité : contenu non fiable stocké sans exécution ni appel LLM.
        if *trust_level == TrustLevel::Untrusted {
            let elapsed = start.elapsed().as_millis() as u64;
            return Ok(System1Output {
                text_response: "Received external content. Stored without automatic execution."
                    .to_string(),
                action_intent: None,
                output_entropy: entropy,
                latency_ms: elapsed,
            });
        }

        // REAL-04 — génération RÉELLE via singleton Qwen (lazy, quantifié candle,
        // CPU, ChatML réel, sampling greedy T=0 déterministe pour le fast-path).
        // AUCUN gabarit : si le moteur est indisponible (GGUF/tokenizer absent,
        // timeout, forward en échec) → Err HONNÊTE, jamais de phrase de secours.
        // `cfg!(test)` : 8 tokens (rapide, toujours réel) ; prod : 64.
        let max_tokens = if cfg!(test) { 8 } else { 64 };
        let timeout_ms = self.llm_timeout_ms();
        let prompt_owned = prompt.to_string();
        let handle = tokio::task::spawn_blocking(move || {
            let mut engine = hakille_inference::global_qwen_engine()?;
            engine.generate(&prompt_owned, max_tokens, 0.0, 1.0)
        });
        let gen_out = tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), handle)
            .await
            .map_err(|_| {
                HakilleError::InferenceError(format!(
                    "System1 LLM timeout après {} ms (décodage CPU Qwen 0.5B Q4_K_M ; augmentez HAKILLE_QWEN_TIMEOUT_MS)",
                    timeout_ms
                ))
            })?
            .map_err(|e| {
                HakilleError::InferenceError(format!("System1 tâche LLM interrompue : {}", e))
            })??;

        // Vide honnête : le moteur a loggé `warn!` (EOS immédiat) ; on propage
        // vide sans inventer (jamais de gabarit de secours).
        if gen_out.text.is_empty() {
            tracing::warn!("System1 génération réelle vide (EOS immédiat) — propagée telle quelle");
        }
        let elapsed = start.elapsed().as_millis() as u64;
        if elapsed > self.max_latency_budget_ms {
            tracing::warn!(
                elapsed_ms = elapsed,
                budget_ms = self.max_latency_budget_ms,
                engine_ms = gen_out.latency_ms,
                "System1 dépasse le budget fast-path (décodage réel CPU, honnête)"
            );
        }

        Ok(System1Output {
            text_response: gen_out.text,
            action_intent: None,
            output_entropy: entropy,
            latency_ms: elapsed,
        })
    }
}
