use crate::webgpu::WebGpuDetector;
use hakille_substrate::{Compute, ComputeBackend};

pub struct WebComputeBackend {
    detector: WebGpuDetector,
}

impl WebComputeBackend {
    pub fn new() -> Self {
        Self {
            detector: WebGpuDetector::new(),
        }
    }

    pub fn detector(&self) -> &WebGpuDetector {
        &self.detector
    }
}

impl Default for WebComputeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Compute for WebComputeBackend {
    fn available_backends(&self) -> Vec<ComputeBackend> {
        let mut backends = vec![ComputeBackend::Cpu];
        if self.detector.capabilities().is_webgpu_supported {
            backends.insert(0, ComputeBackend::WebGpu);
        }
        backends
    }

    fn preferred_backend(&self) -> ComputeBackend {
        self.detector.optimal_compute_backend()
    }

    fn vram_available_bytes(&self) -> u64 {
        if self.detector.capabilities().is_webgpu_supported {
            self.detector.capabilities().limits.max_buffer_size
        } else {
            0
        }
    }

    fn ram_available_bytes(&self) -> u64 {
        // WASM 32-bit linear memory limit is typically 4 GB (or 2 GB standard)
        4 * 1024 * 1024 * 1024
    }

    fn supports_quantization(&self, quant: &str) -> bool {
        matches!(quant, "int8" | "q4_0" | "q4_k")
    }
}
