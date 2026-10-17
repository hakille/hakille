use crate::mach_iokit::{PAGE_SIZE_APPLE_SILICON, read_host_vm_statistics};
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

    pub fn total_physical_ram(&self) -> u64 {
        read_host_vm_statistics().total_physical_ram(PAGE_SIZE_APPLE_SILICON)
    }

    pub fn available_physical_ram(&self) -> u64 {
        read_host_vm_statistics().available_physical_ram(PAGE_SIZE_APPLE_SILICON)
    }

    pub fn wired_ram(&self) -> u64 {
        read_host_vm_statistics().wired_ram(PAGE_SIZE_APPLE_SILICON)
    }
}

impl Default for MacosComputeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Compute for MacosComputeBackend {
    fn available_backends(&self) -> Vec<ComputeBackend> {
        vec![ComputeBackend::Metal, ComputeBackend::Cpu]
    }

    fn preferred_backend(&self) -> ComputeBackend {
        self.preferred.clone()
    }

    fn vram_available_bytes(&self) -> u64 {
        // Honest: on Apple Silicon the shareable unified-memory fraction is
        // dynamic and only knowable via a Metal / IORegistry query (not
        // linked here). 0 means "unknown", never a hard-coded 3/4 share.
        0
    }

    fn ram_available_bytes(&self) -> u64 {
        self.available_physical_ram()
    }

    fn supports_quantization(&self, quant: &str) -> bool {
        matches!(quant, "int8" | "q4_0" | "q4_k" | "q8_0" | "fp16")
    }
}
