//! Chemin de confiance (spec §17) : rendu de confiance, interface sans
//! pouvoir, bande de provenance, contenu externe marqué, validation hors
//! bande, signal d'attention sécurisé, presse-papiers, verrouillage.

use serde::{Deserialize, Serialize};

/// Bande de provenance non éditable portée par chaque carte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProvenanceBand {
    Hakille,
    VerifiedSkill { publisher: String, name: String },
    External,
}

impl ProvenanceBand {
    pub fn label(&self) -> String {
        match self {
            Self::Hakille => "Hakille".to_string(),
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

/// Assainit le texte fourni par un skill : aucun markup, bloc script/style,
/// image ou séquence d'échappement ANSI n'atteint une carte.
pub fn sanitize_skill_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let lower = input.to_lowercase();
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    // Étape 1 : suppression intégrale des blocs <script>...</script> et <style>...</style>,
    // puis des balises HTML générales <...>
    while i < len {
        if bytes[i] == b'<' {
            let rest_lower = &lower[i..];
            if rest_lower.starts_with("<script") {
                if let Some(end_idx) = rest_lower.find("</script>") {
                    i += end_idx + "</script>".len();
                    continue;
                }
            } else if rest_lower.starts_with("<style") {
                if let Some(end_idx) = rest_lower.find("</style>") {
                    i += end_idx + "</style>".len();
                    continue;
                }
            }
            while i < len && bytes[i] != b'>' {
                i += 1;
            }
            if i < len && bytes[i] == b'>' {
                i += 1;
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }

    // Étape 2 : suppression complète des séquences d'échappement terminales ANSI
    // (CSI \x1b[...m, OSC \x1b]..., séquences Fe, et octets d'échappement nus)
    let mut clean = String::with_capacity(out.len());
    let chars: Vec<char> = out.chars().collect();
    let c_len = chars.len();
    let mut j = 0;

    while j < c_len {
        if chars[j] == '\u{001b}' {
            j += 1;
            if j < c_len {
                if chars[j] == '[' {
                    // Séquence CSI : consomme paramètres puis lettre de commande
                    j += 1;
                    while j < c_len
                        && (chars[j].is_ascii_digit() || chars[j] == ';' || chars[j] == '?')
                    {
                        j += 1;
                    }
                    if j < c_len && (chars[j].is_ascii_alphabetic() || chars[j] == '@') {
                        j += 1;
                    }
                } else if chars[j] == ']' {
                    // Séquence OSC : consomme jusqu'à BEL (\x07) ou String Terminator (\x1b\)
                    j += 1;
                    while j < c_len {
                        if chars[j] == '\u{0007}' {
                            j += 1;
                            break;
                        } else if chars[j] == '\u{001b}' && j + 1 < c_len && chars[j + 1] == '\\' {
                            j += 2;
                            break;
                        }
                        j += 1;
                    }
                } else if chars[j] >= '@' && chars[j] <= '_' {
                    // Séquence d'échappement 2 octets
                    j += 1;
                }
            }
        } else {
            clean.push(chars[j]);
            j += 1;
        }
    }

    clean
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
        let evil = "<script>stealKeys(); alert(1);</script><style>body { color:red; }</style><b style=\"color:red\">Clique !</b>\u{1b}[31m\u{1b}[1m";
        let clean = sanitize_skill_text(evil);
        assert!(!clean.contains('<') && !clean.contains('>'));
        assert!(!clean.contains('\u{1b}'));
        assert!(
            !clean.contains("[31m"),
            "Must not leak ANSI CSI parameters: {clean}"
        );
        assert!(
            !clean.contains("stealKeys"),
            "Must strip script body: {clean}"
        );
        assert!(
            !clean.contains("color:red"),
            "Must strip style body and attributes: {clean}"
        );
        assert_eq!(clean, "Clique !");
    }

    #[test]
    fn test_provenance_bands_and_squatting() {
        assert_eq!(ProvenanceBand::Hakille.label(), "Hakille");
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
