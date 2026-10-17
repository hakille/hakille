use crate::win32_ffi::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use annora_substrate::{Compute, ComputeBackend};

pub struct WindowsComputeBackend {
    preferred: ComputeBackend,
}

impl WindowsComputeBackend {
    pub fn new() -> Self {
        // Detect GPU or default to best available compute accelerator
        let has_cuda = std::env::var("CUDA_PATH").is_ok()
            || std::path::Path::new("C:\\Program Files\\NVIDIA GPU Computing Toolkit\\CUDA")
                .exists();

        let preferred = if has_cuda {
            ComputeBackend::Cuda
        } else {
            ComputeBackend::Vulkan
        };

        Self { preferred }
    }

    /// Query actual physical RAM installed on this Windows machine in bytes.
    pub fn total_physical_ram(&self) -> u64 {
        let mut mem = MEMORYSTATUSEX::default();
        unsafe {
            if GlobalMemoryStatusEx(&mut mem) != 0 {
                mem.ullTotalPhys
            } else {
                16 * 1024 * 1024 * 1024
            }
        }
    }

    /// Query actual available physical RAM on this Windows machine in bytes.
    pub fn available_physical_ram(&self) -> u64 {
        let mut mem = MEMORYSTATUSEX::default();
        unsafe {
            if GlobalMemoryStatusEx(&mut mem) != 0 {
                mem.ullAvailPhys
            } else {
                8 * 1024 * 1024 * 1024
            }
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
        // Honest: dedicated VRAM is unknown without a DXGI adapter query
        // (no DXGI binding linked); 0 means "unknown", never an 8 GiB claim.
        0
    }

    fn ram_available_bytes(&self) -> u64 {
        self.available_physical_ram()
    }

    fn supports_quantization(&self, quant: &str) -> bool {
        matches!(
            quant,
            "int8" | "q4_0" | "q4_k" | "q4_k_m" | "q5_k_m" | "q8_0" | "fp16"
        )
    }
}
