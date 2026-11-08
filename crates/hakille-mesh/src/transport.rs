//! Abstraction de Transport Maillé P2P (§35).
//!
//! Définit le contrat universel d'échange de trames chiffrées
//! entre appareils souverains Hakille (BLE, Wi-Fi Direct, Bus Virtuel).

use crate::frame::{MeshError, MeshFrame};
use async_trait::async_trait;

/// Contrat de transport pour le réseau maillé pair-à-pair.
#[async_trait]
pub trait MeshTransport: Send + Sync {
    /// Identifiant unique du pair local.
    fn peer_id(&self) -> &str;

    /// Diffuse une trame chiffrée à tous les pairs à portée radio/réseau.
    async fn broadcast(&self, frame: MeshFrame) -> Result<usize, MeshError>;

    /// Transmet une trame chiffrée directement à un pair identifié.
    async fn send_direct(&self, target_peer_id: &str, frame: MeshFrame) -> Result<bool, MeshError>;

    /// Réceptionne la prochaine trame disponible du réseau.
    async fn receive(&self) -> Result<MeshFrame, MeshError>;
}
