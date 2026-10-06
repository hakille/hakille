use annora_core::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShadowState {
    Disconnected,
    Connecting,
    SyncingState,
    ActiveStreaming,
    SuspendedDozing,
    ReconnectingBackoff,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowDelta {
    pub key: String,
    pub value: Vec<u8>,
    pub version: u64,
    pub timestamp: Timestamp,
    pub is_deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowDaemonConfig {
    pub cluster_endpoint: String, // e.g., "tikv://cluster.annora.local:2379"
    pub device_id: String,
    pub push_token: Option<String>, // APNs / FCM token
    pub heartbeat_secs: u32,
}

impl Default for ShadowDaemonConfig {
    fn default() -> Self {
        Self {
            cluster_endpoint: "tikv://cluster.annora.local:2379".to_string(),
            device_id: "mobile-edge-node-01".to_string(),
            push_token: Some("apns-push-token-xyz".to_string()),
            heartbeat_secs: 30,
        }
    }
}

pub struct MobileShadowSync {
    config: ShadowDaemonConfig,
    state: ShadowState,
    last_synced: Timestamp,
    pending_local_deltas: VecDeque<ShadowDelta>,
    received_shadow_deltas: VecDeque<ShadowDelta>,
    edge_state: std::collections::HashMap<String, ShadowDelta>,
    current_version: u64,
    retry_count: u32,
}

impl MobileShadowSync {
    pub fn new(config: ShadowDaemonConfig) -> Self {
        Self {
            config,
            state: ShadowState::Disconnected,
            last_synced: Timestamp::now(),
            pending_local_deltas: VecDeque::new(),
            received_shadow_deltas: VecDeque::new(),
            edge_state: std::collections::HashMap::new(),
            current_version: 1,
            retry_count: 0,
        }
    }

    pub fn state(&self) -> ShadowState {
        self.state
    }

    pub fn config(&self) -> &ShadowDaemonConfig {
        &self.config
    }

    pub fn last_synced(&self) -> Timestamp {
        self.last_synced.clone()
    }

    pub fn get_value(&self, key: &str) -> Option<Vec<u8>> {
        self.edge_state.get(key).and_then(|d| {
            if d.is_deleted {
                None
            } else {
                Some(d.value.clone())
            }
        })
    }

    pub fn apply_delta_crdt(&mut self, delta: ShadowDelta) -> bool {
        let key = delta.key.clone();
        if let Some(existing) = self.edge_state.get(&key) {
            // CRDT LWW resolution: Higher version wins, then timestamp, with tombstone precedence on tie
            let incoming_wins = if delta.version > existing.version {
                true
            } else if delta.version == existing.version {
                if delta.timestamp.0 > existing.timestamp.0 {
                    true
                } else if delta.timestamp.0 == existing.timestamp.0 {
                    delta.is_deleted && !existing.is_deleted
                } else {
                    false
                }
            } else {
                false
            };

            if incoming_wins {
                self.edge_state.insert(key, delta);
                true
            } else {
                false
            }
        } else {
            self.edge_state.insert(key, delta);
            true
        }
    }

    pub fn connect(&mut self) -> Result<(), String> {
        self.state = ShadowState::Connecting;
        // In real execution, handshakes with TiKV cluster shadow daemon
        self.state = ShadowState::SyncingState;
        self.sync_deltas();
        self.state = ShadowState::ActiveStreaming;
        self.retry_count = 0;
        Ok(())
    }

    pub fn queue_local_delta(&mut self, key: &str, value: &[u8]) {
        self.current_version += 1;
        let delta = ShadowDelta {
            key: key.to_string(),
            value: value.to_vec(),
            version: self.current_version,
            timestamp: Timestamp::now(),
            is_deleted: false,
        };
        self.apply_delta_crdt(delta.clone());
        self.pending_local_deltas.push_back(delta);
    }

    pub fn queue_local_deletion(&mut self, key: &str) {
        self.current_version += 1;
        let delta = ShadowDelta {
            key: key.to_string(),
            value: Vec::new(),
            version: self.current_version,
            timestamp: Timestamp::now(),
            is_deleted: true,
        };
        self.apply_delta_crdt(delta.clone());
        self.pending_local_deltas.push_back(delta);
    }

    pub fn push_remote_delta(&mut self, delta: ShadowDelta) {
        if delta.version > self.current_version {
            self.current_version = delta.version;
        }
        self.apply_delta_crdt(delta.clone());
        self.received_shadow_deltas.push_back(delta);
    }

    pub fn received_deltas(&self) -> &VecDeque<ShadowDelta> {
        &self.received_shadow_deltas
    }

    pub fn sync_deltas(&mut self) -> usize {
        let count = self.pending_local_deltas.len();
        self.pending_local_deltas.clear();
        self.last_synced = Timestamp::now();
        count
    }

    pub fn enter_doze(&mut self) {
        // Device entering background sleep (iOS BGTaskScheduler or Android Doze)
        self.state = ShadowState::SuspendedDozing;
    }

    pub fn wake_and_resync(&mut self) -> usize {
        self.state = ShadowState::SyncingState;
        let drained = self.sync_deltas();
        self.state = ShadowState::ActiveStreaming;
        drained
    }

    pub fn handle_network_disconnect(&mut self) {
        self.retry_count += 1;
        self.state = ShadowState::ReconnectingBackoff;
    }

    pub fn backoff_delay_ms(&self) -> u64 {
        let base: u64 = 500;
        let exp = self.retry_count.min(6);
        base * (1 << exp)
    }
}

impl Default for MobileShadowSync {
    fn default() -> Self {
        Self::new(ShadowDaemonConfig::default())
    }
}
