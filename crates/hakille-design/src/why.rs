//! Feuille « Pourquoi » à trois niveaux (spec §25) : phrase, faits, raison.
//!
//! Reconstruite du journal signé, jamais générée après coup. Si l'info
//! n'existe pas, la feuille le dit. Contenu externe entre filets.
//! Confiance en mots en surface (basse/moyenne/haute), valeur au niveau 3.

use serde::{Deserialize, Serialize};

/// Fait du graphe mobilisé, avec date, source, fraîcheur + correction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactRef {
    pub label: String,
    pub kind: String,
    pub date: String,
    pub source: String,
    pub freshness: String,
}

/// Feuille à trois niveaux. L'utilisateur s'arrête où il veut.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhySheet {
    /// Niveau 1 : en une phrase.
    pub sentence: String,
    /// Niveau 2 : faits utilisés (+ bouton « Ce fait est faux » par fait).
    pub facts: Vec<FactRef>,
    /// Niveau 3 : plan, pouvoirs, confiance (mot + valeur), alternatives,
    /// modèle/fournisseur, coût, sortie données, bascule de décision.
    pub plan: String,
    pub powers: Vec<String>,
    pub confidence_word: String,
    pub confidence_value: f64,
    pub alternatives: Vec<String>,
    pub model_line: String,
    pub cost_label: String,
    pub data_line: String,
    pub would_change: String,
}

impl WhySheet {
    /// Mot de confiance en surface : basse / moyenne / haute.
    pub fn confidence_word(value: f64) -> &'static str {
        if value < 0.5 {
            "basse"
        } else if value < 0.85 {
            "moyenne"
        } else {
            "haute"
        }
    }

    /// Feuille honnête quand la trace manque.
    pub fn missing(reason: &str) -> Self {
        Self {
            sentence: "Je n'ai pas gardé la raison de ce choix.".to_string(),
            facts: vec![],
            plan: reason.to_string(),
            powers: vec![],
            confidence_word: "basse".to_string(),
            confidence_value: 0.0,
            alternatives: vec![],
            model_line: "inconnu".to_string(),
            cost_label: "inconnu".to_string(),
            data_line: "inconnu".to_string(),
            would_change: "inconnu".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_why_sheet_words_and_missing() {
        assert_eq!(WhySheet::confidence_word(0.2), "basse");
        assert_eq!(WhySheet::confidence_word(0.7), "moyenne");
        assert_eq!(WhySheet::confidence_word(0.95), "haute");
        let m = WhySheet::missing("entrée sans trace");
        assert!(m.sentence.contains("pas gardé"));
        assert!(m.facts.is_empty());
    }
}
