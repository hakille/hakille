use annora_substrate::{Compute, ComputeBackend};

pub struct WindowsComputeBackend {
    preferred: ComputeBackend,
}

impl WindowsComputeBackend {
    pub fn new() -> Self {
        Self {
            preferred: ComputeBackend::Cuda,
        }
    }
}

impl Default for WindowsComputeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Compute for WindowsComputeBackend {
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
        8 * 1024 * 1024 * 1024 // 8 GB simulated
    }

    fn ram_available_bytes(&self) -> u64 {
        32 * 1024 * 1024 * 1024 // 32 GB simulated
    }

    fn supports_quantization(&self, quant: &str) -> bool {
        matches!(quant, "int8" | "q4_0" | "q4_k" | "q8_0")
    }
}
