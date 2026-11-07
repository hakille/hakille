//! DTO de streaming et modèles de données pour l'Aura Ring HUD ambiant (Pilier 5).
//!
//! L'Aura Ring est un indicateur visuel signature réactif qui reflète en temps réel
//! le niveau de confiance continu $\tau \in [0.0, 1.0]$, les défis d'élévation
//! de privilèges (step-up liveness) et les modes de verrouillage / détresse.

use serde::{Deserialize, Serialize};

/// DTO transmis en continu du noyau vers l'interface native / shell HUD
/// pour le rendu réactif de l'Aura Ring.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AuraRingUpdateDto {
    /// Score continu de confiance $\tau \in [0.0, 1.0]$.
    pub tau: f32,
    /// Nom de l'état chromatique ("EmeraldHighTrust", "AmberAmbient", "VioletStepUp", "RedLocked").
    pub state_name: String,
    /// Code couleur hexadécimal primaire de l'anneau (ex. "#10B981").
    pub primary_hex: String,
    /// Code couleur hexadécimal de la lueur / bloom avec canal alpha (ex. "#10B98173").
    pub glow_hex: String,
    /// Période de pulsation / respiration en millisecondes.
    pub pulse_period_ms: u32,
    /// Facteur d'échelle périodique sinusoïdal d'animation $\in [0.95, 1.05]$.
    pub scale_factor: f32,
    /// Horodatage Unix en millisecondes au moment de l'émission.
    pub timestamp_ms: u64,
}

impl AuraRingUpdateDto {
    /// Crée une nouvelle instance d'AuraRingUpdateDto.
    pub fn new(
        tau: f32,
        state_name: impl Into<String>,
        primary_hex: impl Into<String>,
        glow_hex: impl Into<String>,
        pulse_period_ms: u32,
        scale_factor: f32,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            tau,
            state_name: state_name.into(),
            primary_hex: primary_hex.into(),
            glow_hex: glow_hex.into(),
            pulse_period_ms,
            scale_factor,
            timestamp_ms,
        }
    }

    /// `true` si l'anneau est dans l'état de haute confiance émeraude (tau >= 0.90).
    pub fn is_high_trust(&self) -> bool {
        self.state_name == "EmeraldHighTrust"
    }

    /// `true` si l'anneau est dans l'état ambiant fluide ambre (0.40 <= tau < 0.90).
    pub fn is_ambient(&self) -> bool {
        self.state_name == "AmberAmbient"
    }

    /// `true` si l'anneau est dans l'état de défi step-up violet.
    pub fn is_step_up(&self) -> bool {
        self.state_name == "VioletStepUp"
    }

    /// `true` si l'anneau est dans l'état verrouillé / détresse rouge.
    pub fn is_locked(&self) -> bool {
        self.state_name == "RedLocked"
    }
}
