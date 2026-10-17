//! Niveaux d'interruption N0–N4 (spec §18) : le budget de silence, côté UI.
//!
//! Défaut : 3 crédits/jour (spec §29, à calibrer après 4 semaines de panel),
//! sans report. Deux refus d'affilée sur un type → désactivé 14 jours.
//! Réunion/Concentration plafonnent à N1 ; seul un N4 autorisé traverse.

/// Niveau d'interruption d'une proposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptLevel {
    /// Journal seulement. Tâche de routine terminée.
    Silent,
    /// La Présence change d'état, rien d'autre.
    Glow,
    /// Haptique ou son bref. Réponse à une demande directe.
    Discreet,
    /// Akora prend la parole. Suggestion proactive à haute valeur.
    Voice,
    /// Braise + son + haptique long-court. Alarme/sécurité définies.
    Urgent,
}

impl InterruptLevel {
    /// Coût en crédits : seule la proposition inférée (Voice) coûte 1.
    /// N4 (urgence définie par l'utilisateur) : 0, revue dans le Journal.
    pub fn credit_cost(&self) -> u32 {
        match self {
            Self::Voice => 1,
            _ => 0,
        }
    }

    /// Réunion/Concentration plafonnent à N1 (Glow).
    pub fn capped_at_glow(&self) -> Self {
        match self {
            Self::Silent | Self::Glow => *self,
            Self::Urgent => Self::Urgent, // seul un N4 autorisé traverse
            _ => Self::Glow,
        }
    }
}

/// Défaut : 3 crédits par jour, sans report d'un jour sur l'autre.
pub const DEFAULT_DAILY_CREDITS: u32 = 3;
/// Deux refus d'affilée → désactivé 14 jours.
pub const SNOOZE_AFTER_CONSECUTIVE_REFUSALS: u32 = 2;
pub const SNOOZE_DAYS: u32 = 14;

/// Compte les crédits restants visibles dans Réglages (lien « Pourquoi ce
/// message ? » vers la trace sur chaque interruption).
pub fn remaining_credits(daily: u32, spent_on_voice: u32) -> u32 {
    daily.saturating_sub(spent_on_voice)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interrupt_costs_and_caps() {
        assert_eq!(DEFAULT_DAILY_CREDITS, 3);
        assert_eq!(InterruptLevel::Voice.credit_cost(), 1);
        assert_eq!(InterruptLevel::Silent.credit_cost(), 0);
        assert_eq!(InterruptLevel::Urgent.credit_cost(), 0);
        assert_eq!(
            InterruptLevel::Discreet.capped_at_glow(),
            InterruptLevel::Glow
        );
        assert_eq!(
            InterruptLevel::Urgent.capped_at_glow(),
            InterruptLevel::Urgent
        );
        assert_eq!(remaining_credits(3, 1), 2);
        assert_eq!(remaining_credits(3, 9), 0);
        assert_eq!(SNOOZE_DAYS, 14);
    }
}
