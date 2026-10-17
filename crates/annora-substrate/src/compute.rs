use crate::attestation::ComputeBackend;

/// Backend de calcul abstrait d'un substrate.
///
/// Les implémentations exposent les backends disponibles et celui
/// préféré pour l'inférence locale.
pub trait Compute: Send + Sync + 'static {
    fn available_backends(&self) -> Vec<ComputeBackend>;
    fn preferred_backend(&self) -> ComputeBackend;
    fn vram_available_bytes(&self) -> u64;
    fn ram_available_bytes(&self) -> u64;
    fn supports_quantization(&self, quant: &str) -> bool;

    /// Vérifie qu'au moins `required_bytes` de RAM sont disponibles.
    fn has_enough_ram(&self, required_bytes: u64) -> bool {
        self.ram_available_bytes() >= required_bytes
    }
}
