//! Feature 15: Multi-Platform Host Abstraction (Milestone M4)
//!
//! Specifications: T1_F15_01 to T1_F15_05

use hakille_ui_platform::{HotkeySpec, OverlayBackend, PlatformKind, TraySpec};

/// `T1_F15_01`: PlatformKind Detection Completeness
#[test]
fn test_t1_f15_01_platform_kind_detection_completeness() {
    assert_eq!(PlatformKind::for_os("windows"), PlatformKind::Windows);
    assert_eq!(PlatformKind::for_os("linux"), PlatformKind::Linux);
    assert_eq!(PlatformKind::for_os("macos"), PlatformKind::MacOs);
    assert_eq!(PlatformKind::for_os("redox"), PlatformKind::Redox);
    assert_eq!(PlatformKind::for_os("android"), PlatformKind::Mobile);
    assert_eq!(PlatformKind::for_os("web"), PlatformKind::Web);
    assert_eq!(PlatformKind::for_os("solaris"), PlatformKind::Unsupported);
}

/// `T1_F15_02`: Linux Overlay Backend Selection
#[test]
fn test_t1_f15_02_linux_overlay_backend_selection() {
    let backend = OverlayBackend::for_platform("linux");
    assert_eq!(backend, OverlayBackend::WlrLayerShell);
}

/// `T1_F15_03`: macOS Host Hotkey & Overlay
#[test]
fn test_t1_f15_03_macos_host_hotkey_and_overlay() {
    let hotkey = HotkeySpec::default_for("macos");
    assert_eq!(hotkey.accelerators, vec!["Cmd+Space"]);

    let backend = OverlayBackend::for_platform("macos");
    assert_eq!(backend, OverlayBackend::MacPopover);
}

/// `T1_F15_04`: Redox Sovereign Orbital Scheme Connection
#[test]
fn test_t1_f15_04_redox_sovereign_orbital_scheme_connection() {
    let backend = OverlayBackend::for_platform("redox");
    assert_eq!(backend, OverlayBackend::OrbitalDot);
}

/// `T1_F15_05`: System Tray Menu Specification
#[test]
fn test_t1_f15_05_system_tray_menu_specification() {
    let spec = TraySpec::hakille("Veille");
    assert_eq!(spec.menu.len(), 5, "Tray menu must have exactly 5 items");
    assert_eq!(spec.menu[0], "Afficher le HUD");
    assert_eq!(spec.menu[1], "Ouvrir le journal");
    assert_eq!(spec.menu[2], "Ouvrir la mémoire");
    assert_eq!(spec.menu[3], "Ouvrir les consentements");
    assert_eq!(spec.menu[4], "Quitter");
}
