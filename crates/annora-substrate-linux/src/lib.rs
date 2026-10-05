pub mod compute_impl;
pub mod daemon;
pub mod input;
pub mod seccomp;
pub mod substrate_impl;

pub use compute_impl::*;
pub use daemon::*;
pub use input::*;
pub use seccomp::*;
pub use substrate_impl::*;

#[cfg(test)]
mod tests {
    use super::*;
    use annora_core::{CapabilityScope, Tier};
    use annora_substrate::{Compute, Substrate};

    #[tokio::test]
    async fn test_linux_substrate_manifest_and_attestation() {
        let substrate = LinuxSubstrate::new();
        let manifest = substrate.manifest();
        assert_eq!(manifest.tier, Tier::Native);
        assert_eq!(manifest.os_name, "Linux");
        assert!(manifest.has_tpm);
        assert!(manifest.capabilities_granted.contains(&CapabilityScope::Filesystem));
        assert!(manifest.capabilities_denied.contains(&CapabilityScope::FullSystem));

        let attestation = substrate.attest(b"linux-nonce").await.expect("Attestation failed");
        assert_eq!(attestation.platform_id, "Linux-Substrate-Host");
        assert!(!attestation.signature.0.is_empty());
    }

    #[test]
    fn test_linux_compute_backends_and_quantization() {
        let compute = LinuxComputeBackend::new();
        let backends = compute.available_backends();
        assert!(backends.contains(&annora_substrate::ComputeBackend::Cpu));
        assert!(backends.contains(&annora_substrate::ComputeBackend::Cuda));
        assert!(compute.supports_quantization("q4_k"));
        assert!(compute.supports_quantization("int8"));
        assert!(!compute.supports_quantization("unsupported_quant"));
    }

    #[test]
    fn test_seccomp_profile_defaults() {
        let profile = SeccompProfile::default();
        assert_eq!(profile.default_action, "errno");
        assert!(profile.allowed_syscalls.contains(&"read".to_string()));
        assert!(profile.allowed_syscalls.contains(&"write".to_string()));
        assert!(profile.isolated_namespaces.contains(&"pid".to_string()));
    }
}

