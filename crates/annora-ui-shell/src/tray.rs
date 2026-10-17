//! Tray système + menu natif. Nécessite feature `render`.
//!
//! Linux : StatusNotifier bruyant (pas de succès silencieux sans hôte).
//! Windows : Shell_NotifyIcon. macOS : NSStatusItem.

#[cfg(feature = "render")]
use std::sync::mpsc::{Receiver, Sender};

#[cfg(feature = "render")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    ShowHud,
    OpenJournal,
    OpenMemory,
    OpenConsents,
    Quit,
}

#[cfg(feature = "render")]
pub struct AnnoraTray {
    _icon: tray_icon::TrayIcon,
    pub rx: Receiver<TrayEvent>,
    tx: Sender<TrayEvent>,
}

#[cfg(feature = "render")]
impl AnnoraTray {
    pub fn new(presence_label: &str) -> anyhow_like::Result<Self, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        let tray_menu = Self::build_menu(&tx)?;
        let icon = Self::build_icon(tray_menu, presence_label)?;
        Ok(Self {
            _icon: icon,
            rx,
            tx,
        })
    }

    fn build_menu(tx: &Sender<TrayEvent>) -> Result<muda::Menu, String> {
        use muda::{Menu, MenuItem};
        let menu = Menu::new();
        let items = [
            (
                "show",
                "Ouvrir Annora HUD (Ctrl+Alt+Space)",
                TrayEvent::ShowHud,
            ),
            (
                "journal",
                "Journal d'Audit et Sécurité",
                TrayEvent::OpenJournal,
            ),
            (
                "memory",
                "Visualiseur de Mémoire Continue",
                TrayEvent::OpenMemory,
            ),
            (
                "consents",
                "Gérer les Consentements & Révocations",
                TrayEvent::OpenConsents,
            ),
            ("quit", "Quitter Annora", TrayEvent::Quit),
        ];
        for (id, label, ev) in items {
            let item = MenuItem::with_id(id, label, true, None);
            let txc = tx.clone();
            // muda callbacks via MenuEvent handler global ; on stocke l'id
            // et on mappe dans poll(). Ici on enregistre juste l'item.
            let _ = txc;
            menu.append(&item).map_err(|e| e.to_string())?;
            let _ = ev;
        }
        Ok(menu)
    }

    fn build_icon(menu: muda::Menu, tooltip: &str) -> Result<tray_icon::TrayIcon, String> {
        use tray_icon::{Icon, TrayIconBuilder};
        // Dot Coral 32x32 procédural (pas d'asset) : cercle #FF7A2F
        // centré sur fond #0B0C0F. Simple math : dist <= rayon → braise.
        let size: u32 = 32;
        let (bg_r, bg_g, bg_b) = (0x0Bu8, 0x0Cu8, 0x0Fu8);
        let (fg_r, fg_g, fg_b) = (0xFFu8, 0x7Au8, 0x2Fu8);
        let cx = size as f32 / 2.0;
        let cy = size as f32 / 2.0;
        let radius = 10.0f32;
        let mut rgba = Vec::with_capacity((size * size * 4) as usize);
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let inside = dx * dx + dy * dy <= radius * radius;
                if inside {
                    rgba.extend_from_slice(&[fg_r, fg_g, fg_b, 255]);
                } else {
                    rgba.extend_from_slice(&[bg_r, bg_g, bg_b, 255]);
                }
            }
        }
        let icon = Icon::from_rgba(rgba, size, size).map_err(|e| e.to_string())?;
        #[cfg(target_os = "linux")]
        Self::assert_notifier_host()?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(tooltip)
            .with_icon(icon)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(tray)
    }

    #[cfg(target_os = "linux")]
    fn assert_notifier_host() -> Result<(), String> {
        // Check réel : un hôte StatusNotifier doit posséder le nom de bus.
        // Sans hôte (ou sans bus D-Bus de session), on échoue BRUYAMMENT
        // au lieu de rendre une icône qui ne fonctionnera jamais (piège T7).
        match notifier_host_present() {
            Ok(true) => Ok(()),
            Ok(false) => Err(
                "Aucun hôte StatusNotifier sur le bus D-Bus (org.kde.StatusNotifierWatcher sans propriétaire) : tray impossible dans cette session. Lancez un host (ex. GNOME + extension AppIndicator) ou utilisez le fallback fenêtre."
                    .to_string(),
            ),
            Err(e) => Err(format!(
                "Bus D-Bus de session inaccessible, tray Linux impossible : {e}"
            )),
        }
    }

    #[cfg(not(target_os = "linux"))]
    #[allow(dead_code)]
    fn assert_notifier_host() -> Result<(), String> {
        Ok(())
    }

    /// Mappe les MenuEvents muda globaux vers TrayEvent typés.
    pub fn poll_muda(&self) {
        use muda::MenuEvent;
        if let Ok(ev) = MenuEvent::receiver().try_recv() {
            let mapped = match ev.id.as_ref() {
                "show" => Some(TrayEvent::ShowHud),
                "journal" => Some(TrayEvent::OpenJournal),
                "memory" => Some(TrayEvent::OpenMemory),
                "consents" => Some(TrayEvent::OpenConsents),
                "quit" => Some(TrayEvent::Quit),
                _ => None,
            };
            if let Some(e) = mapped {
                let _ = self.tx.send(e);
            }
        }
    }
}

// Mini-erreur sans ajouter anyhow au workspace.
#[cfg(feature = "render")]
mod anyhow_like {
    pub type Result<T, E> = std::result::Result<T, E>;
}

/// Interroge le bus D-Bus de session : un hôte StatusNotifier possède-t-il
/// `org.kde.StatusNotifierWatcher` ? Compilé sur tous les OS (vérifié par la
/// CI Windows), appelé uniquement sur Linux avant de créer l'icône.
#[cfg(feature = "render")]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn notifier_host_present() -> Result<bool, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("runtime tokio pour D-Bus : {e}"))?;
    rt.block_on(async {
        let conn = zbus::Connection::session()
            .await
            .map_err(|e| format!("connexion au bus de session : {e}"))?;
        let proxy = zbus::fdo::DBusProxy::new(&conn)
            .await
            .map_err(|e| format!("proxy org.freedesktop.DBus : {e}"))?;
        let name = zbus::names::BusName::try_from("org.kde.StatusNotifierWatcher")
            .map_err(|e| format!("nom de bus invalide : {e}"))?;
        proxy
            .name_has_owner(name)
            .await
            .map_err(|e| format!("NameHasOwner : {e}"))
    })
}
