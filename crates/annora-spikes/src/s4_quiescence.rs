//! Spike S4 — rendu événementiel : 0 réveil/s en Veille (spec §10, §11).
//!
//! Preuve outillée ici : en Veille (Idle + HUD masqué + pas de validation),
//! le shell ne programme AUCUN travail — ni frame, ni timer, ni tick.
//! Critère labo restant : 0 réveil/s mesuré côté OS en Veille, boucle
//! suspendue quand la fenêtre est masquée (plan B sinon).

use annora_ui::{CostCounter, EphemeralHud, HudState, PresenceIndicator, SilenceBudgetTracker};
use annora_ui_shell::ShellSnapshot;

/// Le shell a-t-il du travail programmé (frame/timer) à cet instant ?
/// Veille vraie ⇒ false : l'event-loop peut dormir jusqu'au prochain
/// événement OS (hotkey, VAD, tray). Tout état animé ⇒ true.
pub fn shell_needs_frame(presence: &PresenceIndicator, hud: &EphemeralHud, _now_ms: u64) -> bool {
    use annora_ui::PresenceState;
    match presence.state() {
        // Veille + HUD masqué : statique, 0 image/s.
        PresenceState::Idle => !matches!(hud.state(), HudState::Hidden),
        // Respiration/anneaux/parole/urgence : animation pilotée.
        PresenceState::Listening
        | PresenceState::Acting
        | PresenceState::Speaking
        | PresenceState::Urgent => true,
        // Anneau braise fixe : statique sauf si le HUD vit à côté.
        PresenceState::DecisionRequired => !matches!(hud.state(), HudState::Hidden),
    }
}

/// Construit un snapshot représentatif sans travail (pour la galerie texte).
pub fn idle_snapshot() -> ShellSnapshot {
    let presence = PresenceIndicator::new();
    let hud = EphemeralHud::new();
    let cost = CostCounter::new(500, "EUR");
    let silence = SilenceBudgetTracker::new(3);
    ShellSnapshot::from_states(&presence, &hud, &cost, &silence, 0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use annora_ui::PresenceState;

    #[test]
    fn test_s4_idle_is_quiescent() {
        let presence = PresenceIndicator::new();
        let hud = EphemeralHud::new();
        assert!(!shell_needs_frame(&presence, &hud, 0));
        assert!(!shell_needs_frame(&presence, &hud, 1_000_000));
    }

    #[test]
    fn test_s4_animated_states_need_frames() {
        for state in [
            PresenceState::Listening,
            PresenceState::Acting,
            PresenceState::Speaking,
            PresenceState::Urgent,
        ] {
            let mut presence = PresenceIndicator::new();
            presence.set_state(state);
            let hud = EphemeralHud::new();
            assert!(shell_needs_frame(&presence, &hud, 0), "{state:?}");
        }
        // Décision requise seule : anneau fixe, pas de frame.
        let mut presence = PresenceIndicator::new();
        presence.set_state(PresenceState::DecisionRequired);
        assert!(!shell_needs_frame(&presence, &EphemeralHud::new(), 0));
    }

    #[test]
    fn test_s4_hud_life_needs_frames_until_hidden() {
        let presence = PresenceIndicator::new();
        let mut hud = EphemeralHud::new();
        hud.open_prompt();
        assert!(shell_needs_frame(&presence, &hud, 0));
        hud.dismiss();
        assert!(!shell_needs_frame(&presence, &hud, 0));
    }
}
