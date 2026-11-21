//! Synchronisation pair-à-pair chiffrée du Jumeau Domotique via `hakille-mesh` (Air-Gap / LAN).
//!
//! Permet à plusieurs nœuds Hakille d'un même foyer (ex. bureau principal, hub mural,
//! passerelle locale) de répliquer l'inventaire des appareils, les pièces 3D, les scènes
//! et les règles d'automatisation dans des trames `MeshFrame` chiffrées AEAD
//! `ChaCha20-Poly1305` et signées `Ed25519`, strictement confinées au LAN/Air-Gap.

use crate::automation::AutomationRule;
use crate::hub::HomeScene;
use crate::model::{HomeDevice, HomeError, HomeResult, RoomZone};
use chrono::Utc;
use ed25519_dalek::SigningKey;
use hakille_mesh::{LinkLocalGuard, MeshFrame, MeshTransport, VirtualMeshPeer};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Delta sérialisé du jumeau domotique échangé entre nœuds souverains du foyer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HomeTwinSyncDelta {
    pub origin_node_id: String,
    pub sequence: u64,
    pub generated_at: String,
    pub rooms: Vec<RoomZone>,
    pub devices: Vec<HomeDevice>,
    pub scenes: Vec<HomeScene>,
    pub automations: Vec<AutomationRule>,
}

/// Résumé honnête d'une opération de fusion de delta maillé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomeMeshSyncReport {
    pub origin_node_id: String,
    pub frame_id: String,
    pub sequence: u64,
    pub rooms_merged: usize,
    pub devices_merged: usize,
    pub scenes_merged: usize,
    pub automations_merged: usize,
}

impl HomeTwinSyncDelta {
    /// Chiffre (ChaCha20-Poly1305) et signe (Ed25519) le delta dans une trame `hakille-mesh`.
    pub fn seal_mesh_frame(
        &self,
        target_peer_id: Option<String>,
        symmetric_key: &[u8; 32],
        nonce: [u8; 12],
        signing_key: &SigningKey,
    ) -> HomeResult<MeshFrame> {
        let payload = serde_json::to_vec(self)
            .map_err(|e| HomeError::InvalidCommand(format!("Sérialisation delta mesh: {e}")))?;
        MeshFrame::create_encrypted(
            &self.origin_node_id,
            target_peer_id,
            self.sequence,
            &payload,
            symmetric_key,
            nonce,
            signing_key,
        )
        .map_err(|e| HomeError::TransportUnavailable {
            protocol: "hakille-mesh".into(),
            reason: e.to_string(),
        })
    }

    /// Vérifie la signature Ed25519 et déchiffre AEAD une trame `MeshFrame` reçue.
    /// Échoue fail-closed en cas d'altération du texte chiffré ou de clé invalide.
    pub fn open_mesh_frame(frame: &MeshFrame, symmetric_key: &[u8; 32]) -> HomeResult<Self> {
        let plain = frame
            .verify_and_decrypt(symmetric_key)
            .map_err(|e| HomeError::CapabilityDenied(format!("Trame mesh rejetée : {e}")))?;
        serde_json::from_slice(&plain).map_err(|e| HomeError::InvalidFrame {
            protocol: "hakille-mesh".into(),
            details: format!("Payload delta domotique invalide : {e}"),
        })
    }
}

/// Vérifie que la destination de synchronisation maillée respecte strictement `LinkLocalGuard`.
pub fn validate_mesh_peer_endpoint(endpoint: &str) -> HomeResult<()> {
    if LinkLocalGuard::is_link_local_safe(endpoint) {
        Ok(())
    } else {
        Err(HomeError::LanGuardBlocked {
            endpoint: endpoint.to_string(),
            reason: "Synchronisation maillée refusée : adresse hors link-local / air-gap".into(),
        })
    }
}

/// Diffuse un delta domotique chiffré sur un pair `VirtualMeshPeer` local et renvoie la trame scellée.
pub async fn broadcast_home_delta_on_mesh(
    peer: &VirtualMeshPeer,
    delta: &HomeTwinSyncDelta,
    symmetric_key: &[u8; 32],
    nonce: [u8; 12],
    signing_key: &SigningKey,
) -> HomeResult<MeshFrame> {
    let frame = delta.seal_mesh_frame(None, symmetric_key, nonce, signing_key)?;
    peer.broadcast(frame.clone())
        .await
        .map_err(|e| HomeError::TransportUnavailable {
            protocol: "hakille-mesh".into(),
            reason: e.to_string(),
        })?;
    Ok(frame)
}

/// Fusionne de manière déterministe (LWW par identifiant + horodatage) des appareils reçus.
pub fn merge_devices_lww(
    local: &mut BTreeMap<String, HomeDevice>,
    incoming: &[HomeDevice],
) -> usize {
    let mut merged = 0;
    for dev in incoming {
        let replace = match local.get(&dev.id) {
            None => true,
            Some(existing) => dev.state.updated_at >= existing.state.updated_at,
        };
        if replace {
            local.insert(dev.id.clone(), dev.clone());
            merged += 1;
        }
    }
    merged
}

/// Construit un delta à partir de tranches d'état courantes.
pub fn build_sync_delta(
    origin_node_id: impl Into<String>,
    sequence: u64,
    rooms: Vec<RoomZone>,
    devices: Vec<HomeDevice>,
    scenes: Vec<HomeScene>,
    automations: Vec<AutomationRule>,
) -> HomeTwinSyncDelta {
    HomeTwinSyncDelta {
        origin_node_id: origin_node_id.into(),
        sequence,
        generated_at: Utc::now().to_rfc3339(),
        rooms,
        devices,
        scenes,
        automations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DeviceCategory, ProtocolKind};
    use hakille_mesh::VirtualAirGapBus;

    #[tokio::test]
    async fn mesh_sync_roundtrip_and_tamper_rejection() {
        assert!(validate_mesh_peer_endpoint("airgap://foyer-nord").is_ok());
        assert!(validate_mesh_peer_endpoint("https://cloud.example.com").is_err());

        let bus = VirtualAirGapBus::new(64);
        let peer_a = bus.join_peer("hub-salon");
        let peer_b = bus.join_peer("hub-bureau");

        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let sym_key = [42u8; 32];
        let nonce = [3u8; 12];

        let dev = HomeDevice::new(
            "lampe-1",
            "Lampe Salon",
            "salon",
            ProtocolKind::Matter,
            DeviceCategory::Light,
            "192.168.1.40:5540",
        );
        let delta = build_sync_delta("hub-salon", 1, vec![], vec![dev], vec![], vec![]);

        let sent = broadcast_home_delta_on_mesh(&peer_a, &delta, &sym_key, nonce, &signing_key)
            .await
            .unwrap();
        let received = peer_b.receive().await.unwrap();
        assert_eq!(received.frame_id, sent.frame_id);

        let decoded = HomeTwinSyncDelta::open_mesh_frame(&received, &sym_key).unwrap();
        assert_eq!(decoded.devices.len(), 1);
        assert_eq!(decoded.devices[0].id, "lampe-1");

        // Falsification d'un octet du ciphertext => rejet cryptographique fail-closed
        let mut tampered = received.clone();
        tampered.encrypted_payload[0] ^= 0xFF;
        assert!(HomeTwinSyncDelta::open_mesh_frame(&tampered, &sym_key).is_err());
    }
}
