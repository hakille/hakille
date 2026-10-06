pub mod budget;
pub mod confidence;
pub mod context;
pub mod contradiction;
pub mod curiosity;
pub mod fabric;
pub mod orchestrator;
pub mod planner;
pub mod priority;
pub mod router;
pub mod self_model;
pub mod simulator;
pub mod system1;
pub mod system2;
pub mod working_memory;

pub use budget::*;
pub use confidence::*;
pub use context::*;
pub use contradiction::*;
pub use curiosity::*;
pub use fabric::*;
pub use orchestrator::*;
pub use planner::*;
pub use priority::*;
pub use router::*;
pub use self_model::*;
pub use simulator::*;
pub use system1::*;
pub use system2::*;
pub use working_memory::*;

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

    #[test]
    fn test_working_memory_scratchpad_and_distillation() {
        let mut wm = WorkingMemory::new("task-plan-meeting");
        wm.add_note("User wants meeting tomorrow at 14h");
        wm.record_observation(1, "CalendarCheck", "Slot 14h is free");
        wm.set_variable("slot", "14:00");
        wm.queue_for_distillation("Meeting", "time", "14:00", 0.95);

        assert_eq!(wm.scratchpad_notes.len(), 1);
        assert_eq!(wm.observations.len(), 1);
        assert_eq!(wm.get_variable("slot"), Some(&"14:00".to_string()));

        let distilled = wm.distill_and_clear();
        assert_eq!(distilled.len(), 1);
        assert_eq!(distilled[0].object, "14:00");
        assert!(wm.scratchpad_notes.is_empty(), "Scratchpad must be cleared after distillation");
    }

    #[test]
    fn test_deliberate_planner_and_reflection() {
        let mut plan = ExecutionPlan::new("plan-send-email", "Send weekly status update");
        plan.add_step(
            "Draft status email",
            annora_core::CapabilityScope::Filesystem,
            annora_core::CapabilityAction::Read,
            false,
        );
        plan.add_step(
            "Send through network",
            annora_core::CapabilityScope::Network,
            annora_core::CapabilityAction::Execute,
            false,
        );

        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.estimated_risk, RiskLevel::Low);

        let reflection = DeliberatePlanner::evaluate_reflection(&plan, 2, 2);
        assert!(reflection.all_steps_succeeded);
        assert!(reflection.confidence_delta > 0.0);
    }

    #[test]
    fn test_self_model_introspection_and_hesitation() {
        let mut self_model = SelfModel::new();
        assert_eq!(self_model.current_state, CognitiveState::Standby);

        self_model.set_state(CognitiveState::Reasoning);
        self_model.current_task_summary = Some("Analyse de contrat légal".to_string());
        assert!(self_model.query_current_activity().contains("Analyse de contrat"));

        self_model.enter_hesitation(402, "Risque financier élevé sans double confirmation");
        assert!(self_model.query_hesitation_reason().is_some());
        assert!(self_model.query_current_activity().contains("Risque financier"));
    }

    #[test]
    fn test_dry_run_simulator() {
        let report = DryRunSimulator::simulate_payment("Alice", 2500, "EUR", 10000);
        assert!(report.simulation_successful);
        assert_eq!(report.diffs.len(), 1);
        assert_eq!(report.diffs[0].state_after, "75.00 EUR");

        let delete_report = DryRunSimulator::simulate_file_mutation("/etc/config.json", true, 2048, 0);
        assert!(delete_report.diffs[0].is_destructive);
        assert!(delete_report.warning.is_some());
    }
}

