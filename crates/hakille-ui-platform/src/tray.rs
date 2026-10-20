//! Tray — spécification pure (menu 5 items FR).

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TrayError {
    #[error("hôte notifier manquant: {0}")]
    NotifierMissing(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraySpec {
    pub tooltip: String,
    pub menu: Vec<String>,
    pub icon_braise_hex: String,
}

impl TraySpec {
    pub fn hakille(presence_label: &str) -> Self {
        Self {
            tooltip: format!("Hakille — {presence_label}"),
            menu: vec![
                "Afficher le HUD".into(),
                "Ouvrir le journal".into(),
                "Ouvrir la mémoire".into(),
                "Ouvrir les consentements".into(),
                "Quitter".into(),
            ],
            icon_braise_hex: "#FF7A2F".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tray_has_5_items() {
        let t = TraySpec::hakille("Veille");
        assert_eq!(t.menu.len(), 5);
        assert_eq!(t.icon_braise_hex, "#FF7A2F");
    }
}
