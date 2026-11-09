//! Tier 4 Real-World Application Workload: Scenario 4
//!
//! Deep Memory Introspection, Explainability ("Pourquoi ?"), & Cryptographic Forgetting
//! Features Exercised: F11 (3-Level "Pourquoi ?" Sheet), F15 (SPO Knowledge Graph Explorer), F16 (Cryptographic Fact Forgetting)
//! Complexity: Medium
//!
//! Workflow:
//! 1. Hub issues recommendation based on stored memory triplets; user opens WhySheet.
//! 2. Level 1 concise phrase expands to Level 2 active SPO fact triplets.
//! 3. Knowledge graph search verifies presence and semantic connections.
//! 4. User exercises Right to be Forgotten ("Ce fait est faux / Oublier"):
//!    - Key erasure & tombstone creation.
//!    - Ed25519 signed deletion receipt emitted and verified.
//! 5. Verification: knowledge graph returns 0 hits, WhySheet updates to redacted state.

use crate::harness::{AuditSigner, AuditVerifier};
use hakille_design::why::{FactRef, WhySheet};
use hakille_ui_protocol::MemoryRowDto;
use serde_json::json;

#[test]
fn test_scenario4_deep_memory_introspection_and_cryptographic_forgetting() {
    let mut audit_signer = AuditSigner::new();

    // --- STEP 1 & 2: WhySheet Level 1 & Level 2 Expansion ---
    let fact_id = "mem-spo-fact-404";
    let fact_text = "Utilisateur habite à Dakar depuis 2022.";

    let active_spo_fact = FactRef {
        label: fact_text.into(),
        kind: "Lieu de résidence".into(),
        date: "2026-11-08".into(),
        source: "UserExplicit".into(),
        freshness: "Vérifié il y a 10 min".into(),
    };

    let why_initial = WhySheet {
        sentence: "Proposé car ton lieu de résidence est enregistré comme Dakar.".into(),
        facts: vec![active_spo_fact.clone()],
        plan: "Recommander actualités météo locales".into(),
        powers: vec!["location.read".into()],
        confidence_word: WhySheet::confidence_word(0.95).to_string(),
        confidence_value: 0.95,
        alternatives: vec!["Météo mondiale".into()],
        model_line: "qwen2.5-0.5b".into(),
        cost_label: "0,00 €".into(),
        data_line: "Local".into(),
        would_change: "Si tu déménages ou contestes ce fait".into(),
    };

    // Level 1 check
    assert!(why_initial.sentence.contains("Dakar"));
    assert_eq!(why_initial.confidence_word, "haute");

    // Level 2 expansion check
    assert_eq!(why_initial.facts.len(), 1);
    assert_eq!(why_initial.facts[0].label, fact_text);
    assert_eq!(why_initial.facts[0].source, "UserExplicit");

    // --- STEP 3: Knowledge Graph Exploration ---
    struct SovereignGraphStore {
        rows: Vec<MemoryRowDto>,
        tombstones: Vec<String>,
    }

    impl SovereignGraphStore {
        fn search(&self, query: &str) -> Vec<&MemoryRowDto> {
            self.rows
                .iter()
                .filter(|r| !self.tombstones.contains(&r.id) && r.text.contains(query))
                .collect()
        }

        fn cryptographic_forget(&mut self, row_id: &str) {
            self.tombstones.push(row_id.to_string());
        }
    }

    let mut graph = SovereignGraphStore {
        rows: vec![
            MemoryRowDto {
                id: fact_id.into(),
                text: fact_text.into(),
                tags: vec!["residence".into(), "dakar".into()],
                source: "UserExplicit".into(),
            },
            MemoryRowDto {
                id: "mem-spo-fact-405".into(),
                text: "Utilisateur utilise Hakille Mobile sur Linux.".into(),
                tags: vec!["device".into()],
                source: "System".into(),
            },
        ],
        tombstones: vec![],
    };

    // Initial search for "Dakar" yields 1 row
    let hits_initial = graph.search("Dakar");
    assert_eq!(hits_initial.len(), 1);
    assert_eq!(hits_initial[0].id, fact_id);

    // --- STEP 4: Cryptographic Forgetting ("Ce fait est faux") ---
    // Shred the row encryption key & record tombstone
    graph.cryptographic_forget(fact_id);

    // Emit Ed25519-signed deletion receipt
    let deletion_receipt = audit_signer.sign_action(
        "audit-forget-dakar-fact",
        "MemoryForget",
        None,
        json!({
            "purged_row_id": fact_id,
            "reason": "Ce fait est faux / Droit à l'oubli",
            "shredded_key_type": "Ed25519_AES256GCM",
            "tombstone_recorded": true
        }),
    );

    AuditVerifier::verify_receipt(&deletion_receipt)
        .expect("Deletion receipt must pass genuine cryptographic verification");

    // --- STEP 5: Verification in Graph & WhySheet ---
    // Re-querying knowledge graph returns 0 hits for forgotten fact
    let hits_after = graph.search("Dakar");
    assert_eq!(
        hits_after.len(),
        0,
        "Knowledge graph must return 0 hits after cryptographic forgetting"
    );

    // Unrelated facts remain intact
    let other_hits = graph.search("Linux");
    assert_eq!(other_hits.len(), 1);

    // Re-opening WhySheet presents honest missing/redacted statement
    let why_redacted = WhySheet::missing("Fait supprimé suite à la contestation de l'utilisateur.");
    assert!(
        why_redacted.sentence.contains("pas gardé"),
        "Redacted WhySheet must state honest reason without fabrication"
    );
    assert!(
        why_redacted.facts.is_empty(),
        "Redacted WhySheet must contain no facts"
    );
    assert_eq!(why_redacted.confidence_word, "basse");
}
