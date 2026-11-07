//! A11yInspector: AccessKit accessibility tree auditor and query fixture.
//!
//! Verifies WCAG and sovereign S3 accessibility requirements:
//! 100% labeled interactive controls, explicit 600ms hold announcements,
//! ordered trust pastilles, and screen-reader tree validity.

use accesskit::{Node, NodeId, Role, TreeUpdate};
use hakille_ui_desktop::a11y::{
    ActionCardA11y, audit_interactive_labels, build_action_card_tree, build_shell_tree,
};
use hakille_ui_model::{JournalRow, PowerRow, StableSnapshot};

pub struct A11yInspector;

impl A11yInspector {
    /// Build shell tree from snapshot and viewmodels.
    pub fn build_shell(
        snap: &StableSnapshot,
        journal: &[JournalRow],
        powers: &[PowerRow],
    ) -> TreeUpdate {
        build_shell_tree(snap, journal, powers)
    }

    /// Build action card accessibility tree.
    pub fn build_card(card: &ActionCardA11y) -> TreeUpdate {
        build_action_card_tree(card)
    }

    /// Audit interactive controls (Buttons, TextInputs) for missing labels.
    pub fn audit_labels(update: &TreeUpdate) -> Result<(), Vec<NodeId>> {
        let missing = audit_interactive_labels(update);
        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }

    /// Assert 100% of interactive controls are labeled (S3 Gate requirement).
    pub fn assert_all_interactive_labeled(update: &TreeUpdate) {
        let res = Self::audit_labels(update);
        assert!(
            res.is_ok(),
            "Accessibility violation: Found unlabeled interactive controls with NodeIds: {:?}",
            res.err().unwrap()
        );
    }

    /// Find nodes matching a specific role.
    pub fn find_nodes_by_role<'a>(update: &'a TreeUpdate, role: Role) -> Vec<&'a Node> {
        update
            .nodes
            .iter()
            .filter(|(_, n)| n.role() == role)
            .map(|(_, n)| n)
            .collect()
    }

    /// Find a node by label substring.
    pub fn find_node_by_label_contains<'a>(
        update: &'a TreeUpdate,
        substr: &str,
    ) -> Option<&'a Node> {
        update.nodes.iter().find_map(|(_, n)| {
            if let Some(l) = n.label() {
                if l.contains(substr) {
                    return Some(n);
                }
            }
            if let Some(v) = n.value() {
                if v.contains(substr) {
                    return Some(n);
                }
            }
            None
        })
    }

    /// Assert that the validation button announces the 600ms hold when irreversible.
    pub fn assert_validation_button_announces_hold(card: &ActionCardA11y) {
        assert!(
            card.needs_hold,
            "Test requires irreversible card (needs_hold = true)"
        );
        let update = Self::build_card(card);
        let node = Self::find_node_by_label_contains(&update, "appui long 600 millisecondes");
        assert!(
            node.is_some(),
            "Accessibility announcement missing: Irreversible button must announce 600ms hold!"
        );
    }

    /// Assert trust pastilles are present in the accessibility tree in order.
    pub fn assert_trust_pastilles(update: &TreeUpdate, cost: &str, data: &str, rev: &str) {
        let node = Self::find_node_by_label_contains(update, cost);
        assert!(
            node.is_some(),
            "Trust pastille cost '{}' not found in tree",
            cost
        );
        let text = node.unwrap().value().unwrap_or_default();
        assert!(
            text.contains(data),
            "Trust pastille data '{}' not in text: {}",
            data,
            text
        );
        assert!(
            text.contains(rev),
            "Trust pastille rev '{}' not in text: {}",
            rev,
            text
        );
    }

    /// Assert external badge "[externe — filet 2 px]" is present.
    pub fn assert_external_provenance_badge(update: &TreeUpdate) {
        let node = Self::find_node_by_label_contains(update, "[externe — filet 2 px]");
        assert!(
            node.is_some(),
            "External provenance badge '[externe — filet 2 px]' missing from accessibility tree!"
        );
    }
}
