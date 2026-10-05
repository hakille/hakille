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
}
