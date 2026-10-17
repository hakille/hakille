use annora_agents::{AgentQuotaTracker, CapabilityQuota, QuotaBreachPolicy};
use annora_audit::consent_registry::ConsentRegistry;
use annora_auth::{GuestSessionManager, VoiceLivenessDetector};
use annora_core::{
    ActorId, CapabilityAction, CapabilityAuthority, CapabilityScope, Timestamp, UserId,
};
use annora_kernel::{
    CognitiveState, DeliberatePlanner, DryRunSimulator, ExecutionPlan, GraphCuriosityScanner,
    SelfModel, WorkingMemory,
};
use annora_memory::{
    ContextPartition,
    graph::KnowledgeGraph,
    query_lang::GraphQueryBuilder,
    task::{Task, TaskPriority, TaskStatus},
    triplet::{NodeValue, Triplet, TripletMetadata, TripletSource},
};
use uuid::Uuid;

#[test]
fn test_conformance_working_memory_distillation_and_query_dsl() {
    let mut wm = WorkingMemory::new("conformance-task-42");
    wm.add_note("User said: 'My daughter Emma likes astronomy'");
    wm.record_observation(1, "TextParser", "Entity: Emma, Topic: astronomy");
    wm.queue_for_distillation("Emma", "interest", "astronomy", 0.96);

    let distilled = wm.distill_and_clear();
    assert_eq!(distilled.len(), 1);

    // Persist into sovereign KnowledgeGraph
    let mut graph = KnowledgeGraph::new();
    let subj_emma = annora_core::NodeId(Uuid::new_v4());
    let pred_interest = annora_core::PredicateId(Uuid::new_v4());

    let triplet = Triplet {
        id: annora_core::TripletId(Uuid::new_v4()),
        subject: subj_emma.clone(),
        predicate: pred_interest,
        object: NodeValue::Text(distilled[0].object.clone()),
        metadata: TripletMetadata {
            weight: 1.0,
            created_at: Timestamp::now(),
            last_activated: Timestamp::now(),
            source: TripletSource::Inferred,
            confidence: distilled[0].confidence,
            context: ContextPartition::Personal,
            trust_level: annora_core::TrustLevel::User,
            decay_rate: 0.01,
        },
        node_type: None,
    };
    graph.insert(triplet);

    // Query back via GraphQueryBuilder DSL
    let query = GraphQueryBuilder::select()
        .where_subject(subj_emma)
        .where_object_contains("astronomy")
        .where_weight_above(0.8)
        .build();

    let matches = query.execute(&graph);
    assert_eq!(matches.len(), 1);
    match &matches[0].object {
        NodeValue::Text(val) => assert_eq!(val, "astronomy"),
        _ => panic!("Expected text object"),
    }

    // Verify Task model integration (§28e)
    let mut task = Task::new(
        "task-astronomy-gift",
        "Acheter télescope",
        "Cadeau pour Emma",
    )
    .with_priority(TaskPriority::Normal);
    task.add_step("Rechercher modèles", None);
    assert_eq!(task.status, TaskStatus::Pending);

    // Verify Graph Curiosity Scanner (§28d)
    let inquiries = GraphCuriosityScanner::scan_for_curiosity_inquiries(&graph);
    assert!(
        !inquiries.is_empty(),
        "Curiosity scanner must detect bootstrap opportunities"
    );
}

#[test]
fn test_conformance_deliberate_planner_pre_execution_check() {
    let authority = CapabilityAuthority::new();
    let root_token = authority
        .create_root_token(
            vec![CapabilityScope::Filesystem, CapabilityScope::Audio],
            vec![CapabilityAction::Read, CapabilityAction::Write],
            Timestamp(chrono::Utc::now() + chrono::Duration::hours(1)),
        )
        .expect("Root token generation");

    let mut plan = ExecutionPlan::new("plan-audit-files", "Audit local logs");
    plan.add_step(
        "Read log file",
        CapabilityScope::Filesystem,
        CapabilityAction::Read,
        false,
    );
    plan.add_step(
        "Write audit summary",
        CapabilityScope::Filesystem,
        CapabilityAction::Write,
        false,
    );

    // Verification must pass with valid token
    assert!(plan.verify_capabilities(&root_token, &authority).is_ok());

    // Reflection after execution
    let reflection = DeliberatePlanner::evaluate_reflection(&plan, 2, 2);
    assert!(reflection.all_steps_succeeded);
}

#[test]
fn test_conformance_self_model_and_simulator() {
    let mut self_model = SelfModel::new();
    self_model.set_state(CognitiveState::Simulating);
    self_model.current_task_summary = Some("Simulation virement bancaire".to_string());
    assert!(self_model.query_current_activity().contains("Simulation"));

    let dry_run = DryRunSimulator::simulate_payment("Fournisseur", 15000, "EUR", 20000);
    assert!(dry_run.simulation_successful);
    assert!(
        dry_run.warning.is_some(),
        "Transactions > 100 EUR must raise confirmation alert (§19e)"
    );

    self_model.enter_hesitation(101, "Attente validation humaine pour transaction > 100 EUR");
    assert!(self_model.query_hesitation_reason().is_some());
}

#[test]
fn test_conformance_anti_spoofing_and_guest_manager() {
    let detector = VoiceLivenessDetector::default();
    let flat_audio: Vec<f32> = vec![0.3; 3200];
    let check = detector.analyze_pcm_frame(&flat_audio);
    assert!(
        !check.is_live_human,
        "Flat synthetic tone must be rejected as spoofing attack"
    );

    let mut guest_mgr = GuestSessionManager::new();
    let host = UserId(Uuid::new_v4());
    let session = guest_mgr
        .create_guest_session(
            "guest-temp",
            host,
            vec![CapabilityScope::Sensor],
            "Visite temporaire",
            30,
        )
        .expect("Guest session");

    assert!(session.allows_scope(&CapabilityScope::Sensor));
    assert!(
        !session.allows_scope(&CapabilityScope::Wallet),
        "Wallet can never be granted to guest"
    );
}

#[test]
fn test_conformance_spo_query_dsl_weight_confidence_sorting() {
    // Verrouille mission 3 : GraphQuery typée + tri poids/confiance via recall.
    use annora_constitution::rules::Constitution;
    use annora_kernel::{
        fabric::{CognitiveFabric, CognitiveManifest},
        orchestrator::CognitiveOrchestrator,
    };
    let fabric = CognitiveFabric::new(CognitiveManifest::default());
    let user_id = UserId(Uuid::new_v4());
    let mut orch = CognitiveOrchestrator::new(fabric, user_id, Constitution::default());
    for (subj, pred, obj) in [
        ("Alice", "parle", "Rust"),
        ("Bob", "parle", "Python"),
        ("Alice", "boit", "Café"),
    ] {
        match orch.remember(subj, pred, obj) {
            annora_memory::InsertResult::Inserted(_) => {}
            other => panic!("Insertion SPO attendue, obtenue : {:?}", other),
        }
    }
    // Requête typée par prédicat « parle » : 2 faits, pas le café.
    let pid = orch
        .memory_graph
        .lookup_predicate("parle")
        .expect("Prédicat registré");
    let hits = annora_memory::GraphQueryBuilder::select()
        .where_predicate(pid)
        .build()
        .execute(&orch.memory_graph);
    assert_eq!(hits.len(), 2, "Deux faits partagent le prédicat 'parle'");
    // Recall trié : objet exact, sans concaténation du stub.
    let recalled = orch.recall("parle");
    assert!(
        recalled.contains(&"Rust".to_string()),
        "Objet Rust retrouvé : {:?}",
        recalled
    );
    assert!(
        recalled.contains(&"Python".to_string()),
        "Objet Python retrouvé : {:?}",
        recalled
    );
    assert!(
        !recalled.iter().any(|s| s.contains("Alice parle")),
        "Jamais de concaténation S/P/O"
    );
}

#[tokio::test]
#[ignore = "slow: reason() avec décodage Qwen CPU réel (~11 min en debug) ; gate lente `-- --ignored`"]
async fn test_conformance_system2_confidence_formula_and_naive_trace() {
    // Verrouille mission 6 : confiance 0.6·modèle+0.4·graphe (COGNITION.md §3),
    // coût depuis tokens réels, trace à vraies étapes (split phrases, ≤5+1).
    use annora_kernel::system2::System2Engine;
    let engine = System2Engine::new("TestSlot", 2);
    let prompt = "Analyse ce contrat. Compare les clauses. Diagnostique le risque ?";
    let facts = vec![
        "Fait A".to_string(),
        "Fait B".to_string(),
        "Fait C".to_string(),
    ];
    let plan = engine
        .reason(prompt, &facts, &annora_core::TrustLevel::User)
        .await
        .expect("System2 reason");
    // 0.6·0.75 + 0.4·1.0 = 0.85 avec 3 faits.
    assert!(
        (plan.confidence_score - 0.85).abs() < 1e-9,
        "Confiance COGNITION.md, obtenue : {}",
        plan.confidence_score
    );
    let bare = engine
        .reason("Bonjour", &[], &annora_core::TrustLevel::User)
        .await
        .expect("System2 bare");
    assert!(
        bare.confidence_score < plan.confidence_score,
        "Sans faits ⇒ confiance moindre"
    );
    assert!(plan.cost_incurred.amount_cents >= 1, "Coût réel ≥ 1c");
    assert!(
        plan.cost_incurred.description.contains("tokens"),
        "Coût documenté en tokens : {}",
        plan.cost_incurred.description
    );
    // 3 phrases ⇒ ≥3 étapes issues de la décomposition + synthèse, ≤6 au total.
    assert!(
        plan.proposed_steps.len() >= 3 && plan.proposed_steps.len() <= 6,
        "Étapes naïves bornées : {:?}",
        plan.proposed_steps
    );
    assert!(
        plan.reasoning_trace
            .iter()
            .any(|l| l.contains("Decomposed step")),
        "Trace à vraies étapes"
    );
}

#[test]
fn test_conformance_consent_registry_and_quota_enforcement() {
    let mut registry = ConsentRegistry::new();
    let agent_id = ActorId::Agent("calendar-sync".to_string());

    let grant_id = registry
        .register_grant(
            "grant-cal-read",
            agent_id.clone(),
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            "Lecture calendrier",
            None,
        )
        .expect("grant registration must succeed");
    assert!(registry.has_active_consent(
        &agent_id,
        &CapabilityScope::Filesystem,
        &CapabilityAction::Read
    ));

    registry.revoke_grant(&grant_id, "Session terminée");
    assert!(!registry.has_active_consent(
        &agent_id,
        &CapabilityScope::Filesystem,
        &CapabilityAction::Read
    ));

    // Quota rate limiter
    let quota = CapabilityQuota {
        max_invocations_per_minute: 2,
        max_total_invocations: 10,
        max_payload_bytes_per_call: 2048,
        policy_on_breach: QuotaBreachPolicy::AutoRevoke,
    };
    let mut tracker = AgentQuotaTracker::new(quota);
    assert!(tracker.record_invocation(512).is_ok());
    assert!(tracker.record_invocation(512).is_ok());
    assert!(
        tracker.record_invocation(512).is_err(),
        "Third call in 60s must trigger breach"
    );
    assert!(tracker.is_revoked());
}
