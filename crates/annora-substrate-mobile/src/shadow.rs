use annora_core::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowSyncStatus {
    pub shadow_endpoint: String,
    pub last_synced: Timestamp,
    pub pending_shadow_notifications: usize,
    pub is_connected: bool,
}

pub struct MobileShadowSync {
    status: ShadowSyncStatus,
}

impl MobileShadowSync {
    pub fn new(shadow_endpoint: impl Into<String>) -> Self {
        Self {
            status: ShadowSyncStatus {
                shadow_endpoint: shadow_endpoint.into(),
                last_synced: Timestamp::now(),
                pending_shadow_notifications: 0,
                is_connected: false,
            },
        }
    }

    pub fn wake_and_resync(&mut self) -> usize {
        self.status.is_connected = true;
        self.status.last_synced = Timestamp::now();
        let notifications = self.status.pending_shadow_notifications;
        self.status.pending_shadow_notifications = 0;
        notifications
    }

    pub fn status(&self) -> &ShadowSyncStatus {
        &self.status
    }
}
