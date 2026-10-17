//! Annora ambient shell bridge — headless-first, Slint-native rendering.
//!
//! `annora-ui` owns the tested state machines. This crate maps that state to
//! Slint properties (`ui/annora.slint`) and to OS compositor payloads without
//! ever using a WebView (ADR-007). All inference labels are honest:
//! `Inspected / Simulated / Real` — never "Operational" for stubs.

pub mod actions;
pub mod agents;
pub mod bridge;
pub mod inference_status;
pub mod overlay;
pub mod shell_config;
pub mod views;

#[cfg(feature = "render")]
pub mod hotkey;
#[cfg(feature = "render")]
pub mod render;
#[cfg(feature = "render")]
pub mod tray;

#[cfg(feature = "render")]
slint::include_modules!();

pub use actions::*;
pub use agents::*;
pub use bridge::*;
pub use inference_status::*;
pub use overlay::*;
pub use shell_config::*;
pub use views::*;
