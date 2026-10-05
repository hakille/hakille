use annora_substrate::{Compute, ComputeBackend};

pub struct MacosComputeBackend {
    preferred: ComputeBackend,
}

impl MacosComputeBackend {
    pub fn new() -> Self {
        Self {
            preferred: ComputeBackend::Metal,
        }
    }
}

impl Default for MacosComputeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Compute for MacosComputeBackend {
    fn available_backends(&self) -> Vec<ComputeBackend> {
        vec![
            ComputeBackend::Metal,
            ComputeBackend::Cpu,
        ]
    }

    fn preferred_backend(&self) -> ComputeBackend {
        self.preferred.clone()
    }

    fn vram_available_bytes(&self) -> u64 {
        24 * 1024 * 1024 * 1024 // Unified memory 24 GB
    }

    fn ram_available_bytes(&self) -> u64 {
        36 * 1024 * 1024 * 1024 // Unified memory 36 GB
    }

    fn supports_quantization(&self, quant: &str) -> bool {
        matches!(quant, "int8" | "q4_0" | "q4_k" | "q8_0")
    }
}
