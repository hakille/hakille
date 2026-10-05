use annora_constitution::rules::Constitution;
use annora_core::{ConfidenceDecision, Domain, TrustLevel, UserId};
use annora_kernel::{
    confidence::{ConfidenceEngine, ProposedAction},
    fabric::{CognitiveFabric, CognitiveManifest},
    orchestrator::CognitiveOrchestrator,
    priority::{Directive, PriorityEngine, PriorityLevel},
};
use uuid::Uuid;

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
    assert!(engine.can_override(PriorityLevel::Constitution, PriorityLevel::ExplicitUserInstruction));
    assert!(!engine.can_override(PriorityLevel::ExplicitUserInstruction, PriorityLevel::Constitution));
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
async fn test_orchestrator_turn_flow() {
    let fabric = CognitiveFabric::new(CognitiveManifest::default());
    let user_id = UserId(Uuid::now_v7());
    let constitution = Constitution::default();

    let mut orchestrator = CognitiveOrchestrator::new(fabric, user_id, constitution);
    let res = orchestrator
        .process_user_turn("Quel temps fait-il aujourd'hui ?", &TrustLevel::User)
        .await
        .unwrap();

    assert!(!res.response_text.is_empty());
    assert_eq!(orchestrator.context.conversation_stream.len(), 2); // 1 User + 1 Annora
}
