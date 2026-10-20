//! Échelle d'autonomie par domaine (spec §24) : crans 0–4, jamais globaux.
//!
//! Chaque domaine démarre au cran 1. Montée proposée (jamais imposée),
//! descente immédiate en un geste + automatique après annulation d'une
//! action autonome. Crans 3–4 : enveloppe visible, expiration 90 jours.
//! Toujours validés : nouveau pouvoir, partage mémoire, suppression
//! définitive, dépense hors enveloppe.

use serde::{Deserialize, Serialize};

/// Domaine réglable : Mail, Agenda, Maison, Fichiers, Paiement, Code…
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Domain(pub String);

/// Cran 0 Observer … 4 Déléguer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Crank {
    Observe = 0,
    Prepare = 1,
    ActReversible = 2,
    ActEnvelope = 3,
    Delegate = 4,
}

impl Crank {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Observe => "Cran 0 sur 4, observer",
            Self::Prepare => "Cran 1 sur 4, préparer",
            Self::ActReversible => "Cran 2 sur 4, agir sur le réversible",
            Self::ActEnvelope => "Cran 3 sur 4, agir dans une enveloppe",
            Self::Delegate => "Cran 4 sur 4, déléguer",
        }
    }
}

/// Enveloppe visible exigée aux crans 3–4 : plafond, volume, durée.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub max_cost_cents: u64,
    pub max_actions: u32,
    pub days_valid: u32,
}

/// Expiration des crans 3–4 : 90 jours, reconfirmation.
pub const ENVELOPE_EXPIRY_DAYS: u32 = 90;
/// Seuil de proposition de montée : 20 actions validées sans correction
/// sur 30 jours (à calibrer par domaine).
pub const PROMOTION_VALIDATED_ACTIONS: u32 = 20;
pub const PROMOTION_WINDOW_DAYS: u32 = 30;

/// État d'autonomie d'un domaine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainAutonomy {
    pub domain: Domain,
    pub crank: Crank,
    pub envelope: Option<Envelope>,
    pub validated_without_correction: u32,
}

impl DomainAutonomy {
    pub fn new(domain: &str) -> Self {
        Self {
            domain: Domain(domain.to_string()),
            crank: Crank::Prepare, // tout domaine démarre au cran 1
            envelope: None,
            validated_without_correction: 0,
        }
    }

    /// Une action validée sans correction rapproche de la proposition.
    pub fn record_validated(&mut self) {
        self.validated_without_correction += 1;
    }

    /// Annulation d'une action autonome : descente immédiate d'un cran.
    pub fn record_reverted(&mut self) {
        self.crank = match self.crank {
            Crank::Observe => Crank::Observe,
            Crank::Prepare => Crank::Observe,
            Crank::ActReversible => Crank::Prepare,
            Crank::ActEnvelope => Crank::ActReversible,
            Crank::Delegate => Crank::ActEnvelope,
        };
        self.validated_without_correction = 0;
    }

    /// La montée est-elle proposable (jamais imposée) ?
    pub fn promotion_offerable(&self) -> bool {
        (self.crank == Crank::Prepare || self.crank == Crank::ActReversible)
            && self.validated_without_correction >= PROMOTION_VALIDATED_ACTIONS
    }
}

/// Actions toujours validées, quel que soit le cran.
pub fn always_validated(action_kind: &str) -> bool {
    matches!(
        action_kind,
        "new_power" | "share_memory" | "hard_delete" | "spend_over_envelope"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autonomy_ladder() {
        let mut d = DomainAutonomy::new("Mail");
        assert_eq!(d.crank, Crank::Prepare);
        assert_eq!(d.crank.label(), "Cran 1 sur 4, préparer");
        for _ in 0..20 {
            d.record_validated();
        }
        assert!(d.promotion_offerable());
        d.record_reverted(); // cran 1 → 0, compteur remis à zéro
        assert_eq!(d.crank, Crank::Observe);
        assert!(!d.promotion_offerable());
        assert!(always_validated("hard_delete"));
        assert!(always_validated("new_power"));
        assert!(!always_validated("send_reversible"));
        assert_eq!(ENVELOPE_EXPIRY_DAYS, 90);
    }
}
