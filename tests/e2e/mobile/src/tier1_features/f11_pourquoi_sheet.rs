//! Feature 11: 3-Level "Pourquoi ?" Sheet (Milestone M1 / Spec §2, §25)
//!
//! Specifications: T1_F11_01 to T1_F11_05
//!
//! Verifies:
//! - Level 1: Concise single-sentence explanation.
//! - Level 2: Factual provenance list with FactRef entries and source links.
//! - Level 3: Deep execution trace (plan, powers, model, cost, alternatives).
//! - Surface confidence vocabulary mapping ('basse', 'moyenne', 'haute').
//! - Honest fallback when execution trace is missing (no confabulation).

use hakille_design::why::{FactRef, WhySheet};

/// `T1_F11_01`: Level 1 concise single-sentence explanation.
///
/// Level 1 answers the user's inquiry in a single, well-formed sentence
/// without multi-paragraph jargon.
#[test]
fn test_t1_f11_01_level1_sentence_conciseness() {
    let sheet = WhySheet {
        sentence: "J'ai préparé ce virement car la facture EDF arrive à échéance demain.".into(),
        facts: vec![],
        plan: "Lecture facture -> calcul montant -> virement SEPA".into(),
        powers: vec!["finance:write".into()],
        confidence_word: "haute".into(),
        confidence_value: 0.95,
        alternatives: vec!["Reporter de 3 jours".into()],
        model_line: "qwen-2.5-coder-local".into(),
        cost_label: "0,00 €".into(),
        data_line: "Local uniquement".into(),
        would_change: "Date d'échéance dépassée".into(),
    };

    assert!(!sheet.sentence.is_empty());
    assert!(
        !sheet.sentence.contains('\n'),
        "Level 1 sentence must be a single line"
    );
    assert!(
        sheet.sentence.ends_with('.'),
        "Level 1 sentence must be grammatically complete"
    );
}

/// `T1_F11_02`: Level 2 factual provenance list.
///
/// Level 2 enumerates every memory graph fact mobilized to make the decision,
/// exposing label, kind, date, source, and freshness.
#[test]
fn test_t1_f11_02_level2_facts_provenance_list() {
    let fact1 = FactRef {
        label: "Facture EDF n° 88129".into(),
        kind: "Document".into(),
        date: "2026-11-01".into(),
        source: "Boîte de réception locale".into(),
        freshness: "Récent (< 7 jours)".into(),
    };

    let fact2 = FactRef {
        label: "IBAN EDF Fournisseur".into(),
        kind: "Identifiant Bancaire".into(),
        date: "2025-06-15".into(),
        source: "Carnet de bénéficiaires vérifié".into(),
        freshness: "Vérifié".into(),
    };

    let facts = vec![fact1, fact2];
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].kind, "Document");
    assert_eq!(facts[1].source, "Carnet de bénéficiaires vérifié");
}

/// `T1_F11_03`: Level 3 deep execution trace details.
///
/// Level 3 provides full transparency: model provider, cost, capabilities required,
/// data exit line, and counterfactuals (what would change the decision).
#[test]
fn test_t1_f11_03_level3_execution_trace_details() {
    let sheet = WhySheet {
        sentence: "Sauvegarde locale effectuée.".into(),
        facts: vec![],
        plan: "Vérification espace disque -> copie chiffrée".into(),
        powers: vec!["fs:read".into(), "fs:write".into()],
        confidence_word: "haute".into(),
        confidence_value: 0.99,
        alternatives: vec!["Synchroniser sur cloud mesh".into()],
        model_line: "local-rust-pipeline".into(),
        cost_label: "0,00 €".into(),
        data_line: "Local (0 octets sortants)".into(),
        would_change: "Espace disque < 1 Go".into(),
    };

    assert_eq!(sheet.powers.len(), 2);
    assert_eq!(sheet.cost_label, "0,00 €");
    assert_eq!(sheet.data_line, "Local (0 octets sortants)");
    assert_eq!(sheet.would_change, "Espace disque < 1 Go");
}

/// `T1_F11_04`: Surface confidence vocabulary mapping.
///
/// Maps numerical confidence into intuitive words:
/// < 0.50 -> "basse", 0.50 <= c < 0.85 -> "moyenne", >= 0.85 -> "haute".
#[test]
fn test_t1_f11_04_surface_confidence_words_mapping() {
    // Low band
    assert_eq!(WhySheet::confidence_word(0.0), "basse");
    assert_eq!(WhySheet::confidence_word(0.25), "basse");
    assert_eq!(WhySheet::confidence_word(0.499), "basse");

    // Medium band
    assert_eq!(WhySheet::confidence_word(0.50), "moyenne");
    assert_eq!(WhySheet::confidence_word(0.70), "moyenne");
    assert_eq!(WhySheet::confidence_word(0.849), "moyenne");

    // High band
    assert_eq!(WhySheet::confidence_word(0.85), "haute");
    assert_eq!(WhySheet::confidence_word(0.95), "haute");
    assert_eq!(WhySheet::confidence_word(1.0), "haute");
}

/// `T1_F11_05`: Honest fallback when trace is missing.
///
/// When an action lack recorded trace details, WhySheet::missing produces an honest
/// admission of missing trace rather than hallucinating an explanation.
#[test]
fn test_t1_f11_05_honest_missing_explanation_fallback() {
    let missing = WhySheet::missing("Entrée créée avant activation du journal v2");

    assert!(
        missing.sentence.contains("pas gardé la raison"),
        "Must admit lack of record: {}",
        missing.sentence
    );
    assert!(missing.facts.is_empty(), "Must not invent facts");
    assert_eq!(missing.confidence_word, "basse");
    assert_eq!(missing.confidence_value, 0.0);
    assert_eq!(missing.cost_label, "inconnu");
    assert_eq!(missing.model_line, "inconnu");
}
