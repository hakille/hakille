//! Bus Virtuel Air-Gap (§35) : Simulateur souverain multi-nœuds en mémoire
//! permettant des tests E2E rigoureux de synchronisation P2P sans socket externe.

use crate::frame::{MeshError, MeshFrame};
use crate::transport::MeshTransport;
use async_trait::async_trait;
use tokio::sync::broadcast;

/// Bus de communication virtuel simulant l'espace radio P2P entre nœuds souverains.
#[derive(Debug, Clone)]
pub struct VirtualAirGapBus {
    sender: broadcast::Sender<MeshFrame>,
}

impl VirtualAirGapBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(64));
        Self { sender }
    }

    /// Connecte un nouveau pair virtuel au bus radio.
    pub fn join_peer(&self, peer_id: impl Into<String>) -> VirtualMeshPeer {
        VirtualMeshPeer {
            peer_id: peer_id.into(),
            bus_sender: self.sender.clone(),
            bus_receiver: tokio::sync::Mutex::new(self.sender.subscribe()),
        }
    }
}

/// Pair souverain connecté au bus virtuel maillé.
pub struct VirtualMeshPeer {
    pub peer_id: String,
    bus_sender: broadcast::Sender<MeshFrame>,
    bus_receiver: tokio::sync::Mutex<broadcast::Receiver<MeshFrame>>,
}

#[async_trait]
impl MeshTransport for VirtualMeshPeer {
    fn peer_id(&self) -> &str {
        &self.peer_id
    }

    async fn broadcast(&self, frame: MeshFrame) -> Result<usize, MeshError> {
        self.bus_sender
            .send(frame)
            .map_err(|e| MeshError::Transport(e.to_string()))
    }

    async fn send_direct(
        &self,
        target_peer_id: &str,
        mut frame: MeshFrame,
    ) -> Result<bool, MeshError> {
        frame.target_peer_id = Some(target_peer_id.to_string());
        self.bus_sender
            .send(frame)
            .map(|_| true)
            .map_err(|e| MeshError::Transport(e.to_string()))
    }

    async fn receive(&self) -> Result<MeshFrame, MeshError> {
        let mut rx = self.bus_receiver.lock().await;
        loop {
            match rx.recv().await {
                Ok(frame) => {
                    // Ignore ses propres trames
                    if frame.sender_peer_id == self.peer_id {
                        continue;
                    }
                    // Filtre les trames unicast adressées à un tiers
                    if let Some(target) = &frame.target_peer_id {
                        if target != &self.peer_id {
                            continue;
                        }
                    }
                    return Ok(frame);
                }
                Err(e) => return Err(MeshError::Transport(e.to_string())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    #[tokio::test]
    async fn test_virtual_air_gap_bus_broadcast_and_direct_messaging() {
        let bus = VirtualAirGapBus::new(128);
        let peer_a = bus.join_peer("node-a");
        let peer_b = bus.join_peer("node-b");
        let peer_c = bus.join_peer("node-c");

        let signing_key = SigningKey::from_bytes(&[11u8; 32]);
        let sym_key = [99u8; 32];
        let nonce = [1u8; 12];

        // 1. Direct message from A to B (C must not receive it)
        let frame_direct = MeshFrame::create_encrypted(
            "node-a",
            Some("node-b".into()),
            1,
            b"Confidentiel pour B",
            &sym_key,
            nonce,
            &signing_key,
        )
        .unwrap();

        peer_a.send_direct("node-b", frame_direct).await.unwrap();

        let recv_b = peer_b.receive().await.unwrap();
        assert_eq!(recv_b.sender_peer_id, "node-a");
        assert_eq!(
            recv_b.verify_and_decrypt(&sym_key).unwrap(),
            b"Confidentiel pour B"
        );

        // 2. Broadcast message from B to all
        let frame_broadcast = MeshFrame::create_encrypted(
            "node-b",
            None,
            2,
            b"Annonce souveraine globale",
            &sym_key,
            nonce,
            &signing_key,
        )
        .unwrap();

        peer_b.broadcast(frame_broadcast).await.unwrap();

        let recv_c = peer_c.receive().await.unwrap();
        assert_eq!(recv_c.sender_peer_id, "node-b");
        assert_eq!(
            recv_c.verify_and_decrypt(&sym_key).unwrap(),
            b"Annonce souveraine globale"
        );
    }
}
