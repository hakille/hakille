use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowsPresenceState {
    Idle,
    Listening,
    Acting,
}

/// Windows Shell System Tray Presence integration (§13).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsPresenceTray {
    pub tooltip: String,
    pub icon_state: WindowsPresenceState,
    pub is_visible: bool,
    pub unread_alerts: u32,
}

impl WindowsPresenceTray {
    pub fn new() -> Self {
        Self {
            tooltip: "Annora Cognitive OS — Sovereign".to_string(),
            icon_state: WindowsPresenceState::Idle,
            is_visible: true,
            unread_alerts: 0,
        }
    }

    /// Update presence state displayed in Windows taskbar system notification area.
    pub fn set_state(&mut self, state: WindowsPresenceState) {
        self.icon_state = state;
        self.tooltip = match state {
            WindowsPresenceState::Idle => "Annora — Idle (Sovereign OS)".to_string(),
            WindowsPresenceState::Listening => "Annora — Listening (Microphone Active)".to_string(),
            WindowsPresenceState::Acting => "Annora — Acting (Executing Autonomous Intent)".to_string(),
        };
    }

    pub fn state(&self) -> WindowsPresenceState {
        self.icon_state
    }

    pub fn tooltip(&self) -> &str {
        &self.tooltip
    }

    /// Sinusoidal breathing pulse factor (0.0 to 1.0) for taskbar icon animation
    pub fn pulse_factor(&self, timestamp_ms: u64) -> f32 {
        let period = match self.icon_state {
            WindowsPresenceState::Idle => 0,
            WindowsPresenceState::Listening => 2000,
            WindowsPresenceState::Acting => 600,
        };
        if period == 0 {
            0.0
        } else {
            let phase = (timestamp_ms % period as u64) as f32 / period as f32;
            0.5 + 0.5 * (phase * 2.0 * std::f32::consts::PI).sin()
        }
    }

    pub fn status_color_rgb(&self) -> (u8, u8, u8) {
        match self.icon_state {
            WindowsPresenceState::Idle => (120, 120, 120),
            WindowsPresenceState::Listening => (64, 196, 255),
            WindowsPresenceState::Acting => (255, 87, 34), // Coral #FF5722
        }
    }

    pub fn context_menu_items(&self) -> Vec<&'static str> {
        vec![
            "Ouvrir HUD Annora (Alt+Space)",
            "Visualiser Mémoire Souveraine",
            "Journal d'Audit Cryptographique",
            "Révoquer Consentements",
            "Quitter Annora",
        ]
    }
}

impl Default for WindowsPresenceTray {
    fn default() -> Self {
        Self::new()
    }
}

/// Windows DirectComposition / Acrylic translucent HUD Overlay Window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsAcrylicHudWindow {
    pub width: i32,
    pub height: i32,
    pub is_visible: bool,
    pub acrylic_blur: bool,
    pub dark_mode: bool,
    pub corner_preference: u32, // 2 = DWMWCP_ROUND
    pub opacity: f32,
}

impl WindowsAcrylicHudWindow {
    pub fn new() -> Self {
        Self {
            width: 720,
            height: 64,
            is_visible: false,
            acrylic_blur: true,
            dark_mode: true,
            corner_preference: 2, // Round corners
            opacity: 0.88,
        }
    }

    /// Computes centered screen position (centered horizontally, upper third vertically)
    pub fn center_on_screen(&self, screen_w: i32, screen_h: i32) -> (i32, i32) {
        let x = (screen_w - self.width) / 2;
        let y = (screen_h / 6).max(80);
        (x, y)
    }

    pub fn show(&mut self, expanded: bool) {
        self.is_visible = true;
        self.height = if expanded { 240 } else { 64 };
    }

    pub fn hide(&mut self) {
        self.is_visible = false;
    }

    pub fn dwm_composition_attributes(&self) -> serde_json::Value {
        serde_json::json!({
            "DWMWA_SYSTEMBACKDROP_TYPE": 3, // Acrylic
            "DWMWA_USE_IMMERSIVE_DARK_MODE": self.dark_mode,
            "DWMWA_WINDOW_CORNER_PREFERENCE": self.corner_preference,
            "width": self.width,
            "height": self.height,
            "is_visible": self.is_visible,
            "opacity": self.opacity,
        })
    }
}

impl Default for WindowsAcrylicHudWindow {
    fn default() -> Self {
        Self::new()
    }
}

/// Windows global keyboard hook registration (`Alt+Space` for Spotlight HUD Quickbar).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsGlobalHotkey {
    pub key_combination: String,
    pub is_registered: bool,
    pub hotkey_id: i32,
}

impl WindowsGlobalHotkey {
    pub fn new() -> Self {
        Self {
            key_combination: "Alt+Space".to_string(),
            is_registered: false,
            hotkey_id: 1001,
        }
    }

    /// Register Alt+Space hotkey for HUD activation.
    pub fn register(&mut self) -> bool {
        self.is_registered = true;
        true
    }

    /// Unregister hotkey on shutdown.
    pub fn unregister(&mut self) {
        self.is_registered = false;
    }
}

impl Default for WindowsGlobalHotkey {
    fn default() -> Self {
        Self::new()
    }
}

