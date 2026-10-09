use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresenceState {
    /// Ambient standby, passive listening
    Idle,
    /// Slow pulse, active listening / voice capture
    Listening,
    /// Fast pulse, executing action (tapping opens the audit journal)
    Acting,
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
    /// Accent color is STRICTLY RESERVED for actions requiring user validation
    pub validation_accent: Color,
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            background: Color { r: 18, g: 18, b: 18 },       // Dark #121212
            text: Color { r: 224, g: 224, b: 224 },           // Light grey #E0E0E0
            validation_accent: Color { r: 255, g: 87, b: 34 }, // Coral #FF5722 (Only for validation!)
        }
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

    pub fn pulse_period_ms(&self) -> u32 {
        match self.state {
            PresenceState::Idle => 0,        // Static
            PresenceState::Listening => 2000, // Slow breathing pulse (2s)
            PresenceState::Acting => 600,     // Quick rhythmic pulse (600ms)
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

    pub fn color_for_state(&self) -> Color {
        match self.state {
            PresenceState::Idle => Color { r: 120, g: 120, b: 120 }, // Muted grey
            PresenceState::Listening => Color { r: 64, g: 196, b: 255 }, // Electric blue
            PresenceState::Acting => Color { r: 255, g: 87, b: 34 }, // Coral #FF5722
        }
    }

    pub fn context_menu_items(&self) -> Vec<&'static str> {
        vec![
            "Ouvrir Annora HUD (Alt+Space)",
            "Visualiseur de Mémoire Continue",
            "Journal d'Audit et Sécurité",
            "Gérer les Consentements & Révocations",
            "Quitter Annora",
        ]
    }
}

impl Default for PresenceIndicator {
    fn default() -> Self {
        Self::new()
    }
}

