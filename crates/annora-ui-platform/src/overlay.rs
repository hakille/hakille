//! Placement overlay HUD — géométrie futuriste validée (720×64 r12).

use serde::{Deserialize, Serialize};

/// Backend d'overlay par OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlayBackend {
    WindowsAcrylic,
    WlrLayerShell,
    MacPopover,
    OrbitalDot,
    WebCanvas,
}

impl OverlayBackend {
    pub fn for_platform(platform: &str) -> Self {
        match platform {
            "windows" => Self::WindowsAcrylic,
            "linux" => Self::WlrLayerShell,
            "macos" => Self::MacPopover,
            "redox" => Self::OrbitalDot,
            _ => Self::WebCanvas,
        }
    }
}

/// Placement HUD : 720×64, rayon 12, offset haut 120, topmost.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayPlacement {
    pub width: u32,
    pub height: u32,
    pub radius: u32,
    pub top_offset: i32,
    pub topmost: bool,
    pub backend: OverlayBackend,
    pub visible: bool,
}

impl OverlayPlacement {
    pub fn hud(backend: OverlayBackend) -> Self {
        Self { width: 720, height: 64, radius: 12, top_offset: 120, topmost: true, backend, visible: false }
    }

    /// Géométrie futuriste : 640×56 rejetée (ADR-007).
    pub fn is_futurist_geometry(&self) -> bool {
        self.width == 720 && self.height == 64 && self.radius == 12
    }

    pub fn centered_on(&self, screen_w: u32, screen_h: u32) -> (i32, i32) {
        let x = (screen_w.saturating_sub(self.width) / 2) as i32;
        let y = (screen_h as i32 / 6).max(80);
        (x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn futurist_geometry_accepted() {
        let p = OverlayPlacement::hud(OverlayBackend::WindowsAcrylic);
        assert!(p.is_futurist_geometry());
        let bad = OverlayPlacement { width: 640, height: 56, radius: 8, top_offset: 120, topmost: true, backend: OverlayBackend::WindowsAcrylic, visible: false };
        assert!(!bad.is_futurist_geometry());
    }

    #[test]
    fn centered_on_screen() {
        let p = OverlayPlacement::hud(OverlayBackend::WindowsAcrylic);
        let (x, y) = p.centered_on(1920, 1080);
        assert_eq!(x, 600);
        assert_eq!(y, 180);
    }
}
