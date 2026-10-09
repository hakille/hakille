use crate::budget::{LatencyTracker, PipelineBudgetConfig};
use crate::confidence::{ConfidenceEngine, ProposedAction};
use crate::contradiction::InternalContradictionEngine;
use crate::context::InteractionContext;
use crate::fabric::CognitiveFabric;
use crate::priority::PriorityEngine;
use crate::router::{CognitiveInput, CognitiveRouter, RoutingDecision};
use crate::system1::System1Engine;
use crate::system2::System2Engine;
use annora_constitution::rules::Constitution;
use annora_core::{ConfidenceDecision, Domain, Result, TrustLevel, UserId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorResult {
    pub response_text: String,
    pub routing_used: String,
    pub latency_ms: u64,
    pub confidence_decision: ConfidenceDecision,
    pub estimated_cost_cents: Option<u64>,
}

pub struct CognitiveOrchestrator {
    pub fabric: CognitiveFabric,
    pub router: CognitiveRouter,
    pub system1: System1Engine,
    pub system2: System2Engine,
    pub context: InteractionContext,
    pub confidence_engine: ConfidenceEngine,
    pub contradiction_engine: InternalContradictionEngine,
    pub priority_engine: PriorityEngine,
    pub constitution: Constitution,
    pub memory_graph: annora_memory::KnowledgeGraph,
}

impl CognitiveOrchestrator {
    pub fn new(
        fabric: CognitiveFabric,
        user_id: UserId,
        constitution: Constitution,
    ) -> Self {
        Self {
            fabric,
            router: CognitiveRouter::with_default_classifier(),
            system1: System1Engine::new(100),
            system2: System2Engine::new("DefaultSystem2Slot", 2),
            context: InteractionContext::new(user_id, 100),
            confidence_engine: ConfidenceEngine::with_default_thresholds(),
            contradiction_engine: InternalContradictionEngine::new(),
            priority_engine: PriorityEngine::new(),
            constitution,
            memory_graph: annora_memory::KnowledgeGraph::new(),
        }
    }

    pub fn set_inference_engine(&mut self, engine: annora_inference::GgufInferenceEngine) {
        self.system1.set_inference_engine(engine.clone());
        self.system2.set_inference_engine(engine);
    }

    pub fn remember(&mut self, subject: &str, predicate: &str, object: &str) -> annora_core::TripletId {
        let triplet = annora_memory::Triplet {
            id: annora_core::TripletId(uuid::Uuid::now_v7()),
            subject: annora_core::NodeId(uuid::Uuid::now_v7()),
            predicate: annora_core::PredicateId(uuid::Uuid::now_v7()),
            object: annora_memory::NodeValue::Text(format!("{} {} {}", subject, predicate, object)),
            metadata: annora_memory::TripletMetadata {
                weight: 1.0,
                created_at: annora_core::Timestamp::now(),
                last_activated: annora_core::Timestamp::now(),
                source: annora_memory::TripletSource::User,
                confidence: 1.0,
                context: annora_memory::ContextPartition::Personal,
                trust_level: annora_core::TrustLevel::User,
                decay_rate: 0.01,
            },
        };
        let id = triplet.id.clone();
        self.memory_graph.insert(triplet);
        id
    }

    pub fn recall(&self, query: &str) -> Vec<String> {
        self.memory_graph
            .search_text(query)
            .into_iter()
            .map(|t| match &t.object {
                annora_memory::NodeValue::Text(s) => s.clone(),
                _ => format!("{:?}", t.object),
            })
            .collect()
    }

    pub async fn process_user_turn(
        &mut self,
        prompt: &str,
        trust_level: &TrustLevel,
    ) -> Result<OrchestratorResult> {
        let tracker = LatencyTracker::start(PipelineBudgetConfig::default());

        // 0. Automatic memory extraction from statements
        let lower = prompt.to_lowercase();
        if (lower.contains("je m'appelle") || lower.contains("mon nom est")) && !lower.contains('?') {
            let name = prompt.split("m'appelle").nth(1).or_else(|| prompt.split("est").nth(1)).unwrap_or("").trim();
            if !name.is_empty() {
                self.remember("Utilisateur", "nom", name);
            }
        } else if lower.contains("s'appelle") && !lower.contains('?') {
            let parts: Vec<&str> = prompt.split("s'appelle").collect();
            if parts.len() == 2 {
                let subj = parts[0].trim();
                let obj = parts[1].trim();
                self.remember(subj, "s'appelle", obj);
            }
        } else if lower.starts_with("retiens que ") || lower.starts_with("enregistre que ") {
            let fact = prompt.trim_start_matches("retiens que ").trim_start_matches("enregistre que ").trim();
            self.remember("Fait", "mémorisé", fact);
        }

        // 1. Search relevant memory facts from KnowledgeGraph
        let mut relevant_facts = Vec::new();
        for word in prompt.split_whitespace() {
            let clean = word.trim_matches(|c: char| !c.is_alphanumeric());
            if clean.len() >= 3 {
                for fact in self.recall(clean) {
                    if !relevant_facts.contains(&fact) {
                        relevant_facts.push(fact);
                    }
                }
            }
        }

        // 2. Record interaction in continuous context
        self.context.record_interaction("User", prompt, vec!["user_query".to_string()]);

        // 3. Cognitive routing
        let input = CognitiveInput {
            raw_prompt: prompt.to_string(),
            trust_level: trust_level.clone(),
            has_image: false,
            has_audio: false,
            estimated_token_count: prompt.len() / 4,
        };

        let decision = self.router.route(&input, 0.2).await;

        let (base_response, routing_str, cost_opt) = match decision {
            RoutingDecision::System1 => {
                let s1_res = self.system1.process(prompt, trust_level).await?;
                (s1_res.text_response, "System 1 (Local sub-100ms)".to_string(), None)
            }
            RoutingDecision::System2 { estimated_cost, reason: _ } => {
                let s2_res = self.system2.reason(prompt, &relevant_facts, trust_level).await?;
                (
                    s2_res.synthesized_answer,
                    "System 2 (Distant deep reasoning)".to_string(),
                    Some(estimated_cost.amount_cents),
                )
            }
        };

        let response = if !relevant_facts.is_empty() && (prompt.contains('?') || lower.contains("comment") || lower.contains("quel") || lower.contains("qui")) {
            format!("D'après votre graphe de mémoire souverain : {}\n({})", relevant_facts.join(" | "), base_response)
        } else {
            base_response
        };

        // 3. Confidence & contradiction evaluation
        let proposed = ProposedAction {
            action_name: "ReplyToUser".to_string(),
            domain: Domain::General,
            model_confidence: 0.85,
            parameters_summary: prompt.to_string(),
        };
        let (conf_decision, _) = self.confidence_engine.evaluate(&proposed, 0.9);

        // 4. Record system response in context
        self.context.record_interaction("Annora", &response, vec!["annora_response".to_string()]);

        let elapsed = tracker.total_elapsed().as_millis() as u64;

        Ok(OrchestratorResult {
            response_text: response,
            routing_used: routing_str,
            latency_ms: elapsed,
            confidence_decision: conf_decision,
            estimated_cost_cents: cost_opt,
        })
    }
}
