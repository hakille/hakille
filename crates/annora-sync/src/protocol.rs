use crate::crdt::VectorClock;
use crate::handoff::ConversationHandoffState;
use annora_core::{EncryptedBlob, Tier};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncMessage {
    Handshake {
        device_id: String,
        tier: Tier,
        clock: VectorClock,
    },
    SyncRequest {
        from_device_id: String,
        from_clock: VectorClock,
    },
    SyncResponse {
        from_device_id: String,
        encrypted_delta: EncryptedBlob,
        current_clock: VectorClock,
    },
    HandoffPush {
        state: ConversationHandoffState,
    },
    Acknowledge {
        device_id: String,
        success: bool,
    },
}
