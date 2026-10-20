use crate::telemetry::{detect_linux_vram_bytes, read_host_meminfo};
use hakille_substrate::{Compute, ComputeBackend};

/// Whether a real NVIDIA driver interface is exposed by this host
/// (`/dev/nvidiactl` or `/proc/driver/nvidia/version`). Off Linux this is
/// always `false` — a CUDA backend is never claimed without the device.
pub fn detect_cuda_runtime() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new("/dev/nvidiactl").exists()
            || std::path::Path::new("/proc/driver/nvidia/version").exists()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Whether a DRM render node (`/dev/dri/renderD*`) exists, the prerequisite
/// for a Vulkan device. Off Linux this is always `false`.
pub fn detect_vulkan_runtime() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_dir("/dev/dri")
            .map(|entries| {
                entries
                    .flatten()
                    .any(|entry| entry.file_name().to_string_lossy().starts_with("renderD"))
            })
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

pub struct LinuxComputeBackend {
    preferred: ComputeBackend,
}

impl LinuxComputeBackend {
    pub fn new() -> Self {
        let preferred = if detect_cuda_runtime() {
            ComputeBackend::Cuda
        } else if detect_vulkan_runtime() {
            ComputeBackend::Vulkan
        } else {
            ComputeBackend::Cpu
        };
        Self { preferred }
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
        let mut backends = Vec::new();
        if detect_cuda_runtime() {
            backends.push(ComputeBackend::Cuda);
        }
        if detect_vulkan_runtime() {
            backends.push(ComputeBackend::Vulkan);
        }
        backends.push(ComputeBackend::Cpu);
        backends
    }

    fn preferred_backend(&self) -> ComputeBackend {
        self.preferred
    }

    fn vram_available_bytes(&self) -> u64 {
        // Honest: discrete VRAM summed from sysfs DRM when discoverable,
        // else 0 (unknown — shared-memory iGPUs and headless VMs report
        // nothing; never an invented envelope).
        detect_linux_vram_bytes().unwrap_or(0)
    }

    fn ram_available_bytes(&self) -> u64 {
        self.available_physical_ram()
    }

    fn supports_quantization(&self, quant: &str) -> bool {
        matches!(quant, "int8" | "q4_0" | "q4_k" | "q8_0")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backends_always_include_cpu_and_never_deny_cpu() {
        let compute = LinuxComputeBackend::new();
        let backends = compute.available_backends();
        assert!(backends.contains(&ComputeBackend::Cpu));
        assert_eq!(
            backends.contains(&ComputeBackend::Cuda),
            detect_cuda_runtime(),
            "Cuda must mirror the real driver probe, never a default claim"
        );
        assert_eq!(
            backends.contains(&ComputeBackend::Vulkan),
            detect_vulkan_runtime(),
            "Vulkan must mirror the real render-node probe"
        );
        assert_eq!(compute.preferred_backend(), backends[0]);
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn backends_off_linux_are_cpu_only() {
        let compute = LinuxComputeBackend::new();
        assert_eq!(compute.available_backends(), vec![ComputeBackend::Cpu]);
        assert_eq!(compute.preferred_backend(), ComputeBackend::Cpu);
        assert!(!detect_cuda_runtime());
        assert!(!detect_vulkan_runtime());
        assert_eq!(compute.vram_available_bytes(), 0);
    }
}
