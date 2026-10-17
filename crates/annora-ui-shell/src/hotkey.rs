//! Hotkey global spec §8/§29 : Ctrl+Alt+Space (Cmd+Space sur macOS, alias Alt+Space accepté). Nécessite feature `render`.

#[cfg(feature = "render")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    ToggleHud,
}

#[cfg(feature = "render")]
pub struct AnnoraHotkey {
    _manager: global_hotkey::GlobalHotKeyManager,
    _hotkey: global_hotkey::hotkey::HotKey,
    pub rx: std::sync::mpsc::Receiver<HotkeyEvent>,
    _tx: std::sync::mpsc::Sender<HotkeyEvent>,
}

#[cfg(feature = "render")]
impl AnnoraHotkey {
    /// Parse le raccourci : défaut Ctrl+Alt+Space, Cmd+Space sur macOS,
    /// alias historique Alt+Space accepté (pas de casse utilisateur).
    pub fn parse_modifiers(hotkey_str: &str) -> String {
        if hotkey_str.contains("Cmd") {
            "META+Space".to_string()
        } else if hotkey_str.contains("Ctrl") && hotkey_str.contains("Alt") {
            "CONTROL+ALT+Space".to_string()
        } else {
            // Alias Alt+Space (et tout autre repli) : ALT+Space.
            "ALT+Space".to_string()
        }
    }

    pub fn register(hotkey_str: &str) -> Result<Self, String> {
        use global_hotkey::hotkey::{Code, HotKey, Modifiers};
        use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};
        let (mods, code) = if hotkey_str.contains("Cmd") {
            (Modifiers::META, Code::Space)
        } else if hotkey_str.contains("Ctrl") && hotkey_str.contains("Alt") {
            (Modifiers::CONTROL.union(Modifiers::ALT), Code::Space)
        } else {
            // Alias Alt+Space accepté.
            (Modifiers::ALT, Code::Space)
        };
        let hk = HotKey::new(Some(mods), code);
        let manager = GlobalHotKeyManager::new().map_err(|e| e.to_string())?;
        manager.register(hk).map_err(|e| e.to_string())?;
        let (tx, rx) = std::sync::mpsc::channel();
        // Pompe les events globaux vers notre canal typé (appelée chaque frame).
        let txc = tx.clone();
        std::thread::spawn(move || {
            let receiver = GlobalHotKeyEvent::receiver();
            while let Ok(ev) = receiver.recv() {
                if ev.id == hk.id() {
                    let _ = txc.send(HotkeyEvent::ToggleHud);
                }
            }
        });
        Ok(Self {
            _manager: manager,
            _hotkey: hk,
            rx,
            _tx: tx,
        })
    }
}

#[cfg(all(test, feature = "render"))]
mod tests {
    use super::*;

    #[test]
    fn test_hotkey_parses_default_and_alias() {
        assert_eq!(
            AnnoraHotkey::parse_modifiers("Ctrl+Alt+Space"),
            "CONTROL+ALT+Space"
        );
        assert_eq!(AnnoraHotkey::parse_modifiers("Cmd+Space"), "META+Space");
        // Alias historique accepté.
        assert_eq!(AnnoraHotkey::parse_modifiers("Alt+Space"), "ALT+Space");
    }
}
