//! Hakille UI model — frontière stable indépendante du renderer.
//!
//! Règles non négociables (plan refonte §3) :
//! - le renderer ne reçoit que des view-models immuables ;
//! - aucune clé, réseau, mémoire brute ou capability ici ;
//! - toute interaction produit un `UiEvent` typé, converti en `UiToKernel`.
//!
//! Ce crate est pur Rust, aucun GPU, aucun toolkit de rendu, testable headless.
//! `hakille-ui-shell::bridge::ShellSnapshot` reste la référence historique ;
//! `StableSnapshot` ici est la version canonique vers laquelle le shell
//! migre (champs identiques, sérialisable, versionnée).

pub mod events;
pub mod snapshot;
pub mod viewmodels;

pub use events::{KernelEvent, UiEvent, UiEventError};
pub use snapshot::{Destination, ShellPrefs, StableSnapshot};
pub use viewmodels::{JournalRow, MemoryRow, PowerRow, PowersView};
