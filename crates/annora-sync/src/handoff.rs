use annora_core::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationHandoffState {
    pub source_device_id: String,
    pub target_device_id: String,
    pub last_utterance: String,
    pub pending_action: Option<String>,
    pub interaction_focus: Option<String>,
    pub handoff_token: String,
    pub timestamp: Timestamp,
}

pub struct HandoffCoordinator {
    device_id: String,
    active_handoff: Option<ConversationHandoffState>,
}

impl HandoffCoordinator {
    pub fn new(device_id: impl Into<String>) -> Self {
        Self {
            device_id: device_id.into(),
            active_handoff: None,
        }
    }

    pub fn prepare_handoff(
        &mut self,
        target_device_id: &str,
        last_utterance: &str,
        pending_action: Option<String>,
        focus: Option<String>,
    ) -> ConversationHandoffState {
        let state = ConversationHandoffState {
            source_device_id: self.device_id.clone(),
            target_device_id: target_device_id.to_string(),
            last_utterance: last_utterance.to_string(),
            pending_action,
            interaction_focus: focus,
            handoff_token: uuid::Uuid::now_v7().to_string(),
            timestamp: Timestamp::now(),
        };
        self.active_handoff = Some(state.clone());
        state
    }

    pub fn receive_handoff(&mut self, state: ConversationHandoffState) -> bool {
        if state.target_device_id == self.device_id {
            self.active_handoff = Some(state);
            true
        } else {
            false
        }
    }

    pub fn current_state(&self) -> Option<&ConversationHandoffState> {
        self.active_handoff.as_ref()
    }
}
