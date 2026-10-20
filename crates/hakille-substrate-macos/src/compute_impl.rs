use crate::mach_iokit::{
    read_host_page_size_bytes, read_host_vm_statistics, read_total_physical_ram_bytes,
};
use hakille_substrate::{Compute, ComputeBackend};

pub struct MacosComputeBackend {
    preferred: ComputeBackend,
}

impl MacosComputeBackend {
    pub fn new() -> Self {
        Self {
            preferred: ComputeBackend::Metal,
        }
    }

    /// Total physical RAM: real `sysctl hw.memsize` under macOS; if the query
    /// fails it is 0 (= unknown), never an invented size. Off macOS the value
    /// comes from the explicitly simulated fallback profile.
    pub fn total_physical_ram(&self) -> u64 {
        read_total_physical_ram_bytes()
    }

    /// Available physical RAM: real `vm_stat(1)` counters scaled by the real
    /// `sysctl hw.pagesize` under macOS; 0 (= unknown) when either query
    /// fails. Off macOS: explicitly simulated fallback profile.
    pub fn available_physical_ram(&self) -> u64 {
        read_host_vm_statistics().available_physical_ram(read_host_page_size_bytes())
    }

    /// Wired RAM: real `vm_stat(1)` counter scaled by the real page size
    /// under macOS; 0 (= unknown) when the queries fail. Off macOS:
    /// explicitly simulated fallback profile.
    pub fn wired_ram(&self) -> u64 {
        read_host_vm_statistics().wired_ram(read_host_page_size_bytes())
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
        self.preferred
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
