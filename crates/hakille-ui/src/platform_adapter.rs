use crate::presence::{Color, PresenceState, lueur};
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
        #[cfg(not(any(
            target_os = "windows",
            target_os = "linux",
            target_os = "macos",
            target_os = "redox",
            target_family = "wasm"
        )))]
        let platform = PlatformKind::Mobile;

        let global_hotkey =
            crate::ephemeral_hud::EphemeralHud::default_hotkey_for(platform).to_string();

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
    pub fn create_tray_spec(
        &self,
        state: PresenceState,
        pending_actions_count: usize,
    ) -> TrayIconSpec {
        let (color, period, tooltip) = match state {
            PresenceState::Idle => (
                Color {
                    r: 244,
                    g: 235,
                    b: 221,
                }, // lueur à 40 %, statique
                0,
                "Hakille — Veille ambiante".to_string(),
            ),
            PresenceState::Listening => (lueur(), 4000, "Hakille — Écoute active".to_string()),
            PresenceState::Acting => (
                lueur(),
                1200,
                "Hakille — Action en cours (toucher pour le journal)".to_string(),
            ),
            PresenceState::Speaking => (lueur(), 16, "Hakille — Parole en cours".to_string()),
            PresenceState::DecisionRequired => (
                Color {
                    r: 255,
                    g: 122,
                    b: 47,
                }, // anneau braise fixe
                0,
                "Hakille — Décision requise (toucher pour voir)".to_string(),
            ),
            PresenceState::Urgent => (
                Color {
                    r: 255,
                    g: 122,
                    b: 47,
                }, // double impulsion braise 0,8 s
                800,
                "Hakille — Urgence".to_string(),
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
            PlatformKind::Windows => {
                "Windows Shell_NotifyIcon + DirectComposition acrylic overlay + RegisterHotKey(Ctrl+Alt+Space, alias Alt+Space)"
            }
            PlatformKind::Linux => {
                "D-Bus StatusNotifierItem (KStatusNotifier) + wlr-layer-shell / X11 overlay + xdg-desktop-portal"
            }
            PlatformKind::MacOS => {
                "NSStatusItem MenuBar + NSVisualEffectView acrylic panel + CGEventTap hotkey"
            }
            PlatformKind::Redox => {
                "Redox orbital scheme ambient dot + scheme-based IPC ephemeral buffer"
            }
            PlatformKind::Mobile => {
                "Android / iOS Foreground Service persistent pill + Haptic Engine + Edge swipe"
            }
            PlatformKind::Web => "Web Audio + Notification API + HTML5 floating canvas overlay",
        }
    }
}
