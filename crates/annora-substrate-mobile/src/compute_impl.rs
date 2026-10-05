use annora_substrate::{Compute, ComputeBackend};

pub struct MobileComputeBackend;

impl MobileComputeBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MobileComputeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Compute for MobileComputeBackend {
    fn available_backends(&self) -> Vec<ComputeBackend> {
        vec![ComputeBackend::Npu, ComputeBackend::Cpu]
    }

    fn preferred_backend(&self) -> ComputeBackend {
        ComputeBackend::Npu
    }

    fn vram_available_bytes(&self) -> u64 {
        2 * 1024 * 1024 * 1024 // 2 GB shared NPU/GPU
    }

    fn ram_available_bytes(&self) -> u64 {
        8 * 1024 * 1024 * 1024 // 8 GB RAM
    }

    fn supports_quantization(&self, quant: &str) -> bool {
        matches!(quant, "int8" | "q4_0")
    }
}
