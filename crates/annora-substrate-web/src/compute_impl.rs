use annora_substrate::{Compute, ComputeBackend};

pub struct WebComputeBackend;

impl WebComputeBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebComputeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Compute for WebComputeBackend {
    fn available_backends(&self) -> Vec<ComputeBackend> {
        vec![ComputeBackend::WebGpu, ComputeBackend::Cpu]
    }

    fn preferred_backend(&self) -> ComputeBackend {
        ComputeBackend::WebGpu
    }

    fn vram_available_bytes(&self) -> u64 {
        1024 * 1024 * 1024 // 1 GB WebGPU limit
    }

    fn ram_available_bytes(&self) -> u64 {
        2 * 1024 * 1024 * 1024 // 2 GB browser heap limit
    }

    fn supports_quantization(&self, quant: &str) -> bool {
        matches!(quant, "int8" | "q4_0")
    }
}
