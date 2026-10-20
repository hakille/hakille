//! Appui long de validation (spec §8) : 600 ms, anneau braise, ticks.
//!
//! L'interface ne transmet JAMAIS la durée : elle envoie les événements
//! press/release horodatés, le NOYAU mesure par le chemin d'entrée de
//! confiance et n'accepte que `hold_ms >= 600`. Pas de double modale.

use crate::motion::HOLD_TO_CONFIRM_MS;

/// Événement du chemin d'entrée de confiance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldEvent {
    Pressed { at_ms: u64 },
    Released { at_ms: u64 },
    Tick { elapsed_ms: u64 },
}

/// État de l'appui long côté noyau.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldState {
    Idle,
    Holding { elapsed_ms: u16 },
    Confirmed,
    Cancelled,
}

/// Mesure noyau de l'appui long. `feed` retourne l'état + s'il faut émettre
/// un tick haptique/sonore (début et fin uniquement).
pub fn feed_hold(state: HoldState, event: HoldEvent) -> (HoldState, bool) {
    match (state, event) {
        (HoldState::Idle, HoldEvent::Pressed { .. }) => {
            (HoldState::Holding { elapsed_ms: 0 }, true)
        }
        (HoldState::Holding { elapsed_ms }, HoldEvent::Tick { elapsed_ms: dt }) => {
            let next = elapsed_ms.saturating_add(dt.min(u16::MAX as u64) as u16);
            if next >= HOLD_TO_CONFIRM_MS {
                (HoldState::Confirmed, true)
            } else {
                (HoldState::Holding { elapsed_ms: next }, false)
            }
        }
        (HoldState::Holding { elapsed_ms }, HoldEvent::Released { .. }) => {
            if elapsed_ms >= HOLD_TO_CONFIRM_MS {
                (HoldState::Confirmed, true)
            } else {
                (HoldState::Cancelled, false)
            }
        }
        (s, _) => (s, false),
    }
}

/// Fraction d'anneau braise 0..1 pour le rendu.
pub fn hold_progress(state: HoldState) -> f32 {
    match state {
        HoldState::Holding { elapsed_ms } => {
            (elapsed_ms as f32 / HOLD_TO_CONFIRM_MS as f32).clamp(0.0, 1.0)
        }
        HoldState::Confirmed => 1.0,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hold_confirm_and_cancel() {
        let (s, tick) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 0 });
        assert_eq!(s, HoldState::Holding { elapsed_ms: 0 });
        assert!(tick);
        let (s, _) = feed_hold(s, HoldEvent::Tick { elapsed_ms: 599 });
        assert!(matches!(s, HoldState::Holding { elapsed_ms: 599 }));
        assert!((hold_progress(s) - 599.0 / 600.0).abs() < 1e-6);
        // Relâchement précoce → annulé, pas de tick.
        let (s, tick) = feed_hold(s, HoldEvent::Released { at_ms: 700 });
        assert_eq!(s, HoldState::Cancelled);
        assert!(!tick);
        // Tenu 600 → confirmé avec tick final.
        let (s, _) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: 0 });
        let (s, tick) = feed_hold(s, HoldEvent::Tick { elapsed_ms: 600 });
        assert_eq!(s, HoldState::Confirmed);
        assert!(tick);
        assert_eq!(hold_progress(s), 1.0);
    }
}
