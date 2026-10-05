use annora_audit::entry::{ActionRecord, AuditEntry, ReasoningStep, ReasoningTrace};
use annora_audit::journal::AuditJournal;
use annora_constitution::rules::Constitution;
use annora_core::{
    ActorId, CapabilityAction, CapabilityAuthority, CapabilityScope, EntryId, KeyPair,
    NodeId, PredicateId, Timestamp, TripletId, TrustLevel, UserId,
};
use annora_kernel::{
    fabric::{CognitiveFabric, CognitiveManifest},
    orchestrator::CognitiveOrchestrator,
};
use annora_memory::{
    ContextPartition, KnowledgeGraph, NodeValue, Triplet, TripletMetadata, TripletSource,
};
use annora_metrics::{
    BoundedInitiativeTracker, ContinuityTracker, ValuesDriftDetector,
};
use annora_substrate::Substrate;
use annora_substrate_windows::WindowsSubstrate;
use annora_sync::CrdtSync;
use tempfile::NamedTempFile;
use uuid::Uuid;

#[tokio::test]
async fn test_full_system_end_to_end_integration() {
    // =========================================================================
    // LAYER 1: Substrate & Hardware Attestation
    // =========================================================================
    let substrate = WindowsSubstrate::new();
    let manifest = substrate.manifest();
    assert_eq!(manifest.os_name, "Windows");
    assert_eq!(manifest.os_version, "11 (24H2)");
    assert_eq!(manifest.tier, annora_core::Tier::Native);
    assert!(manifest.has_tpm);

    let attestation_payload = b"annora-e2e-session-verification";
    let attestation = substrate
        .attest(attestation_payload)
        .await
        .expect("Attestation failed");
    assert!(!attestation.signature.0.is_empty());
    assert_eq!(attestation.platform_id, "Windows-Substrate-Host");

    // =========================================================================
    // LAYER 2: Biscuit Capability Authority & Monotone Attenuation
    // =========================================================================
    let authority = CapabilityAuthority::new();
    let root_token = authority
        .create_root_token(
            vec![
                CapabilityScope::ModelInference,
                CapabilityScope::Wallet,
                CapabilityScope::Filesystem,
            ],
            vec![CapabilityAction::Read, CapabilityAction::Execute],
            Timestamp::now(),
        )
        .expect("Root token creation failed");

    // Attenuate to strictly ModelInference Read only
    let restricted_token = authority
        .attenuate(
            &root_token,
            vec![CapabilityScope::ModelInference],
            vec![CapabilityAction::Read],
            Timestamp::now(),
        )
        .expect("Attenuation failed");

    // Verify authorized scope succeeds
    assert!(authority
        .verify(
            &restricted_token,
            CapabilityScope::ModelInference,
            CapabilityAction::Read,
        )
        .is_ok());

    // Verify unauthorized scope strictly denied by Biscuit crypto
    assert!(authority
        .verify(
            &restricted_token,
            CapabilityScope::Wallet,
            CapabilityAction::Execute,
        )
        .is_err());

    // =========================================================================
    // LAYER 3: Sovereign Memory Knowledge Graph (SPO Triplet Engine)
    // =========================================================================
    let mut memory = KnowledgeGraph::new();
    let user_node = NodeId(Uuid::now_v7());
    let pred_lang = PredicateId(Uuid::now_v7());

    let triplet = Triplet {
        id: TripletId(Uuid::now_v7()),
        subject: user_node.clone(),
        predicate: pred_lang,
        object: NodeValue::Text("Préférence langage: Rust 2024".to_string()),
        metadata: TripletMetadata {
            weight: 1.0,
            created_at: Timestamp::now(),
            last_activated: Timestamp::now(),
            source: TripletSource::User,
            confidence: 1.0,
            context: ContextPartition::Personal,
            trust_level: TrustLevel::User,
            decay_rate: 0.005,
        },
    };
    memory.insert(triplet);

    let recalled = memory.search_text("Rust");
    assert_eq!(recalled.len(), 1);

    // =========================================================================
    // LAYER 4: Cognitive Fabric & Orchestrator Multi-Turn Conversation
    // =========================================================================
    let user_id = UserId(Uuid::now_v7());
    let constitution = Constitution::default_v1();
    let fabric = CognitiveFabric::new(CognitiveManifest::default());
    let mut orchestrator = CognitiveOrchestrator::new(fabric, user_id.clone(), constitution);

    // Turn 1: User introduces themselves
    let turn1_res = orchestrator
        .process_user_turn("Bonjour, je m'appelle Amadou et j'adore Rust", &TrustLevel::User)
        .await
        .expect("Turn 1 failed");
    assert!(!turn1_res.response_text.is_empty());

    // Turn 2: Query relying on auto-extracted memory
    let turn2_res = orchestrator
        .process_user_turn("Quel est mon nom et de quoi avons-nous parlé ?", &TrustLevel::User)
        .await
        .expect("Turn 2 failed");
    assert!(
        turn2_res.response_text.contains("Amadou"),
        "Expected response to contain recalled name Amadou, got: {}",
        turn2_res.response_text
    );

    // =========================================================================
    // LAYER 5: Ed25519 Signed Audit Journal & SHA-256 Hash Chaining
    // =========================================================================
    let signing_keypair = KeyPair::generate();
    let temp_db = NamedTempFile::new().expect("Temp file failed");
    let journal = AuditJournal::new(
        temp_db.path().to_str().unwrap(),
        signing_keypair.signing_key.clone(),
    )
    .expect("Audit journal init failed");

    let entry_id = EntryId(Uuid::now_v7());
    let audit_entry = AuditEntry {
        id: entry_id.clone(),
        timestamp: Timestamp::now(),
        actor: ActorId::Kernel,
        action: ActionRecord {
            action_type: "CognitiveTurnExecution".to_string(),
            description: "Executed multi-turn reasoning with sovereign memory recall".to_string(),
            parameters: serde_json::json!({
                "query": "Quel est mon nom ?",
                "recalled_fact": "Amadou",
            }),
            reversible: true,
            reversed: false,
        },
        capability_used: Some(CapabilityScope::ModelInference),
        reasoning_trace: ReasoningTrace {
            steps: vec![ReasoningStep {
                description: "Recalled user identity from SPO graph and synthesized reply".to_string(),
                confidence: 0.98,
                evidence: vec![],
            }],
            model_used: None,
            duration_ms: turn2_res.latency_ms,
        },
        graph_nodes_used: vec![],
        confidence: 0.98,
        cost: None,
        signature: vec![],
        prev_hash: vec![],
    };

    let appended_id = journal.append(audit_entry).expect("Append entry failed");
    assert_eq!(appended_id, entry_id);

    // Verify chain integrity
    let chain_status = journal.verify_chain().expect("Chain verify failed");
    assert_eq!(chain_status.valid_entries, 1);
    assert_eq!(chain_status.broken_at, None);

    // Test reversible audit rollback
    let reverted_entry = journal.revert(&entry_id).expect("Revert failed");
    assert!(reverted_entry.action.reversed);

    // =========================================================================
    // LAYER 6: Multi-Device CRDT Synchronization
    // =========================================================================
    let mut node_desktop = CrdtSync::new();
    let mut node_mobile = CrdtSync::new();

    let shared_triplet = Triplet {
        id: TripletId(Uuid::now_v7()),
        subject: NodeId(Uuid::now_v7()),
        predicate: PredicateId(Uuid::now_v7()),
        object: NodeValue::Text("User preferred dark theme & high contrast".to_string()),
        metadata: TripletMetadata {
            weight: 1.0,
            created_at: Timestamp::now(),
            last_activated: Timestamp::now(),
            source: TripletSource::User,
            confidence: 1.0,
            context: ContextPartition::Personal,
            trust_level: TrustLevel::User,
            decay_rate: 0.01,
        },
    };
    let shared_id = shared_triplet.id.clone();

    // Node A creates a state entry
    node_desktop.insert_triplet(shared_triplet, "desktop-01");
    assert!(node_desktop.elements.contains_key(&shared_id));

    // Exchange CRDT delta with Node B
    node_mobile.merge(&node_desktop);

    // Node B now has exact convergent state
    assert!(node_mobile.elements.contains_key(&shared_id));
    assert_eq!(node_mobile.vector_clock.get("desktop-01"), 1);

    // Tombstone deletion test
    node_desktop.remove_triplet(shared_id.clone(), "desktop-01");
    assert!(node_desktop.tombstones.contains(&shared_id));
    assert_eq!(node_desktop.active_triplets().len(), 0);

    node_mobile.merge(&node_desktop);
    assert!(node_mobile.tombstones.contains(&shared_id));
    assert_eq!(node_mobile.active_triplets().len(), 0);

    // =========================================================================
    // LAYER 7: North-Star Metrics & Constitutional Invariants
    // =========================================================================
    let mut continuity = ContinuityTracker::new();
    continuity.record_task("coding_turn_1", false, 1);
    continuity.record_task("coding_turn_2", false, 2);
    assert_eq!(continuity.continuity_rate(), 1.0);

    let mut bounded_init = BoundedInitiativeTracker::new();
    bounded_init.record_proposal("suggest_refactor", true, true);
    assert_eq!(bounded_init.compliance_rate(), 1.0);

    let drift_detector = ValuesDriftDetector::default_constitutional();
    let alert_opt = drift_detector.evaluate_current_vector(&[0.98, 0.99, 1.0, 0.97, 0.99]);
    assert!(alert_opt.is_none(), "Expected no values drift detected");
}
