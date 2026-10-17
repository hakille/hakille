//! Mouvement (spec §6) : durées, easings, ressort carte, haptique.
//!
//! Tout est token, jamais en dur. Mouvement réduit : les pulsations
//! deviennent des paliers d'opacité (voir `reduced_motion_opacity`).

/// Durées ms : 120 / 200 / 320 / 600.
pub const DURATION_MS: [u16; 4] = [120, 200, 320, 600];
/// Entrée : cubic-bezier(0.2, 0, 0, 1).
pub const EASE_IN: &str = "cubic-bezier(0.2, 0, 0, 1)";
/// Sortie : cubic-bezier(0.3, 0, 0.8, 0.15).
pub const EASE_OUT: &str = "cubic-bezier(0.3, 0, 0.8, 0.15)";
/// Ressort déplacement de cartes : raideur 380, amortissement 32, masse 1.
pub const SPRING_STIFFNESS: u16 = 380;
pub const SPRING_DAMPING: u16 = 32;
pub const SPRING_MASS: u16 = 1;
/// Haptique : tick 10 ms = validé.
pub const HAPTIC_TICK_MS: u16 = 10;
/// Barge-in : les 21 fils retombent en moins de 80 ms.
pub const BARGE_IN_COLLAPSE_MS: u16 = 80;
/// Silence : 600 ms après la dernière parole, les fils se replient en point.
pub const SPEECH_FOLD_SILENCE_MS: u16 = 600;
/// Appui long irréversible : 600 ms avec anneau braise.
pub const HOLD_TO_CONFIRM_MS: u16 = 600;
/// Anneau invité 1 px qui se vide (durée = durée du pass, défaut 1 h).
pub const GUEST_DEFAULT_SECONDS: u32 = 3600;

/// Pulsation Présence : (période ms, opacité min, opacité max, échelle max).
/// Veille = statique (période 0). Parole pilotée par l'enveloppe audio.
pub fn presence_rhythm(state: &str) -> (u32, f32, f32, f32) {
    match state {
        "listening" => (4000, 0.6, 1.0, 1.15),
        "acting" => (1200, 0.5, 1.0, 1.0),
        "urgent" => (800, 0.6, 1.0, 1.0),
        _ => (0, 0.4, 0.4, 1.0),
    }
}

/// Mouvement réduit : opacité par paliers, jamais d'échelle ni de
/// défilement automatique.
pub fn reduced_motion_opacity(phase01: f32) -> f32 {
    if phase01 < 0.5 { 0.6 } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_motion_tokens() {
        assert_eq!(DURATION_MS, [120, 200, 320, 600]);
        assert_eq!(HOLD_TO_CONFIRM_MS, 600);
        assert_eq!(BARGE_IN_COLLAPSE_MS, 80);
        let (period, lo, hi, _) = presence_rhythm("listening");
        assert_eq!(period, 4000);
        assert!((lo - 0.6).abs() < 1e-6 && (hi - 1.0).abs() < 1e-6);
        assert_eq!(presence_rhythm("idle").0, 0);
        assert_eq!(reduced_motion_opacity(0.2), 0.6);
        assert_eq!(reduced_motion_opacity(0.9), 1.0);
    }
}
