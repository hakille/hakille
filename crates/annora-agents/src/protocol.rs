use annora_core::{KeyPair, Signature, Timestamp};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterAgentMessage {
    pub message_id: Uuid,
    pub sender_id: String,
    pub recipient_id: String,
    pub action: String,
    pub payload: serde_json::Value,
    pub timestamp: Timestamp,
    pub signature: Signature,
}

impl InterAgentMessage {
    pub fn sign_and_create(
        sender_id: impl Into<String>,
        recipient_id: impl Into<String>,
        action: impl Into<String>,
        payload: serde_json::Value,
        keypair: &KeyPair,
    ) -> Self {
        let msg_id = Uuid::now_v7();
        let ts = Timestamp::now();
        let s_id = sender_id.into();
        let r_id = recipient_id.into();
        let act = action.into();

        let mut data_to_sign = Vec::new();
        data_to_sign.extend_from_slice(msg_id.as_bytes());
        data_to_sign.extend_from_slice(s_id.as_bytes());
        data_to_sign.extend_from_slice(r_id.as_bytes());
        data_to_sign.extend_from_slice(act.as_bytes());
        data_to_sign.extend_from_slice(payload.to_string().as_bytes());

        let sig = keypair.sign(&data_to_sign);

        Self {
            message_id: msg_id,
            sender_id: s_id,
            recipient_id: r_id,
            action: act,
            payload,
            timestamp: ts,
            signature: sig,
        }
    }

    pub fn verify_signature(&self, keypair: &KeyPair) -> bool {
        let mut data = Vec::new();
        data.extend_from_slice(self.message_id.as_bytes());
        data.extend_from_slice(self.sender_id.as_bytes());
        data.extend_from_slice(self.recipient_id.as_bytes());
        data.extend_from_slice(self.action.as_bytes());
        data.extend_from_slice(self.payload.to_string().as_bytes());

        keypair.verify(&data, &self.signature)
    }
}
