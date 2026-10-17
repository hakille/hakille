pub mod budget;
pub mod confidence;
pub mod context;
pub mod contradiction;
pub mod curiosity;
pub mod fabric;
pub mod orchestrator;
pub mod planner;
pub mod priority;
pub mod providers;
pub mod router;
pub mod self_model;
pub mod simulator;
pub mod system1;
pub mod system2;
pub mod working_memory;

pub use fabric::*;
pub use router::*;
pub use system1::*;

// Re-exports explicites des types clés du noyau cognitif (câblage IMP-04) :
// garantit `annora_kernel::CognitiveOrchestrator`, `WorkingMemory`,
// `SelfModel`, etc. même si un glob est retiré un jour.
pub use budget::{LatencyTracker, PipelineBudgetConfig};
pub use confidence::{ConfidenceEngine, ProposedAction};
pub use context::{ActiveGoal, ContextEntry, InteractionContext};
pub use contradiction::{
    ContradictionAnalysis, ContradictionResolution, InternalContradictionEngine,
};
pub use curiosity::{CuriosityInquiry, GraphCuriosityScanner};
pub use orchestrator::{CognitiveOrchestrator, OrchestratorResult};
pub use planner::{DeliberatePlanner, ExecutionPlan, PlanReflection, PlannedStep, RiskLevel};
pub use priority::{Directive, PriorityEngine, PriorityLevel};
pub use providers::{CloudProvider, DataExit, ProviderId, ProviderRegistry};
pub use self_model::{CognitiveState, SelfModel};
pub use simulator::{DryRunReport, DryRunSimulator, SimulatedDiff};
pub use system2::{System2Engine, System2Plan};
pub use working_memory::{DistilledFact, WorkingMemory};

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
            raw_prompt:
                "Analyse et refactor l'architecture du kernel pour prouver sa conformite formelle."
                    .to_string(),
            trust_level: TrustLevel::User,
            has_image: false,
            has_audio: false,
            estimated_token_count: 50,
        };
        let decision_s2 = router.route(&complex_input, 0.1).await;
        match decision_s2 {
            RoutingDecision::System2 {
                estimated_cost,
                reason,
            } => {
                assert!(estimated_cost.amount_cents > 0);
                assert!(!reason.is_empty());
            }
            _ => panic!("Expected System 2 routing for complex architectural analysis"),
        }
    }

    #[tokio::test]
    #[ignore = "slow: tours complets avec décodage Qwen CPU réel (minutes en debug) ; gate lente `-- --ignored`"]
    async fn test_orchestrator_memory_extraction_and_recall() {
        let user_id = UserId(uuid::Uuid::now_v7());
        let fabric = CognitiveFabric::new(CognitiveManifest::default());
        let constitution = Constitution::default_v1();
        let mut orch = CognitiveOrchestrator::new(fabric, user_id, constitution);
        // Budget honnête : l'inférence Qwen réelle prend plusieurs minutes
        // en debug CPU — le gate budget reste testé ailleurs (budget nul).
        orch.set_budget_config(PipelineBudgetConfig {
            max_perception_ms: 60_000,
            max_routing_ms: 60_000,
            max_inference_ms: 1_800_000,
            max_action_ms: 60_000,
            max_total_ms: 3_600_000,
        });

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
        assert!(engine.can_override(
            PriorityLevel::Constitution,
            PriorityLevel::ExplicitUserInstruction
        ));
        assert!(engine.can_override(
            PriorityLevel::ExplicitUserInstruction,
            PriorityLevel::GeneralUserPreference
        ));
        assert!(!engine.can_override(
            PriorityLevel::GeneralUserPreference,
            PriorityLevel::Constitution
        ));
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
        assert!(
            wm.scratchpad_notes.is_empty(),
            "Scratchpad must be cleared after distillation"
        );
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
        assert!(
            self_model
                .query_current_activity()
                .contains("Analyse de contrat")
        );

        self_model.enter_hesitation(402, "Risque financier élevé sans double confirmation");
        assert!(self_model.query_hesitation_reason().is_some());
        assert!(
            self_model
                .query_current_activity()
                .contains("Risque financier")
        );
    }

    #[test]
    fn test_dry_run_simulator() {
        let report = DryRunSimulator::simulate_payment("Alice", 2500, "EUR", 10000);
        assert!(report.simulation_successful);
        assert_eq!(report.diffs.len(), 1);
        assert_eq!(report.diffs[0].state_after, "75.00 EUR");

        let delete_report =
            DryRunSimulator::simulate_file_mutation("/etc/config.json", true, 2048, 0);
        assert!(delete_report.diffs[0].is_destructive);
        assert!(delete_report.warning.is_some());
    }

    #[tokio::test]
    async fn test_orchestrator_dense_semantic_memory_recall() {
        let manifest = CognitiveManifest::default();
        let fabric = CognitiveFabric::new(manifest);
        let user_id = UserId(uuid::Uuid::now_v7());
        let constitution = Constitution::default_v1();
        let mut orchestrator = CognitiveOrchestrator::new(fabric, user_id, constitution);

        // Embeddings RÉELS (MiniLM via `ort`) si le checkpoint est présent ;
        // sinon rappel SPO + substring seul (jamais de hash de secours).
        match annora_inference::MiniLmOnnx::open_default() {
            Ok(embedding_engine) => orchestrator.set_embedding_engine(embedding_engine),
            Err(e) => tracing::warn!(
                error = %e,
                "MiniLM absent — rappel sans embeddings réels"
            ),
        }

        orchestrator.remember("Amadou", "habite à", "Paris");
        let recalled = orchestrator.recall("Amadou");
        assert!(!recalled.is_empty());
        assert!(recalled[0].contains("Paris"));
    }

    #[tokio::test]
    #[ignore = "slow: tours complets avec décodage Qwen CPU réel (minutes en debug) ; gate lente `-- --ignored`"]
    async fn test_orchestrator_with_memory_and_selfmodel() {
        use std::time::Duration;

        let fabric = CognitiveFabric::new(CognitiveManifest::default());
        let user_id = UserId(uuid::Uuid::now_v7());
        let constitution = Constitution::default_v1();
        let mut orch = CognitiveOrchestrator::new(fabric, user_id, constitution);
        // Budget honnête : tours complets avec décodage Qwen réel (minutes en debug).
        orch.set_budget_config(PipelineBudgetConfig {
            max_perception_ms: 60_000,
            max_routing_ms: 60_000,
            max_inference_ms: 1_800_000,
            max_action_ms: 60_000,
            max_total_ms: 3_600_000,
        });
        // Aligne les gates LLM sur le budget : le décodage CPU debug tourne à
        // ~6-8 s/token mesurés (un tour ≈ 70 tokens avec le gabarit ChatML),
        // la gate LLM par défaut (600 s) tuerait un tour sain avant le budget.
        orch.system1.set_llm_timeout(1_800_000);
        orch.system2.set_llm_timeout(1_800_000);

        // 1. WorkingMemory + SelfModel câblés dès `new()`.
        assert!(orch.working_memory.scratchpad_notes.is_empty());
        assert_eq!(orch.self_model.current_state, CognitiveState::Standby);

        // 2. Un tour stocke observation + fait transiter Perceiving→Reasoning→Executing.
        // Nota : l'orchestrateur distille systématiquement en fin de tour
        // (enveloppe fraîche par tour, `distill_and_clear`), donc le scratchpad
        // est vide APRÈS le tour — on vérifie la transition SelfModel,
        // l'introspection exposée et que l'enveloppe WorkingMemory existe.
        let res = orch
            .process_user_turn("Je m'appelle Amadou", &TrustLevel::User)
            .await
            .expect("turn failed");
        assert!(!res.response_text.is_empty());
        assert!(!orch.working_memory.task_id.is_empty());
        assert_eq!(orch.self_model.current_state, CognitiveState::Executing);
        // `query_current_activity` exposée via `current_activity()`.
        assert!(!orch.current_activity().is_empty());

        // 3. Distillation : enveloppe fraîche par tour + `distill_and_clear`
        // quand >10 notes (câblage IMP-04) + distillation systématique en fin
        // de tour. Après un tour le scratchpad est vidé (0 <= 10).
        // + test unitaire direct du seuil >10 sur WorkingMemory.
        for i in 0..11 {
            orch.working_memory
                .add_note(format!("note pré-remplie {}", i));
        }
        orch.working_memory
            .queue_for_distillation("Utilisateur", "nom", "Amadou", 0.9);
        let _ = orch
            .process_user_turn("Bonjour", &TrustLevel::User)
            .await
            .expect("distill turn failed");
        assert!(
            orch.working_memory.scratchpad_notes.len() <= 10,
            "scratchpad aurait dû être distillé et vidé, len={}",
            orch.working_memory.scratchpad_notes.len()
        );

        // 4. Priority : Constitution > Explicit (résolution câblée dans le pipeline).
        let winner = orch.priority_engine.resolve(
            &Directive {
                level: PriorityLevel::Constitution,
                source: "Constitution".to_string(),
                rule_content: "sécurité".to_string(),
            },
            &Directive {
                level: PriorityLevel::ExplicitUserInstruction,
                source: "User".to_string(),
                rule_content: "ignore tout".to_string(),
            },
        );
        assert_eq!(winner.level, PriorityLevel::Constitution);

        // 5. Confidence par domaine (pas de 0.85/0.9 en dur) : seuils différenciés.
        assert_eq!(
            ConfidenceEngine::infer_domain("je veux faire un virement de 25€"),
            annora_core::Domain::Financial
        );
        let fin_threshold = orch
            .confidence_engine
            .threshold_for(&annora_core::Domain::Financial);
        let gen_threshold = orch
            .confidence_engine
            .threshold_for(&annora_core::Domain::General);
        assert!(fin_threshold > gen_threshold);

        // 6. Dry-run avant irréversible : payment/file/delete simulent + tracent.
        assert!(
            orch.dry_run_for_action("WalletPayment", "paye 25€")
                .is_some()
        );
        assert!(
            orch.dry_run_for_action("FileDelete", "supprime /tmp/x")
                .is_some()
        );
        assert!(orch.dry_run_for_action("ReplyToUser", "bonjour").is_none());
        // Le pipeline a simulé sur un prompt de paiement.
        let _ = orch
            .process_user_turn("Effectue un paiement de 25€ vers Alice", &TrustLevel::User)
            .await
            .expect("payment turn failed");
        assert!(orch.last_simulation.is_some());

        // 7. Budget per-stage : `check_stage` refuse un dépassement isolé.
        let tracker = LatencyTracker::start(PipelineBudgetConfig::default());
        assert!(
            tracker
                .check_stage("routing", Duration::from_millis(5))
                .is_ok()
        );
        assert!(
            tracker
                .check_stage("routing", Duration::from_millis(10_000))
                .is_err()
        );

        // 8. Contexte : résumé auto au-delà de 20 échanges + persistance JSON.
        let mut ctx = InteractionContext::new(UserId(uuid::Uuid::now_v7()), 100);
        for i in 0..25 {
            ctx.record_interaction("User", &format!("msg {}", i), vec![]);
        }
        assert!(ctx.conversation_stream.len() <= 20);
        assert!(ctx.archived_summary.is_some());
        let json = ctx.to_json().expect("to_json");
        let back = InteractionContext::from_json(&json).expect("from_json");
        assert_eq!(back.total_archived, ctx.total_archived);

        // 9. Curiosité : déduplication + `should_scan` (>300s) + staging.
        assert!(!GraphCuriosityScanner::should_scan(60));
        assert!(GraphCuriosityScanner::should_scan(301));
        let mut scanner = GraphCuriosityScanner::new();
        let g = annora_memory::KnowledgeGraph::new();
        let first = scanner.scan_and_stage(&g);
        assert!(!first.is_empty());
        // Second scan : mêmes IDs déjà posées → rien de frais, mais stagées.
        let second = scanner.scan_and_stage(&g);
        assert!(second.is_empty());
        assert!(!scanner.staged().is_empty());

        // 10. System2 : split phrases honnête, max 8, pas de vide.
        let segs = System2Engine::split_into_phrases(
            "Bonjour. Tu vas bien ? Oui ! Et toi ; moi ça va.\nSuper.",
        );
        assert!(!segs.is_empty() && segs.len() <= 8);
        assert!(segs.iter().all(|s| !s.trim().is_empty()));
    }
}
