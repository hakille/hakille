use crate::attestation::ComputeBackend;

pub trait Compute: Send + Sync + 'static {
    fn available_backends(&self) -> Vec<ComputeBackend>;
    fn preferred_backend(&self) -> ComputeBackend;
    fn vram_available_bytes(&self) -> u64;
    fn ram_available_bytes(&self) -> u64;
    fn supports_quantization(&self, quant: &str) -> bool;
}
