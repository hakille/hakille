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
}

impl Default for PresenceIndicator {
    fn default() -> Self {
        Self::new()
    }
}
