pub mod appcontainer;
pub mod compute_impl;
pub mod input_injector;
pub mod screen_capture;
pub mod service;
pub mod substrate_impl;
pub mod tray_and_hotkey;
pub mod win32_ffi;

pub use appcontainer::*;
pub use compute_impl::*;
pub use input_injector::*;
pub use screen_capture::*;
pub use service::*;
pub use substrate_impl::*;
pub use tray_and_hotkey::*;

#[cfg(test)]
mod tests {
    use super::*;
    use annora_core::{CapabilityScope, Tier, VirtualPath};
    use annora_substrate::{Compute, InputAction, MouseButton, Substrate};

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

        // Verify real physical RAM query
        let total_ram = compute.total_physical_ram();
        let avail_ram = compute.available_physical_ram();
        assert!(total_ram > 0, "Total RAM must be greater than 0");
        assert!(avail_ram > 0, "Available RAM must be greater than 0");
    }

    #[test]
    fn test_appcontainer_profile_creation() {
        let temp_dir = std::env::temp_dir().join("annora_ac_test");
        let mut profile = AppContainerSandbox::create_profile(
            "MailClient",
            vec![CapabilityScope::Network, CapabilityScope::Filesystem],
            temp_dir.to_str().unwrap(),
        ).expect("Profile creation failed");

        assert_eq!(profile.package_name, "Annora.AppContainer.MailClient");
        assert!(profile.allowed_capabilities.contains(&CapabilityScope::Network));
        assert!(profile.capability_sids.contains(&"S-1-15-3-1".to_string()));
        assert!(profile.is_active);

        AppContainerSandbox::teardown(&mut profile).unwrap();
        assert!(!profile.is_active);
    }

    #[tokio::test]
    async fn test_windows_real_screen_capture_and_input_injection() {
        let (w, h) = WindowsScreenCapturer::display_dimensions();
        assert!(w > 0 && h > 0, "Display dimensions must be positive integers");

        let capture = WindowsScreenCapturer::capture_primary_display();
        assert_eq!(capture.width, w as u32);
        assert_eq!(capture.height, h as u32);
        assert_eq!(capture.data.len(), (w * h * 4) as usize);

        // Test input injection
        let click = InputAction::Click {
            x: 500,
            y: 400,
            button: MouseButton::Left,
        };
        let res = WindowsInputInjector::inject(&click);
        assert!(res.is_ok(), "Input injection should succeed");
    }

    #[tokio::test]
    async fn test_windows_filesystem_isolation_and_power() {
        let temp_dir = std::env::temp_dir().join("annora_vfs_test");
        let _ = std::fs::remove_dir_all(&temp_dir);

        let substrate = WindowsSubstrate::new().with_virtual_fs_root(temp_dir.clone());
        let cap = annora_core::CapabilityAuthority::new()
            .create_root_token(vec![CapabilityScope::Filesystem], vec![annora_core::CapabilityAction::Write], annora_core::Timestamp::now())
            .unwrap();

        let vpath = VirtualPath("test_notes.txt".to_string());
        substrate.write_file(&vpath, b"Annora Windows Native Substrate", &cap).await.unwrap();

        let read_data = substrate.read_file(&vpath, &cap).await.unwrap();
        assert_eq!(read_data, b"Annora Windows Native Substrate");

        let list = substrate.list_dir(&VirtualPath("".to_string()), &cap).await.unwrap();
        assert!(list.iter().any(|p| p.0 == "test_notes.txt"));

        substrate.delete_file(&vpath, &cap).await.unwrap();

        // Check power state
        let power = substrate.power_state();
        assert!(power.battery_percent >= 0.0 && power.battery_percent <= 100.0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_windows_tray_and_hotkey() {
        let mut tray = WindowsPresenceTray::new();
        assert_eq!(tray.state(), WindowsPresenceState::Idle);

        tray.set_state(WindowsPresenceState::Acting);
        assert_eq!(tray.state(), WindowsPresenceState::Acting);
        assert!(tray.tooltip.contains("Acting"));

        let mut hotkey = WindowsGlobalHotkey::new();
        assert_eq!(hotkey.key_combination, "Alt+Space");
        assert!(hotkey.register());
        assert!(hotkey.is_registered);
        hotkey.unregister();
        assert!(!hotkey.is_registered);
    }
}
