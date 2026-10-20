use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxLimits {
    pub max_memory_bytes: usize,
    pub max_fuel: u64,
    pub max_execution_time_ms: u64,
    pub allow_network: bool,
    pub allow_filesystem: bool,
    /// Allowlist d'egress (IP ou hôtes exacts) exigée EN PLUS de
    /// `allow_network` : liste vide = aucun egress autorisé (fail-closed).
    #[serde(default)]
    pub egress_allowlist: Vec<String>,
}

impl Default for SandboxLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 64 * 1024 * 1024, // 64 MB
            max_fuel: 10_000_000,
            max_execution_time_ms: 5000,
            allow_network: false,
            allow_filesystem: false,
            egress_allowlist: Vec::new(),
        }
    }
}

impl SandboxLimits {
    /// Egress autorisé seulement si `allow_network` ET adresse présente
    /// dans l'allowlist (comparaison exacte, fail-closed sinon).
    pub fn is_egress_allowed(&self, host_or_ip: &str) -> bool {
        if !self.allow_network {
            return false;
        }
        let needle = host_or_ip.trim().to_lowercase();
        self.egress_allowlist
            .iter()
            .any(|a| a.trim().to_lowercase() == needle)
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
