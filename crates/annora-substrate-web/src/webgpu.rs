use annora_substrate::ComputeBackend;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebGpuLimits {
    pub max_buffer_size: u64,
    pub max_compute_workgroup_storage_size: u32,
    pub max_storage_buffer_binding_size: u64,
    pub max_compute_invocations_per_workgroup: u32,
}

impl Default for WebGpuLimits {
    fn default() -> Self {
        Self {
            max_buffer_size: 2 * 1024 * 1024 * 1024,          // 2 GB max buffer
            max_compute_workgroup_storage_size: 32 * 1024,      // 32 KiB workgroup shared memory
            max_storage_buffer_binding_size: 2 * 1024 * 1024 * 1024,
            max_compute_invocations_per_workgroup: 256,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebGpuCapabilities {
    pub is_webgpu_supported: bool,
    pub adapter_name: String,
    pub has_shader_f16: bool,
    pub has_subgroups: bool,
    pub has_timestamp_query: bool,
    pub limits: WebGpuLimits,
}

impl Default for WebGpuCapabilities {
    fn default() -> Self {
        Self {
            is_webgpu_supported: true,
            adapter_name: "WebGPU Sovereign Adapter (Chrome / Edge / Safari)".to_string(),
            has_shader_f16: true,
            has_subgroups: true,
            has_timestamp_query: false,
            limits: WebGpuLimits::default(),
        }
    }
}

pub struct WebGpuDetector {
    capabilities: WebGpuCapabilities,
}

impl WebGpuDetector {
    pub fn new() -> Self {
        Self {
            capabilities: WebGpuCapabilities::default(),
        }
    }

    pub fn with_capabilities(capabilities: WebGpuCapabilities) -> Self {
        Self { capabilities }
    }

    pub fn capabilities(&self) -> &WebGpuCapabilities {
        &self.capabilities
    }

    pub fn optimal_compute_backend(&self) -> ComputeBackend {
        if self.capabilities.is_webgpu_supported {
            ComputeBackend::WebGpu
        } else {
            ComputeBackend::Cpu // WASM SIMD fallback
        }
    }

    pub fn can_run_half_precision_gemm(&self) -> bool {
        self.capabilities.is_webgpu_supported && self.capabilities.has_shader_f16
    }

    pub fn validate_tensor_allocation(&self, size_bytes: u64) -> Result<(), String> {
        if !self.capabilities.is_webgpu_supported {
            return Err("WebGPU is not supported on this platform".to_string());
        }
        if size_bytes > self.capabilities.limits.max_buffer_size {
            return Err(format!(
                "Requested tensor buffer size ({} bytes) exceeds WebGPU max_buffer_size ({} bytes)",
                size_bytes, self.capabilities.limits.max_buffer_size
            ));
        }
        if size_bytes > self.capabilities.limits.max_storage_buffer_binding_size {
            return Err(format!(
                "Requested tensor buffer size ({} bytes) exceeds max_storage_buffer_binding_size ({} bytes)",
                size_bytes, self.capabilities.limits.max_storage_buffer_binding_size
            ));
        }
        Ok(())
    }

    pub fn validate_workgroup_dispatch(&self, invocations: u32, shared_mem_bytes: u32) -> Result<(), String> {
        if invocations > self.capabilities.limits.max_compute_invocations_per_workgroup {
            return Err(format!(
                "Compute workgroup invocations ({}) exceed limit ({})",
                invocations, self.capabilities.limits.max_compute_invocations_per_workgroup
            ));
        }
        if shared_mem_bytes > self.capabilities.limits.max_compute_workgroup_storage_size {
            return Err(format!(
                "Workgroup shared memory ({} bytes) exceeds limit ({} bytes)",
                shared_mem_bytes, self.capabilities.limits.max_compute_workgroup_storage_size
            ));
        }
        Ok(())
    }
}

impl Default for WebGpuDetector {
    fn default() -> Self {
        Self::new()
    }
}
