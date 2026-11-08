//! # Hakille Mesh
//!
//! Réseau maillé pair-à-pair souverain, transport Air-Gap et bus virtuel anti-fuite cloud (§35).
//!
//! Principes souverains de Hakille :
//! - 100% Rust natif, zéro dépendance cloud.
//! - Chiffrement AEAD ChaCha20-Poly1305 et signatures Ed25519 par trame.
//! - Garde link-local fail-closed interdisant tout routage WAN.

pub mod frame;
pub mod transport;
pub mod virtual_bus;

pub use frame::{LinkLocalGuard, MeshError, MeshFrame};
pub use transport::MeshTransport;
pub use virtual_bus::{VirtualAirGapBus, VirtualMeshPeer};
