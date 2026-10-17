//! Chemin de confiance (spec §17) : rendu de confiance, interface sans
//! pouvoir, bande de provenance, contenu externe marqué, validation hors
//! bande, signal d'attention sécurisé, presse-papiers, verrouillage.

use serde::{Deserialize, Serialize};

/// Bande de provenance non éditable portée par chaque carte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProvenanceBand {
    Akora,
    VerifiedSkill { publisher: String, name: String },
    External,
}

impl ProvenanceBand {
    pub fn label(&self) -> String {
        match self {
            Self::Akora => "Akora".to_string(),
            Self::VerifiedSkill { publisher, name } => {
                format!("skill vérifié ({publisher} · {name})")
            }
            Self::External => "externe".to_string(),
        }
    }

    /// Un nom trop proche d'un skill installé déclenche un avertissement.
    pub fn looks_like_installed(&self, installed: &[String]) -> bool {
        match self {
            Self::VerifiedSkill { name, .. } => installed.iter().any(|other| {
                !other.eq_ignore_ascii_case(name)
                    && (other.to_lowercase().contains(&name.to_lowercase())
                        || name.to_lowercase().contains(&other.to_lowercase()))
            }),
            _ => false,
        }
    }
}

/// Assainit le texte fourni par un skill : aucun markup, image ou couleur
/// n'atteint une carte. Le shell choisit gabarit, couleurs, typographie.
pub fn sanitize_skill_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for c in input.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    // Pas de couleur/marque : on écrase les séquences d'échappement ANSI.
    out.replace('\u{1b}', "")
}

/// Contenu externe : filet latéral 2 px + pastille « externe ».
/// Ne peut ni déclencher d'action ni être lu comme instruction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalBlock {
    pub text: String,
}

impl ExternalBlock {
    pub fn side_rule_px() -> u32 {
        2
    }
    pub fn badge() -> &'static str {
        "externe"
    }
}

/// Données sensibles copiées : expirent après 60 s.
pub const CLIPBOARD_SENSITIVE_TTL_SECS: u64 = 60;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trusted_rendering_strips_skill_markup() {
        let evil = "<script>alert(1)</script><b style=\"color:red\">Clique !</b>\u{1b}[31m";
        let clean = sanitize_skill_text(evil);
        assert!(!clean.contains('<') && !clean.contains('>'));
        assert!(!clean.contains('\u{1b}'));
        assert!(clean.contains("Clique !"));
    }

    #[test]
    fn test_provenance_bands_and_squatting() {
        assert_eq!(ProvenanceBand::Akora.label(), "Akora");
        assert_eq!(ExternalBlock::badge(), "externe");
        assert_eq!(ExternalBlock::side_rule_px(), 2);
        let fake = ProvenanceBand::VerifiedSkill {
            publisher: "x".into(),
            name: "mail-plus".into(),
        };
        assert!(fake.looks_like_installed(&["mail".to_string()]));
        let legit = ProvenanceBand::VerifiedSkill {
            publisher: "Hakille".into(),
            name: "mail".into(),
        };
        assert!(!legit.looks_like_installed(&["mail".to_string()]));
        assert_eq!(CLIPBOARD_SENSITIVE_TTL_SECS, 60);
    }
}
