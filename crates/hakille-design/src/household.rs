//! Foyer, invités, moments sensibles (spec §21).
//!
//! Initiale 600 ms à la reconnaissance ; doute → pas de personnalisation.
//! Invité : pass 1 h par défaut, anneau 1 px qui se vide, expiration auto
//! au registre. Mineur : simplifié, sans wallet, supervision par catégories.
//! Détresse : écran calme, pas de braise, pas d'animation, aide à joindre.

use serde::{Deserialize, Serialize};

/// Durée d'affichage de l'initiale à la reconnaissance : 600 ms.
pub const IDENTITY_INITIAL_MS: u32 = 600;

/// Pass invité : pouvoir précis + durée (défaut 1 h), anneau 1 px.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestPass {
    pub name: String,
    pub power: String,
    pub duration_secs: u32,
    pub elapsed_secs: u32,
}

impl GuestPass {
    pub fn new(name: &str, power: &str, duration_secs: u32) -> Self {
        Self {
            name: name.to_string(),
            power: power.to_string(),
            duration_secs,
            elapsed_secs: 0,
        }
    }

    /// Anneau 1 px qui se vide : fraction restante 0..1.
    pub fn ring_fraction(&self) -> f32 {
        if self.duration_secs == 0 {
            return 0.0;
        }
        (1.0 - self.elapsed_secs as f32 / self.duration_secs as f32).clamp(0.0, 1.0)
    }

    pub fn expired(&self) -> bool {
        self.elapsed_secs >= self.duration_secs
    }
}

pub const GUEST_RING_PX: u32 = 1;

/// Mode détresse : calme, pas de braise, pas d'animation, une phrase,
/// geste contact de confiance + urgences du pays. Rien n'est noté.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistressMode {
    pub active: bool,
    pub trusted_contact: Option<String>,
}

impl DistressMode {
    pub fn calm_copy() -> &'static str {
        "Je reste là. Un geste pour appeler ton contact, un autre pour les urgences."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guest_ring_and_distress() {
        let mut pass = GuestPass::new("Samir", "wifi", 3600);
        assert!((pass.ring_fraction() - 1.0).abs() < 1e-6);
        pass.elapsed_secs = 1800;
        assert!((pass.ring_fraction() - 0.5).abs() < 1e-6);
        assert!(!pass.expired());
        pass.elapsed_secs = 3600;
        assert!(pass.expired());
        assert_eq!(GUEST_RING_PX, 1);
        assert!(DistressMode::calm_copy().contains("contact"));
    }
}
