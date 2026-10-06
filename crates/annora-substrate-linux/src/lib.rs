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
    use annora_substrate::{Compute, InputAction, Key, MouseButton, Substrate};

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
        let res_arch_mismatch = SeccompProfile::simulate_bpf(&x86_instructions, SyscallArch::Aarch64, 9);
        assert_eq!(res_arch_mismatch, SECCOMP_RET_KILL_PROCESS);

        let arm_instructions = wasm_profile.compile_bpf(SyscallArch::Aarch64);
        assert_eq!(x86_instructions.len(), arm_instructions.len());

        let net_profile = SeccompProfile::agent_network();
        assert!(net_profile.allowed_syscalls.contains(&"socket".to_string()));
        assert!(net_profile.allowed_syscalls.contains(&"epoll_wait".to_string()));

        let lockdown = SeccompProfile::strict_lockdown();
        assert_eq!(lockdown.default_action, "kill");
        let lockdown_bpf = lockdown.compile_bpf(SyscallArch::X86_64);
        let res_lockdown_denied = SeccompProfile::simulate_bpf(&lockdown_bpf, SyscallArch::X86_64, 0); // read -> kill
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
    async fn test_linux_display_and_input_injection() {
        let substrate = LinuxSubstrate::new();
        let cap_token = annora_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::GuiInput],
                vec![annora_core::CapabilityAction::Execute],
                annora_core::Timestamp::now(),
            )
            .unwrap();

        // Display capture test
        let screen = substrate.capture_screen(&cap_token).await.unwrap();
        assert_eq!(screen.width, 1920);
        assert_eq!(screen.height, 1080);
        assert_eq!(screen.data.len(), 1920 * 1080 * 4);

        // Test Wayland and X11 screencopy engines directly
        let wayland_engine = LinuxDisplayCaptureEngine::with_geometry(LinuxDisplayServer::Wayland, 1920, 1080);
        let wl_screen = wayland_engine.capture_screen();
        assert_eq!(wl_screen.width, 1920);

        let x11_engine = LinuxDisplayCaptureEngine::with_geometry(LinuxDisplayServer::X11, 1920, 1080);
        let x11_screen = x11_engine.capture_screen();
        assert_eq!(x11_screen.width, 1920);

        // Input injection test with uppercase, digits, punctuation, and absolute coordinates
        substrate.inject_input(
            InputAction::Click { x: 500, y: 300, button: MouseButton::Left },
            &cap_token,
        ).await.unwrap();

        substrate.inject_input(
            InputAction::Type { text: "Hello 100.".to_string() },
            &cap_token,
        ).await.unwrap();

        substrate.inject_input(
            InputAction::Hotkey { keys: vec![Key::Ctrl, Key::Alt, Key::Enter] },
            &cap_token,
        ).await.unwrap();

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
}
