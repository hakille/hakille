use crate::crdt::VectorClock;
use crate::handoff::ConversationHandoffState;
use hakille_core::{EncryptedBlob, Tier};
use serde::{Deserialize, Serialize};

/// Version du protocole de sync (négociée via `Handshake`).
pub const SYNC_PROTOCOL_VERSION: u32 = 1;

fn default_sync_protocol_version() -> u32 {
    SYNC_PROTOCOL_VERSION
}

/// Compatibilité inter-versions : pour l'instant égalité stricte (v1 ↔ v1).
/// Toute évolution majeure devra incrémenter `SYNC_PROTOCOL_VERSION` et
/// élargir cette fonction (ex. plage compatible), jamais de compat
/// silencieuse.
pub fn versions_compatible(a: u32, b: u32) -> bool {
    a == b
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncMessage {
    Handshake {
        device_id: String,
        tier: Tier,
        clock: VectorClock,
        #[serde(default = "default_sync_protocol_version")]
        protocol_version: u32,
    },
    SyncRequest {
        from_device_id: String,
        from_clock: VectorClock,
        #[serde(default = "default_sync_protocol_version")]
        protocol_version: u32,
    },
    SyncResponse {
        from_device_id: String,
        encrypted_delta: EncryptedBlob,
        current_clock: VectorClock,
        #[serde(default = "default_sync_protocol_version")]
        protocol_version: u32,
    },
    HandoffPush {
        state: ConversationHandoffState,
        #[serde(default = "default_sync_protocol_version")]
        protocol_version: u32,
    },
    Acknowledge {
        device_id: String,
        success: bool,
        #[serde(default = "default_sync_protocol_version")]
        protocol_version: u32,
    },
}

impl SyncMessage {
    /// Champ version porté par chaque message (négociation + rejet poli).
    pub fn version(&self) -> u32 {
        match self {
            SyncMessage::Handshake {
                protocol_version, ..
            }
            | SyncMessage::SyncRequest {
                protocol_version, ..
            }
            | SyncMessage::SyncResponse {
                protocol_version, ..
            }
            | SyncMessage::HandoffPush {
                protocol_version, ..
            }
            | SyncMessage::Acknowledge {
                protocol_version, ..
            } => *protocol_version,
        }
    }

    /// `true` si `other_version` est interopérable avec ce message.
    pub fn compatible(&self, other_version: u32) -> bool {
        versions_compatible(self.version(), other_version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version_compatible() {
        let msg = SyncMessage::Acknowledge {
            device_id: "n1".to_string(),
            success: true,
            protocol_version: SYNC_PROTOCOL_VERSION,
        };
        assert_eq!(msg.version(), 1);
        assert!(msg.compatible(1));
        assert!(!msg.compatible(2));
        assert!(versions_compatible(1, 1));
        assert!(!versions_compatible(1, 0));
    }

    #[test]
    fn test_protocol_version_wire_default() {
        // Les messages anciens sans champ version se désérialisent en v1.
        let json = r#"{"Acknowledge":{"device_id":"n1","success":true}}"#;
        let msg: SyncMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.version(), SYNC_PROTOCOL_VERSION);
    }
}
