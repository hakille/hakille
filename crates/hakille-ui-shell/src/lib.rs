//! Hakille ambient shell baseline — headless snapshot, view-models, honnêteté.
//!
//! `hakille-ui` owns the tested state machines. This crate is the frozen
//! renderer-independent baseline of the migration (ADR-009): it maps that
//! state to [`ShellSnapshot`] and pure view-models, without any GPU or
//! toolkit dependency. All inference labels are honest:
//! `Inspected / Simulated / Real` — never "Operational" for stubs.
//!
//! Le shell desktop actif vit dans `hakille-ui-desktop` ; cette baseline est
//! conservée comme filet de sécurité tant que la parité reste testée
//! (`hakille-ui-model` compare `StableSnapshot` et `ShellSnapshot`).

pub mod actions;
pub mod agents;
pub mod bridge;
pub mod inference_status;
pub mod overlay;
pub mod shell_config;
pub mod views;

pub use actions::*;
pub use agents::*;
pub use bridge::*;
pub use inference_status::*;
pub use overlay::*;
pub use shell_config::*;
pub use views::*;
