pub mod appcontainer;
pub mod compute_impl;
pub mod service;
pub mod substrate_impl;

pub use appcontainer::*;
pub use compute_impl::*;
pub use service::*;
pub use substrate_impl::*;

#[cfg(test)]
mod tests {
    use super::*;
    use annora_core::{CapabilityScope, Tier};
    use annora_substrate::{Compute, Substrate};

    #[tokio::test]
    async fn test_windows_substrate_manifest_and_attestation() {
        let substrate = WindowsSubstrate::new();
        let manifest = substrate.manifest();
        assert_eq!(manifest.tier, Tier::Native);
        assert_eq!(manifest.os_name, "Windows");
        assert!(manifest.has_tpm);
        assert!(manifest.capabilities_granted.contains(&CapabilityScope::Filesystem));
        assert!(manifest.capabilities_denied.contains(&CapabilityScope::FullSystem));

        let attestation = substrate.attest(b"test-nonce").await.expect("Attestation failed");
        assert_eq!(attestation.platform_id, "Windows-Substrate-Host");
        assert!(!attestation.signature.0.is_empty());
    }

    #[test]
    fn test_windows_compute_backends_and_quantization() {
        let compute = WindowsComputeBackend::new();
        let backends = compute.available_backends();
        assert!(backends.contains(&annora_substrate::ComputeBackend::Cpu));
        assert!(backends.contains(&annora_substrate::ComputeBackend::Cuda));
        assert!(compute.supports_quantization("q4_k"));
        assert!(compute.supports_quantization("int8"));
        assert!(!compute.supports_quantization("unsupported_quant"));
    }

    #[test]
    fn test_appcontainer_profile_creation() {
        let profile = AppContainerSandbox::create_profile(
            "MailClient",
            vec![CapabilityScope::Network, CapabilityScope::Filesystem],
            "C:\\ProgramData\\Annora\\Containers\\MailClient",
        ).expect("Profile creation failed");

        assert_eq!(profile.package_name, "Annora.AppContainer.MailClient");
        assert!(profile.allowed_capabilities.contains(&CapabilityScope::Network));
    }
}

