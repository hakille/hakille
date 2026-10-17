//! États : vide, attente, hors ligne, erreur (spec §19).
//!
//! Jamais d'écran blanc, jamais de « service indisponible ». Gabarit
//! d'erreur d'action, toujours dans cet ordre : fait → pas fait → actions.

use serde::{Deserialize, Serialize};

/// État d'un écran ou d'une carte asynchrone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScreenState {
    Empty { hint: String, examples: Vec<String> },
    LoadingFast,                       // < 400 ms : rien, pas de spinner
    LoadingSkeleton,                   // 400 ms → 2 s : squelette, sans balayage
    LoadingHonest { message: String }, // > 2 s : texte honnête + Annuler
    Offline { queued: usize },         // pastille « Local », file visible
    ProviderDown { provider: String },
    Locked, // ré-auth requise ; Journal lisible, masqué
}

impl ScreenState {
    /// Seuil spinner : rien sous 400 ms.
    pub const SPINNER_AFTER_MS: u32 = 400;
    /// Au-delà de 2 s : message honnête + bouton Annuler.
    pub const HONEST_AFTER_MS: u32 = 2000;

    pub fn for_elapsed_ms(ms: u32) -> Self {
        if ms < Self::SPINNER_AFTER_MS {
            Self::LoadingFast
        } else if ms < Self::HONEST_AFTER_MS {
            Self::LoadingSkeleton
        } else {
            Self::LoadingHonest {
                message: "Je cherche dans ta mémoire…".to_string(),
            }
        }
    }
}

/// Gabarit d'erreur d'action : fait → pas fait → que faire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionError {
    pub done: String,
    pub not_done: String,
    pub retry_label: String,
    pub undo_label: String,
    pub technical_detail: String,
}

impl ActionError {
    pub fn render(&self) -> String {
        format!(
            "Fait : {}\nPas fait : {}\n[{}] [{}]\nDétails : {}",
            self.done, self.not_done, self.retry_label, self.undo_label, self.technical_detail
        )
    }
}

/// File hors ligne : reprise sûre, rejouable sans doublon ; un envoi différé
/// de plus de 15 minutes est revalidé avant de partir.
pub const OFFLINE_REVALIDATE_MINUTES: u32 = 15;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loading_thresholds_and_error_template() {
        assert_eq!(ScreenState::for_elapsed_ms(100), ScreenState::LoadingFast);
        assert_eq!(
            ScreenState::for_elapsed_ms(500),
            ScreenState::LoadingSkeleton
        );
        match ScreenState::for_elapsed_ms(5000) {
            ScreenState::LoadingHonest { message } => assert!(message.contains("mémoire")),
            _ => panic!("attendu LoadingHonest"),
        }
        let e = ActionError {
            done: "Brouillon enregistré.".into(),
            not_done: "Mail non envoyé (hors ligne).".into(),
            retry_label: "Réessayer".into(),
            undo_label: "Annuler le brouillon".into(),
            technical_detail: "E_CONN 0x1A".into(),
        };
        let r = e.render();
        assert!(r.find("Fait").unwrap() < r.find("Pas fait").unwrap());
        assert!(r.find("Pas fait").unwrap() < r.find("Réessayer").unwrap());
        assert_eq!(OFFLINE_REVALIDATE_MINUTES, 15);
    }
}
