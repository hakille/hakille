//! Intégration Tray, Hotkey et Overlay Windows.
//!
//! Réconcilié avec les spécifications unifiées `DesktopHost`, `TraySpec`,
//! `OverlayPlacement` et `HotkeySpec` de `hakille_ui_platform`.
//! Les structures autonomes historiques (`WindowsPresenceTray`, `WindowsAcrylicHudWindow`,
//! `WindowsGlobalHotkey`) sont alignées sur le contrat souverain de l'hôte de bureau.

use serde::{Deserialize, Serialize};

/// État de présence pour le Shell Windows.
///
/// Réconcilié avec `hakille_ui::PresenceState`.
#[deprecated(
    since = "0.2.0",
    note = "WindowsPresenceState est déprécié en faveur de hakille_ui::PresenceState."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowsPresenceState {
    Idle,
    Listening,
    Acting,
}

/// Intégration zone de notification Windows (System Tray Presence, §13).
///
/// Réconcilié avec `hakille_ui_platform::TraySpec` et `DesktopHost`.
#[deprecated(
    since = "0.2.0",
    note = "WindowsPresenceTray est réconcilié avec hakille_ui_platform::TraySpec et DesktopHost."
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsPresenceTray {
    pub tooltip: String,
    #[allow(deprecated)]
    pub icon_state: WindowsPresenceState,
    pub is_visible: bool,
    pub unread_alerts: u32,
}

#[allow(deprecated)]
impl WindowsPresenceTray {
    pub fn new() -> Self {
        Self {
            tooltip: "Hakille Cognitive OS — Sovereign".to_string(),
            icon_state: WindowsPresenceState::Idle,
            is_visible: true,
            unread_alerts: 0,
        }
    }

    /// Met à jour l'état de présence affiché dans la zone de notification Windows.
    pub fn set_state(&mut self, state: WindowsPresenceState) {
        self.icon_state = state;
        self.tooltip = match state {
            WindowsPresenceState::Idle => "Hakille — Idle (Sovereign OS)".to_string(),
            WindowsPresenceState::Listening => {
                "Hakille — Listening (Microphone Active)".to_string()
            }
            WindowsPresenceState::Acting => {
                "Hakille — Acting (Executing Autonomous Intent)".to_string()
            }
        };
    }

    pub fn state(&self) -> WindowsPresenceState {
        self.icon_state
    }

    pub fn tooltip(&self) -> &str {
        &self.tooltip
    }

    /// Facteur de pulsation sinusoïdale respirante (0.0 à 1.0) pour l'icône de la barre des tâches.
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
            WindowsPresenceState::Acting => (255, 122, 47), // braise #FF7A2F (spec §3)
        }
    }

    /// Éléments du menu contextuel (5 items canoniques réconciliés avec `TraySpec`).
    pub fn context_menu_items(&self) -> Vec<&'static str> {
        vec![
            "Ouvrir HUD Hakille (Alt+Space)",
            "Visualiser Mémoire Souveraine",
            "Journal d'Audit Cryptographique",
            "Révoquer Consentements",
            "Quitter Hakille",
        ]
    }

    /// Découpage conforme à `hakille_ui_platform::TraySpec` : (tooltip, menu, icon_braise_hex).
    pub fn as_tray_spec_parts(&self) -> (&str, Vec<&'static str>, &'static str) {
        (&self.tooltip, self.context_menu_items(), "#FF7A2F")
    }
}

#[allow(deprecated)]
impl Default for WindowsPresenceTray {
    fn default() -> Self {
        Self::new()
    }
}

/// Fenêtre HUD acrylique / DirectComposition Windows.
///
/// Réconcilié avec `hakille_ui_platform::OverlayPlacement` (720x64, r12, top 120).
#[deprecated(
    since = "0.2.0",
    note = "WindowsAcrylicHudWindow est réconcilié avec hakille_ui_platform::OverlayPlacement."
)]
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

#[allow(deprecated)]
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

    /// Calcule le positionnement centré à l'écran (tiers supérieur, hauteur min 80).
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

    /// Vérifie la cohérence géométrique avec `OverlayPlacement`.
    pub fn is_futurist_geometry(&self) -> bool {
        self.width == 720 && (self.height == 64 || self.height == 240)
    }
}

#[allow(deprecated)]
impl Default for WindowsAcrylicHudWindow {
    fn default() -> Self {
        Self::new()
    }
}

/// Enregistrement du raccourci clavier global (`Alt+Space` / `Ctrl+Alt+Space`).
///
/// Réconcilié avec `hakille_ui_platform::HotkeySpec`.
#[deprecated(
    since = "0.2.0",
    note = "WindowsGlobalHotkey est réconcilié avec hakille_ui_platform::HotkeySpec."
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsGlobalHotkey {
    pub key_combination: String,
    pub is_registered: bool,
    pub hotkey_id: i32,
}

#[allow(deprecated)]
impl WindowsGlobalHotkey {
    pub fn new() -> Self {
        Self {
            key_combination: "Alt+Space".to_string(),
            is_registered: false,
            hotkey_id: 1001,
        }
    }

    /// Enregistre le hotkey Alt+Space pour l'activation du HUD.
    pub fn register(&mut self) -> bool {
        self.is_registered = true;
        true
    }

    /// Désenregistre le hotkey à l'arrêt.
    pub fn unregister(&mut self) {
        self.is_registered = false;
    }

    /// Liste des accélérateurs normalisés réconciliés avec `HotkeySpec::default_for("windows")`.
    pub fn normalized_accelerators(&self) -> Vec<String> {
        vec!["Ctrl+Alt+Space".to_string(), self.key_combination.clone()]
    }
}

#[allow(deprecated)]
impl Default for WindowsGlobalHotkey {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(deprecated)]
    fn test_reconciled_windows_tray_spec_parts() {
        let tray = WindowsPresenceTray::new();
        let (tooltip, menu, hex) = tray.as_tray_spec_parts();
        assert!(tooltip.contains("Hakille"));
        assert_eq!(menu.len(), 5);
        assert_eq!(hex, "#FF7A2F");
    }

    #[test]
    #[allow(deprecated)]
    fn test_reconciled_windows_hud_geometry() {
        let mut hud = WindowsAcrylicHudWindow::new();
        assert!(hud.is_futurist_geometry());
        assert_eq!(hud.center_on_screen(1920, 1080), (600, 180));
        hud.show(true);
        assert_eq!(hud.height, 240);
        assert!(hud.is_futurist_geometry());
    }

    #[test]
    #[allow(deprecated)]
    fn test_reconciled_windows_hotkey_accelerators() {
        let hotkey = WindowsGlobalHotkey::new();
        let accels = hotkey.normalized_accelerators();
        assert!(accels.contains(&"Ctrl+Alt+Space".to_string()));
        assert!(accels.contains(&"Alt+Space".to_string()));
    }
}
