//! Sélection du substrate réel du démon, sans repli vers un autre OS.
//!
//! Les autres plateformes disposent de crates expérimentales, mais pas encore
//! d'un hôte de démon validé. Leur point d'entrée refuse explicitement le boot.

#[cfg(target_os = "linux")]
pub use hakille_substrate_linux::{
    LinuxComputeBackend as HostCompute, LinuxSubstrate as HostSubstrate,
};
#[cfg(target_os = "macos")]
pub use hakille_substrate_macos::{
    MacosComputeBackend as HostCompute, MacosSubstrate as HostSubstrate,
};
#[cfg(target_os = "windows")]
pub use hakille_substrate_windows::{
    WindowsComputeBackend as HostCompute, WindowsSubstrate as HostSubstrate,
};

/// La racine de l'autorité est épinglée avant toute opération à effet.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub fn create_host(root: biscuit_auth::PublicKey) -> (HostSubstrate, HostCompute) {
    (
        HostSubstrate::new().with_trusted_root(root),
        HostCompute::new(),
    )
}

#[cfg(all(
    test,
    any(target_os = "linux", target_os = "macos", target_os = "windows")
))]
mod tests {
    use super::*;
    use hakille_core::{
        CapabilityAction, CapabilityAuthority, CapabilityScope, HakilleError, Timestamp,
        VirtualPath,
    };
    use hakille_substrate::{Compute, ComputeBackend, Substrate};

    #[test]
    fn host_matches_compilation_target() {
        let authority = CapabilityAuthority::new();
        let (substrate, compute) = create_host(authority.root_public_key());
        #[cfg(target_os = "linux")]
        assert_eq!(substrate.manifest().os_name, "Linux");
        #[cfg(target_os = "macos")]
        assert_eq!(substrate.manifest().os_name, "macOS");
        #[cfg(target_os = "windows")]
        assert_eq!(substrate.manifest().os_name, "Windows");
        assert!(compute.available_backends().contains(&ComputeBackend::Cpu));
        assert!(
            compute
                .available_backends()
                .contains(&compute.preferred_backend())
        );
    }

    #[tokio::test]
    async fn host_rejects_a_foreign_capability_root() {
        let authority = CapabilityAuthority::new();
        let foreign = CapabilityAuthority::new();
        let (substrate, _) = create_host(authority.root_public_key());
        let cap = foreign
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Read],
                Timestamp(chrono::Utc::now() + chrono::Duration::minutes(1)),
            )
            .unwrap();
        let err = substrate
            .read_file(&VirtualPath("bootstrap-probe.txt".into()), &cap)
            .await
            .expect_err("racine étrangère refusée avant accès fichier");
        assert!(matches!(err, HakilleError::CapabilityDenied(_)), "{err}");
    }
}
