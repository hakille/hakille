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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_handoff_lifecycle() {
        let mut desktop = HandoffCoordinator::new("desktop-node-01");
        let mut mobile = HandoffCoordinator::new("mobile-node-02");
        let mut rogue_device = HandoffCoordinator::new("unauthorized-node-03");

        let handoff_state = desktop.prepare_handoff(
            "mobile-node-02",
            "Peux-tu resumer ce document ?",
            Some("SummarizeDocument".to_string()),
            Some("DocumentViewer".to_string()),
        );

        assert_eq!(handoff_state.source_device_id, "desktop-node-01");
        assert_eq!(handoff_state.target_device_id, "mobile-node-02");
        assert!(!handoff_state.handoff_token.is_empty());

        // Unauthorized node cannot receive handoff
        let intercepted = rogue_device.receive_handoff(handoff_state.clone());
        assert!(!intercepted);
        assert!(rogue_device.current_state().is_none());

        // Legitimate target successfully receives handoff and resumes context
        let received = mobile.receive_handoff(handoff_state);
        assert!(received);
        let active = mobile.current_state().expect("State should be active");
        assert_eq!(active.last_utterance, "Peux-tu resumer ce document ?");
        assert_eq!(active.interaction_focus, Some("DocumentViewer".to_string()));
    }
}

