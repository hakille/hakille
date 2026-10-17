//! Hôte desktop — seule frontière entre modèle pur et OS.
//!
//! `DesktopHost` est implémenté par le runtime winit (réel) et par
//! `NoopHost` (tests headless). Aucun secret ne transite ici, seuls des
//! view-models immuables et des `UiEvent`.

use crate::{HotkeySpec, InMemoryClipboard, OverlayPlacement, TraySpec, WindowHints};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HostError {
    #[error("fenêtre: {0}")]
    Window(String),
    #[error("hotkey: {0}")]
    Hotkey(String),
    #[error("tray: {0}")]
    Tray(String),
    #[error("clipboard: {0}")]
    Clipboard(String),
    #[error("overlay: {0}")]
    Overlay(String),
    #[error("gpu indisponible: {0} — fallback logiciel actif")]
    GpuUnavailable(String),
}

pub trait DesktopHost {
    fn show_hud(&mut self) -> Result<(), HostError>;
    fn hide_hud(&mut self) -> Result<(), HostError>;
    fn is_hud_visible(&self) -> bool;
    fn set_tooltip(&mut self, tooltip: &str) -> Result<(), HostError>;
    fn hotkey(&self) -> &HotkeySpec;
    fn placement(&self) -> &OverlayPlacement;
    fn tray(&self) -> &TraySpec;
    fn copy_text(&mut self, text: &str) -> Result<(), HostError>;
    fn paste_text(&self) -> Result<String, HostError>;
    fn window_hints(&self) -> &WindowHints;
}

/// Hôte nul — tests + CI sans OS.
pub struct NoopHost {
    visible: bool,
    hotkey: HotkeySpec,
    placement: OverlayPlacement,
    tray: TraySpec,
    hints: WindowHints,
    clipboard: InMemoryClipboard,
}

impl NoopHost {
    pub fn new() -> Self {
        use crate::OverlayBackend;
        Self {
            visible: false,
            hotkey: HotkeySpec::default_for("windows"),
            placement: OverlayPlacement::hud(OverlayBackend::WindowsAcrylic),
            tray: TraySpec::annora("Veille ambiante"),
            hints: WindowHints::hud(),
            clipboard: InMemoryClipboard::default(),
        }
    }
}

impl Default for NoopHost {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopHost for NoopHost {
    fn show_hud(&mut self) -> Result<(), HostError> {
        self.visible = true;
        Ok(())
    }
    fn hide_hud(&mut self) -> Result<(), HostError> {
        self.visible = false;
        Ok(())
    }
    fn is_hud_visible(&self) -> bool {
        self.visible
    }
    fn set_tooltip(&mut self, tooltip: &str) -> Result<(), HostError> {
        self.tray.tooltip = tooltip.to_string();
        Ok(())
    }
    fn hotkey(&self) -> &HotkeySpec {
        &self.hotkey
    }
    fn placement(&self) -> &OverlayPlacement {
        &self.placement
    }
    fn tray(&self) -> &TraySpec {
        &self.tray
    }
    fn copy_text(&mut self, text: &str) -> Result<(), HostError> {
        use crate::clipboard::Clipboard as _;
        self.clipboard
            .copy_text(text)
            .map_err(|e| HostError::Clipboard(e.to_string()))
    }
    fn paste_text(&self) -> Result<String, HostError> {
        use crate::clipboard::Clipboard as _;
        self.clipboard
            .paste_text()
            .map_err(|e| HostError::Clipboard(e.to_string()))
    }
    fn window_hints(&self) -> &WindowHints {
        &self.hints
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn noop_show_hide() {
        let mut h = NoopHost::new();
        assert!(!h.is_hud_visible());
        h.show_hud().unwrap();
        assert!(h.is_hud_visible());
        h.copy_text("x").unwrap();
        assert_eq!(h.paste_text().unwrap(), "x");
    }
}
