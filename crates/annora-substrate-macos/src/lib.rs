pub mod compute_impl;
pub mod endpoint_security;
pub mod sandbox;
pub mod substrate_impl;

pub use compute_impl::*;
pub use endpoint_security::*;
pub use sandbox::*;
pub use substrate_impl::*;

#[cfg(test)]
mod tests {
    use super::*;
    use annora_core::{CapabilityScope, Tier};
    use annora_substrate::{Compute, Substrate};

    #[tokio::test]
    async fn test_macos_substrate_manifest_and_attestation() {
        let substrate = MacosSubstrate::new();
        let manifest = substrate.manifest();
        assert_eq!(manifest.tier, Tier::Native);
        assert_eq!(manifest.os_name, "macOS");
        assert!(manifest.has_secure_enclave);
        assert!(manifest.capabilities_granted.contains(&CapabilityScope::Filesystem));
        assert!(manifest.capabilities_denied.contains(&CapabilityScope::FullSystem));

        let attestation = substrate.attest(b"macos-nonce").await.expect("Attestation failed");
        assert_eq!(attestation.platform_id, "macOS-AppleSilicon-Host");
        assert!(!attestation.signature.0.is_empty());
    }

    #[test]
    fn test_macos_compute_backends_and_quantization() {
        let compute = MacosComputeBackend::new();
        let backends = compute.available_backends();
        assert!(backends.contains(&annora_substrate::ComputeBackend::Cpu));
        assert!(backends.contains(&annora_substrate::ComputeBackend::Metal));
        assert!(compute.supports_quantization("q4_k"));
        assert!(compute.supports_quantization("int8"));
        assert!(!compute.supports_quantization("unsupported_quant"));
    }

    #[test]
    fn test_macos_sandbox_profile_defaults() {
        let profile = MacosSandboxProfile::default();
        assert!(profile.allow_file_read);
        assert!(!profile.allow_file_write);
        assert!(!profile.allow_network_outbound);
        assert!(profile.profile_string.contains("(deny default)"));
    }
}

