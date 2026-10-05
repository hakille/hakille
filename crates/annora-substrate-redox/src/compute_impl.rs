use annora_substrate::{Compute, ComputeBackend};

pub struct RedoxComputeBackend;

impl RedoxComputeBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RedoxComputeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Compute for RedoxComputeBackend {
    fn available_backends(&self) -> Vec<ComputeBackend> {
        vec![ComputeBackend::Cpu, ComputeBackend::Vulkan]
    }

    fn preferred_backend(&self) -> ComputeBackend {
        ComputeBackend::Cpu
    }

    fn vram_available_bytes(&self) -> u64 {
        0
    }

    fn ram_available_bytes(&self) -> u64 {
        64 * 1024 * 1024 * 1024 // 64 GB
    }

    fn supports_quantization(&self, quant: &str) -> bool {
        matches!(quant, "int8" | "q4_0" | "q4_k")
    }
}
