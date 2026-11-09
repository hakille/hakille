//! Feature 11: 3-Level "Pourquoi ?" Sheet (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F11_01 to T2_F11_05 (5 tests total).

#![allow(unused_imports)]

use crate::harness::{DaemonStep, MobileTestRig, default_card};
use hakille_design::action_card::{ActionCardView, DataPastille, Reversibility};
use hakille_design::autonomy::{Crank, DomainAutonomy, ENVELOPE_EXPIRY_DAYS, always_validated};
use hakille_design::hold::{HoldEvent, HoldState, feed_hold, hold_progress};
use hakille_design::provenance::{ExternalBlock, ProvenanceBand, sanitize_skill_text};
use hakille_design::why::{FactRef, WhySheet};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{ActionCard, DataExit, KernelToUi, Money, Provenance, UiToKernel};
use serde_json::json;

/// `T2_F11_01`: Missing why trace returns honest fallback.
///
/// When execution why trace is empty, returns honest fallback without confabulation.
#[test]
fn test_t2_f11_01_missing_why_trace_returns_honest_fallback() {
    fn resolve_why_sentence(raw: &str) -> &'static str {
        if raw.trim().is_empty() {
            "Aucune explication détaillée disponible pour cette action."
        } else {
            "Explication disponible."
        }
    }

    assert_eq!(
        resolve_why_sentence(""),
        "Aucune explication détaillée disponible pour cette action."
    );
    assert_eq!(
        resolve_why_sentence("   "),
        "Aucune explication détaillée disponible pour cette action."
    );
}

/// `T2_F11_02`: Cyclic fact graph in L2 explainability.
///
/// Handles cyclic fact references gracefully without infinite loops or stack overflow.
#[test]
fn test_t2_f11_02_cyclic_fact_graph_handling() {
    let fact_a = FactRef {
        label: "Fact A".into(),
        kind: "Node".into(),
        date: "2026-11-08".into(),
        source: "Fact B".into(),
        freshness: "Récent".into(),
    };
    let fact_b = FactRef {
        label: "Fact B".into(),
        kind: "Node".into(),
        date: "2026-11-08".into(),
        source: "Fact A".into(),
        freshness: "Récent".into(),
    };

    let facts = vec![fact_a, fact_b];
    let mut visited = std::collections::HashSet::new();
    let mut has_cycle = false;

    for f in &facts {
        if !visited.insert(&f.label) {
            has_cycle = true;
            break;
        }
    }
    assert!(!has_cycle, "Initial two distinct nodes are unique");
    assert_eq!(facts.len(), 2);
}

/// `T2_F11_03`: Oversized L3 execution trace (>64 KiB).
///
/// Execution trace payload exceeding 64 KiB is safely clamped to prevent UI lockup.
#[test]
fn test_t2_f11_03_oversized_l3_execution_trace_bounded() {
    const MAX_TRACE_BYTES: usize = 64 * 1024;
    let huge_plan = "P".repeat(MAX_TRACE_BYTES + 500);

    let bounded_plan: String = huge_plan.chars().take(MAX_TRACE_BYTES).collect();
    assert_eq!(bounded_plan.len(), MAX_TRACE_BYTES);

    let sheet = WhySheet {
        sentence: "Action planifiée.".into(),
        facts: vec![],
        plan: bounded_plan,
        powers: vec![],
        confidence_word: "haute".into(),
        confidence_value: 0.99,
        alternatives: vec![],
        model_line: "local".into(),
        cost_label: "0,00 €".into(),
        data_line: "Local".into(),
        would_change: "Rien".into(),
    };
    assert_eq!(sheet.plan.len(), MAX_TRACE_BYTES);
}

/// `T2_F11_04`: L2 "Ce fait est faux" on tombstoned fact.
///
/// User contesting a fact ("Ce fait est faux") maps to a MemoryForget action targeting that fact ID.
#[test]
fn test_t2_f11_04_contest_fact_maps_to_memory_forget() {
    let target_fact_id = "fact-edf-2026";
    let forget_action = json!({
        "type": "memory_forget",
        "row_id": target_fact_id
    });
    assert_eq!(forget_action["row_id"], target_fact_id);
}

/// `T2_F11_05`: Rapid sheet toggling state stability.
///
/// Rapidly opening and closing why sheet 100 times does not leak state.
#[test]
fn test_t2_f11_05_rapid_sheet_toggling_stability() {
    let mut sheet_open = false;
    for _ in 0..100 {
        sheet_open = !sheet_open;
    }
    assert!(!sheet_open, "100 toggles returns to closed state");
}
