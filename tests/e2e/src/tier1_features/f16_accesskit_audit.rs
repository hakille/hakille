//! Feature 16: AccessKit Tree & Screen-Reader Audit (Milestone M4)
//!
//! Specifications: T1_F16_01 to T1_F16_05

use crate::harness::a11y_inspector::A11yInspector;
use hakille_ui::{CostCounter, EphemeralHud, PresenceIndicator, SilenceBudgetTracker};
use hakille_ui_desktop::a11y::{ActionCardA11y, build_shell_tree};
use hakille_ui_model::{JournalRow, PowerRow, StableSnapshot};

fn sample_viewmodels() -> (StableSnapshot, Vec<JournalRow>, Vec<PowerRow>) {
    let presence = PresenceIndicator::new();
    let mut hud = EphemeralHud::new();
    hud.set_streaming(String::from("Test query"), String::from("Response"), true);
    let cost = CostCounter::new(500, "EUR");
    let silence = SilenceBudgetTracker::new(3);
    let snap = StableSnapshot::from_states(&presence, &hud, &cost, &silence, 0, 0);

    let journal = vec![JournalRow {
        id: "1".into(),
        time: "12:00:00".into(),
        actor: "kernel".into(),
        action: "Inférence".into(),
        reason: "Test".into(),
        confidence_pct: 95,
        cost: "0.00 EUR".into(),
        verified: true,
        can_revert: true,
    }];

    let powers = vec![PowerRow {
        group: "network".into(),
        scope: "read".into(),
        enabled: true,
        policy_locked: false,
    }];

    (snap, journal, powers)
}

/// `T1_F16_01`: Full Shell Tree Construction
#[test]
fn test_t1_f16_01_full_shell_tree_construction() {
    let (snap, journal, powers) = sample_viewmodels();
    let tree = build_shell_tree(&snap, &journal, &powers);
    assert!(!tree.nodes.is_empty(), "Shell tree must contain nodes");
    assert!(tree.tree.is_some(), "Shell tree must have a root tree");
}

/// `T1_F16_02`: Zero-Unlabelled Interactive Elements Audit
#[test]
fn test_t1_f16_02_zero_unlabelled_interactive_elements_audit() {
    let (snap, journal, powers) = sample_viewmodels();
    let tree = build_shell_tree(&snap, &journal, &powers);
    A11yInspector::assert_all_interactive_labeled(&tree);
}

/// `T1_F16_03`: Irreversible Action Card Label Announcement
#[test]
fn test_t1_f16_03_irreversible_action_card_label_announcement() {
    let card = ActionCardA11y {
        verb: "Refactoriser".into(),
        object: "main.rs".into(),
        reason: "Perf".into(),
        why_trace: "t-1".into(),
        cost_label: "0.00 EUR".into(),
        data_label: "Local".into(),
        reversibility_label: "définitif".into(),
        needs_hold: true,
        provenance_label: "Local".into(),
        is_external: false,
    };
    A11yInspector::assert_validation_button_announces_hold(&card);
}

/// `T1_F16_04`: Journal List Row Accessibility Details
#[test]
fn test_t1_f16_04_journal_list_row_accessibility_details() {
    let (snap, journal, powers) = sample_viewmodels();
    let tree = build_shell_tree(&snap, &journal, &powers);
    let item = A11yInspector::find_node_by_label_contains(&tree, "Inférence");
    assert!(item.is_some(), "Journal row accessibility node must exist");
}

/// `T1_F16_05`: Live Region Toast Notifications
#[test]
fn test_t1_f16_05_live_region_toast_notifications() {
    let (mut snap, journal, powers) = sample_viewmodels();
    snap.hud_response = "Toast: Action complétée".into();
    let tree = build_shell_tree(&snap, &journal, &powers);
    let node = A11yInspector::find_node_by_label_contains(&tree, "Toast: Action complétée");
    assert!(node.is_some(), "Notification text must be accessible");
}
