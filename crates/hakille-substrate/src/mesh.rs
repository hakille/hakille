//! Air-Gapped Sovereign Mesh Substrate & Hardware Bearer Abstraction (F45).
//!
//! Provides OS-agnostic bearer abstractions for Wi-Fi Aware (NAN), BLE Mesh,
//! and Ad-Hoc air-gapped direct transports, enabling delay-tolerant peer-to-peer communication.

use async_trait::async_trait;
use hakille_core::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;
use uuid::Uuid;

/// Hardware and protocol bearer kinds supported by the sovereign mesh
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MeshBearerKind {
    /// Wi-Fi Alliance NAN (Neighbor Awareness Networking / Wi-Fi Aware)
    WiFiAware,
    /// Bluetooth Low Energy Mesh (flooding or managed flood)
    BleMesh,
    /// Direct link local UDP broadcast/multicast across air-gapped subnet
    AdHocUdp,
}

impl std::fmt::Display for MeshBearerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WiFiAware => write!(f, "Wi-Fi Aware (NAN)"),
            Self::BleMesh => write!(f, "BLE Mesh"),
            Self::AdHocUdp => write!(f, "Ad-Hoc UDP Subnet"),
        }
    }
}

/// Metadata and link quality for a discovered sovereign mesh peer
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshPeerInfo {
    /// Decentralized Identifier of the peer
    pub peer_did: String,
    /// Physical bearer over which the peer was discovered
    pub bearer: MeshBearerKind,
    /// Received Signal Strength Indicator in dBm (e.g., -65 dBm)
    pub rssi: i16,
    /// Unix timestamp in milliseconds when the peer beacon was received
    pub last_seen_ms: u64,
}

impl MeshPeerInfo {
    pub fn new(
        peer_did: impl Into<String>,
        bearer: MeshBearerKind,
        rssi: i16,
        last_seen_ms: u64,
    ) -> Self {
        Self {
            peer_did: peer_did.into(),
            bearer,
            rssi,
            last_seen_ms,
        }
    }

    /// Evaluates if peer signal is sufficiently strong for high-throughput sync
    pub fn is_high_quality_link(&self) -> bool {
        self.rssi >= -75
    }
}

/// Authenticated and encrypted frame transmitted over air-gapped mesh
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedMeshFrame {
    /// Globally unique identifier for frame deduplication
    pub frame_id: Uuid,
    /// Ephemeral X25519 or Ed25519 public key of the sending node
    pub sender_ephemeral_pubkey: [u8; 32],
    /// Optional truncated destination filter (None indicates broadcast/epidemic gossip)
    pub recipient_filter: Option<[u8; 16]>,
    /// Encrypted payload bytes (ChaCha20-Poly1305 or XChaCha20-Poly1305)
    pub ciphertext: Vec<u8>,
    /// Authentication tag (16 bytes Poly1305)
    pub tag: [u8; 16],
    /// Remaining network hops allowed before drop (default: 5)
    pub hop_limit: u8,
}

impl EncryptedMeshFrame {
    /// Create a new broadcast gossip frame
    pub fn new_broadcast(
        sender_ephemeral_pubkey: [u8; 32],
        ciphertext: Vec<u8>,
        tag: [u8; 16],
        hop_limit: u8,
    ) -> Self {
        Self {
            frame_id: Uuid::new_v4(),
            sender_ephemeral_pubkey,
            recipient_filter: None,
            ciphertext,
            tag,
            hop_limit,
        }
    }

    /// Create a directed/filtered unicast frame
    pub fn new_unicast(
        sender_ephemeral_pubkey: [u8; 32],
        recipient_filter: [u8; 16],
        ciphertext: Vec<u8>,
        tag: [u8; 16],
        hop_limit: u8,
    ) -> Self {
        Self {
            frame_id: Uuid::new_v4(),
            sender_ephemeral_pubkey,
            recipient_filter: Some(recipient_filter),
            ciphertext,
            tag,
            hop_limit,
        }
    }

    /// Decrements the hop limit, returning true if the frame is still valid to forward
    pub fn decrement_hop(&mut self) -> bool {
        if self.hop_limit > 0 {
            self.hop_limit -= 1;
            true
        } else {
            false
        }
    }

    /// Returns true if this frame is addressed to all peers (broadcast)
    pub fn is_broadcast(&self) -> bool {
        self.recipient_filter.is_none()
    }
}

/// Hardware mesh bearer abstraction trait implemented per platform/radio
#[async_trait]
pub trait MeshSubstrate: Send + Sync + 'static {
    /// Initiates active neighbor discovery on the specified bearers
    async fn start_mesh_discovery(&self, bearers: &[MeshBearerKind]) -> Result<()>;

    /// Stops neighbor discovery to save radio power
    async fn stop_mesh_discovery(&self) -> Result<()>;

    /// Returns the currently visible sovereign peers
    async fn discover_peers(&self) -> Result<Vec<MeshPeerInfo>>;

    /// Broadcasts an encrypted frame across all enabled active bearers
    async fn broadcast_frame(&self, frame: EncryptedMeshFrame) -> Result<()>;

    /// Reads the next incoming frame from the radio receive queue if available
    async fn receive_frame(&self) -> Result<Option<EncryptedMeshFrame>>;
}

/// In-memory loopback and multi-peer simulation implementation of MeshSubstrate.
/// Used for deterministic unit and scenario testing as well as air-gapped simulation.
pub struct InMemoryMeshSubstrate {
    peers: RwLock<Vec<MeshPeerInfo>>,
    active_bearers: RwLock<HashSet<MeshBearerKind>>,
    is_discovering: AtomicBool,
    incoming_queue: Mutex<VecDeque<EncryptedMeshFrame>>,
    broadcasted_frames: Mutex<Vec<EncryptedMeshFrame>>,
}

impl Default for InMemoryMeshSubstrate {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryMeshSubstrate {
    pub fn new() -> Self {
        Self {
            peers: RwLock::new(Vec::new()),
            active_bearers: RwLock::new(HashSet::new()),
            is_discovering: AtomicBool::new(false),
            incoming_queue: Mutex::new(VecDeque::new()),
            broadcasted_frames: Mutex::new(Vec::new()),
        }
    }

    /// Register a simulated peer
    pub fn add_peer(&self, peer: MeshPeerInfo) {
        let mut peers = self.peers.write().unwrap();
        peers.retain(|p| p.peer_did != peer.peer_did || p.bearer != peer.bearer);
        peers.push(peer);
    }

    /// Remove a simulated peer
    pub fn remove_peer(&self, peer_did: &str) {
        let mut peers = self.peers.write().unwrap();
        peers.retain(|p| p.peer_did != peer_did);
    }

    /// Inject an incoming frame into the substrate's receive queue
    pub async fn inject_incoming_frame(&self, frame: EncryptedMeshFrame) {
        let mut queue = self.incoming_queue.lock().await;
        queue.push_back(frame);
    }

    /// Retrieve all frames broadcasted by this substrate node
    pub async fn get_broadcasted_frames(&self) -> Vec<EncryptedMeshFrame> {
        let frames = self.broadcasted_frames.lock().await;
        frames.clone()
    }

    /// Clear all broadcasted frames history
    pub async fn clear_broadcasted_frames(&self) {
        let mut frames = self.broadcasted_frames.lock().await;
        frames.clear();
    }

    pub fn is_discovery_active(&self) -> bool {
        self.is_discovering.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl MeshSubstrate for InMemoryMeshSubstrate {
    async fn start_mesh_discovery(&self, bearers: &[MeshBearerKind]) -> Result<()> {
        let mut active = self.active_bearers.write().unwrap();
        for bearer in bearers {
            active.insert(*bearer);
        }
        self.is_discovering.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn stop_mesh_discovery(&self) -> Result<()> {
        let mut active = self.active_bearers.write().unwrap();
        active.clear();
        self.is_discovering.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn discover_peers(&self) -> Result<Vec<MeshPeerInfo>> {
        let peers = self.peers.read().unwrap();
        let active = self.active_bearers.read().unwrap();

        // If discovery is active, filter by active bearers; otherwise return empty
        if !self.is_discovering.load(Ordering::SeqCst) {
            return Ok(Vec::new());
        }

        let filtered = peers
            .iter()
            .filter(|p| active.contains(&p.bearer))
            .cloned()
            .collect();

        Ok(filtered)
    }

    async fn broadcast_frame(&self, frame: EncryptedMeshFrame) -> Result<()> {
        let mut broadcasted = self.broadcasted_frames.lock().await;
        broadcasted.push(frame);
        Ok(())
    }

    async fn receive_frame(&self) -> Result<Option<EncryptedMeshFrame>> {
        let mut queue = self.incoming_queue.lock().await;
        Ok(queue.pop_front())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mesh_bearer_display_and_equality() {
        assert_eq!(MeshBearerKind::WiFiAware.to_string(), "Wi-Fi Aware (NAN)");
        assert_eq!(MeshBearerKind::BleMesh.to_string(), "BLE Mesh");
        assert_eq!(MeshBearerKind::AdHocUdp.to_string(), "Ad-Hoc UDP Subnet");
        assert_ne!(MeshBearerKind::WiFiAware, MeshBearerKind::BleMesh);
    }

    #[test]
    fn test_mesh_peer_info_signal_quality() {
        let peer_strong =
            MeshPeerInfo::new("did:key:z6MkuPeer1", MeshBearerKind::WiFiAware, -60, 1000);
        let peer_weak = MeshPeerInfo::new("did:key:z6MkuPeer2", MeshBearerKind::BleMesh, -85, 1000);

        assert!(peer_strong.is_high_quality_link());
        assert!(!peer_weak.is_high_quality_link());
    }

    #[test]
    fn test_mesh_frame_hops_and_filtering() {
        let pubkey = [7u8; 32];
        let mut bcast = EncryptedMeshFrame::new_broadcast(pubkey, vec![1, 2, 3], [0u8; 16], 2);
        assert!(bcast.is_broadcast());
        assert!(bcast.decrement_hop()); // 2 -> 1
        assert!(bcast.decrement_hop()); // 1 -> 0
        assert!(!bcast.decrement_hop()); // 0 -> fails

        let filter = [0xabu8; 16];
        let unicast = EncryptedMeshFrame::new_unicast(pubkey, filter, vec![4, 5, 6], [0u8; 16], 5);
        assert!(!unicast.is_broadcast());
        assert_eq!(unicast.recipient_filter, Some(filter));
    }

    #[tokio::test]
    async fn test_in_memory_mesh_substrate_full_lifecycle() {
        let substrate = InMemoryMeshSubstrate::new();

        // Initially no discovery, discover_peers is empty
        let peers_initial = substrate.discover_peers().await.unwrap();
        assert!(peers_initial.is_empty());

        // Add 3 peers across different bearers
        let p_wifi = MeshPeerInfo::new("did:key:z6MkuWifi", MeshBearerKind::WiFiAware, -55, 100);
        let p_ble = MeshPeerInfo::new("did:key:z6MkuBle", MeshBearerKind::BleMesh, -70, 101);
        let p_udp = MeshPeerInfo::new("did:key:z6MkuUdp", MeshBearerKind::AdHocUdp, -40, 102);

        substrate.add_peer(p_wifi.clone());
        substrate.add_peer(p_ble.clone());
        substrate.add_peer(p_udp.clone());

        // Start discovery only on WiFiAware and BleMesh
        substrate
            .start_mesh_discovery(&[MeshBearerKind::WiFiAware, MeshBearerKind::BleMesh])
            .await
            .unwrap();
        assert!(substrate.is_discovery_active());

        let discovered = substrate.discover_peers().await.unwrap();
        assert_eq!(discovered.len(), 2);
        assert!(discovered.iter().any(|p| p.peer_did == "did:key:z6MkuWifi"));
        assert!(discovered.iter().any(|p| p.peer_did == "did:key:z6MkuBle"));
        assert!(!discovered.iter().any(|p| p.peer_did == "did:key:z6MkuUdp"));

        // Broadcast a frame
        let frame = EncryptedMeshFrame::new_broadcast([1u8; 32], vec![0xca, 0xfe], [0u8; 16], 5);
        substrate.broadcast_frame(frame.clone()).await.unwrap();

        let sent = substrate.get_broadcasted_frames().await;
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].frame_id, frame.frame_id);

        // Inject incoming frame and receive
        assert!(substrate.receive_frame().await.unwrap().is_none());
        substrate.inject_incoming_frame(frame.clone()).await;
        let received = substrate.receive_frame().await.unwrap();
        assert!(received.is_some());
        assert_eq!(received.unwrap().frame_id, frame.frame_id);

        // Stop discovery
        substrate.stop_mesh_discovery().await.unwrap();
        assert!(!substrate.is_discovery_active());
        let discovered_stopped = substrate.discover_peers().await.unwrap();
        assert!(discovered_stopped.is_empty());
    }
}
