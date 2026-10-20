pub mod compute_impl;
pub mod display;
pub mod endpoint_security;
pub mod host_query;
pub mod mach_iokit;
pub mod sandbox;
pub mod substrate_impl;

pub use compute_impl::*;
pub use display::*;
pub use endpoint_security::*;
pub use host_query::*;
pub use mach_iokit::*;
pub use sandbox::*;
pub use substrate_impl::*;

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_core::{CapabilityScope, Tier};
    use hakille_substrate::{Compute, Substrate};

    #[tokio::test]
    async fn test_macos_substrate_manifest_and_attestation() {
        let substrate = MacosSubstrate::new();
        let manifest = substrate.manifest();
        assert_eq!(manifest.tier, Tier::Native);
        assert_eq!(manifest.os_name, "macOS");
        assert!(manifest.has_secure_enclave);
        assert!(
            manifest
                .capabilities_granted
                .contains(&CapabilityScope::Filesystem)
        );
        assert!(
            manifest
                .capabilities_denied
                .contains(&CapabilityScope::FullSystem)
        );

        let attestation = substrate
            .attest(b"macos-nonce")
            .await
            .expect("Attestation failed");
        assert_eq!(attestation.platform_id, "macOS-AppleSilicon-Host");
        assert!(!attestation.signature.0.is_empty());
    }

    #[test]
    fn test_macos_compute_backends_and_quantization() {
        let compute = MacosComputeBackend::new();
        let backends = compute.available_backends();
        assert!(backends.contains(&hakille_substrate::ComputeBackend::Cpu));
        assert!(backends.contains(&hakille_substrate::ComputeBackend::Metal));
        assert!(compute.supports_quantization("q4_k"));
        assert!(compute.supports_quantization("int8"));
        assert!(compute.supports_quantization("fp16"));
        assert!(!compute.supports_quantization("unsupported_quant"));

        // Verify Apple Silicon memory calculations. VRAM is honestly
        // unknown without a Metal/IORegistry query (0 = unknown).
        let total_ram = compute.total_physical_ram();
        let avail_ram = compute.available_physical_ram();
        let wired_ram = compute.wired_ram();
        let vram = compute.vram_available_bytes();

        assert!(total_ram > 0);
        assert!(avail_ram > 0);
        assert!(wired_ram > 0);
        assert_eq!(vram, 0);
    }

    #[test]
    fn test_macos_seatbelt_profile_definitions() {
        let mut mail_profile = MacosSandboxProfile::agent_mail("/Users/alice");
        assert!(mail_profile.allow_file_read);
        assert!(mail_profile.allow_file_write);
        assert!(mail_profile.allow_network_outbound);
        assert!(mail_profile.profile_string.contains("(deny default)"));
        assert!(
            mail_profile
                .profile_string
                .contains("/Users/alice/Library/Hakille/Mail")
        );
        assert!(mail_profile.validate_safety().is_ok());
        assert!(mail_profile.apply().is_ok());

        // Verify flag reset upon recompilation
        mail_profile.rules.clear();
        mail_profile.compile();
        assert!(!mail_profile.allow_file_read);
        assert!(!mail_profile.allow_file_write);
        assert!(!mail_profile.allow_network_outbound);

        let code_profile = MacosSandboxProfile::agent_code_sandbox("/tmp/hakille-code-123");
        assert!(code_profile.allow_file_read);
        assert!(code_profile.allow_file_write);
        assert!(!code_profile.allow_network_outbound);
        assert!(code_profile.profile_string.contains("(deny network*)"));

        let wallet_profile = MacosSandboxProfile::agent_wallet_readonly("/var/db/hakille/wallet");
        assert!(wallet_profile.allow_file_read);
        assert!(!wallet_profile.allow_file_write);
        assert!(!wallet_profile.allow_network_outbound);

        let inference_profile =
            MacosSandboxProfile::inference_engine_profile("/opt/hakille/models");
        assert!(
            inference_profile
                .profile_string
                .contains("com.apple.metal.device")
        );
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
            signing_id: "com.hakille.helper".to_string(),
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
        dict.insert(
            "Power Source State".to_string(),
            "Battery Power".to_string(),
        );
        dict.insert("Is Charging".to_string(), "false".to_string());
        dict.insert("Current Capacity".to_string(), "75".to_string());
        dict.insert("Max Capacity".to_string(), "100".to_string());
        dict.insert("Time to Empty".to_string(), "240".to_string());

        let parsed = parse_iops_dictionary(&dict).expect("well-formed IOPMPowerSource dict");
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
        // Enforcement: capture_screen exige Sensor/Read (pas GuiInput).
        // Expiration future : Timestamp::now() expirerait à la seconde suivante.
        let cap = hakille_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Sensor],
                vec![hakille_core::CapabilityAction::Read],
                hakille_core::Timestamp(chrono::Utc::now() + chrono::Duration::hours(1)),
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

    fn future_expiry() -> hakille_core::Timestamp {
        hakille_core::Timestamp(chrono::Utc::now() + chrono::Duration::hours(1))
    }

    fn network_cap(action: hakille_core::CapabilityAction) -> hakille_core::CapabilityToken {
        hakille_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Network],
                vec![action],
                future_expiry(),
            )
            .unwrap()
    }

    fn get_request(url: String) -> hakille_substrate::HttpRequest {
        hakille_substrate::HttpRequest {
            method: "GET".to_string(),
            url,
            headers: std::collections::HashMap::new(),
            body: Vec::new(),
        }
    }

    /// Minimal loopback HTTP server (raw TCP, no extra deps).
    async fn spawn_local_http_server(body: &'static str) -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback for egress test");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = vec![0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        addr
    }

    #[tokio::test]
    async fn test_macos_http_egress_refuses_without_network_cap() {
        let substrate = MacosSubstrate::new();
        let wrong_cap = hakille_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![hakille_core::CapabilityAction::Read],
                future_expiry(),
            )
            .unwrap();
        let err = substrate
            .http_request(get_request("http://127.0.0.1/".to_string()), &wrong_cap)
            .await
            .expect_err("egress without Network cap must be refused");
        assert!(
            matches!(
                err,
                hakille_core::HakilleError::CapabilityDenied(_)
                    | hakille_core::HakilleError::CapabilityExpired
            ),
            "unexpected error: {err}"
        );

        let expired_cap = hakille_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Network],
                vec![hakille_core::CapabilityAction::Read],
                hakille_core::Timestamp(chrono::Utc::now() - chrono::Duration::hours(1)),
            )
            .unwrap();
        let err = substrate
            .http_request(get_request("http://127.0.0.1/".to_string()), &expired_cap)
            .await
            .expect_err("egress with expired cap must be refused");
        assert!(
            matches!(err, hakille_core::HakilleError::CapabilityExpired),
            "expired cap must yield CapabilityExpired, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_macos_http_egress_refuses_host_off_allowlist() {
        let substrate = MacosSubstrate::new().with_egress_policy(
            hakille_substrate::EgressPolicy::with_allowlist(["example.com"]),
        );
        let cap = network_cap(hakille_core::CapabilityAction::Read);
        let addr = spawn_local_http_server(r#"{"ok":true}"#).await;
        let err = substrate
            .http_request(get_request(format!("http://{addr}/")), &cap)
            .await
            .expect_err("host off allowlist must be refused");
        assert!(
            err.to_string().contains("not in egress allowlist"),
            "refusal must name the allowlist, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_macos_http_egress_loopback_success() {
        let substrate = MacosSubstrate::new();
        assert!(substrate.egress_policy().is_loopback_only());
        let cap = network_cap(hakille_core::CapabilityAction::Read);
        let addr = spawn_local_http_server(r#"{"ok":true}"#).await;
        let response = substrate
            .http_request(get_request(format!("http://{addr}/status")), &cap)
            .await
            .expect("loopback fetch under default policy must succeed");
        assert_eq!(response.status_code, 200);
        assert_eq!(response.body, b"{\"ok\":true}");
    }

    #[tokio::test]
    async fn test_macos_camera_honest_unimplemented() {
        let substrate = MacosSubstrate::new();
        let cap = hakille_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Camera],
                vec![hakille_core::CapabilityAction::Read],
                future_expiry(),
            )
            .unwrap();
        let config = hakille_substrate::CameraConfig {
            width: 1280,
            height: 720,
            fps: 30,
            facing: hakille_substrate::CameraFacing::Front,
        };
        assert!(!config.is_available());
        let err = substrate
            .start_camera_capture(config, &cap)
            .await
            .expect_err("camera without AVFoundation binding must be refused");
        let msg = err.to_string();
        assert!(msg.contains("camera not implemented"), "got: {msg}");
        assert!(
            msg.contains("AVFoundation"),
            "must name the missing backend, got: {msg}"
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn test_macos_host_queries_are_honest_off_macos() {
        // Off macOS the real `sysctl`/`sw_vers`/`vm_stat` reads are
        // impossible: every query must fail explicitly and no simulated
        // value may be presented as a host reading.
        for err in [
            query_hw_memsize().expect_err("hw.memsize must fail off macOS"),
            query_hw_ncpu().expect_err("hw.ncpu must fail off macOS"),
            query_hw_model().expect_err("hw.model must fail off macOS"),
            query_hw_pagesize().expect_err("hw.pagesize must fail off macOS"),
            query_sw_vers_product_version().expect_err("sw_vers must fail off macOS"),
            query_host_load_average().expect_err("vm.loadavg must fail off macOS"),
            query_vm_statistics().expect_err("vm_stat must fail off macOS"),
        ] {
            assert!(
                err.contains("macOS-only"),
                "off-macOS queries must name the missing platform, got: {err}"
            );
        }
        let version = read_macos_os_version();
        assert!(
            version.starts_with("unknown"),
            "off-macOS os_version must be an explicit unknown label, got: {version}"
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn test_macos_ram_fallback_is_labeled_simulation() {
        // Off macOS the RAM figures come from the explicitly simulated
        // profile: identical to `simulated_vm_statistics_fallback`, and the
        // compute backend mirrors it instead of claiming a real reading.
        let vm = read_host_vm_statistics();
        assert_eq!(vm, simulated_vm_statistics_fallback());
        let page_size = read_host_page_size_bytes();
        let compute = MacosComputeBackend::new();
        assert_eq!(
            compute.total_physical_ram(),
            read_total_physical_ram_bytes()
        );
        assert_eq!(
            compute.available_physical_ram(),
            vm.available_physical_ram(page_size)
        );
        assert_eq!(compute.wired_ram(), vm.wired_ram(page_size));
    }

    #[test]
    fn test_macos_thermal_and_vram_honest() {
        // No IOKit thermal binding linked: explicit Unknown, never invented.
        assert!(read_host_thermal_state().is_unknown());
        assert!(MacosSubstrate::new().thermal_state().is_unknown());
        // Unified-memory VRAM share unknown without Metal; envelope unknown
        // without a thermal-pressure source.
        let manifest = MacosSubstrate::new().manifest().clone();
        assert!(manifest.gpu_vram_bytes.is_none());
        assert!(manifest.thermal_envelope_watts.is_none());
        // The pressure-level mapping itself stays valid for future wiring.
        let heavy = OSThermalPressureLevel::Heavy.to_thermal_state();
        assert!(heavy.throttled);
    }
}
