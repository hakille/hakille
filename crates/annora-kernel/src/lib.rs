pub mod budget;
pub mod confidence;
pub mod context;
pub mod contradiction;
pub mod fabric;
pub mod orchestrator;
pub mod priority;
pub mod router;
pub mod system1;
pub mod system2;

pub use budget::*;
pub use confidence::*;
pub use context::*;
pub use contradiction::*;
pub use fabric::*;
pub use orchestrator::*;
pub use priority::*;
pub use router::*;
pub use system1::*;
pub use system2::*;

#[cfg(test)]
mod tests {
    use super::*;
    use annora_constitution::rules::Constitution;
    use annora_core::{TrustLevel, UserId};

    #[tokio::test]
    async fn test_router_s1_vs_s2_classification() {
        let router = CognitiveRouter::with_default_classifier();
        
        let simple_input = CognitiveInput {
            raw_prompt: "Bonjour Annora, comment vas-tu ?".to_string(),
            trust_level: TrustLevel::User,
            has_image: false,
            has_audio: false,
            estimated_token_count: 8,
        };
        let decision_s1 = router.route(&simple_input, 0.1).await;
        assert_eq!(decision_s1, RoutingDecision::System1);

        let complex_input = CognitiveInput {
            raw_prompt: "Analyse et refactor l'architecture du kernel pour prouver sa conformite formelle.".to_string(),
            trust_level: TrustLevel::User,
            has_image: false,
            has_audio: false,
            estimated_token_count: 50,
        };
        let decision_s2 = router.route(&complex_input, 0.1).await;
        match decision_s2 {
            RoutingDecision::System2 { estimated_cost, reason } => {
                assert!(estimated_cost.amount_cents > 0);
                assert!(!reason.is_empty());
            }
            _ => panic!("Expected System 2 routing for complex architectural analysis"),
        }
    }

    #[tokio::test]
    async fn test_orchestrator_memory_extraction_and_recall() {
        let user_id = UserId(uuid::Uuid::now_v7());
        let fabric = CognitiveFabric::new(CognitiveManifest::default());
        let constitution = Constitution::default_v1();
        let mut orch = CognitiveOrchestrator::new(fabric, user_id, constitution);

        // Turn 1: user introduces themselves
        let res1 = orch
            .process_user_turn("Je m'appelle Amadou", &TrustLevel::User)
            .await
            .expect("Turn 1 failed");
        assert!(!res1.response_text.is_empty());

        // Turn 2: ask for the name
        let res2 = orch
            .process_user_turn("Quel est mon nom ?", &TrustLevel::User)
            .await
            .expect("Turn 2 failed");
        
        assert!(
            res2.response_text.contains("Amadou"),
            "Expected orchestrator to recall Amadou from memory graph, got: {}",
            res2.response_text
        );
    }

    #[test]
    fn test_priority_hierarchy() {
        let engine = PriorityEngine::new();
        assert!(engine.can_override(PriorityLevel::Constitution, PriorityLevel::ExplicitUserInstruction));
        assert!(engine.can_override(PriorityLevel::ExplicitUserInstruction, PriorityLevel::GeneralUserPreference));
        assert!(!engine.can_override(PriorityLevel::GeneralUserPreference, PriorityLevel::Constitution));
    }

    #[test]
    fn test_contradiction_detection() {
        let engine = InternalContradictionEngine::new();
        let facts = vec!["preserve all user identity records".to_string()];
        
        let analysis_conflict = engine
            .critique("delete all user identity records", &facts)
            .expect("Critique failed");
        assert!(analysis_conflict.has_contradiction);
        assert_eq!(
            analysis_conflict.resolution_strategy,
            ContradictionResolution::RequiresUserConfirmation
        );

        let analysis_ok = engine
            .critique("read the identity record", &facts)
            .expect("Critique failed");
        assert!(!analysis_ok.has_contradiction);
        assert_eq!(
            analysis_ok.resolution_strategy,
            ContradictionResolution::Affirmed
        );
    }

    #[test]
    fn test_latency_tracker_budget() {
        let config = PipelineBudgetConfig {
            max_perception_ms: 10,
            max_routing_ms: 10,
            max_inference_ms: 10,
            max_action_ms: 10,
            max_total_ms: 5000,
        };
        let tracker = LatencyTracker::start(config);
        assert!(tracker.is_within_budget());
        assert!(tracker.remaining_budget_ms() > 0);
    }
}

