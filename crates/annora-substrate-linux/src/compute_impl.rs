use crate::telemetry::read_host_meminfo;
use annora_substrate::{Compute, ComputeBackend};

pub struct LinuxComputeBackend {
    preferred: ComputeBackend,
}

impl LinuxComputeBackend {
    pub fn new() -> Self {
        Self {
            preferred: ComputeBackend::Cuda,
        }
    }

    pub fn total_physical_ram(&self) -> u64 {
        read_host_meminfo().mem_total_bytes
    }

    pub fn available_physical_ram(&self) -> u64 {
        read_host_meminfo().mem_available_bytes
    }
}

impl Default for LinuxComputeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Compute for LinuxComputeBackend {
    fn available_backends(&self) -> Vec<ComputeBackend> {
        vec![
            ComputeBackend::Cuda,
            ComputeBackend::Vulkan,
            ComputeBackend::Cpu,
        ]
    }

    fn preferred_backend(&self) -> ComputeBackend {
        self.preferred.clone()
    }

    fn vram_available_bytes(&self) -> u64 {
        16 * 1024 * 1024 * 1024 // 16 GB VRAM
    }

    fn ram_available_bytes(&self) -> u64 {
        self.available_physical_ram()
    }

    fn supports_quantization(&self, quant: &str) -> bool {
        matches!(quant, "int8" | "q4_0" | "q4_k" | "q8_0")
    }
}
