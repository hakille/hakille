pub mod compute_impl;
pub mod shadow;
pub mod substrate_impl;

pub use compute_impl::*;
pub use shadow::*;
pub use substrate_impl::*;

#[cfg(test)]
mod tests {
    use super::*;
    use annora_core::{CapabilityScope, Tier};
    use annora_substrate::{Compute, Substrate};

    #[tokio::test]
    async fn test_mobile_substrate_manifest_and_attestation() {
        let substrate = MobileSubstrate::new();
        let manifest = substrate.manifest();
        assert_eq!(manifest.tier, Tier::Guest);
        assert!(manifest.battery_powered);
        assert_eq!(manifest.thermal_envelope_watts, Some(5.0));
        assert!(manifest.capabilities_granted.contains(&CapabilityScope::Audio));
        assert!(manifest.capabilities_denied.contains(&CapabilityScope::Filesystem));

        let attestation = substrate.attest(b"mobile-nonce").await.expect("Attestation failed");
        assert_eq!(attestation.platform_id, "Mobile-Guest-Device");
        assert!(!attestation.signature.0.is_empty());
    }

    #[test]
    fn test_mobile_compute_backends_and_quantization() {
        let compute = MobileComputeBackend::new();
        let backends = compute.available_backends();
        assert!(backends.contains(&annora_substrate::ComputeBackend::Npu));
        assert!(backends.contains(&annora_substrate::ComputeBackend::Cpu));
        assert!(compute.supports_quantization("int8"));
        assert!(compute.supports_quantization("q4_0"));
        assert!(!compute.supports_quantization("q4_k"));
    }

    #[test]
    fn test_mobile_shadow_sync_lifecycle() {
        let mut shadow = MobileShadowSync::new("https://cloud-shadow.annora.internal");
        assert!(!shadow.status().is_connected);
        let count = shadow.wake_and_resync();
        assert_eq!(count, 0);
        assert!(shadow.status().is_connected);
    }
}

