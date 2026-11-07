//! # Hakille SDK
//!
//! Kit de développement officiel et spécification de paquets d'agents souverains `.hap` (§36).
//!
//! Principes souverains de Hakille :
//! - 100% Rust natif, zéro dépendance cloud.
//! - Manifestes déclaratifs stricts (`agent.toml`) avec validation au chargement.
//! - Format de paquet hermétique `.hap` signé cryptographiquement par le développeur (Ed25519).
//! - Trait `HakilleAgent` et contexte sécurisé `AgentContext` avec capacités atténuées.

pub mod agent;
pub mod biscuit;
pub mod manifest;
pub mod package;

pub use agent::{AgentContext, AgentError, HakilleAgent};
pub use biscuit::AgentPermissionRequest;
pub use manifest::{ManifestError, ManifestQuotas, SdkManifest};
pub use package::{HakilleAgentPackage, PackageError};
