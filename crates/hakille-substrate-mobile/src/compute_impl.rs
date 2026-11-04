//! Capacités de calcul observées, sans supposer la présence d'un NPU mobile.

use hakille_core::{HakilleError, Result};
use hakille_substrate::{Compute, ComputeBackend};
use std::sync::Arc;

/// Le CPU hôte existe, mais aucun moteur quantifié/NPU n'est raccordé par défaut.
/// Un adaptateur de calcul réellement initialisé peut être fourni par l'hôte.
#[derive(Default)]
pub struct MobileComputeBackend {
    driver: Option<Arc<dyn Compute>>,
}

impl MobileComputeBackend {
    pub fn new() -> Self {
        Self::default()
    }

    /// Le pilote doit découvrir ses accélérateurs et formats effectivement
    /// exécutables, ainsi que la mémoire disponible. Un simple nom de SoC ou
    /// une taille de RAM commerciale ne constitue pas une découverte.
    pub fn with_driver(driver: Arc<dyn Compute>) -> Result<Self> {
        let backends = driver.available_backends();
        if backends.is_empty() || !backends.contains(&driver.preferred_backend()) {
            return Err(HakilleError::SubstrateError(
                "pilote de calcul mobile incohérent : backend préféré absent des backends disponibles".into(),
            ));
        }
        Ok(Self {
            driver: Some(driver),
        })
    }

    pub fn has_configured_driver(&self) -> bool {
        self.driver.is_some()
    }
}

impl Compute for MobileComputeBackend {
    fn available_backends(&self) -> Vec<ComputeBackend> {
        self.driver
            .as_ref()
            .map_or_else(|| vec![ComputeBackend::Cpu], |d| d.available_backends())
    }

    fn preferred_backend(&self) -> ComputeBackend {
        self.driver
            .as_ref()
            .map_or(ComputeBackend::Cpu, |d| d.preferred_backend())
    }

    /// Zéro signifie inconnu lorsque le pilote natif est absent.
    fn vram_available_bytes(&self) -> u64 {
        self.driver.as_ref().map_or(0, |d| d.vram_available_bytes())
    }

    /// Zéro signifie inconnu, jamais un forfait inventé de 8 Go.
    fn ram_available_bytes(&self) -> u64 {
        self.driver.as_ref().map_or(0, |d| d.ram_available_bytes())
    }

    fn supports_quantization(&self, quant: &str) -> bool {
        self.driver
            .as_ref()
            .is_some_and(|d| d.supports_quantization(quant))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DriverFixture {
        coherent: bool,
    }
    impl Compute for DriverFixture {
        fn available_backends(&self) -> Vec<ComputeBackend> {
            vec![ComputeBackend::Cpu]
        }
        fn preferred_backend(&self) -> ComputeBackend {
            if self.coherent {
                ComputeBackend::Cpu
            } else {
                ComputeBackend::Npu
            }
        }
        fn vram_available_bytes(&self) -> u64 {
            0
        }
        fn ram_available_bytes(&self) -> u64 {
            123_456
        }
        fn supports_quantization(&self, quant: &str) -> bool {
            quant == "fixture-format"
        }
    }

    #[test]
    fn configured_compute_delegates_observations_without_adding_an_npu() {
        let backend =
            MobileComputeBackend::with_driver(Arc::new(DriverFixture { coherent: true })).unwrap();
        assert!(backend.has_configured_driver());
        assert_eq!(backend.available_backends(), vec![ComputeBackend::Cpu]);
        assert_eq!(backend.preferred_backend(), ComputeBackend::Cpu);
        assert_eq!(backend.ram_available_bytes(), 123_456);
        assert!(backend.supports_quantization("fixture-format"));
        assert!(!backend.supports_quantization("int8"));
    }

    #[test]
    fn unreported_preferred_accelerator_is_rejected() {
        assert!(
            MobileComputeBackend::with_driver(Arc::new(DriverFixture { coherent: false })).is_err()
        );
    }
}
