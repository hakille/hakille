pub mod capability;
pub mod gui_agent;
pub mod protocol;
pub mod registry;
pub mod runtime;
pub mod sandbox;

pub use capability::*;
pub use gui_agent::*;
pub use protocol::*;
pub use registry::*;
pub use runtime::*;
pub use sandbox::*;

#[cfg(test)]
mod tests {
    use super::*;
    use annora_core::CapabilityScope;

    #[test]
    fn test_agent_registry_lifecycle() {
        let mut registry = AgentRegistry::new();
        assert_eq!(registry.list_agents().len(), 0);

        let manifest = AgentManifest {
            id: "agent-test-mail".to_string(),
            name: "Mail Agent".to_string(),
            version: "0.1.0".to_string(),
            description: "Sandboxed mail client".to_string(),
            required_capabilities: vec![CapabilityScope::Network, CapabilityScope::Filesystem],
        };

        registry.register(manifest).expect("Registration should succeed");
        assert_eq!(registry.list_agents().len(), 1);
        assert_eq!(registry.get_status("agent-test-mail"), Some(AgentStatus::Installed));

        // State transition to Running
        assert!(registry.set_status("agent-test-mail", AgentStatus::Running));
        assert_eq!(registry.get_status("agent-test-mail"), Some(AgentStatus::Running));

        // State transition to Terminated
        assert!(registry.set_status("agent-test-mail", AgentStatus::Terminated));
        assert_eq!(registry.get_status("agent-test-mail"), Some(AgentStatus::Terminated));
    }

    #[test]
    fn test_uitars_gui_agent_intent_prediction() {
        let agent = UiTarsAgent::new();

        // 1. Click task
        let click_intents = agent.predict_intent_from_task("Click on the Save button");
        assert_eq!(click_intents.len(), 1);
        match &click_intents[0] {
            GuiIntention::Click { x, y, button } => {
                assert_eq!(*x, 0.5);
                assert_eq!(*y, 0.5);
                assert_eq!(*button, annora_substrate::MouseButton::Left);
            }
            _ => panic!("Expected Click intent"),
        }

        // 2. Typing task
        let type_intents = agent.predict_intent_from_task("Write report draft");
        assert_eq!(type_intents.len(), 1);
        match &type_intents[0] {
            GuiIntention::Type { text } => {
                assert!(!text.is_empty());
            }
            _ => panic!("Expected Type intent"),
        }
    }

    #[test]
    fn test_inter_agent_signed_message_protocol() {
        let sender_keys = annora_core::KeyPair::generate();
        let other_keys = annora_core::KeyPair::generate();

        let msg = InterAgentMessage::sign_and_create(
            "agent-mail",
            "agent-calendar",
            "ScheduleMeeting",
            serde_json::json!({ "title": "Architecture Review", "time": "14:00" }),
            &sender_keys,
        );

        // Verification with sender's key succeeds
        assert!(msg.verify_signature(&sender_keys));

        // Verification with another agent's key fails
        assert!(!msg.verify_signature(&other_keys));

        // Tampering with payload fails
        let mut tampered_msg = msg.clone();
        tampered_msg.payload = serde_json::json!({ "title": "Malicious Meeting", "time": "14:00" });
        assert!(!tampered_msg.verify_signature(&sender_keys));
    }

    #[test]
    fn test_capability_quota_enforcement_and_auto_revocation() {
        let quota = CapabilityQuota {
            max_invocations_per_minute: 3,
            max_total_invocations: 5,
            max_payload_bytes_per_call: 1024,
            policy_on_breach: QuotaBreachPolicy::AutoRevoke,
        };
        let mut tracker = AgentQuotaTracker::new(quota);

        // 3 calls within 1 minute succeed
        assert!(tracker.record_invocation(256).is_ok());
        assert!(tracker.record_invocation(256).is_ok());
        assert!(tracker.record_invocation(256).is_ok());
        assert_eq!(tracker.total_calls(), 3);

        // 4th call within the same minute violates rate-limit and triggers AutoRevoke (§31c)
        let rate_breach = tracker.record_invocation(256);
        assert!(rate_breach.is_err());
        assert!(tracker.is_revoked());

        // Subsequent call is blocked due to automatic revocation
        let subsequent = tracker.record_invocation(256);
        assert!(subsequent.is_err());
    }
}
