use crate::presence::{Color, PresenceState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlatformKind {
    Windows,
    Linux,
    MacOS,
    Redox,
    Mobile,
    Web,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrayIconSpec {
    pub tooltip: String,
    pub status_color: Color,
    pub pulse_period_ms: u32,
    pub has_attention_badge: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformAdapterConfig {
    pub platform: PlatformKind,
    pub global_hotkey: String,
    pub system_tray_enabled: bool,
    pub haptic_supported: bool,
}

pub struct PlatformAdapter {
    config: PlatformAdapterConfig,
}

impl PlatformAdapter {
    pub fn new(config: PlatformAdapterConfig) -> Self {
        Self { config }
    }

    pub fn auto_detect() -> Self {
        #[cfg(target_os = "windows")]
        let platform = PlatformKind::Windows;
        #[cfg(target_os = "linux")]
        let platform = PlatformKind::Linux;
        #[cfg(target_os = "macos")]
        let platform = PlatformKind::MacOS;
        #[cfg(target_os = "redox")]
        let platform = PlatformKind::Redox;
        #[cfg(target_family = "wasm")]
        let platform = PlatformKind::Web;
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos", target_os = "redox", target_family = "wasm")))]
        let platform = PlatformKind::Mobile;

        let global_hotkey = match platform {
            PlatformKind::MacOS => "Cmd+Space".to_string(),
            _ => "Alt+Space".to_string(),
        };

        let haptic_supported = matches!(platform, PlatformKind::Mobile);

        Self {
            config: PlatformAdapterConfig {
                platform,
                global_hotkey,
                system_tray_enabled: true,
                haptic_supported,
            },
        }
    }

    pub fn config(&self) -> &PlatformAdapterConfig {
        &self.config
    }

    /// Generates platform-specific system tray status specification
    pub fn create_tray_spec(&self, state: PresenceState, pending_actions_count: usize) -> TrayIconSpec {
        let (color, period, tooltip) = match state {
            PresenceState::Idle => (
                Color { r: 120, g: 120, b: 120 }, // Muted grey
                0,
                "Annora — Veille ambiante".to_string(),
            ),
            PresenceState::Listening => (
                Color { r: 64, g: 196, b: 255 },  // Soft electric blue breathing
                2000,
                "Annora — Écoute active".to_string(),
            ),
            PresenceState::Acting => (
                Color { r: 255, g: 87, b: 34 },   // Distinct action pulse (tap opens audit journal)
                600,
                "Annora — Action en cours (Cliquer pour journal)".to_string(),
            ),
        };

        TrayIconSpec {
            tooltip,
            status_color: color,
            pulse_period_ms: period,
            has_attention_badge: pending_actions_count > 0,
        }
    }

    /// Renders integration dispatch details for the target OS
    pub fn os_integration_details(&self) -> &'static str {
        match self.config.platform {
            PlatformKind::Windows => "Windows Shell_NotifyIcon + DirectComposition acrylic overlay + RegisterHotKey(Alt+Space)",
            PlatformKind::Linux => "D-Bus StatusNotifierItem (KStatusNotifier) + wlr-layer-shell / X11 overlay + xdg-desktop-portal",
            PlatformKind::MacOS => "NSStatusItem MenuBar + NSVisualEffectView acrylic panel + CGEventTap hotkey",
            PlatformKind::Redox => "Redox orbital scheme ambient dot + scheme-based IPC ephemeral buffer",
            PlatformKind::Mobile => "Android / iOS Foreground Service persistent pill + Haptic Engine + Edge swipe",
            PlatformKind::Web => "Web Audio + Notification API + HTML5 floating canvas overlay",
        }
    }
}
