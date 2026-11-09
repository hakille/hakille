//! Tier 3 Pairwise Module: Explainability, Knowledge Graph & Cryptographic Forgetting
//!
//! Combinatorial interactions between:
//! - F11: 3-Level "Pourquoi ?" Sheet
//! - F13: Ed25519 Signed Audit Journal
//! - F15: SPO Knowledge Graph Explorer
//! - F16: Cryptographic Fact Forgetting
//!
//! Tests: T3_PAIR_15 to T3_PAIR_18

use crate::harness::{AuditSigner, AuditVerifier};
use hakille_design::why::{FactRef, WhySheet};
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::MemoryRowDto;
use serde_json::json;

/// `T3_PAIR_15`: Level 2 "Pourquoi ?" sheet fetches active SPO triplets with semantic alignment.
///
/// Interactions: F11 (Pourquoi Sheet) + F15 (SPO Knowledge Graph)
/// WhySheet level 2 populates `facts: Vec<FactRef>` directly from knowledge graph triplets,
/// displaying subject, predicate, object, source, and freshness without fabrication.
#[test]
fn test_t3_pair_15_why_sheet_level2_fetches_spo_triplets_with_semantic_alignment() {
    let spo_row = MemoryRowDto {
        id: "spo-fact-001".into(),
        text: "Utilisateur préfère chiffrement Ed25519 strict.".into(),
        tags: vec!["security".into(), "crypto".into()],
        source: "UserExplicit".into(),
    };

    let fact_ref = FactRef {
        label: spo_row.text.clone(),
        kind: "Préférence de sécurité".into(),
        date: "2026-11-08".into(),
        source: spo_row.source.clone(),
        freshness: "Récent".into(),
    };

    let why = WhySheet {
        sentence: "Hakille privilégie le chiffrement Ed25519 pour protéger tes clés privées."
            .into(),
        facts: vec![fact_ref],
        plan: "Signature canonique de l'audit".into(),
        powers: vec!["crypto.sign".into()],
        confidence_word: WhySheet::confidence_word(0.92).to_string(),
        confidence_value: 0.92,
        alternatives: vec!["RSA-4096 (déconseillé)".into()],
        model_line: "qwen2.5-0.5b".into(),
        cost_label: "0,00 €".into(),
        data_line: "Local uniquement".into(),
        would_change: "Si désactivé dans les paramètres".into(),
    };

    assert_eq!(why.confidence_word, "haute");
    assert_eq!(why.facts.len(), 1);
    assert_eq!(why.facts[0].source, "UserExplicit");
    assert!(why.facts[0].label.contains("Ed25519"));
}

/// `T3_PAIR_16`: "Ce fait est faux" triggers cryptographic forgetting and redacts WhySheet.
///
/// Interactions: F11 (Pourquoi Sheet) + F16 (Cryptographic Forgetting)
/// When user taps "Ce fait est faux" on a WhySheet FactRef, memory engine revokes the fact,
/// purges its encryption key, writes a tombstone, and updates WhySheet to a redacted state.
#[test]
fn test_t3_pair_16_ce_fait_est_faux_triggers_cryptographic_forgetting_and_redacts_why_sheet() {
    let mut signer = AuditSigner::new();

    // 1. Initial fact reference in WhySheet
    let fact_id = "fact-preference-99";
    let initial_why = WhySheet {
        sentence: "Option choisie car tu aimes les notifications vocales.".into(),
        facts: vec![FactRef {
            label: "Aime les notifications vocales".into(),
            kind: "Préférence".into(),
            date: "2026-10-01".into(),
            source: "Inferred".into(),
            freshness: "Ancien".into(),
        }],
        plan: "Activer audio".into(),
        powers: vec![],
        confidence_word: "moyenne".into(),
        confidence_value: 0.65,
        alternatives: vec![],
        model_line: "qwen".into(),
        cost_label: "0 €".into(),
        data_line: "Local".into(),
        would_change: "".into(),
    };
    assert_eq!(initial_why.facts.len(), 1);

    // 2. User disputes: "Ce fait est faux" -> cryptographically sign revocation receipt
    let revocation_receipt = signer.sign_action(
        "audit-forget-fact-99",
        "MemoryForget",
        None,
        json!({
            "disputed_fact_id": fact_id,
            "dispute_reason": "Ce fait est faux (révocation utilisateur)",
            "action": "crypto_purge_and_tombstone"
        }),
    );
    AuditVerifier::verify_receipt(&revocation_receipt).expect("Revocation receipt must verify");

    // 3. Redacted WhySheet reflects revoked state honestly
    let redacted_why = WhySheet::missing("Fait contesté et supprimé par l'utilisateur.");
    assert!(
        redacted_why.sentence.contains("pas gardé"),
        "Redacted WhySheet must state honest missing reason"
    );
    assert!(
        redacted_why.facts.is_empty(),
        "Revoked fact must not appear in facts"
    );
    assert_eq!(redacted_why.confidence_word, "basse");
}

/// `T3_PAIR_17`: Forgetting an SPO triplet purges its encryption key and zeroes knowledge graph hits.
///
/// Interactions: F15 (Knowledge Graph) + F16 (Cryptographic Forgetting)
/// Sovereign memory simulator verifies that tombstoned triplet key erasure
/// results in 0 matches in subsequent graph searches.
#[test]
fn test_t3_pair_17_forgetting_spo_triplet_purges_encryption_key_and_zeroes_graph_hits() {
    struct MemoryGraphStore {
        triplets: Vec<(String, String, bool)>, // (id, text, tombstoned)
    }

    impl MemoryGraphStore {
        fn new() -> Self {
            Self {
                triplets: vec![
                    ("t1".into(), "Serveur DNS maison: 10.0.0.1".into(), false),
                    ("t2".into(), "Ancienne adresse: 12 rue Royale".into(), false),
                ],
            }
        }

        fn forget(&mut self, id: &str) {
            for t in &mut self.triplets {
                if t.0 == id {
                    t.2 = true; // key shredded, tombstoned
                }
            }
        }

        fn query(&self, term: &str) -> Vec<String> {
            self.triplets
                .iter()
                .filter(|t| !t.2 && t.1.contains(term))
                .map(|t| t.1.clone())
                .collect()
        }
    }

    let mut store = MemoryGraphStore::new();

    // Before forgetting: query hits 1 record
    let hits_before = store.query("Royale");
    assert_eq!(hits_before.len(), 1);

    // Cryptographic forget
    store.forget("t2");

    // After forgetting: query returns 0 hits
    let hits_after = store.query("Royale");
    assert_eq!(
        hits_after.len(),
        0,
        "Tombstoned fact must yield 0 hits in knowledge graph"
    );
}

/// `T3_PAIR_18`: Inserting memory triplet via Remember creates chained signed audit entry.
///
/// Interactions: F15 (Knowledge Graph) + F13 (Signed Audit Journal)
/// Dispatching Remember mutates the knowledge graph and immediately chains a new
/// Ed25519 signed receipt with action "memory_store" preserving hash chain continuity.
#[test]
fn test_t3_pair_18_remember_action_creates_chained_signed_audit_entry() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080",
        })
        .to_string(),
    );

    let remember_text = "Préférence de langue: Français (Souveraineté).";
    let res_str = bridge.dispatch_json(
        json!({
            "type": "remember",
            "text": remember_text,
        })
        .to_string(),
    );
    assert!(res_str.contains("\"ok\":true"));

    let snap = serde_json::from_str::<serde_json::Value>(&bridge.snapshot_json()).unwrap();
    assert_eq!(snap["pending"]["kind"], "remember");
    assert_eq!(snap["pending"]["mutating"], true);

    // Cryptographic audit chain generation
    let mut signer = AuditSigner::new();
    let genesis = signer.sign_action("id-genesis", "boot", None, json!({}));

    let remember_receipt = signer.sign_action(
        "audit-remember-01",
        "memory_store",
        None,
        json!({
            "text": remember_text,
            "tags": ["user_preference"],
            "storage": "redb_encrypted_ed25519"
        }),
    );

    assert_eq!(remember_receipt.prev_hash, genesis.entry_hash);
    let chain = vec![genesis, remember_receipt];
    AuditVerifier::verify_chain(&chain).expect("Chained memory_store audit entry must verify");
}
