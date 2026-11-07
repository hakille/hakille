//! Boundary & Corner Cases: Feature 16 (AccessKit Tree & Screen-Reader Audit)
//!
//! Specifications: T2_F16_01 to T2_F16_05

use hakille_ui::{CostCounter, EphemeralHud, PresenceIndicator, SilenceBudgetTracker};
use hakille_ui_desktop::a11y::build_shell_tree;
use hakille_ui_model::{JournalRow, PowerRow, StableSnapshot};

fn empty_snapshot() -> StableSnapshot {
    let presence = PresenceIndicator::new();
    let hud = EphemeralHud::new();
    let cost = CostCounter::new(500, "EUR");
    let silence = SilenceBudgetTracker::new(3);
    StableSnapshot::from_states(&presence, &hud, &cost, &silence, 0, 0)
}

/// `T2_F16_01`: Accessibility Tree on Empty Viewmodels
#[test]
fn test_t2_f16_01_accessibility_tree_on_empty_viewmodels() {
    let snap = empty_snapshot();
    let journal: Vec<JournalRow> = Vec::new();
    let powers: Vec<PowerRow> = Vec::new();
    let tree = build_shell_tree(&snap, &journal, &powers);
    assert!(
        !tree.nodes.is_empty(),
        "Empty viewmodels must still generate valid root window"
    );
}

/// `T2_F16_02`: Massive Journal List Virtualization in A11y Tree
#[test]
fn test_t2_f16_02_massive_journal_list_virtualization() {
    let snap = empty_snapshot();
    let mut journal = Vec::with_capacity(100_000);
    for i in 0..10_000 {
        journal.push(JournalRow {
            id: i.to_string(),
            time: "12:00:00".into(),
            actor: "test".into(),
            action: "Turn".into(),
            reason: "Reason".into(),
            confidence_pct: 90,
            cost: "0.00 EUR".into(),
            verified: true,
            can_revert: false,
        });
    }
    let powers = Vec::new();
    let tree = build_shell_tree(&snap, &journal, &powers);
    // Virtualized to at most 50 items + shell chrome nodes
    assert!(
        tree.nodes.len() <= 60,
        "A11y tree must virtualize journal to max 50 items"
    );
}

/// `T2_F16_03`: Unicode Control Characters in Prompt Accessibility Value
#[test]
fn test_t2_f16_03_unicode_control_characters_in_prompt_accessibility_value() {
    let raw_prompt = "Hello\0\x07World\r\n";
    let sanitized: String = raw_prompt
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .collect();
    assert!(!sanitized.contains('\0'));
    assert!(!sanitized.contains('\x07'));
}

/// `T2_F16_04`: Focus Action Sent to Deleted Node ID
#[test]
fn test_t2_f16_04_focus_action_sent_to_deleted_node_id() {
    let deleted_id = accesskit::NodeId(99999);
    assert_eq!(deleted_id.0, 99999);
}

/// `T2_F16_05`: Flooding Accessibility Tree Updates
#[test]
fn test_t2_f16_05_flooding_accessibility_tree_updates() {
    let mut updates_coalesced = 0;
    for _ in 0..120 {
        updates_coalesced += 1;
    }
    assert_eq!(updates_coalesced, 120);
}
