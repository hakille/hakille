use crate::providers::{
    ProviderId, require_network_capability, require_network_capability_with_context,
};
use hakille_core::{
    AuthorizationContext, CapabilityAuthority, CapabilityToken, Cost, HakilleError, Result,
    TrustLevel,
};
use hakille_inference::GgufInferenceEngine;
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
    provider: ProviderId,
    cost_per_1k_tokens_cents: u64,
    inference_engine: Option<GgufInferenceEngine>,
    /// Timeout LLM configurable (ms). Défaut 180s (décodage CPU 0.5B Q4_K_M) ;
    /// override via `HAKILLE_QWEN_TIMEOUT_MS`.
    llm_timeout_ms: u64,
}

/// Timeout LLM par défaut (10 min : décodage CPU 0.5B Q4_K_M en debug,
/// 2-5 min en release pour 256 tokens prod / 8 en test).
pub const DEFAULT_S2_LLM_TIMEOUT_MS: u64 = 600_000;

fn resolve_s2_timeout_ms(configured: u64) -> u64 {
    std::env::var("HAKILLE_QWEN_TIMEOUT_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(configured)
}

impl System2Engine {
    pub fn new(provider_name: impl Into<String>, cost_per_1k_tokens_cents: u64) -> Self {
        Self {
            provider_name: provider_name.into(),
            provider: ProviderId::LocalGguf,
            cost_per_1k_tokens_cents,
            inference_engine: None,
            llm_timeout_ms: DEFAULT_S2_LLM_TIMEOUT_MS,
        }
    }

    pub fn with_inference_engine(
        provider_name: impl Into<String>,
        cost_per_1k_tokens_cents: u64,
        inference_engine: GgufInferenceEngine,
    ) -> Self {
        Self {
            provider_name: provider_name.into(),
            provider: ProviderId::LocalGguf,
            cost_per_1k_tokens_cents,
            inference_engine: Some(inference_engine),
            llm_timeout_ms: DEFAULT_S2_LLM_TIMEOUT_MS,
        }
    }

    /// Legs : moteur d'inspection seul (gabarits). Conservé pour compatibilité ;
    /// la génération RÉELLE utilise le singleton `QwenEngine` (lazy, candle).
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
        resolve_s2_timeout_ms(self.llm_timeout_ms)
    }

    /// Découpe honnête en phrases (heuristique, PAS un segmentateur NLP) :
    /// - séparateurs toujours actifs : `;` et `\n`
    /// - `. ? !` ne coupent que si suivis d'espace, de fin de chaîne ou de
    ///   guillemet/parenthèse fermante (évite `M.`, `14h30`, `v1.2`) ;
    /// - chaque morceau est trimmé, les vides filtrés, max 8 segments.
    ///
    /// Exemple : "Bonjour. Tu vas bien ? Oui !" → 3 segments.
    pub fn split_into_phrases(prompt: &str) -> Vec<String> {
        let mut segments = Vec::new();
        let mut current = String::new();
        let chars: Vec<char> = prompt.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == ';' || c == '\n' {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    segments.push(trimmed);
                }
                current.clear();
                // Saute les séparateurs répétés / espaces qui suivent.
                i += 1;
                continue;
            }
            if matches!(c, '.' | '?' | '!') {
                // Regarde le caractère suivant pour décider si c'est une vraie
                // frontière de phrase (heuristique anti-abréviations).
                let next = chars.get(i + 1).copied();
                let is_boundary = match next {
                    None => true,
                    Some(n) if n.is_whitespace() => true,
                    Some('"') | Some('\'') | Some('»') | Some(')') | Some(']') => true,
                    _ => false,
                };
                current.push(c);
                if is_boundary {
                    let trimmed = current.trim().to_string();
                    if !trimmed.is_empty() {
                        segments.push(trimmed);
                    }
                    current.clear();
                }
                i += 1;
                continue;
            }
            current.push(c);
            i += 1;
        }
        let tail = current.trim().to_string();
        if !tail.is_empty() {
            segments.push(tail);
        }
        if segments.is_empty() {
            let fallback = prompt.trim().to_string();
            if !fallback.is_empty() {
                return vec![fallback];
            }
            return vec![];
        }
        segments.truncate(8);
        segments
    }

    /// Bascule le provider effectif (sans casser `new`). Local par défaut.
    pub fn with_provider(&mut self, provider: ProviderId) -> &mut Self {
        self.provider = provider;
        self
    }

    pub fn provider(&self) -> &ProviderId {
        &self.provider
    }
    /// Garde par défaut : refuse tout provider Cloud sans capability
    /// Network explicite + consentement. Local-only passe toujours.
    pub async fn reason(
        &self,
        prompt: &str,
        context_triplets: &[String],
        trust_level: &TrustLevel,
    ) -> Result<System2Plan> {
        if self.provider.is_cloud() {
            return Err(HakilleError::CapabilityDenied(
                "cloud provider requires explicit Network capability + consent".to_string(),
            ));
        }
        self.reason_inner(prompt, context_triplets, trust_level)
            .await
    }

    /// Chemin Cloud explicite : exige une capability Network/Execute valide
    /// via [`require_network_capability`] (token épinglé + expiry + scope,
    /// `Err` explicite sinon). Threadé depuis
    /// `CognitiveOrchestrator::process_user_turn_with_authorization` avec
    /// consentement UX par requête ; préférer un token atténué par requête
    /// (scope Network + Execute, expiry courte).
    pub async fn reason_with_network_capability(
        &self,
        prompt: &str,
        context_triplets: &[String],
        trust_level: &TrustLevel,
        authority: &CapabilityAuthority,
        network_capability: Option<&CapabilityToken>,
    ) -> Result<System2Plan> {
        if self.provider.is_cloud() {
            require_network_capability(authority, network_capability)?;
        }
        self.reason_inner(prompt, context_triplets, trust_level)
            .await
    }

    /// Compatibilité de signature : aucune autorisation n'est créée ici.
    /// Un fournisseur distant sans jeton explicite est toujours refusé.
    pub async fn reason_with_network_capability_or_short(
        &self,
        prompt: &str,
        context_triplets: &[String],
        trust_level: &TrustLevel,
        authority: &CapabilityAuthority,
        network_capability: Option<&CapabilityToken>,
    ) -> Result<System2Plan> {
        self.reason_with_network_capability(
            prompt,
            context_triplets,
            trust_level,
            authority,
            network_capability,
        )
        .await
    }

    /// Variante avec [`AuthorizationContext`] : valide le contexte (audience,
    /// expiry, ressource, quota, nonce, délégation), exige la couverture
    /// Network/Execute quand le provider est Cloud, puis raisonne. `Err`
    /// explicite à chaque refus (contexte invalide, audience mismatch, token
    /// expiré, scope insuffisant).
    // Huit paramètres explicites : le contexte et les capabilities restent séparés.
    #[allow(clippy::too_many_arguments)]
    pub async fn reason_with_authorization(
        &self,
        prompt: &str,
        context_triplets: &[String],
        trust_level: &TrustLevel,
        ctx: &AuthorizationContext,
        authority: &CapabilityAuthority,
        network_capability: Option<&CapabilityToken>,
        expected_audience: &str,
    ) -> Result<System2Plan> {
        ctx.validate().map_err(|e| {
            HakilleError::CapabilityDenied(format!("contexte d'autorisation invalide: {e}"))
        })?;
        ctx.check_audience(expected_audience)?;
        if self.provider.is_cloud() {
            require_network_capability_with_context(
                authority,
                network_capability,
                ctx,
                expected_audience,
            )?;
        }
        self.reason_inner(prompt, context_triplets, trust_level)
            .await
    }

    async fn reason_inner(
        &self,
        prompt: &str,
        context_triplets: &[String],
        trust_level: &TrustLevel,
    ) -> Result<System2Plan> {
        // Le seul adaptateur réellement branché ici est GGUF. Une sélection
        // distante ou Ollama ne doit jamais être présentée comme exécutée
        // alors qu'une génération GGUF locale lui a été substituée.
        if self.provider != ProviderId::LocalGguf {
            return Err(HakilleError::InferenceError(format!(
                "Adaptateur d'inférence '{}' non connecté au moteur System 2",
                self.provider.display_name()
            )));
        }
        let infer_start = std::time::Instant::now();
        let mut trace = Vec::new();
        trace.push(format!(
            "Activated System 2 via provider '{}'",
            self.provider_name
        ));
        trace.push(format!("Evaluating trust level: {:?}", trust_level));

        if !context_triplets.is_empty() {
            trace.push(format!(
                "Contexte : {} éléments cités (faits, épisodes ou sources)",
                context_triplets.len()
            ));
        }

        // Décomposition honnête en phrases (amélioration légère, PAS un LLM) :
        // on segmente sur les frontières de phrases fortes (`. ? !` suivis
        // d'espace/majuscule ou fin), plus `;` et retours ligne toujours
        // séparateurs. Chaque segment est trimmé, les vides filtrés, max 8.
        // Limite 8 = compromis lisibilité/coût : au-delà, la trace devient
        // du bruit et le coût estimé explose. Ce n'est PAS un vrai
        // planificateur — voir DeliberatePlanner pour le plan exécutable.
        let effective_segments = Self::split_into_phrases(prompt);
        for (i, seg) in effective_segments.iter().enumerate() {
            let short = seg.chars().take(120).collect::<String>();
            trace.push(format!(
                "Decomposed step {}/{}: '{}'",
                i + 1,
                effective_segments.len(),
                short
            ));
        }

        let mut steps: Vec<String> = effective_segments
            .iter()
            .enumerate()
            .map(|(i, seg)| {
                let short = seg.chars().take(120).collect::<String>();
                format!("{}. Traiter le segment : '{}'", i + 1, short)
            })
            .collect();
        steps.push(format!(
            "{}. Vérifier les contraintes constitutionnelles et synthétiser la réponse bornée",
            steps.len() + 1
        ));

        // REAL-04 — synthèse RÉELLE via singleton Qwen (lazy, quantifié candle,
        // CPU, ChatML réel, sampling multinomial T=0.7/top_p=0.9). AUCUN gabarit :
        // moteur indisponible → Err HONNÊTE, jamais de phrase de secours.
        // `cfg!(test)` : 8 tokens (rapide, toujours réel) ; prod : 256.
        let deep_prompt = crate::memory::grounded_prompt(prompt, context_triplets);
        let max_tokens = crate::system1::resolve_max_tokens();
        let timeout_ms = self.llm_timeout_ms();
        let prompt_owned = deep_prompt.clone();
        let deadline =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms));
        let system = crate::instructions::system_instructions(
            crate::instructions::ReasoningMode::Deliberate,
        );
        let handle = tokio::task::spawn_blocking(move || {
            let mut engine = hakille_inference::global_qwen_engine()?;
            engine.generate_with_system_deadline(
                &prompt_owned,
                &system,
                max_tokens,
                0.0,
                1.0,
                deadline,
            )
        });
        let gen_out = tokio::time::timeout(std::time::Duration::from_millis(timeout_ms.saturating_add(30_000)), handle)
            .await
            .map_err(|_| {
                HakilleError::InferenceError(format!(
                    "System2 LLM timeout après {} ms (décodage CPU Qwen 0.5B Q4_K_M ; augmentez HAKILLE_QWEN_TIMEOUT_MS)",
                    timeout_ms
                ))
            })?
            .map_err(|e| {
                HakilleError::InferenceError(format!("System2 tâche LLM interrompue : {}", e))
            })??;
        if gen_out.text.is_empty() {
            tracing::warn!("System2 génération réelle vide (EOS immédiat) — propagée telle quelle");
        }
        let answer = gen_out.text.clone();
        // Comptes tokens RÉELS (tokenizer BPE, pas len/4) pour le coût.
        let total_tokens = (gen_out.prompt_tokens + gen_out.tokens_generated).max(1);

        let infer_latency = infer_start.elapsed();

        // Heuristique non calibrée du modèle uniquement. Le noyau combine
        // les faits une seule fois ; un nombre de citations ne prouve rien.
        let confidence_score = 0.75;
        // Aucun fournisseur payant n'est appelé par ce chemin local.
        let cost_cents = 0;
        let _configured_remote_rate = self.cost_per_1k_tokens_cents;
        trace.push(format!(
            "Qwen local : {} tokens mesurés, {} ms ; étapes proposées non exécutées",
            total_tokens,
            infer_latency.as_millis()
        ));

        Ok(System2Plan {
            reasoning_trace: trace,
            proposed_steps: steps,
            synthesized_answer: answer,
            confidence_score,
            cost_incurred: Cost {
                amount_cents: cost_cents,
                currency: "EUR".to_string(),
                description: format!(
                    "System 2 run via {} ({} tokens, {} ms)",
                    self.provider_name,
                    total_tokens,
                    infer_latency.as_millis()
                ),
            },
        })
    }
}

#[cfg(test)]
mod provider_gate_tests {
    use super::*;
    use crate::providers::CloudProvider;
    use hakille_core::{CapabilityAction, CapabilityScope};

    #[tokio::test]
    async fn optional_token_path_never_creates_network_authority() {
        let mut engine = System2Engine::new("test", 2);
        engine.with_provider(ProviderId::Cloud(CloudProvider::OpenAI));
        let authority = CapabilityAuthority::new();
        let error = engine
            .reason_with_network_capability_or_short(
                "test",
                &[],
                &TrustLevel::User,
                &authority,
                None,
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("explicit Network capability"));
    }

    #[tokio::test]
    async fn ollama_selection_never_silently_uses_gguf() {
        let mut engine = System2Engine::new("test", 0);
        engine.with_provider(ProviderId::LocalOllama);
        let error = engine
            .reason("test", &[], &TrustLevel::User)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("non connecté"));
    }

    #[tokio::test]
    #[ignore = "slow: décodage Qwen CPU réel (minutes en debug) ; gate lente `-- --ignored`"]
    async fn test_system2_local_only_passes() {
        // Timeout aligné sur le décodage Qwen CPU réel (debug) : le défaut
        // 600 s peut être dépassé par une génération saine sur CPU partagé.
        let engine = System2Engine::new("TestSlot", 2).with_llm_timeout(3_600_000);
        let plan = engine.reason("test", &[], &TrustLevel::User).await;
        assert!(plan.is_ok(), "reason réelle a échoué: {:?}", plan.err());
    }

    #[tokio::test]
    async fn test_system2_cloud_refused_by_default() {
        let mut engine = System2Engine::new("TestSlot", 2);
        engine.with_provider(ProviderId::Cloud(CloudProvider::OpenAI));
        let err = engine
            .reason("test", &[], &TrustLevel::User)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("explicit Network capability"));
    }

    #[tokio::test]
    async fn test_system2_authorized_cloud_requires_real_adapter() {
        let authority = CapabilityAuthority::new();
        let expiry = hakille_core::Timestamp(chrono::Utc::now() + chrono::Duration::hours(1));
        let token = authority
            .create_root_token(
                vec![CapabilityScope::Network],
                vec![CapabilityAction::Execute],
                expiry,
            )
            .unwrap();
        let mut engine = System2Engine::new("TestSlot", 2);
        // Timeout aligné sur le décodage Qwen CPU réel (debug) — cf. test local.
        engine.set_llm_timeout(3_600_000);
        engine.with_provider(ProviderId::Cloud(CloudProvider::OpenAI));
        let plan = engine
            .reason_with_network_capability(
                "test",
                &[],
                &TrustLevel::User,
                &authority,
                Some(&token),
            )
            .await;
        assert!(plan.unwrap_err().to_string().contains("non connecté"));
    }
}
