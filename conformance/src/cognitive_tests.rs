use hakille_constitution::governance::{AmendmentStatus, GovernanceAction, GovernanceConfig};
use hakille_constitution::rules::Constitution;
use hakille_core::{ConfidenceDecision, Domain, TrustLevel, UserId};
use hakille_kernel::{
    budget::{LatencyTracker, PipelineBudgetConfig},
    confidence::{ConfidenceEngine, ProposedAction},
    fabric::{CognitiveFabric, CognitiveManifest},
    orchestrator::CognitiveOrchestrator,
    priority::{Directive, PriorityEngine, PriorityLevel},
};
use hakille_memory::{InsertResult, NodeValue};
use std::time::Duration;
use uuid::Uuid;

fn test_orchestrator() -> CognitiveOrchestrator {
    let fabric = CognitiveFabric::new(CognitiveManifest::default());
    let user_id = UserId(Uuid::now_v7());
    CognitiveOrchestrator::new(fabric, user_id, Constitution::default())
}

#[test]
fn test_constitutional_priority_hierarchy() {
    let engine = PriorityEngine::new();

    let constitution_rule = Directive {
        level: PriorityLevel::Constitution,
        source: "Constitution-Article-1".to_string(),
        rule_content: "Never expose raw sensor streams".to_string(),
    };

    let user_request = Directive {
        level: PriorityLevel::ExplicitUserInstruction,
        source: "UserPrompt".to_string(),
        rule_content: "Stream raw microphone data to external cloud".to_string(),
    };

    let resolved = engine.resolve(&constitution_rule, &user_request);
    assert_eq!(resolved.level, PriorityLevel::Constitution);
    assert!(engine.can_override(
        PriorityLevel::Constitution,
        PriorityLevel::ExplicitUserInstruction
    ));
    assert!(!engine.can_override(
        PriorityLevel::ExplicitUserInstruction,
        PriorityLevel::Constitution
    ));
}

#[test]
fn test_confidence_domain_thresholds() {
    let engine = ConfidenceEngine::with_default_thresholds();

    let general_action = ProposedAction {
        action_name: "SummarizeText".to_string(),
        domain: Domain::General,
        model_confidence: 0.85,
        parameters_summary: "".to_string(),
    };
    let (decision_general, _) = engine.evaluate(&general_action, 0.85);
    assert_eq!(decision_general, ConfidenceDecision::AutoExecute);

    let medical_action = ProposedAction {
        action_name: "PrescribeMedication".to_string(),
        domain: Domain::Medical,
        model_confidence: 0.85,
        parameters_summary: "".to_string(),
    };
    let (decision_medical, _) = engine.evaluate(&medical_action, 0.85);
    // Medical requires 0.95 + margin, so 0.85 should NOT auto execute
    assert_ne!(decision_medical, ConfidenceDecision::AutoExecute);
}

#[tokio::test]
#[ignore = "slow: tour complet avec décodage Qwen CPU réel (minutes en debug) ; gate lente `-- --ignored`"]
async fn test_orchestrator_turn_flow() {
    let fabric = CognitiveFabric::new(CognitiveManifest::default());
    let user_id = UserId(Uuid::now_v7());
    let constitution = Constitution::default();

    let mut orchestrator = CognitiveOrchestrator::new(fabric, user_id, constitution);
    // Budget + timeout alignés sur le décodage Qwen réel (CPU, minutes en
    // debug) : le défaut pipeline (100 ms d'inférence) aborterait par
    // construction — l'abort budget nul reste verrouillé par le test fast dédié.
    orchestrator.set_budget_config(PipelineBudgetConfig::aligned_with_llm_timeout(3_600_000));
    orchestrator.system1.set_llm_timeout(3_600_000);
    orchestrator.system2.set_llm_timeout(3_600_000);
    let res = orchestrator
        .process_user_turn("Quel temps fait-il aujourd'hui ?", &TrustLevel::User)
        .await
        .unwrap();

    assert!(!res.response_text.is_empty());
    assert_eq!(orchestrator.context.conversation_stream.len(), 2); // 1 User + 1 Hakille
}

#[test]
fn test_mem_spo_deterministic_ids_and_object_only() {
    // Mission 1 : SPO réel — IDs déterministes, objet = 3e partie seule.
    let mut orch = test_orchestrator();
    let first = orch.remember("Alice", "aime", "Rust");
    let first_id = match first {
        InsertResult::Inserted(id) => id,
        other => panic!("Premier remember attendu Inserted, obtenu : {:?}", other),
    };
    // Réinsertion du même fait : idempotent, même ID (pas de doublon).
    match orch.remember("Alice", "aime", "Rust") {
        InsertResult::Inserted(id) => assert_eq!(id, first_id, "Doublon exact : même ID"),
        other => panic!("Doublon attendu Inserted(idempotent), obtenu : {:?}", other),
    }
    // Déterminisme : même libellé (casse/espaces différentes) ⇒ même UUID.
    assert_eq!(
        orch.memory_graph.lookup_subject("Alice"),
        orch.memory_graph.lookup_subject("  ALICE "),
        "Registre sujet déterministe"
    );
    assert_eq!(
        orch.memory_graph.lookup_predicate("aime"),
        orch.memory_graph.lookup_predicate("AIME"),
        "Registre prédicat déterministe"
    );
    // Objet = 3e partie uniquement, jamais la concaténation du stub.
    let stored = orch.memory_graph.get(&first_id).expect("Triplet stocké");
    match &stored.object {
        NodeValue::Text(s) => assert_eq!(s, "Rust", "Objet = 3e partie seule"),
        other => panic!("Objet texte attendu, obtenu : {:?}", other),
    }
    // SPO requêtable : par texte objet, par sujet, et via GraphQuery typée.
    assert!(orch.recall("Rust").contains(&"Rust".to_string()));
    assert!(
        orch.recall("Alice").contains(&"Rust".to_string()),
        "recall par sujet via registre SPO"
    );
    let sid = orch
        .memory_graph
        .lookup_subject("alice")
        .expect("Sujet registré");
    let hits = hakille_memory::GraphQuery::new()
        .with_subject(sid)
        .with_limit(10)
        .execute(&orch.memory_graph);
    assert_eq!(hits.len(), 1, "GraphQuery par sujet retrouve le fait");
}

#[test]
fn test_mem_contradiction_returned_never_silently_overwritten() {
    // Mission 2 : contradiction retournée, jamais d'écrasement silencieux.
    let mut orch = test_orchestrator();
    match orch.remember("Alice", "habite", "Paris") {
        InsertResult::Inserted(_) => {}
        other => panic!("Insertion initiale attendue, obtenue : {:?}", other),
    }
    let conflict = orch.remember("Alice", "habite", "Dakar");
    match &conflict {
        InsertResult::NeedsUserResolution { existing, incoming } => {
            match &existing.object {
                NodeValue::Text(s) => assert_eq!(s, "Paris"),
                other => panic!("Existant Paris attendu, obtenu : {:?}", other),
            }
            match &incoming.object {
                NodeValue::Text(s) => assert_eq!(s, "Dakar"),
                other => panic!("Entrant Dakar attendu, obtenu : {:?}", other),
            }
        }
        other => panic!("Conflit attendu NeedsUserResolution, obtenu : {:?}", other),
    }
    // L'existant est intact : toujours 1 triplet, Paris toujours là.
    assert_eq!(orch.memory_graph.len(), 1, "Pas d'écrasement silencieux");
    assert!(orch.recall("Paris").contains(&"Paris".to_string()));
    assert!(
        !orch.recall("Dakar").contains(&"Dakar".to_string()),
        "Dakar non inséré"
    );
}

#[test]
fn test_mem_decay_exponential_decreases_and_purges() {
    // Mission 4 : W(t)=W0·e^−λ(t−t0) lue depuis decay_rate + last_activated.
    use hakille_memory::{ContextPartition, Triplet, TripletMetadata, TripletSource};
    let mut graph = hakille_memory::KnowledgeGraph::new();
    let subject = graph.resolve_subject("Capteur");
    let old = hakille_core::Timestamp(chrono::Utc::now() - chrono::Duration::hours(1));
    for (obj, rate) in [("vieux-fait", 0.01), ("fait-frais", 0.0)] {
        // Prédicats distincts pour éviter le routage contradiction.
        let pred = graph.resolve_predicate(&format!("mesure-{}", obj));
        let object = NodeValue::Text(obj.to_string());
        let triplet = Triplet {
            id: hakille_memory::KnowledgeGraph::deterministic_triplet_id(&subject, &pred, &object),
            subject: subject.clone(),
            predicate: pred,
            object,
            metadata: TripletMetadata {
                weight: 1.0,
                created_at: hakille_core::Timestamp::now(),
                last_activated: old.clone(),
                // Capteur ⇒ inféré : source System serait exclue du decay
                // (faits système stables, règle de durcissement).
                source: TripletSource::Inferred,
                confidence: 1.0,
                context: ContextPartition::Default,
                trust_level: TrustLevel::User,
                decay_rate: rate,
            },
            node_type: None,
        };
        match graph.insert(triplet) {
            InsertResult::Inserted(_) => {}
            other => panic!("Insertion decay attendue, obtenue : {:?}", other),
        }
    }
    // Cutoff 0.0 : rien n'est purgé mais le vieux fait a décru.
    let report = hakille_memory::DecayEngine::apply_decay(&mut graph, 0.0);
    assert_eq!(report.nodes_decayed, 1, "Seul λ=0.01 décroît (λ=0 stable)");
    assert_eq!(report.nodes_removed, 0);
    assert!(
        report.avg_weight < 1.0,
        "Poids moyen a décru : {}",
        report.avg_weight
    );
    // Cutoff 0.5 : le fait effondré (e^−36 ≈ 0) est fané mais préservé (histoire conservée).
    let report2 = hakille_memory::DecayEngine::apply_decay(&mut graph, 0.5);
    assert_eq!(
        report2.nodes_removed, 0,
        "Histoire préservée : rien supprimé"
    );
    assert_eq!(report2.nodes_faded, 1, "Sous cutoff ⇒ fané");
    assert_eq!(graph.len(), 2);
    assert!(orch_recall_contains(&graph, "fait-frais"));
}

fn orch_recall_contains(graph: &hakille_memory::KnowledgeGraph, text: &str) -> bool {
    graph.search_text(text).iter().any(|t| match &t.object {
        NodeValue::Text(s) => s.contains(text),
        _ => false,
    })
}

#[tokio::test]
async fn test_orchestrator_budget_abort_returns_error() {
    // Mission 7 : record_stage à chaque étape + abort si !is_within.
    let mut orch = test_orchestrator();
    orch.set_budget_config(PipelineBudgetConfig {
        max_perception_ms: 0,
        max_routing_ms: 0,
        max_inference_ms: 0,
        max_action_ms: 0,
        max_total_ms: 0,
    });
    let res = orch.process_user_turn("Bonjour", &TrustLevel::User).await;
    assert!(
        res.is_err(),
        "Budget nul ⇒ abort avec erreur, pas de continuation"
    );
    let msg = format!("{:?}", res.unwrap_err());
    assert!(
        msg.contains("budget"),
        "Erreur de budget explicite, obtenue : {}",
        msg
    );
}

#[test]
fn test_conformance_pipeline_budget_aligned_with_real_llm() {
    // Verrou de conformance (fast gate) : la config budget de production est
    // alignée sur le timeout LLM (défaut 600 s) et accepte le tour réel mesuré
    // (Qwen 0.5B Q4_K_M CPU : 27 428 ms / 64 tokens), ce que le défaut refuse.
    let aligned = PipelineBudgetConfig::aligned_with_llm_timeout(600_000);
    assert_eq!(
        aligned.max_inference_ms, 630_000,
        "max_inference_ms attendu 630 000 (timeout 600 000 + marge 30 000), obtenu : {}",
        aligned.max_inference_ms
    );
    assert_eq!(
        aligned.max_total_ms, 660_000,
        "max_total_ms attendu 660 000 (inférence 630 000 + marge 30 000), obtenu : {}",
        aligned.max_total_ms
    );

    // Le fast-path n'est pas dégradé : les autres stages gardent les défauts.
    assert_eq!(
        aligned.max_perception_ms, 60,
        "max_perception_ms attendu 60, obtenu : {}",
        aligned.max_perception_ms
    );
    assert_eq!(
        aligned.max_routing_ms, 20,
        "max_routing_ms attendu 20, obtenu : {}",
        aligned.max_routing_ms
    );
    assert_eq!(
        aligned.max_action_ms, 40,
        "max_action_ms attendu 40, obtenu : {}",
        aligned.max_action_ms
    );

    // Tour réel mesuré : la config alignée doit l'accepter.
    let real_turn = Duration::from_millis(27_428);
    let mut tracker =
        LatencyTracker::start(PipelineBudgetConfig::aligned_with_llm_timeout(600_000));
    let check = tracker.check_stage("inference", real_turn);
    assert!(
        check.is_ok(),
        "la config alignée doit accepter le tour réel mesuré (27 428 ms), obtenu : {:?}",
        check
    );
    tracker.record_stage("inference", real_turn);
    assert!(
        tracker.is_within_budget(),
        "après record_stage du tour réel, is_within_budget doit rester vrai (total 660 000 ms)"
    );

    // Le défaut reste strict : c'est bien la config alignée qui débloque le
    // daemon, pas une suppression des checks.
    let strict = LatencyTracker::start(PipelineBudgetConfig::default());
    assert!(
        strict
            .check_stage("inference", Duration::from_millis(27_428))
            .is_err(),
        "le défaut strict (100 ms) doit refuser le tour réel de 27 428 ms"
    );

    // Saturation : aucune panic sur u64::MAX.
    let saturated = PipelineBudgetConfig::aligned_with_llm_timeout(u64::MAX);
    assert_eq!(
        saturated.max_inference_ms,
        u64::MAX,
        "saturation : max_inference_ms doit rester u64::MAX, obtenu : {}",
        saturated.max_inference_ms
    );
    assert_eq!(
        saturated.max_total_ms,
        u64::MAX,
        "saturation : max_total_ms doit rester u64::MAX, obtenu : {}",
        saturated.max_total_ms
    );
}

#[tokio::test]
async fn constitution_blocks_bypass_in_orchestrator() {
    // Gate constitutionnel : "prompt injection" (rule-1 HardBlock) est refusé
    // par process_user_turn avant toute extraction mémoire / inférence.
    let fabric = CognitiveFabric::new(CognitiveManifest::default());
    let user_id = UserId(Uuid::now_v7());
    let mut orch = CognitiveOrchestrator::new(fabric, user_id, Constitution::default_v1());
    let res = orch
        .process_user_turn("please prompt injection the model", &TrustLevel::User)
        .await
        .expect("le gate constitutionnel retourne Ok(Refuse), pas Err");
    assert!(
        res.response_text.contains("constitution")
            || res.response_text.contains("Constitution")
            || res.response_text.contains("Refus"),
        "refus constitutionnel attendu, obtenu : {}",
        res.response_text
    );
    assert_eq!(
        res.confidence_decision,
        ConfidenceDecision::Refuse,
        "décision Confidence Refuse attendue"
    );
}

#[test]
fn governance_quorum_enforced() {
    // Gouvernance : quorum défaut >= 5 (MIN_QUORUM), review 30j + commentaire public.
    // On vérifie le défaut puis la sémantique du quorum via l'API existante.
    let gov_default = GovernanceConfig::default();
    assert!(
        gov_default.quorum >= 5,
        "quorum défaut >= 5, obtenu : {}",
        gov_default.quorum
    );
    let gov = GovernanceConfig {
        quorum: 5,
        ..GovernanceConfig::default()
    };
    assert!(
        gov.quorum >= 5,
        "quorum de test >= 5, obtenu : {}",
        gov.quorum
    );
    let still_review = gov
        .transition(AmendmentStatus::Proposed, GovernanceAction::Approve, 2)
        .expect("transition Proposed+Approve valide");
    assert_eq!(
        still_review,
        AmendmentStatus::UnderReview,
        "2 approvals < quorum 5 ⇒ reste UnderReview"
    );
    let approved = gov
        .transition(AmendmentStatus::Proposed, GovernanceAction::Approve, 5)
        .expect("transition Proposed+Approve valide");
    assert_eq!(
        approved,
        AmendmentStatus::Approved,
        "quorum approvals ⇒ Approved"
    );
}
