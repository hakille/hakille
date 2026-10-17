//! Adaptateurs OS — `DesktopHost` est la seule frontière platform.
//!
//! Tray, hotkeys, overlay, clipboard, window : ce sont des adaptateurs OS,
//! pas des concepts Slint. Pur Rust, testable sans fenêtre.

pub mod clipboard;
pub mod host;
pub mod hotkey;
pub mod overlay;
pub mod tray;
pub mod window;

pub use clipboard::{Clipboard, InMemoryClipboard};
pub use host::{DesktopHost, HostError, NoopHost};
pub use hotkey::{HotkeyError, HotkeySpec};
pub use overlay::{OverlayBackend, OverlayPlacement};
pub use tray::{TrayError, TraySpec};
pub use window::{PlatformKind, WindowHints};
