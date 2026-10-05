use annora_substrate::{Compute, Substrate};
use annora_substrate_linux::{LinuxComputeBackend, LinuxSubstrate};
use annora_substrate_macos::{MacosComputeBackend, MacosSubstrate};
use annora_substrate_mobile::{MobileComputeBackend, MobileSubstrate};
use annora_substrate_redox::{RedoxComputeBackend, RedoxSubstrate};
use annora_substrate_web::{WebComputeBackend, WebSubstrate};
use annora_substrate_windows::{WindowsComputeBackend, WindowsSubstrate};

#[tokio::test]
async fn test_all_substrates_manifest_and_attestation() {
    let win = WindowsSubstrate::new();
    assert_eq!(win.manifest().tier, annora_core::Tier::Native);
    let att = win.attest(b"test").await.unwrap();
    assert_eq!(att.platform_id, "Windows-Substrate-Host");

    let lin = LinuxSubstrate::new();
    assert_eq!(lin.manifest().tier, annora_core::Tier::Native);
    let att = lin.attest(b"test").await.unwrap();
    assert_eq!(att.platform_id, "Linux-Substrate-Host");

    let mac = MacosSubstrate::new();
    assert_eq!(mac.manifest().tier, annora_core::Tier::Native);
    let att = mac.attest(b"test").await.unwrap();
    assert_eq!(att.platform_id, "macOS-AppleSilicon-Host");

    let redox = RedoxSubstrate::new();
    assert_eq!(redox.manifest().tier, annora_core::Tier::Sovereign);
    let att = redox.attest(b"test").await.unwrap();
    assert_eq!(att.platform_id, "Redox-Sovereign-Dedicated-Node");

    let mob = MobileSubstrate::new();
    assert_eq!(mob.manifest().tier, annora_core::Tier::Guest);

    let web = WebSubstrate::new();
    assert_eq!(web.manifest().tier, annora_core::Tier::Guest);
}

#[tokio::test]
async fn test_all_compute_backends() {
    let win_comp = WindowsComputeBackend::new();
    assert!(!win_comp.available_backends().is_empty());

    let lin_comp = LinuxComputeBackend::new();
    assert!(!lin_comp.available_backends().is_empty());

    let mac_comp = MacosComputeBackend::new();
    assert!(!mac_comp.available_backends().is_empty());

    let redox_comp = RedoxComputeBackend::new();
    assert!(!redox_comp.available_backends().is_empty());

    let mob_comp = MobileComputeBackend::new();
    assert!(!mob_comp.available_backends().is_empty());

    let web_comp = WebComputeBackend::new();
    assert!(!web_comp.available_backends().is_empty());
}
