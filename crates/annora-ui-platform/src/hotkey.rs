//! Spécification hotkey globale — parsing + défauts par OS.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HotkeyError {
    #[error("hotkey vide")]
    Empty,
    #[error("hotkey invalide: {0}")]
    Invalid(String),
}

/// Spec normalisée : `Ctrl+Alt+Space` (Win/Linux), `Cmd+Space` (macOS),
/// alias `Alt+Space` accepté partout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeySpec {
    pub accelerators: Vec<String>,
    pub id: u32,
}

impl HotkeySpec {
    pub fn default_for(platform: &str) -> Self {
        match platform {
            "macos" => Self { accelerators: vec!["Cmd+Space".into()], id: 1001 },
            _ => Self { accelerators: vec!["Ctrl+Alt+Space".into(), "Alt+Space".into()], id: 1001 },
        }
    }

    pub fn parse(s: &str) -> Result<Self, HotkeyError> {
        let t = s.trim();
        if t.is_empty() {
            return Err(HotkeyError::Empty);
        }
        if !t.contains('+') && t.len() < 2 {
            return Err(HotkeyError::Invalid(t.into()));
        }
        Ok(Self { accelerators: vec![t.into()], id: 1001 })
    }

    pub fn is_alias_accepted(&self, pressed: &str) -> bool {
        let norm = pressed.to_lowercase().replace(' ', "");
        self.accelerators.iter().any(|a| a.to_lowercase().replace(' ', "") == norm)
            || norm == "alt+space"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hotkeys_per_os() {
        assert!(HotkeySpec::default_for("macos").accelerators.contains(&"Cmd+Space".to_string()));
        assert!(HotkeySpec::default_for("windows").accelerators.contains(&"Ctrl+Alt+Space".to_string()));
    }

    #[test]
    fn alias_alt_space_accepted() {
        let h = HotkeySpec::default_for("windows");
        assert!(h.is_alias_accepted("Alt+Space"));
        assert!(!h.is_alias_accepted("Ctrl+X"));
    }

    #[test]
    fn parse_rejects_empty() {
        assert!(HotkeySpec::parse("").is_err());
        assert!(HotkeySpec::parse("Ctrl+Alt+Space").is_ok());
    }
}
