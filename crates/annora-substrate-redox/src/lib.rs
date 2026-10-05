pub mod compute_impl;
pub mod scheme;
pub mod substrate_impl;

pub use compute_impl::*;
pub use scheme::*;
pub use substrate_impl::*;

#[cfg(test)]
mod tests {
    use super::*;
    use annora_core::{CapabilityScope, Tier};
    use annora_substrate::Substrate;

    #[tokio::test]
    async fn test_redox_substrate_manifest_and_attestation() {
        let mut substrate = RedoxSubstrate::new();
        let manifest = substrate.manifest();
        assert_eq!(manifest.tier, Tier::Sovereign);
        assert_eq!(manifest.os_name, "Redox OS");
        assert!(manifest.capabilities_granted.contains(&CapabilityScope::FullSystem));

        assert!(!substrate.is_killswitch_active());
        substrate.set_killswitch(true);
        assert!(substrate.is_killswitch_active());

        let attestation = substrate.attest(b"redox-nonce").await.expect("Attestation failed");
        assert_eq!(attestation.platform_id, "Redox-Sovereign-Dedicated-Node");
        assert!(!attestation.signature.0.is_empty());
    }

    #[test]
    fn test_redox_native_schemes() {
        let handler = RedoxSchemeHandler::new();
        assert!(handler.is_scheme_supported("annora:"));
        assert!(handler.is_scheme_supported("memory:"));
        assert!(handler.is_scheme_supported("agent:"));
        assert!(!handler.is_scheme_supported("untrusted:"));
    }
}

