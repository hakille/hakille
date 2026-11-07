//! Moteur de rendu et machine à états pour l'Aura Ring HUD ambiant (Pilier 5).
//!
//! L'Aura Ring est l'anneau visuel chromatique signature de Hakille qui matérialise
//! en continu le score de confiance multi-factoriel $\tau \in [0.0, 1.0]$.
//!
//! Les 4 états chromatiques stricts sont :
//! 1. `EmeraldHighTrust` ($\tau \ge 0.90$) : Vert Émeraude 500 (`#10B981`), respiration lente et sereine (6000 ms), alpha lueur 0.45.
//! 2. `AmberAmbient` ($0.40 \le \tau < 0.90$) : Ambre 500 (`#F59E0B`), pulsation fluide normale (3500 ms), alpha lueur 0.30.
//! 3. `VioletStepUp` : Défi actif d'élévation / liveness biométrique en cours, Violet 500 (`#8B5CF6`), ondulation rapide (1000 ms), alpha lueur 0.60.
//! 4. `RedLocked` ($\tau < 0.40$, verrouillage, détresse, ou révocation) : Rouge 500 (`#EF4444`), double flash urgent (800 ms), alpha lueur 0.70.
//!
//! L'animation oscille périodiquement selon une onde sinusoïdale d'échelle bornée dans $[0.95, 1.05]$.

use hakille_ui_protocol::AuraRingUpdateDto;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

/// Représentation chromatique RGBA avec composante alpha flottante normalisée [0.0, 1.0].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AuraColorRgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

impl AuraColorRgba {
    /// Crée une nouvelle couleur RGBA.
    pub const fn new(r: u8, g: u8, b: u8, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Convertit la couleur au format hexadécimal RVB standard à 6 caractères (ex. `"#10B981"`).
    pub fn to_hex_rgb(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    /// Convertit la couleur au format hexadécimal RGBA à 8 caractères avec alpha quantifié sur 8 bits (ex. `"#10B98173"`).
    pub fn to_hex_rgba(&self) -> String {
        let alpha_byte = (self.a.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!(
            "#{:02X}{:02X}{:02X}{:02X}",
            self.r, self.g, self.b, alpha_byte
        )
    }

    /// Convertit la couleur en chaîne de style CSS standard `rgba(r, g, b, a)`.
    pub fn to_css_rgba(&self) -> String {
        format!("rgba({}, {}, {}, {:.2})", self.r, self.g, self.b, self.a)
    }
}

impl std::fmt::Display for AuraColorRgba {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex_rgba())
    }
}

/// Les quatre états visuels signature de l'Aura Ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuraVisualState {
    /// Vert Émeraude 500 : Haute confiance ($\tau \ge 0.90$), autorisations critiques (Wallet/Admin) accordées.
    EmeraldHighTrust,
    /// Ambre 500 : Session ambiante nominale ($0.40 \le \tau < 0.90$), présence passive.
    AmberAmbient,
    /// Violet 500 : Défi actif d'élévation / challenge de vivacité en cours (Step-Up Liveness).
    VioletStepUp,
    /// Rouge 500 : Confiance insuffisante ($\tau < 0.40$), poste verrouillé, détresse active ou révocation.
    RedLocked,
}

impl AuraVisualState {
    /// Nom canonique lisible de l'état chromatique.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::EmeraldHighTrust => "EmeraldHighTrust",
            Self::AmberAmbient => "AmberAmbient",
            Self::VioletStepUp => "VioletStepUp",
            Self::RedLocked => "RedLocked",
        }
    }

    /// Alias de commodité pour `name()`.
    pub const fn state_name(&self) -> &'static str {
        self.name()
    }

    /// Couleur primaire pleine (alpha = 1.0) de l'anneau visuel.
    pub const fn primary_color(&self) -> AuraColorRgba {
        match self {
            // Emerald 500 (#10B981)
            Self::EmeraldHighTrust => AuraColorRgba::new(16, 185, 129, 1.0),
            // Amber 500 (#F59E0B)
            Self::AmberAmbient => AuraColorRgba::new(245, 158, 11, 1.0),
            // Violet 500 (#8B5CF6)
            Self::VioletStepUp => AuraColorRgba::new(139, 92, 246, 1.0),
            // Red 500 (#EF4444)
            Self::RedLocked => AuraColorRgba::new(239, 68, 68, 1.0),
        }
    }

    /// Opacité normalisée de la lueur diffuse ambiante (bloom).
    pub const fn glow_alpha(&self) -> f32 {
        match self {
            Self::EmeraldHighTrust => 0.45,
            Self::AmberAmbient => 0.30,
            Self::VioletStepUp => 0.60,
            Self::RedLocked => 0.70,
        }
    }

    /// Couleur de la lueur ambiante avec son canal alpha signature.
    pub const fn glow_color(&self) -> AuraColorRgba {
        let base = self.primary_color();
        AuraColorRgba::new(base.r, base.g, base.b, self.glow_alpha())
    }

    /// Période complète du cycle de pulsation / respiration en millisecondes.
    pub const fn pulse_period_ms(&self) -> u32 {
        match self {
            Self::EmeraldHighTrust => 6000, // Respiration calme et lente (6,0 s)
            Self::AmberAmbient => 3500,     // Rythme fluide régulier (3,5 s)
            Self::VioletStepUp => 1000,     // Ondulation rapide de balayage (1,0 s)
            Self::RedLocked => 800,         // Double impulsion d'urgence (0,8 s)
        }
    }

    /// Code hexadécimal primaire (#RRGGBB).
    pub const fn primary_hex(&self) -> &'static str {
        match self {
            Self::EmeraldHighTrust => "#10B981",
            Self::AmberAmbient => "#F59E0B",
            Self::VioletStepUp => "#8B5CF6",
            Self::RedLocked => "#EF4444",
        }
    }

    /// Code hexadécimal de la lueur avec canal alpha (#RRGGBBAA).
    pub fn glow_hex(&self) -> String {
        self.glow_color().to_hex_rgba()
    }

    /// Calcule le facteur d'échelle d'animation sinusoïdale $[0.95, 1.05]$ pour un horodatage donné en millisecondes.
    pub fn animation_factor(&self, timestamp_ms: u64) -> f32 {
        let period = self.pulse_period_ms() as f32;
        if period <= 0.0 {
            return 1.0;
        }
        let phase = (timestamp_ms as f32 % period) / period;
        let wave = (phase * 2.0 * PI).sin();
        1.0 + 0.05 * wave
    }
}

/// Moteur de widget réactif de l'Aura Ring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuraRingWidget {
    /// Niveau continu de confiance $\tau \in [0.0, 1.0]$.
    pub tau: f32,
    /// Indique si un défi step-up / vivacité est en cours d'exécution.
    pub is_step_up_active: bool,
    /// Verrouillage manuel ou fail-closed.
    pub is_locked: bool,
    /// Mode détresse sous contrainte (Silent Distress Blink).
    pub is_distress: bool,
    /// Révocation cryptographique active.
    pub is_revoked: bool,
    /// État chromatique résolu.
    pub visual_state: AuraVisualState,
}

impl Default for AuraRingWidget {
    fn default() -> Self {
        Self::new(0.50)
    }
}

impl AuraRingWidget {
    /// Crée un nouveau widget Aura Ring initialisé avec une valeur de $\tau$.
    pub fn new(initial_tau: f32) -> Self {
        let mut widget = Self {
            tau: initial_tau.clamp(0.0, 1.0),
            is_step_up_active: false,
            is_locked: false,
            is_distress: false,
            is_revoked: false,
            visual_state: AuraVisualState::AmberAmbient,
        };
        widget.recompute_state();
        widget
    }

    /// Met à jour le score continu de confiance $\tau \in [0.0, 1.0]$ et réévalue l'état visuel.
    pub fn update_confidence(&mut self, tau: f32) {
        self.tau = tau.clamp(0.0, 1.0);
        self.recompute_state();
    }

    /// Active ou désactive le mode défi step-up (liveness / vérification active).
    pub fn set_step_up_active(&mut self, active: bool) {
        self.is_step_up_active = active;
        self.recompute_state();
    }

    /// Active ou désactive l'état verrouillé (lock / fail-closed).
    pub fn set_locked(&mut self, locked: bool) {
        self.is_locked = locked;
        self.recompute_state();
    }

    /// Déclenche le mode détresse (force l'état RedLocked).
    pub fn set_distress(&mut self, distress: bool) {
        self.is_distress = distress;
        self.recompute_state();
    }

    /// Active l'état de révocation (force l'état RedLocked).
    pub fn set_revoked(&mut self, revoked: bool) {
        self.is_revoked = revoked;
        self.recompute_state();
    }

    /// Retourne la valeur courante de confiance $\tau$.
    pub fn tau(&self) -> f32 {
        self.tau
    }

    /// Retourne `true` si le défi step-up est actif.
    pub fn is_step_up_active(&self) -> bool {
        self.is_step_up_active
    }

    /// Retourne `true` si le système est verrouillé.
    pub fn is_locked(&self) -> bool {
        self.is_locked
    }

    /// Retourne `true` si le mode détresse est actif.
    pub fn is_distress(&self) -> bool {
        self.is_distress
    }

    /// Retourne `true` si l'état est révoqué.
    pub fn is_revoked(&self) -> bool {
        self.is_revoked
    }

    /// Retourne l'état visuel chromatique calculé.
    pub fn visual_state(&self) -> AuraVisualState {
        self.visual_state
    }

    /// Couleur primaire de l'anneau pour l'état courant.
    pub fn primary_color(&self) -> AuraColorRgba {
        self.visual_state.primary_color()
    }

    /// Couleur de la lueur de l'anneau pour l'état courant.
    pub fn glow_color(&self) -> AuraColorRgba {
        self.visual_state.glow_color()
    }

    /// Période de pulsation en millisecondes pour l'état courant.
    pub fn pulse_period_ms(&self) -> u32 {
        self.visual_state.pulse_period_ms()
    }

    /// Code hexadécimal primaire (#RRGGBB) pour l'état courant.
    pub fn primary_hex(&self) -> &'static str {
        self.visual_state.primary_hex()
    }

    /// Opacité normalisée de la lueur ambiante pour l'état courant.
    pub fn glow_alpha(&self) -> f32 {
        self.visual_state.glow_alpha()
    }

    /// Code hexadécimal de la lueur avec canal alpha (#RRGGBBAA) pour l'état courant.
    pub fn glow_hex(&self) -> String {
        self.visual_state.glow_hex()
    }

    /// Facteur d'échelle périodique sinusoïdal d'animation $\in [0.95, 1.05]$.
    pub fn animation_factor(&self, timestamp_ms: u64) -> f32 {
        self.visual_state.animation_factor(timestamp_ms)
    }

    /// Alias de commodité pour `animation_factor()`.
    pub fn scale_factor(&self, timestamp_ms: u64) -> f32 {
        self.animation_factor(timestamp_ms)
    }

    /// Génère un DTO de streaming pour transmission vers le shell ou protocole IPC.
    pub fn to_update_dto(&self, timestamp_ms: u64) -> AuraRingUpdateDto {
        AuraRingUpdateDto::new(
            self.tau,
            self.visual_state.name(),
            self.visual_state.primary_hex(),
            self.visual_state.glow_hex(),
            self.visual_state.pulse_period_ms(),
            self.animation_factor(timestamp_ms),
            timestamp_ms,
        )
    }

    /// Re-calcule l'état visuel en appliquant la hiérarchie de priorités de sécurité :
    /// 1. Verrouillage, détresse, révocation ou confiance critique ($\tau < 0.40$) -> `RedLocked` (priorité absolue fail-closed).
    /// 2. Défi step-up en cours -> `VioletStepUp`.
    /// 3. Haute confiance ($\tau \ge 0.90$) -> `EmeraldHighTrust`.
    /// 4. Confiance nominale ($0.40 \le \tau < 0.90$) -> `AmberAmbient`.
    fn recompute_state(&mut self) {
        if self.is_locked || self.is_distress || self.is_revoked || self.tau < 0.40 {
            self.visual_state = AuraVisualState::RedLocked;
        } else if self.is_step_up_active {
            self.visual_state = AuraVisualState::VioletStepUp;
        } else if self.tau >= 0.90 {
            self.visual_state = AuraVisualState::EmeraldHighTrust;
        } else {
            self.visual_state = AuraVisualState::AmberAmbient;
        }
    }
}
