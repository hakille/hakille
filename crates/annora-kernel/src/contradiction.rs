use annora_core::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContradictionAnalysis {
    pub has_contradiction: bool,
    pub primary_argument: String,
    pub counter_argument: String,
    pub resolution_strategy: ContradictionResolution,
    pub confidence_penalty: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContradictionResolution {
    Affirmed,
    Overruled,
    RequiresUserConfirmation,
}

pub struct InternalContradictionEngine;

impl InternalContradictionEngine {
    pub fn new() -> Self {
        Self
    }

    /// Découpe un texte en tokens alphanumériques (unicode) pour des
    /// comparaisons à mot exact — indispensable en FR où « non » et « oui »
    /// sont des sous-chaînes de mots courants (« nom », « Louis »).
    fn tokens(lower: &str) -> Vec<String> {
        lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .map(|t| t.to_string())
            .collect()
    }

    fn has_substring(hay: &str, needles: &[&str]) -> bool {
        needles.iter().any(|n| hay.contains(n))
    }

    fn has_token(tokens: &[String], needles: &[&str]) -> bool {
        tokens.iter().any(|t| needles.contains(&t.as_str()))
    }

    pub fn critique(
        &self,
        hypothesis: &str,
        context_facts: &[String],
    ) -> Result<ContradictionAnalysis> {
        let lower_hypo = hypothesis.to_lowercase();
        let hypo_tokens = Self::tokens(&lower_hypo);

        // Paires EN historiques (sous-chaînes, comportement préservé).
        const EN_PAIRS: [(&[&str], &[&str]); 3] = [
            (
                &["delete", "deletes", "deleted", "deleting"],
                &["preserve", "preserves", "keep"],
            ),
            (
                &["allow", "allows", "allowed"],
                &["deny", "denies", "denied"],
            ),
            (&["true"], &["false"]),
        ];
        // Paires FR (tokens exacts : « regarde » ≠ « garde », « nom » ≠ « non »).
        const FR_PAIRS: [(&[&str], &[&str]); 3] = [
            (
                &[
                    "supprime",
                    "supprimer",
                    "suppression",
                    "efface",
                    "effacer",
                    "détruit",
                    "détruire",
                ],
                &[
                    "conserve",
                    "conserver",
                    "préserve",
                    "préserver",
                    "garde",
                    "garder",
                    "maintien",
                    "maintenir",
                ],
            ),
            (
                &[
                    "autorise",
                    "autoriser",
                    "permet",
                    "permettre",
                    "accepte",
                    "accepter",
                ],
                &[
                    "interdit",
                    "interdire",
                    "refuse",
                    "refuser",
                    "rejette",
                    "rejeter",
                    "bloque",
                    "bloquer",
                ],
            ),
            (&["oui", "vrai"], &["non", "faux"]),
        ];

        let mut contradicting_fact = None;
        for fact in context_facts {
            let lower_fact = fact.to_lowercase();
            let fact_tokens = Self::tokens(&lower_fact);
            // Sens historique : hypothèse-A + fait-B (bidirectionnel ajouté,
            // le sens historique reste détecté en premier).
            let mut opposed = false;
            for (a, b) in &EN_PAIRS {
                if (Self::has_substring(&lower_hypo, a) && Self::has_substring(&lower_fact, b))
                    || (Self::has_substring(&lower_hypo, b) && Self::has_substring(&lower_fact, a))
                {
                    opposed = true;
                    break;
                }
            }
            if !opposed {
                for (a, b) in &FR_PAIRS {
                    if (Self::has_token(&hypo_tokens, a) && Self::has_token(&fact_tokens, b))
                        || (Self::has_token(&hypo_tokens, b) && Self::has_token(&fact_tokens, a))
                    {
                        opposed = true;
                        break;
                    }
                }
            }
            if opposed {
                contradicting_fact = Some(fact.clone());
                break;
            }
        }

        if let Some(fact) = contradicting_fact {
            Ok(ContradictionAnalysis {
                has_contradiction: true,
                primary_argument: hypothesis.to_string(),
                counter_argument: format!("Direct contradiction found with known fact: '{}'", fact),
                resolution_strategy: ContradictionResolution::RequiresUserConfirmation,
                confidence_penalty: 0.35,
            })
        } else {
            Ok(ContradictionAnalysis {
                has_contradiction: false,
                primary_argument: hypothesis.to_string(),
                counter_argument: "No internal contradiction identified against existing memory"
                    .to_string(),
                resolution_strategy: ContradictionResolution::Affirmed,
                confidence_penalty: 0.0,
            })
        }
    }
}
