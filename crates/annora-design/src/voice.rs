//! Voix d'Akora (spec §9, §20) : phrases courtes, verbes d'action,
//! tutoiement FR par défaut, incertitude réelle dans le ton.
//!
//! Ni émojis, ni « En tant qu'IA… », ni excuses de politesse.
//! Résultat d'abord, nuance ensuite. Max deux phrases à l'oral.

/// Niveau de détail oral : Court (défaut), Normal, Détaillé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    Short,
    Normal,
    Detailed,
}

/// Vitesse 1,0 par défaut, réglable 0,7 → 1,6.
pub const DEFAULT_SPEECH_RATE: f32 = 1.0;
pub const SPEECH_RATE_MIN: f32 = 0.7;
pub const SPEECH_RATE_MAX: f32 = 1.6;

/// Serre une vitesse demandée dans [0,7 ; 1,6] (délais réglables, §12).
pub fn clamp_speech_rate(requested: f32) -> f32 {
    requested.clamp(SPEECH_RATE_MIN, SPEECH_RATE_MAX)
}
/// Listes : trois éléments à l'oral, le reste au Journal.
pub const SPOKEN_LIST_MAX: usize = 3;
/// Sans réponse après 8 s : la proposition passe en N0, sans insister.
pub const NO_ANSWER_SILENCE_SECS: u64 = 8;

/// Formule une réponse orale honnête : résultat d'abord, nuance ensuite,
/// hedges proportionnés à l'incertitude réelle (jamais la même assurance).
pub fn speak_short(result: &str, confidence: f64, detail_hint: &str) -> String {
    let result = result.trim().trim_end_matches('.');
    if confidence >= 0.85 {
        format!("{result}. {detail_hint}")
    } else if confidence >= 0.5 {
        format!("Je pense que {result}, mais je n'en suis pas sûr. Tu peux confirmer ?")
    } else {
        format!("Je ne suis pas sûr. {detail_hint} Tu peux confirmer ?")
    }
}

/// Tronque une liste orale à 3 éléments + renvoi Journal.
pub fn spoken_list(items: &[String]) -> (Vec<String>, Option<String>) {
    if items.len() <= SPOKEN_LIST_MAX {
        (items.to_vec(), None)
    } else {
        (
            items[..SPOKEN_LIST_MAX].to_vec(),
            Some("Je te mets le détail dans le Journal.".to_string()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_tone_follows_uncertainty() {
        let sure = speak_short("J'arrive à 18 h", 0.95, "Rien ne quitte ton appareil.");
        assert!(sure.starts_with("J'arrive à 18 h."));
        assert!(!sure.contains("😊") && !sure.contains("En tant qu'IA"));
        let unsure = speak_short("la date est le 12", 0.4, "Détail au Journal.");
        assert!(unsure.contains("Je ne suis pas sûr"));
        let (spoken, overflow) = spoken_list(&[
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ]);
        assert_eq!(spoken.len(), 3);
        assert!(overflow.unwrap().contains("Journal"));
        assert_eq!(DEFAULT_SPEECH_RATE, 1.0);
        assert_eq!(NO_ANSWER_SILENCE_SECS, 8);
        assert_eq!(clamp_speech_rate(2.0), 1.6);
        assert_eq!(clamp_speech_rate(0.1), 0.7);
        assert_eq!(clamp_speech_rate(1.2), 1.2);
    }
}
