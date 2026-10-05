use annora_core::{CapabilityScope, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub required_capabilities: Vec<CapabilityScope>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentStatus {
    Installed,
    Running,
    Paused,
    Terminated,
}

pub struct AgentRegistry {
    agents: HashMap<String, (AgentManifest, AgentStatus)>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }

    pub fn register(&mut self, manifest: AgentManifest) -> Result<()> {
        let id = manifest.id.clone();
        self.agents.insert(id, (manifest, AgentStatus::Installed));
        Ok(())
    }

    pub fn get_manifest(&self, agent_id: &str) -> Option<&AgentManifest> {
        self.agents.get(agent_id).map(|(m, _)| m)
    }

    pub fn get_status(&self, agent_id: &str) -> Option<AgentStatus> {
        self.agents.get(agent_id).map(|(_, s)| s.clone())
    }

    pub fn set_status(&mut self, agent_id: &str, status: AgentStatus) -> bool {
        if let Some((_, s)) = self.agents.get_mut(agent_id) {
            *s = status;
            true
        } else {
            false
        }
    }

    pub fn list_agents(&self) -> Vec<&AgentManifest> {
        self.agents.values().map(|(m, _)| m).collect()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
