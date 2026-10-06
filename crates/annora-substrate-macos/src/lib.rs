pub mod compute_impl;
pub mod display;
pub mod endpoint_security;
pub mod mach_iokit;
pub mod sandbox;
pub mod substrate_impl;

pub use compute_impl::*;
pub use display::*;
pub use endpoint_security::*;
pub use mach_iokit::*;
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
        assert!(compute.supports_quantization("fp16"));
        assert!(!compute.supports_quantization("unsupported_quant"));

        // Verify Apple Silicon memory calculations
        let total_ram = compute.total_physical_ram();
        let avail_ram = compute.available_physical_ram();
        let wired_ram = compute.wired_ram();
        let vram = compute.vram_available_bytes();

        assert!(total_ram > 0);
        assert!(avail_ram > 0);
        assert!(wired_ram > 0);
        assert!(vram > 0 && vram <= total_ram);
    }

    #[test]
    fn test_macos_seatbelt_profile_definitions() {
        let mut mail_profile = MacosSandboxProfile::agent_mail("/Users/alice");
        assert!(mail_profile.allow_file_read);
        assert!(mail_profile.allow_file_write);
        assert!(mail_profile.allow_network_outbound);
        assert!(mail_profile.profile_string.contains("(deny default)"));
        assert!(mail_profile.profile_string.contains("/Users/alice/Library/Annora/Mail"));
        assert!(mail_profile.validate_safety().is_ok());
        assert!(mail_profile.apply().is_ok());

        // Verify flag reset upon recompilation
        mail_profile.rules.clear();
        mail_profile.compile();
        assert!(!mail_profile.allow_file_read);
        assert!(!mail_profile.allow_file_write);
        assert!(!mail_profile.allow_network_outbound);

        let code_profile = MacosSandboxProfile::agent_code_sandbox("/tmp/annora-code-123");
        assert!(code_profile.allow_file_read);
        assert!(code_profile.allow_file_write);
        assert!(!code_profile.allow_network_outbound);
        assert!(code_profile.profile_string.contains("(deny network*)"));

        let wallet_profile = MacosSandboxProfile::agent_wallet_readonly("/var/db/annora/wallet");
        assert!(wallet_profile.allow_file_read);
        assert!(!wallet_profile.allow_file_write);
        assert!(!wallet_profile.allow_network_outbound);

        let inference_profile = MacosSandboxProfile::inference_engine_profile("/opt/annora/models");
        assert!(inference_profile.profile_string.contains("com.apple.metal.device"));
    }

    #[test]
    fn test_macos_endpoint_security_abstractions() {
        let mut client = EndpointSecurityClient::default();
        assert!(client.connect().is_ok());
        assert!(client.is_connected());

        // Verify mute paths
        assert!(client.is_path_muted("/System/Library/CoreServices/loginwindow"));
        assert!(!client.is_path_muted("/Users/alice/malicious/binary"));

        // Verify process muting (by PID)
        client.add_mute_pid(4096);
        assert!(client.is_pid_muted(4096));

        let muted_proc = EsProcess {
            pid: 4096,
            ppid: 1,
            signing_id: "com.annora.helper".to_string(),
            team_id: None,
            cdhash: "abcdef0123456789".to_string(),
        };
        let muted_exec = EsEvent::AuthExec {
            target_path: "/tmp/custom_script.sh".to_string(),
            process: muted_proc,
            args: vec![],
        };
        assert_eq!(client.handle_event(muted_exec), Some(EsAuthResult::Allow));

        // Verify auth evaluation for safe and unsafe execution
        let proc = EsProcess {
            pid: 1024,
            ppid: 1,
            signing_id: "com.apple.sh".to_string(),
            team_id: None,
            cdhash: "0123456789abcdef".to_string(),
        };

        let safe_exec = EsEvent::AuthExec {
            target_path: "/System/Library/Frameworks/Security.framework".to_string(),
            process: proc.clone(),
            args: vec![],
        };
        assert_eq!(client.handle_event(safe_exec), Some(EsAuthResult::Allow));

        let unsafe_exec = EsEvent::AuthExec {
            target_path: "/tmp/malicious/payload.trojan".to_string(),
            process: proc.clone(),
            args: vec![],
        };
        assert_eq!(client.handle_event(unsafe_exec), Some(EsAuthResult::Deny));

        let unsafe_open = EsEvent::AuthOpen {
            file: EsFile {
                path: "/etc/master.passwd".to_string(),
                size_bytes: 4096,
            },
            flags: 0,
            process: proc,
        };
        assert_eq!(client.handle_event(unsafe_open), Some(EsAuthResult::Deny));
    }

    #[test]
    fn test_macos_mach_and_iokit_telemetry() {
        let vm = VmStatistics64::default();
        let total = vm.total_physical_ram(PAGE_SIZE_APPLE_SILICON);
        let avail = vm.available_physical_ram(PAGE_SIZE_APPLE_SILICON);
        assert!(total >= avail);

        let mut dict = std::collections::HashMap::new();
        dict.insert("Power Source State".to_string(), "Battery Power".to_string());
        dict.insert("Is Charging".to_string(), "false".to_string());
        dict.insert("Current Capacity".to_string(), "75".to_string());
        dict.insert("Max Capacity".to_string(), "100".to_string());
        dict.insert("Time to Empty".to_string(), "240".to_string());

        let parsed = parse_iops_dictionary(&dict);
        assert_eq!(parsed.state, "Battery Power");
        assert!(!parsed.is_charging);
        assert_eq!(parsed.current_capacity, 75);
        assert_eq!(parsed.time_to_empty_min, Some(240));

        let thermal = OSThermalPressureLevel::Heavy.to_thermal_state();
        assert!(thermal.throttled);
        assert!(thermal.temperature_celsius > 80.0);
    }

    #[tokio::test]
    async fn test_macos_quartz_display_capture() {
        let substrate = MacosSubstrate::new();
        let cap = annora_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::GuiInput],
                vec![annora_core::CapabilityAction::Read],
                annora_core::Timestamp::now(),
            )
            .unwrap();

        let screen = substrate.capture_screen(&cap).await.unwrap();
        assert_eq!(screen.width, 2560);
        assert_eq!(screen.height, 1600);
        assert_eq!(screen.data.len(), 2560 * 1600 * 4);

        let quartz = MacosQuartzDisplayCapture::new();
        assert_eq!(quartz.physical_width(), 5120);
        assert_eq!(quartz.physical_height(), 3200);
    }
}
