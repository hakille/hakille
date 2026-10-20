use serde::{Deserialize, Serialize};

/// Présence signature Hakille (spec Hakille §7) : un point, six états.
/// Repos = lueur #F4EBDD ; braise #FF7A2F = décision requise, rien d'autre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresenceState {
    /// Veille : point 6 px, lueur à 40 %, statique, 0 image/s.
    Idle,
    /// Écoute : point 8 px qui respire, 4,0 s, opacité 60→100 %, échelle 1→1,15.
    Listening,
    /// Action : anneau fin autour du point, pulsation 1,2 s.
    Acting,
    /// Parole : 21 fils verticaux (7 sous 24 px), amplitude = enveloppe audio.
    Speaking,
    /// Décision requise : anneau braise fixe, sans pulsation.
    DecisionRequired,
    /// Urgence : double impulsion braise, 0,8 s.
    Urgent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiTheme {
    pub background: Color,
    pub text: Color,
    /// Braise strictement réservée aux actions exigeant validation (spec §3).
    pub validation_accent: Color,
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            background: Color {
                r: 11,
                g: 12,
                b: 15,
            }, // surface/0 #0B0C0F
            text: Color {
                r: 236,
                g: 238,
                b: 242,
            }, // texte/1 #ECEEF2
            validation_accent: Color {
                r: 255,
                g: 122,
                b: 47,
            }, // braise #FF7A2F (validation uniquement !)
        }
    }
}

/// Lueur #F4EBDD : présence au repos.
pub fn lueur() -> Color {
    Color {
        r: 244,
        g: 235,
        b: 221,
    }
}

pub struct PresenceIndicator {
    state: PresenceState,
    theme: UiTheme,
}

impl PresenceIndicator {
    pub fn new() -> Self {
        Self {
            state: PresenceState::Idle,
            theme: UiTheme::default(),
        }
    }

    pub fn set_state(&mut self, state: PresenceState) {
        self.state = state;
    }

    pub fn state(&self) -> PresenceState {
        self.state
    }

    /// Période de pulsation (spec §7 + motion tokens).
    /// Veille et Décision requise : statique (anneau fixe). Parole : 60 i/s.
    pub fn pulse_period_ms(&self) -> u32 {
        match self.state {
            PresenceState::Idle => 0,
            PresenceState::Listening => 4000,
            PresenceState::Acting => 1200,
            PresenceState::Speaking => 16,
            PresenceState::DecisionRequired => 0,
            PresenceState::Urgent => 800,
        }
    }

    pub fn theme(&self) -> &UiTheme {
        &self.theme
    }

    /// Computes the exact sinusoidal animation factor (0.0 to 1.0) for the tray icon or HUD aura
    pub fn pulse_factor(&self, timestamp_ms: u64) -> f32 {
        let period = self.pulse_period_ms();
        if period == 0 {
            0.0 // Static
        } else {
            let phase = (timestamp_ms % period as u64) as f32 / period as f32;
            let sin_val = (phase * 2.0 * std::f32::consts::PI).sin();
            0.5 + 0.5 * sin_val
        }
    }

    /// Couleur de remplissage du point.
    pub fn color_for_state(&self) -> Color {
        match self.state {
            PresenceState::Idle => Color {
                r: 244,
                g: 235,
                b: 221,
            }, // lueur à 40 % (opacité via pulse_factor/période 0 → statique)
            PresenceState::Listening => lueur(),
            PresenceState::Acting => lueur(),
            PresenceState::Speaking => lueur(),
            PresenceState::DecisionRequired => lueur(),
            PresenceState::Urgent => Color {
                r: 255,
                g: 122,
                b: 47,
            }, // double impulsion braise
        }
    }

    /// Anneau autour du point (Action fin, Décision braise fixe, Urgence braise).
    /// Retourne la couleur de l'anneau, ou None (pas d'anneau).
    pub fn ring_color_for_state(&self) -> Option<Color> {
        match self.state {
            PresenceState::Acting => Some(Color {
                r: 236,
                g: 238,
                b: 242,
            }), // anneau fin texte/1
            PresenceState::DecisionRequired | PresenceState::Urgent => Some(Color {
                r: 255,
                g: 122,
                b: 47,
            }), // anneau braise
            _ => None,
        }
    }

    /// Amplitude des fils Parole : enveloppe audio 0..1 → 0..24 px.
    /// Barge-in : retombée < 80 ms gérée par l'appelant (échantillon suivant) ;
    /// 600 ms de silence → repli en point (retourner 0).
    pub fn speech_wire_height_px(envelope_01: f32) -> f32 {
        (envelope_01.clamp(0.0, 1.0) * 24.0).round()
    }

    pub fn context_menu_items(&self) -> Vec<&'static str> {
        vec![
            "Ouvrir Hakille HUD",
            "Visualiseur de Mémoire Continue",
            "Journal d'Audit et Sécurité",
            "Gérer les Consentements & Révocations",
            "Quitter Hakille",
        ]
    }
}

impl Default for PresenceIndicator {
    fn default() -> Self {
        Self::new()
    }
}
