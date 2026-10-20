//! Hakille design system (spec Hakille v0.1) — source unique `tokens/`.
//!
//! `generated.rs` est produit par `cargo xtask tokens`, jamais édité à la
//! main. Les composants ci-dessous n'embarquent aucune logique métier :
//! ce sont des projections pures, consommables par tout renderer (desktop,
//! mobile SwiftUI/Compose, WASM).

pub mod action_card;
pub mod autonomy;
pub mod brand;
pub mod color;
pub mod generated;
pub mod hold;
pub mod household;
pub mod layout;
pub mod motion;
pub mod provenance;
pub mod silence;
pub mod sound;
pub mod states;
pub mod typo;
pub mod voice;
pub mod why;

pub use action_card::*;
pub use autonomy::*;
pub use brand::*;
pub use color::*;
pub use hold::*;
pub use household::*;
pub use layout::*;
pub use motion::*;
pub use provenance::*;
pub use silence::*;
pub use sound::*;
pub use states::*;
pub use typo::*;
pub use voice::*;
pub use why::*;
