pub mod cgroups;
pub mod compute_impl;
pub mod daemon;
pub mod display;
pub mod input;
pub mod namespaces;
pub mod seccomp;
pub mod substrate_impl;
pub mod telemetry;

pub use cgroups::*;
pub use compute_impl::*;
pub use daemon::*;
pub use display::*;
pub use input::*;
pub use namespaces::*;
pub use seccomp::*;
pub use substrate_impl::*;
pub use telemetry::*;

#[cfg(test)]
mod tests {
    use super::*;
    use annora_core::{CapabilityScope, Tier};
    use annora_substrate::{
        Compute, EgressPolicy, HttpRequest, InputAction, Key, MouseButton, Substrate,
    };
    use std::collections::HashMap;

    /// Expiry one hour in the future (a bare `Timestamp::now()` would race
    /// the one-second capability lifetime check).
    fn future_expiry() -> annora_core::Timestamp {
        annora_core::Timestamp(chrono::Utc::now() + chrono::Duration::hours(1))
    }

    fn network_cap(action: annora_core::CapabilityAction) -> annora_core::CapabilityToken {
        annora_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Network],
                vec![action],
                future_expiry(),
            )
            .unwrap()
    }

    /// Minimal loopback HTTP server (raw TCP, no extra deps): answers one
    /// request with `body`, then stops. Returns its socket address.
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

    fn get_request(url: String) -> HttpRequest {
        HttpRequest {
            method: "GET".to_string(),
            url,
            headers: HashMap::new(),
            body: Vec::new(),
        }
    }

    #[tokio::test]
    async fn test_linux_substrate_manifest_and_attestation() {
        let substrate = LinuxSubstrate::new();
        let manifest = substrate.manifest();
        assert_eq!(manifest.tier, Tier::Native);
        assert_eq!(manifest.os_name, "Linux");
        assert!(manifest.has_tpm);
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
            .attest(b"linux-nonce")
            .await
            .expect("Attestation failed");
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

        assert!(compute.total_physical_ram() > 0);
        assert!(compute.available_physical_ram() > 0);
    }

    #[test]
    fn test_seccomp_bpf_generation() {
        let wasm_profile = SeccompProfile::wasm_sandbox();
        assert_eq!(wasm_profile.default_action, "errno");
        assert!(wasm_profile.allowed_syscalls.contains(&"mmap".to_string()));
        assert!(wasm_profile.allowed_syscalls.contains(&"futex".to_string()));

        let x86_instructions = wasm_profile.compile_bpf(SyscallArch::X86_64);
        assert!(!x86_instructions.is_empty());
        // Check first instruction loads arch
        assert_eq!(x86_instructions[0].code, BPF_LD | BPF_W | BPF_ABS);
        assert_eq!(x86_instructions[0].k, SECCOMP_DATA_ARCH_OFFSET);

        let (filters, fprog) = wasm_profile.compile_sock_fprog(SyscallArch::X86_64);
        assert_eq!(filters.len(), fprog.len as usize);

        // Simulate BPF execution
        // 1. Allowed syscall (mmap = 9 on x86_64) -> ALLOW
        let res_allowed = SeccompProfile::simulate_bpf(&x86_instructions, SyscallArch::X86_64, 9);
        assert_eq!(res_allowed, SECCOMP_RET_ALLOW);

        // 2. Disallowed syscall (999) -> ERRNO
        let res_denied = SeccompProfile::simulate_bpf(&x86_instructions, SyscallArch::X86_64, 999);
        assert_eq!(res_denied, SECCOMP_RET_ERRNO | 1);

        // 3. Architecture mismatch -> KILL
        let res_arch_mismatch =
            SeccompProfile::simulate_bpf(&x86_instructions, SyscallArch::Aarch64, 9);
        assert_eq!(res_arch_mismatch, SECCOMP_RET_KILL_PROCESS);

        let arm_instructions = wasm_profile.compile_bpf(SyscallArch::Aarch64);
        assert_eq!(x86_instructions.len(), arm_instructions.len());

        let net_profile = SeccompProfile::agent_network();
        assert!(net_profile.allowed_syscalls.contains(&"socket".to_string()));
        assert!(
            net_profile
                .allowed_syscalls
                .contains(&"epoll_wait".to_string())
        );

        let lockdown = SeccompProfile::strict_lockdown();
        assert_eq!(lockdown.default_action, "kill");
        let lockdown_bpf = lockdown.compile_bpf(SyscallArch::X86_64);
        let res_lockdown_denied =
            SeccompProfile::simulate_bpf(&lockdown_bpf, SyscallArch::X86_64, 0); // read -> kill
        assert_eq!(res_lockdown_denied, SECCOMP_RET_KILL_PROCESS);
    }

    #[test]
    fn test_linux_namespaces_configuration() {
        let mut ns_config = NamespaceConfig::default();
        assert!(ns_config.is_pid_isolated());
        assert!(ns_config.is_network_isolated());
        assert!(ns_config.is_mount_isolated());

        ns_config.add_bind_mount("/host/models", "/sandbox/models", true);
        assert_eq!(ns_config.mounts.len(), 3);
        let last_mount = &ns_config.mounts[2];
        assert_eq!(last_mount.source, "/host/models");
        assert_eq!(last_mount.target, "/sandbox/models");
        assert_eq!(last_mount.flags & MS_RDONLY, MS_RDONLY);
    }

    #[test]
    fn test_linux_cgroups_v2_controller() {
        let config = CgroupV2Config {
            memory_max_bytes: Some(512 * 1024 * 1024),
            memory_high_bytes: Some(400 * 1024 * 1024),
            cpu_max_quota_us: Some(50_000),
            cpu_max_period_us: Some(100_000),
            cpu_weight: Some(200),
            pids_max: Some(32),
            io_max_rbps: Some(10 * 1024 * 1024),
            io_max_wbps: Some(5 * 1024 * 1024),
        };
        let mgr = CgroupManager::new("agent-wasm-01", config);
        assert_eq!(mgr.agent_id(), "agent-wasm-01");
        let metrics = mgr.read_metrics();
        assert!(metrics.memory_current_bytes > 0);
        assert!(metrics.pids_current > 0);
    }

    #[test]
    fn test_linux_meminfo_and_sysfs_parsing() {
        let sample_meminfo = r#"
MemTotal:       32890624 kB
MemFree:         4194304 kB
MemAvailable:   20971520 kB
Buffers:          524288 kB
Cached:          8388608 kB
SwapTotal:       4194304 kB
SwapFree:        4194304 kB
"#;
        let parsed = parse_meminfo(sample_meminfo).expect("Failed to parse meminfo");
        assert_eq!(parsed.mem_total_bytes, 32890624 * 1024);
        assert_eq!(parsed.mem_available_bytes, 20971520 * 1024);
        assert_eq!(parsed.used_ram_bytes(), (32890624 - 20971520) * 1024);

        let power = parse_power_supply_fields("BAT0", "Charging", "88", "Battery", Some("1"));
        assert_eq!(power.capacity_pct, 88.0);
        assert!(power.online);

        let thermal = parse_thermal_zone("48500", "x86_pkg_temp", "thermal_zone0");
        assert_eq!(thermal.temp_celsius, 48.5);
        assert!(!thermal.is_throttled);

        let hot_thermal = parse_thermal_zone("92000", "acpitz", "thermal_zone1");
        assert_eq!(hot_thermal.temp_celsius, 92.0);
        assert!(hot_thermal.is_throttled);
    }

    #[tokio::test]
    async fn test_linux_http_egress_refuses_without_network_cap() {
        let substrate = LinuxSubstrate::new();
        // Wrong scope (Filesystem instead of Network) must be refused.
        let wrong_cap = annora_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![annora_core::CapabilityAction::Read],
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
                annora_core::AnnoraError::CapabilityDenied(_)
                    | annora_core::AnnoraError::CapabilityExpired
            ),
            "unexpected error: {err}"
        );

        // Expired Network cap must be refused explicitly as expired.
        let expired_cap = annora_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Network],
                vec![annora_core::CapabilityAction::Read],
                annora_core::Timestamp(chrono::Utc::now() - chrono::Duration::hours(1)),
            )
            .unwrap();
        let err = substrate
            .http_request(get_request("http://127.0.0.1/".to_string()), &expired_cap)
            .await
            .expect_err("egress with expired cap must be refused");
        assert!(
            matches!(err, annora_core::AnnoraError::CapabilityExpired),
            "expired cap must yield CapabilityExpired, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_linux_http_egress_refuses_host_off_allowlist() {
        // Allowlist without loopback: even loopback must be refused.
        let substrate =
            LinuxSubstrate::new().with_egress_policy(EgressPolicy::with_allowlist(["example.com"]));
        let cap = network_cap(annora_core::CapabilityAction::Read);
        let addr = spawn_local_http_server(r#"{"ok":true}"#).await;
        let err = substrate
            .http_request(get_request(format!("http://{addr}/")), &cap)
            .await
            .expect_err("host off allowlist must be refused");
        let msg = err.to_string();
        assert!(
            msg.contains("not in egress allowlist"),
            "refusal must name the allowlist, got: {msg}"
        );
    }

    #[tokio::test]
    async fn test_linux_http_egress_loopback_success() {
        // Default policy is loopback-only: local fetch succeeds for real.
        let substrate = LinuxSubstrate::new();
        assert!(substrate.egress_policy().is_loopback_only());
        let cap = network_cap(annora_core::CapabilityAction::Read);
        let addr = spawn_local_http_server(r#"{"ok":true}"#).await;
        let response = substrate
            .http_request(get_request(format!("http://{addr}/status")), &cap)
            .await
            .expect("loopback fetch under default policy must succeed");
        assert_eq!(response.status_code, 200);
        assert_eq!(response.body, b"{\"ok\":true}");
    }

    #[tokio::test]
    async fn test_linux_camera_honest_unimplemented() {
        let substrate = LinuxSubstrate::new();
        let cap = annora_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Camera],
                vec![annora_core::CapabilityAction::Read],
                future_expiry(),
            )
            .unwrap();
        let config = annora_substrate::CameraConfig {
            width: 1280,
            height: 720,
            fps: 30,
            facing: annora_substrate::CameraFacing::Front,
        };
        assert!(!config.is_available());
        let err = substrate
            .start_camera_capture(config, &cap)
            .await
            .expect_err("camera without V4L2 binding must be refused");
        let msg = err.to_string();
        assert!(msg.contains("camera not implemented"), "got: {msg}");
        assert!(
            msg.contains("V4L2"),
            "must name the missing backend, got: {msg}"
        );
    }

    #[test]
    fn test_linux_thermal_honest() {
        // Real sysfs reading when present, explicit Unknown otherwise —
        // never an invented temperature.
        let state = read_host_thermal_state();
        assert!(
            state.is_unknown() || (-50.0..150.0).contains(&state.temperature_celsius),
            "thermal state must be Unknown or a plausible reading"
        );
        assert!(annora_substrate::ThermalState::Unknown.is_unknown());
        // VRAM detection is host-dependent; it must simply never panic and
        // the manifest must mirror it (None when undiscoverable).
        let substrate = LinuxSubstrate::new();
        assert_eq!(
            substrate.manifest().gpu_vram_bytes,
            detect_linux_vram_bytes()
        );
        assert!(substrate.manifest().thermal_envelope_watts.is_none());
    }

    #[tokio::test]
    async fn test_linux_display_and_input_injection() {
        let substrate = LinuxSubstrate::new();
        // Expiration future : Timestamp::now() expirerait à la seconde suivante
        // maintenant que l'enforcement vérifie réellement l'expiration.
        let expiry = annora_core::Timestamp(chrono::Utc::now() + chrono::Duration::hours(1));
        // Enforcement: capture_screen exige Sensor/Read, inject_input exige GuiInput/Execute.
        let screen_cap = annora_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Sensor],
                vec![annora_core::CapabilityAction::Read],
                expiry.clone(),
            )
            .unwrap();
        let cap_token = annora_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::GuiInput],
                vec![annora_core::CapabilityAction::Execute],
                expiry,
            )
            .unwrap();

        // Display capture test
        let screen = substrate.capture_screen(&screen_cap).await.unwrap();
        assert_eq!(screen.width, 1920);
        assert_eq!(screen.height, 1080);
        assert_eq!(screen.data.len(), 1920 * 1080 * 4);

        // Test Wayland and X11 screencopy engines directly
        let wayland_engine =
            LinuxDisplayCaptureEngine::with_geometry(LinuxDisplayServer::Wayland, 1920, 1080);
        let wl_screen = wayland_engine.capture_screen();
        assert_eq!(wl_screen.width, 1920);

        let x11_engine =
            LinuxDisplayCaptureEngine::with_geometry(LinuxDisplayServer::X11, 1920, 1080);
        let x11_screen = x11_engine.capture_screen();
        assert_eq!(x11_screen.width, 1920);

        // Injection uinput : sur un hôte sans /dev/uinput (CI conteneurisée,
        // Windows), le refus doit être honnête et nommer le device manquant —
        // même philosophie que la caméra V4L2 ci-dessus. Avec /dev/uinput,
        // les événements evdev réels sont générés et enregistrés.
        let click_result = substrate
            .inject_input(
                InputAction::Click {
                    x: 500,
                    y: 300,
                    button: MouseButton::Left,
                },
                &cap_token,
            )
            .await;
        match click_result {
            Ok(()) => {
                substrate
                    .inject_input(
                        InputAction::Type {
                            text: "Hello 100.".to_string(),
                        },
                        &cap_token,
                    )
                    .await
                    .expect("injection clavier uinput");
                substrate
                    .inject_input(
                        InputAction::Hotkey {
                            keys: vec![Key::Ctrl, Key::Alt, Key::Enter],
                        },
                        &cap_token,
                    )
                    .await
                    .expect("injection hotkey uinput");

                let events = substrate.input_injector().recorded_events();
                assert!(!events.is_empty(), "Recorded events must not be empty");

                // Verify shift key for 'H'
                let has_shift = events.iter().any(|ev| ev.code == KEY_LEFTSHIFT);
                assert!(has_shift, "Typing 'H' must generate KEY_LEFTSHIFT");

                // Verify KEY_0 for '0'
                let has_zero = events.iter().any(|ev| ev.code == KEY_0);
                assert!(has_zero, "Typing '0' must generate KEY_0");

                // Verify ABS coordinate events
                let has_abs = events.iter().any(|ev| ev.type_ == EV_ABS);
                assert!(has_abs, "Click must generate EV_ABS event");
            }
            Err(annora_core::AnnoraError::SubstrateError(msg)) => {
                assert!(
                    msg.contains("uinput"),
                    "refus honnête nommant le device manquant, obtenu : {msg}"
                );
            }
            Err(other) => panic!("erreur injection inattendue : {other:?}"),
        }
    }
}
