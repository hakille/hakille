//! Per-OS shell wiring (tray, hotkey, overlay). Le renderer affiche ; l'OS fournit le chrome.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShellPlatform {
    Windows,
    Linux,
    MacOs,
    Redox,
    Mobile,
    Web,
}

impl ShellPlatform {
    pub fn auto() -> Self {
        #[cfg(target_os = "windows")]
        return Self::Windows;
        #[cfg(target_os = "macos")]
        return Self::MacOs;
        #[cfg(target_os = "linux")]
        return Self::Linux;
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        return Self::Web;
    }

    /// Global hotkey shown in the HUD (matches PlatformAdapter, spec §8/§29).
    /// Défaut ⌃⌥Espace modifiable dès l'onboarding ; "Alt+Space" reste un alias accepté.
    pub fn hotkey(&self) -> &'static str {
        match self {
            Self::MacOs => "Cmd+Space",
            _ => "Ctrl+Alt+Space",
        }
    }

    /// Alias historique accepté partout (pas de casse utilisateur).
    pub fn hotkey_alias() -> &'static str {
        "Alt+Space"
    }

    /// Overlay backend per OS (vision §13 + platform_adapter).
    pub fn overlay_backend(&self) -> &'static str {
        match self {
            Self::Windows => "DWM DirectComposition acrylic overlay (topmost, 720x64/240/280)",
            Self::Linux => "wlr-layer-shell ontop + X11 fallback + xdg-portal",
            Self::MacOs => "NSPanel + NSVisualEffectView + CGEventTap hotkey",
            Self::Redox => "orbital scheme overlay + IPC (no WebView)",
            Self::Mobile => "Foreground-service pill + haptic + edge-swipe",
            Self::Web => "canvas + Notification API (demo only)",
        }
    }

    pub fn tray_backend(&self) -> &'static str {
        match self {
            Self::Windows => "Shell_NotifyIcon (zone de notification) + menu natif OS",
            Self::Linux => "D-Bus StatusNotifier (ksni-style, loud NoHost error, no silent GTK Ok)",
            Self::MacOs => "NSStatusItem + menu natif OS",
            Self::Redox => "orbital tray dot + redox IPC",
            Self::Mobile => "persistent notification dot (no desktop tray)",
            Self::Web => "favicon badge (demo)",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hotkey_and_backends_are_explicit() {
        assert_eq!(ShellPlatform::Windows.hotkey(), "Ctrl+Alt+Space");
        assert_eq!(ShellPlatform::MacOs.hotkey(), "Cmd+Space");
        assert_eq!(ShellPlatform::hotkey_alias(), "Alt+Space");
        assert!(ShellPlatform::Windows.overlay_backend().contains("DWM"));
        assert!(
            ShellPlatform::Linux
                .tray_backend()
                .contains("StatusNotifier")
        );
    }
}
