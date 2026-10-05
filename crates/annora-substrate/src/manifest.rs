use annora_core::{CapabilityScope, Tier};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformManifest {
    pub tier: Tier,
    pub capabilities_granted: Vec<CapabilityScope>,
    pub capabilities_denied: Vec<CapabilityScope>,
    pub max_memory_bytes: u64,
    pub gpu_vram_bytes: Option<u64>,
    pub has_secure_enclave: bool,
    pub has_tpm: bool,
    pub battery_powered: bool,
    pub thermal_envelope_watts: Option<f64>,
    pub os_name: String,
    pub os_version: String,
}
