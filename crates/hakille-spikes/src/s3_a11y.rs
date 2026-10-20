//! Spike S3 — accessibilité : carte + journal au clavier et lecteur d'écran.
//!
//! Gate bloquant (plan refonte §6) : NVDA Windows, VoiceOver macOS, Orca
//! Linux passent les mêmes parcours. Preuve outillée ici : l'arbre AccessKit
//! du nouveau shell (`hakille-ui-desktop::a11y`, accesskit 0.24) est complet —
//! zéro contrôle interactif sans label, focus défini, coût dans Valider,
//! appui long annoncé, verrouillage politique sans action.
//! Critère labo restant : parcours réels par les 3 lecteurs d'écran.

use hakille_design::{ActionCardView, DataPastille, Reversibility};
use hakille_ui::{CostCounter, EphemeralHud, PresenceIndicator, SilenceBudgetTracker};
use hakille_ui_desktop::a11y::{ActionCardA11y, audit_interactive_labels, build_shell_tree};
use hakille_ui_model::{JournalRow, PowerRow, StableSnapshot};

/// Snapshot représentatif pour le parcours carte + journal.
pub fn s3_sample_snapshot() -> StableSnapshot {
    let presence = PresenceIndicator::new();
    let mut hud = EphemeralHud::new();
    hud.request_validation("w-1", "Envoi 25.00 EUR", Some(2500), "Financial");
    let cost = CostCounter::new(500, "EUR");
    let silence = SilenceBudgetTracker::new(3);
    StableSnapshot::from_states(&presence, &hud, &cost, &silence, 0, 1)
}

/// Lignes journal S3 — signature et réversibilité en toutes lettres.
pub fn s3_journal_rows() -> Vec<JournalRow> {
    vec![
        JournalRow {
            id: "j-1".into(),
            time: "14:32".into(),
            actor: "Hakille".into(),
            action: "Envoi du récapitulatif à Awa".into(),
            reason: "Demandé explicitement".into(),
            confidence_pct: 92,
            cost: "0.00 EUR".into(),
            verified: true,
            can_revert: true,
        },
        JournalRow {
            id: "j-2".into(),
            time: "14:33".into(),
            actor: "Hakille".into(),
            action: "Paiement facture énergie".into(),
            reason: "Échéance détectée".into(),
            confidence_pct: 71,
            cost: "25.00 EUR".into(),
            verified: true,
            can_revert: false,
        },
    ]
}

/// Parcours clavier : tous les rôles interactifs ont label + action.
pub fn s3_keyboard_walkthrough() -> bool {
    let snap = s3_sample_snapshot();
    let powers = vec![
        PowerRow {
            group: "Voir".into(),
            scope: "Camera".into(),
            enabled: false,
            policy_locked: true,
        },
        PowerRow {
            group: "Agir".into(),
            scope: "GuiInput".into(),
            enabled: true,
            policy_locked: false,
        },
    ];
    let update = build_shell_tree(&snap, &s3_journal_rows(), &powers);
    audit_interactive_labels(&update).is_empty()
}

/// Carte S3 prête pour lecteur d'écran (verbe+objet, raison, pastilles, hold).
pub fn s3_card_a11y(needs_hold: bool, is_external: bool) -> ActionCardA11y {
    let card = ActionCardView {
        verb: "Envoi".into(),
        object: "du récapitulatif à Awa".into(),
        reason: "Tu as demandé de répondre.".into(),
        why_trace: "Mail cran 1".into(),
        cost_label: "0 €".into(),
        data: DataPastille::Local,
        reversibility: if needs_hold {
            Reversibility::Final
        } else {
            Reversibility::Undoable { undo_seconds: 30 }
        },
        needs_hold,
        provenance_label: "Hakille".into(),
        is_external,
    };
    ActionCardA11y {
        verb: card.verb.clone(),
        object: card.object.clone(),
        reason: card.reason.clone(),
        why_trace: card.why_trace.clone(),
        cost_label: card.cost_label.clone(),
        data_label: format!("{:?}", card.data),
        reversibility_label: format!("{:?}", card.reversibility),
        needs_hold: card.needs_hold,
        provenance_label: card.provenance_label.clone(),
        is_external: card.is_external,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_ui_desktop::a11y::build_action_card_tree;
    use hakille_ui_desktop::theme::Theme;

    #[test]
    fn test_s3_keyboard_walkthrough_complete() {
        assert!(
            s3_keyboard_walkthrough(),
            "zéro contrôle interactif sans label exigé"
        );
    }

    #[test]
    fn test_s3_card_hold_and_external_announced() {
        let soft = build_action_card_tree(&s3_card_a11y(false, false));
        assert!(audit_interactive_labels(&soft).is_empty());
        let hard = build_action_card_tree(&s3_card_a11y(true, false));
        let validate = hard.nodes.iter().find(|(i, _)| i.0 == 40).unwrap();
        assert!(validate.1.label().unwrap().contains("600"));
        let ext = build_action_card_tree(&s3_card_a11y(false, true));
        let title = ext.nodes.iter().find(|(i, _)| i.0 == 1).unwrap();
        assert!(title.1.value().unwrap().contains("[externe"));
    }

    #[test]
    fn test_s3_journal_rows_announce_reversibility() {
        let snap = s3_sample_snapshot();
        let update = build_shell_tree(&snap, &s3_journal_rows(), &[]);
        let labels: Vec<String> = update
            .nodes
            .iter()
            .filter(|(i, _)| i.0 >= 100 && i.0 < 200)
            .map(|(_, n)| n.label().unwrap_or_default().to_string())
            .collect();
        assert_eq!(labels.len(), 2);
        assert!(labels[0].contains("annulable"));
        assert!(labels[1].contains("définitif"));
    }

    #[test]
    fn test_s3_contrast_high_mode_tokens() {
        // WCAG 2.2 AA minimum : texte/1 sur surface/0.
        let theme = Theme::dark();
        assert!(theme.contrast_ratio(theme.text1, theme.bg) >= 4.5);
        let hc = Theme::dark().with_high_contrast(true);
        assert!(hc.contrast_ratio(hc.text1, hc.bg) >= 7.0);
    }
}
