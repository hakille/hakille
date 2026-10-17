//! Carte d'action (spec §8) : verbe + objet, raison + Pourquoi, 3 pastilles,
//! Valider (braise) / Modifier / Refuser. Jamais plus d'une carte « décision
//! requise » à l'écran. Irréversible → appui long 600 ms, pas de modale.

use serde::{Deserialize, Serialize};

/// Pastille Données : où vont les données si on valide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataPastille {
    Local,
    Cloud {
        provider: String,
        weight_ko: Option<u64>,
    },
}

impl DataPastille {
    pub fn label(&self) -> String {
        match self {
            Self::Local => "Local".to_string(),
            Self::Cloud {
                provider,
                weight_ko,
            } => match weight_ko {
                Some(ko) => format!("Cloud : {provider} (≈ {ko} Ko)"),
                None => format!("Cloud : {provider}"),
            },
        }
    }
}

/// Pastille Réversibilité : icône + mot, jamais couleur seule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reversibility {
    Undoable { undo_seconds: u32 },
    Final,
}

impl Reversibility {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Undoable { .. } => "− annulable",
            Self::Final => "+ définitif",
        }
    }
}

/// Carte d'action rendue par le shell (données fournies par le skill).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionCardView {
    pub verb: String,
    pub object: String,
    pub reason: String,
    pub why_trace: String,
    pub cost_label: String,
    pub data: DataPastille,
    pub reversibility: Reversibility,
    /// Irréversible → Valider exige l'appui long (anneau braise).
    pub needs_hold: bool,
    pub provenance_label: String,
    pub is_external: bool,
}

impl ActionCardView {
    /// Une seule carte « décision requise » à la fois : le shell refuse la
    /// seconde tant que la première est ouverte.
    pub fn single_decision_open(open: &[ActionCardView]) -> bool {
        open.len() <= 1
    }

    /// Pastilles dans l'ordre imposé : coût, données, réversibilité.
    pub fn pastilles(&self) -> [String; 3] {
        [
            self.cost_label.clone(),
            self.data.label(),
            self.reversibility.label().to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_card_pastilles_order_and_hold() {
        let card = ActionCardView {
            verb: "Envoyer".into(),
            object: "ce mail à Awa".into(),
            reason: "Tu as demandé de répondre.".into(),
            why_trace: "…".into(),
            cost_label: "0 €".into(),
            data: DataPastille::Local,
            reversibility: Reversibility::Undoable { undo_seconds: 30 },
            needs_hold: false,
            provenance_label: "Akora".into(),
            is_external: false,
        };
        assert_eq!(
            card.pastilles(),
            [
                "0 €".to_string(),
                "Local".to_string(),
                "− annulable".to_string()
            ]
        );
        assert!(ActionCardView::single_decision_open(&[card]));
        let cloud = DataPastille::Cloud {
            provider: "OpenAI".into(),
            weight_ko: Some(40),
        };
        assert_eq!(cloud.label(), "Cloud : OpenAI (≈ 40 Ko)");
    }
}
