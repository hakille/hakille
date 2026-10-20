//! Spécification hotkey globale — parsing + défauts par OS.
//!
//! L'enregistrement OS réel (`HakilleHotkey`) vit ici derrière la feature
//! `native` ; la spec pure (`HotkeySpec`) reste sans dépendance.

#[cfg(feature = "native")]
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
#[cfg(feature = "native")]
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HotkeyError {
    #[error("hotkey vide")]
    Empty,
    #[error("hotkey invalide: {0}")]
    Invalid(String),
    #[error("initialisation du gestionnaire de hotkeys échouée: {0}")]
    ManagerInit(String),
    #[error("enregistrement du hotkey refusé: {0}")]
    Registration(String),
}

/// Spec normalisée : `Ctrl+Alt+Space` (Win/Linux), `Cmd+Space` (macOS),
/// alias `Alt+Space` accepté partout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeySpec {
    pub accelerators: Vec<String>,
    pub id: u32,
}

impl HotkeySpec {
    /// Défaut par OS — table pure, testable sur n'importe quelle cible.
    ///
    /// macOS : `Cmd+Space` (contrat module). Windows/Linux/Redox : primaire
    /// `Ctrl+Alt+Space`, alias `Alt+Space`. Mobile/Web/OS non pris en charge :
    /// même suggestion que le desktop — aucun enregistrement OS n'est feint,
    /// il vit exclusivement dans `HakilleHotkey`.
    pub fn default_for(platform: &str) -> Self {
        match platform {
            "macos" => Self {
                accelerators: vec!["Cmd+Space".into()],
                id: 1001,
            },
            _ => Self {
                accelerators: vec!["Ctrl+Alt+Space".into(), "Alt+Space".into()],
                id: 1001,
            },
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
        Ok(Self {
            accelerators: vec![t.into()],
            id: 1001,
        })
    }

    pub fn is_alias_accepted(&self, pressed: &str) -> bool {
        let norm = pressed.to_lowercase().replace(' ', "");
        self.accelerators
            .iter()
            .any(|a| a.to_lowercase().replace(' ', "") == norm)
            || norm == "alt+space"
    }
}

/// Événement hotkey global relayé vers l'app.
#[cfg(feature = "native")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    /// Appui effectif sur le raccourci : basculer la HUD.
    ToggleHud,
}

/// Labels du défaut natif pour un OS donné : `(primaire, repli alias)`.
///
/// macOS : `Cmd+Space` puis `Alt+Space` ; autres OS : `Ctrl+Alt+Space`
/// puis `Alt+Space`. Pure — testable depuis toute cible.
#[cfg(feature = "native")]
pub fn default_hotkey_labels_for(platform: &str) -> (&'static str, &'static str) {
    match platform {
        "macos" => ("Cmd+Space", "Alt+Space"),
        _ => ("Ctrl+Alt+Space", "Alt+Space"),
    }
}

/// Label lisible du défaut sur l'OS courant pour logs/UI :
/// `"Ctrl+Alt+Space"` (Windows/Linux), `"Cmd+Space"` (macOS).
#[cfg(feature = "native")]
pub fn default_hotkey_label() -> &'static str {
    default_hotkey_labels_for(std::env::consts::OS).0
}

/// Hotkey global enregistré auprès de l'OS (feature `native`).
///
/// Le manager et le `HotKey` restent vivants dans la struct ; l'hôte récupère
/// le canal via [`HakilleHotkey::take_receiver`] et le transfère à un thread
/// relais qui réveille sa boucle d'événements (`EventLoopProxy`).
/// Un `ToggleHud` par appui, sans doublon release.
#[cfg(feature = "native")]
pub struct HakilleHotkey {
    rx: Option<std::sync::mpsc::Receiver<HotkeyEvent>>,
    _manager: GlobalHotKeyManager,
    _hotkey: HotKey,
    _tx: std::sync::mpsc::Sender<HotkeyEvent>,
}

#[cfg(feature = "native")]
impl HakilleHotkey {
    /// Défaut de l'OS courant ; si indisponible, tente l'alias `Alt+Space` ;
    /// sinon `Err` portant les deux causes réelles (jamais de succès feint).
    pub fn register_default() -> Result<Self, HotkeyError> {
        let (primary, alias) = default_hotkey_labels_for(std::env::consts::OS);
        match Self::register(primary) {
            Ok(hotkey) => Ok(hotkey),
            Err(primary_err) => match Self::register(alias) {
                Ok(hotkey) => Ok(hotkey),
                Err(alias_err) => Err(HotkeyError::Registration(format!(
                    "défaut {primary} indisponible ({primary_err}) ; \
                     repli {alias} indisponible ({alias_err})"
                ))),
            },
        }
    }

    /// Raccourci explicite : `"Ctrl+Alt+Space"` | `"Alt+Space"` | `"Cmd+Space"`.
    pub fn register(hotkey_str: &str) -> Result<Self, HotkeyError> {
        let mods = parse_modifiers(hotkey_str)?;
        let hotkey = HotKey::new(Some(mods), Code::Space);
        let manager =
            GlobalHotKeyManager::new().map_err(|e| HotkeyError::ManagerInit(e.to_string()))?;
        manager
            .register(hotkey)
            .map_err(|e| HotkeyError::Registration(e.to_string()))?;

        let (tx, rx) = std::sync::mpsc::channel();
        let pump_tx = tx.clone();
        let id = hotkey.id();
        std::thread::spawn(move || {
            let receiver = GlobalHotKeyEvent::receiver();
            while let Ok(event) = receiver.recv() {
                if event.id == id && event.state == HotKeyState::Pressed {
                    let _ = pump_tx.send(HotkeyEvent::ToggleHud);
                }
            }
        });

        Ok(Self {
            rx: Some(rx),
            _manager: manager,
            _hotkey: hotkey,
            _tx: tx,
        })
    }

    /// Transfère le canal d'événements à l'hôte (thread relais → proxy winit) ;
    /// `None` si déjà transféré.
    pub fn take_receiver(&mut self) -> Option<std::sync::mpsc::Receiver<HotkeyEvent>> {
        self.rx.take()
    }
}

/// Normalise les formes admises (`Ctrl+Alt+Space`, `Alt+Space`, `Cmd+Space`)
/// vers les modificateurs global-hotkey ; toute autre chaîne est refusée.
#[cfg(feature = "native")]
fn parse_modifiers(hotkey_str: &str) -> Result<Modifiers, HotkeyError> {
    let norm = hotkey_str
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase();
    if norm.is_empty() {
        return Err(HotkeyError::Empty);
    }
    match (
        norm.contains("cmd") || norm.contains("meta") || norm.contains("super"),
        norm.contains("ctrl") || norm.contains("control"),
        norm.contains("alt"),
        norm.ends_with("space"),
    ) {
        (true, false, false, true) => Ok(Modifiers::META),
        (false, true, true, true) => Ok(Modifiers::CONTROL | Modifiers::ALT),
        (false, false, true, true) => Ok(Modifiers::ALT),
        _ => Err(HotkeyError::Invalid(hotkey_str.trim().to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hotkey_table_covers_all_platform_kinds() {
        use crate::window::PlatformKind;
        let all = [
            PlatformKind::Windows,
            PlatformKind::Linux,
            PlatformKind::MacOs,
            PlatformKind::Redox,
            PlatformKind::Mobile,
            PlatformKind::Web,
            PlatformKind::Unsupported,
        ];
        for kind in all {
            let spec = HotkeySpec::default_for(kind.as_str());
            assert_eq!(spec.id, 1001, "kind={kind:?}");
            if kind == PlatformKind::MacOs {
                assert_eq!(
                    spec.accelerators,
                    vec!["Cmd+Space".to_string()],
                    "kind={kind:?}"
                );
            } else {
                assert!(
                    spec.accelerators.contains(&"Ctrl+Alt+Space".to_string()),
                    "kind={kind:?}"
                );
                assert!(
                    spec.accelerators.contains(&"Alt+Space".to_string()),
                    "kind={kind:?}"
                );
            }
            assert!(spec.is_alias_accepted("Alt+Space"), "kind={kind:?}");
            assert!(
                spec.is_alias_accepted(&spec.accelerators[0]),
                "kind={kind:?}"
            );
            assert!(!spec.is_alias_accepted("Ctrl+X"), "kind={kind:?}");
        }
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

#[cfg(all(test, feature = "native"))]
mod native_tests {
    use super::*;
    use global_hotkey::hotkey::Modifiers;

    #[test]
    fn default_labels_cover_whole_os_table() {
        assert_eq!(
            default_hotkey_labels_for("macos"),
            ("Cmd+Space", "Alt+Space")
        );
        for os in [
            "windows", "linux", "redox", "android", "ios", "web", "unknown", "freebsd", "",
        ] {
            assert_eq!(
                default_hotkey_labels_for(os),
                ("Ctrl+Alt+Space", "Alt+Space"),
                "os={os}"
            );
        }
    }

    #[test]
    fn default_label_matches_host_os() {
        // Le binaire annonce le défaut réellement tenté en premier.
        assert_eq!(
            default_hotkey_label(),
            default_hotkey_labels_for(std::env::consts::OS).0
        );
        #[cfg(target_os = "windows")]
        assert_eq!(default_hotkey_label(), "Ctrl+Alt+Space");
    }

    #[test]
    fn native_primary_label_matches_pure_spec_default_for_every_os() {
        use crate::window::PlatformKind;
        for kind in [
            PlatformKind::Windows,
            PlatformKind::Linux,
            PlatformKind::MacOs,
            PlatformKind::Redox,
            PlatformKind::Mobile,
            PlatformKind::Web,
            PlatformKind::Unsupported,
        ] {
            let (primary, _) = default_hotkey_labels_for(kind.as_str());
            assert_eq!(
                HotkeySpec::default_for(kind.as_str()).accelerators[0],
                primary,
                "kind={kind:?}"
            );
        }
    }

    #[test]
    fn parse_modifiers_accepts_contract_forms() {
        assert_eq!(
            parse_modifiers("Ctrl+Alt+Space").unwrap(),
            Modifiers::CONTROL | Modifiers::ALT
        );
        assert_eq!(parse_modifiers("Alt+Space").unwrap(), Modifiers::ALT);
        assert_eq!(parse_modifiers("Cmd+Space").unwrap(), Modifiers::META);
    }

    #[test]
    fn parse_modifiers_normalizes_case_and_spaces() {
        assert_eq!(
            parse_modifiers(" ctrl + ALT + space ").unwrap(),
            Modifiers::CONTROL | Modifiers::ALT
        );
        assert_eq!(parse_modifiers("CMD+SPACE").unwrap(), Modifiers::META);
    }

    #[test]
    fn parse_modifiers_rejects_empty_and_unknown() {
        assert!(matches!(parse_modifiers(""), Err(HotkeyError::Empty)));
        assert!(matches!(
            parse_modifiers("Shift+X"),
            Err(HotkeyError::Invalid(_))
        ));
        assert!(matches!(
            parse_modifiers("Alt+X"),
            Err(HotkeyError::Invalid(_))
        ));
    }
}
