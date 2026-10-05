pub mod compute_impl;
pub mod substrate_impl;
pub mod web_audio;

pub use compute_impl::*;
pub use substrate_impl::*;
pub use web_audio::*;

#[cfg(test)]
mod tests {
    use super::*;
    use annora_core::{CapabilityScope, Tier};
    use annora_substrate::{Compute, Substrate};

    #[tokio::test]
    async fn test_web_substrate_manifest_and_attestation() {
        let substrate = WebSubstrate::new();
        let manifest = substrate.manifest();
        assert_eq!(manifest.tier, Tier::Guest);
        assert_eq!(manifest.max_memory_bytes, 2 * 1024 * 1024 * 1024);
        assert!(manifest.capabilities_granted.contains(&CapabilityScope::Audio));
        assert!(manifest.capabilities_denied.contains(&CapabilityScope::Filesystem));

        let attestation = substrate.attest(b"web-nonce").await.expect("Attestation failed");
        assert_eq!(attestation.platform_id, "Browser-Wasm-Guest");
        assert!(!attestation.signature.0.is_empty());
    }

    #[test]
    fn test_web_compute_backends_and_quantization() {
        let compute = WebComputeBackend::new();
        let backends = compute.available_backends();
        assert!(backends.contains(&annora_substrate::ComputeBackend::WebGpu));
        assert!(backends.contains(&annora_substrate::ComputeBackend::Cpu));
        assert!(compute.supports_quantization("int8"));
        assert!(!compute.supports_quantization("q4_k"));
    }

    #[test]
    fn test_web_audio_bridge() {
        let mut bridge = WebAudioBridge::default();
        assert!(!bridge.config().is_muted);
        assert_eq!(bridge.config().sample_rate, 48000);

        bridge.set_muted(true);
        assert!(bridge.config().is_muted);
    }
}

