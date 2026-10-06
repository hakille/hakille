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
}

impl Default for WindowsPresenceTray {
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
