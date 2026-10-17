//! Fenêtre — hints multi-DPI, jamais de fenêtre vide silencieuse.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlatformKind {
    Windows,
    Linux,
    MacOs,
    Redox,
    Mobile,
    Web,
}

impl PlatformKind {
    pub fn auto() -> Self {
        #[cfg(target_os = "windows")]
        return Self::Windows;
        #[cfg(target_os = "macos")]
        return Self::MacOs;
        #[cfg(target_os = "linux")]
        return Self::Linux;
        #[cfg(target_os = "redox")]
        return Self::Redox;
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux", target_os = "redox")))]
        return Self::Web;
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::MacOs => "macos",
            Self::Redox => "redox",
            Self::Mobile => "mobile",
            Self::Web => "web",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowHints {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub transparent: bool,
    pub decorations: bool,
    pub topmost: bool,
    pub resizable: bool,
}

impl WindowHints {
    pub fn hud() -> Self {
        Self { title: "Akora".into(), width: 720, height: 64, transparent: true, decorations: false, topmost: true, resizable: false }
    }
    pub fn journal() -> Self {
        Self { title: "Akora — Journal".into(), width: 720, height: 640, transparent: false, decorations: true, topmost: false, resizable: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hud_hints_frameless_topmost() {
        let h = WindowHints::hud();
        assert!(!h.decorations);
        assert!(h.topmost);
        assert_eq!(h.width, 720);
    }
}
