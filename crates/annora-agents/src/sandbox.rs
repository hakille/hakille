use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxLimits {
    pub max_memory_bytes: usize,
    pub max_fuel: u64,
    pub max_execution_time_ms: u64,
    pub allow_network: bool,
    pub allow_filesystem: bool,
}

impl Default for SandboxLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 64 * 1024 * 1024, // 64 MB
            max_fuel: 10_000_000,
            max_execution_time_ms: 5000,
            allow_network: false,
            allow_filesystem: false,
        }
    }
}

pub struct AgentSandboxConfig {
    pub limits: SandboxLimits,
    pub environment_variables: Vec<(String, String)>,
}

impl AgentSandboxConfig {
    pub fn new(limits: SandboxLimits) -> Self {
        Self {
            limits,
            environment_variables: Vec::new(),
        }
    }

    pub fn default_secure() -> Self {
        Self::new(SandboxLimits::default())
    }
}
