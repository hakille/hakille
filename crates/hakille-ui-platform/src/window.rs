//! Fenêtre — hints multi-DPI, jamais de fenêtre vide silencieuse.

use serde::{Deserialize, Serialize};

/// Cible d'exécution du shell.
///
/// `Unsupported` est un état explicite, jamais un faux succès : aucun
/// chemin fenêtre n'est implémenté pour cet OS tant qu'un projet de
/// compatibilité dédié n'a pas été ouvert (plan refonte, étape 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlatformKind {
    Windows,
    Linux,
    MacOs,
    Redox,
    Mobile,
    Web,
    Unsupported,
}

impl PlatformKind {
    /// Table OS pure — mêmes valeurs que `cfg(target_os)`, mais testable
    /// depuis n'importe quelle cible.
    ///
    /// `"unknown"` désigne `wasm32-unknown-unknown` (seule cible dont
    /// `target_os` vaut `unknown`) ; `"wasi"` et `"emscripten"` sont des
    /// cibles WASM. Tout OS hors table (freebsd, haiku, fuchsia, …) est
    /// `Unsupported`, pas `Web`.
    pub fn for_os(os: &str) -> Self {
        match os {
            "windows" => Self::Windows,
            "linux" => Self::Linux,
            "macos" => Self::MacOs,
            "redox" => Self::Redox,
            "android" | "ios" | "mobile" => Self::Mobile,
            "web" | "unknown" | "wasi" | "emscripten" => Self::Web,
            _ => Self::Unsupported,
        }
    }

    pub fn auto() -> Self {
        Self::for_os(std::env::consts::OS)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::MacOs => "macos",
            Self::Redox => "redox",
            Self::Mobile => "mobile",
            Self::Web => "web",
            Self::Unsupported => "unsupported",
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
        Self {
            title: "Hakille".into(),
            width: 720,
            height: 64,
            transparent: true,
            decorations: false,
            topmost: true,
            resizable: false,
        }
    }
    pub fn journal() -> Self {
        Self {
            title: "Hakille — Journal".into(),
            width: 720,
            height: 640,
            transparent: false,
            decorations: true,
            topmost: false,
            resizable: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Table OS de référence : `(cible/std OS, variante attendue)`.
    const OS_TABLE: &[(&str, PlatformKind)] = &[
        ("windows", PlatformKind::Windows),
        ("linux", PlatformKind::Linux),
        ("macos", PlatformKind::MacOs),
        ("redox", PlatformKind::Redox),
        ("android", PlatformKind::Mobile),
        ("ios", PlatformKind::Mobile),
        ("mobile", PlatformKind::Mobile),
        ("web", PlatformKind::Web),
        ("unknown", PlatformKind::Web),
        ("wasi", PlatformKind::Web),
        ("emscripten", PlatformKind::Web),
        ("freebsd", PlatformKind::Unsupported),
        ("haiku", PlatformKind::Unsupported),
        ("fuchsia", PlatformKind::Unsupported),
        ("", PlatformKind::Unsupported),
    ];

    #[test]
    fn hud_hints_frameless_topmost() {
        let h = WindowHints::hud();
        assert!(!h.decorations);
        assert!(h.topmost);
        assert_eq!(h.width, 720);
    }

    #[test]
    fn for_os_covers_whole_table_without_fake_fallback() {
        for (os, expected) in OS_TABLE {
            assert_eq!(PlatformKind::for_os(os), *expected, "os={os}");
        }
    }

    #[test]
    fn as_str_roundtrips_through_for_os_for_every_variant() {
        for (_, kind) in OS_TABLE {
            assert_eq!(PlatformKind::for_os(kind.as_str()), *kind, "kind={kind:?}");
        }
        assert_eq!(PlatformKind::Unsupported.as_str(), "unsupported");
    }

    #[test]
    fn auto_matches_pure_table_of_host_os() {
        assert_eq!(
            PlatformKind::auto(),
            PlatformKind::for_os(std::env::consts::OS)
        );
        #[cfg(target_os = "windows")]
        assert_eq!(PlatformKind::auto(), PlatformKind::Windows);
    }

    #[test]
    fn window_hints_contract_holds_for_every_platform_kind() {
        // Les hints ne divergent pas encore par OS : aucune branche par OS
        // n'est inventée ici ; ce test fige le contrat pour tout le tableau.
        for (_, kind) in OS_TABLE {
            let hud = WindowHints::hud();
            assert!(!hud.decorations, "hud frameless sur {kind:?}");
            assert!(hud.topmost, "hud topmost sur {kind:?}");
            assert!(hud.transparent && !hud.resizable, "hud sur {kind:?}");
            assert_eq!((hud.width, hud.height), (720, 64));

            let journal = WindowHints::journal();
            assert!(journal.decorations, "journal décoré sur {kind:?}");
            assert!(
                !journal.topmost && journal.resizable,
                "journal sur {kind:?}"
            );
            assert!(!journal.transparent);
        }
    }
}
