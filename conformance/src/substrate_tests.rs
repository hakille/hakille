use annora_core::{CapabilityScope, Tier};
use annora_substrate::{Compute, Substrate};
use annora_substrate_linux::{
    LinuxComputeBackend, LinuxSubstrate, SeccompProfile, SyscallArch,
};
use annora_substrate_macos::{
    MacosComputeBackend, MacosSandboxProfile, MacosSubstrate,
};
use annora_substrate_mobile::{
    BatteryTier, MobileComputeBackend, MobileShadowSync, MobileSubstrate, ScopedStorageSandbox,
    ShadowState, StorageScope,
};
use annora_substrate_redox::{
    RedoxCapabilityTable, RedoxComputeBackend, RedoxSchemeHandler, RedoxSubstrate,
    SchemeHandler, CAP_READ, CAP_WRITE,
};
use annora_substrate_web::{
    WebAudioBridge, WebAudioNodeConfig, WebComputeBackend, WebGpuDetector, WebSubstrate,
};
use annora_substrate_windows::{WindowsComputeBackend, WindowsSubstrate};

#[tokio::test]
async fn test_all_substrates_manifest_and_attestation() {
    let win = WindowsSubstrate::new();
    assert_eq!(win.manifest().tier, Tier::Native);
    let att = win.attest(b"test").await.unwrap();
    assert_eq!(att.platform_id, "Windows-Substrate-Host");

    let lin = LinuxSubstrate::new();
    assert_eq!(lin.manifest().tier, Tier::Native);
    let att = lin.attest(b"test").await.unwrap();
    assert_eq!(att.platform_id, "Linux-Substrate-Host");

    let mac = MacosSubstrate::new();
    assert_eq!(mac.manifest().tier, Tier::Native);
    let att = mac.attest(b"test").await.unwrap();
    assert_eq!(att.platform_id, "macOS-AppleSilicon-Host");

    let redox = RedoxSubstrate::new();
    assert_eq!(redox.manifest().tier, Tier::Sovereign);
    let att = redox.attest(b"test").await.unwrap();
    assert_eq!(att.platform_id, "Redox-Sovereign-Dedicated-Node");

    let mob = MobileSubstrate::new();
    assert_eq!(mob.manifest().tier, Tier::Guest);
    let att = mob.attest(b"test").await.unwrap();
    assert_eq!(att.platform_id, "Mobile-Edge-Shadow-Terminal");

    let web = WebSubstrate::new();
    assert_eq!(web.manifest().tier, Tier::Guest);
    let att = web.attest(b"test").await.unwrap();
    assert_eq!(att.platform_id, "Wasm-Browser-Client");
}

#[tokio::test]
async fn test_all_compute_backends() {
    let win_comp = WindowsComputeBackend::new();
    assert!(!win_comp.available_backends().is_empty());
    assert!(win_comp.supports_quantization("q4_k"));

    let lin_comp = LinuxComputeBackend::new();
    assert!(!lin_comp.available_backends().is_empty());
    assert!(lin_comp.supports_quantization("q4_k"));
    assert!(lin_comp.total_physical_ram() > 0);

    let mac_comp = MacosComputeBackend::new();
    assert!(!mac_comp.available_backends().is_empty());
    assert!(mac_comp.supports_quantization("fp16"));
    assert!(mac_comp.total_physical_ram() > 0);

    let redox_comp = RedoxComputeBackend::new();
    assert!(!redox_comp.available_backends().is_empty());

    let mob_comp = MobileComputeBackend::new();
    assert!(!mob_comp.available_backends().is_empty());
    assert!(mob_comp.supports_quantization("int8"));

    let web_comp = WebComputeBackend::new();
    assert!(!web_comp.available_backends().is_empty());
    assert!(web_comp.supports_quantization("q4_k"));
}

#[tokio::test]
async fn test_cross_substrate_screen_capture_conformance() {
    let cap = annora_core::CapabilityAuthority::new()
        .create_root_token(
            vec![CapabilityScope::GuiInput],
            vec![annora_core::CapabilityAction::Read],
            annora_core::Timestamp::now(),
        )
        .unwrap();

    let win = WindowsSubstrate::new();
    let win_screen = win.capture_screen(&cap).await.unwrap();
    assert!(win_screen.width > 0 && win_screen.height > 0);
    assert!(!win_screen.data.is_empty());

    let lin = LinuxSubstrate::new();
    let lin_screen = lin.capture_screen(&cap).await.unwrap();
    assert_eq!(lin_screen.width, 1920);
    assert_eq!(lin_screen.height, 1080);
    assert_eq!(lin_screen.data.len(), 1920 * 1080 * 4);

    let mac = MacosSubstrate::new();
    let mac_screen = mac.capture_screen(&cap).await.unwrap();
    assert_eq!(mac_screen.width, 2560);
    assert_eq!(mac_screen.height, 1600);
    assert_eq!(mac_screen.data.len(), 2560 * 1600 * 4);

    let redox = RedoxSubstrate::new();
    let redox_screen = redox.capture_screen(&cap).await.unwrap();
    assert_eq!(redox_screen.width, 1920);
    assert_eq!(redox_screen.height, 1080);

    let mob = MobileSubstrate::new();
    let mob_screen = mob.capture_screen(&cap).await.unwrap();
    assert_eq!(mob_screen.width, 1170);
    assert_eq!(mob_screen.height, 2532);

    let web = WebSubstrate::new();
    let web_screen = web.capture_screen(&cap).await.unwrap();
    assert_eq!(web_screen.width, 1920);
    assert_eq!(web_screen.height, 1080);
}

#[test]
fn test_linux_conformance_seccomp_and_sysfs() {
    let profile = SeccompProfile::wasm_sandbox();
    let bpf = profile.compile_bpf(SyscallArch::X86_64);
    assert!(bpf.len() > 10);

    // Verify BPF simulation
    let allow_res = SeccompProfile::simulate_bpf(&bpf, SyscallArch::X86_64, 9); // mmap
    assert_eq!(allow_res, annora_substrate_linux::SECCOMP_RET_ALLOW);

    let deny_res = SeccompProfile::simulate_bpf(&bpf, SyscallArch::X86_64, 999);
    assert_eq!(deny_res, annora_substrate_linux::SECCOMP_RET_ERRNO | 1);

    let lockdown = SeccompProfile::strict_lockdown();
    let lockdown_bpf = lockdown.compile_bpf(SyscallArch::X86_64);
    assert_eq!(
        SeccompProfile::simulate_bpf(&lockdown_bpf, SyscallArch::X86_64, 0),
        annora_substrate_linux::SECCOMP_RET_KILL_PROCESS
    );

    let meminfo = annora_substrate_linux::read_host_meminfo();
    assert!(meminfo.mem_total_bytes > 0);
    assert!(meminfo.mem_available_bytes > 0);

    let power = annora_substrate_linux::read_host_power_state();
    assert!(power.battery_percent >= 0.0 && power.battery_percent <= 100.0);
}

#[test]
fn test_macos_conformance_seatbelt_and_mach() {
    let mut profile = MacosSandboxProfile::agent_mail("/Users/tester");
    assert!(profile.profile_string.contains("(deny default)"));
    assert!(profile.validate_safety().is_ok());
    assert!(profile.apply().is_ok());

    // Flag reset verification
    profile.rules.clear();
    profile.compile();
    assert!(!profile.allow_file_read);

    // Endpoint Security PID muting
    let mut es_client = annora_substrate_macos::EndpointSecurityClient::default();
    es_client.add_mute_pid(9999);
    assert!(es_client.is_pid_muted(9999));

    let vm = annora_substrate_macos::read_host_vm_statistics();
    assert!(vm.total_physical_ram(16384) > 0);

    let quartz = annora_substrate_macos::MacosQuartzDisplayCapture::new();
    assert_eq!(quartz.physical_width(), 5120);
}

#[test]
fn test_redox_conformance_schemes_and_capabilities() {
    let handler = RedoxSchemeHandler::new();
    let fd = handler.dispatch_open("chan:agent/test", 0).unwrap();
    assert!(handler.chan().write(fd, b"sovereign_packet").is_ok());

    // RedoxPacket microkernel IPC handling
    let mut mem_pool = vec![0u8; 512];
    let path = b"chan:conformance/stream";
    mem_pool[..path.len()].copy_from_slice(path);
    let mut open_pkt = annora_substrate_redox::RedoxPacket::new(10, annora_substrate_redox::SYS_OPEN, 0, 0, path.len(), 0);
    let handle = handler.handle_packet(&mut open_pkt, &mut mem_pool).unwrap();
    assert_eq!(open_pkt.a, handle);

    let table = RedoxCapabilityTable::new();
    let fd_test = table.allocate_handle("disk:", "block0", CAP_READ | CAP_WRITE);
    assert!(table.verify_access(fd_test, CAP_READ).is_ok());
    assert!(table.verify_access(fd_test, CAP_WRITE).is_ok());

    // Capability attenuation
    let derived_ro = table.derive_restricted_capability(fd_test, CAP_READ).unwrap();
    assert!(table.verify_access(derived_ro, CAP_READ).is_ok());
    assert!(table.verify_access(derived_ro, CAP_WRITE).is_err());
}

#[test]
fn test_mobile_conformance_shadow_and_battery() {
    let mut shadow = MobileShadowSync::default();
    shadow.queue_local_delta("draft.txt", b"secret draft");
    assert!(shadow.connect().is_ok());
    assert_eq!(shadow.state(), ShadowState::ActiveStreaming);
    assert_eq!(shadow.get_value("draft.txt"), Some(b"secret draft".to_vec()));

    // CRDT LWW resolution
    let remote_delta = annora_substrate_mobile::ShadowDelta {
        key: "draft.txt".to_string(),
        value: b"newer draft from TiKV".to_vec(),
        version: 5,
        timestamp: annora_core::Timestamp::now(),
        is_deleted: false,
    };
    shadow.push_remote_delta(remote_delta);
    assert_eq!(shadow.get_value("draft.txt"), Some(b"newer draft from TiKV".to_vec()));

    let sandbox = ScopedStorageSandbox::default();
    let resolved = sandbox
        .resolve_path(StorageScope::Documents, &annora_core::VirtualPath("db.sqlite".to_string()))
        .unwrap();
    assert!(resolved.ends_with("Documents/db.sqlite"));

    // Drive traversal rejection
    assert!(sandbox.resolve_path(StorageScope::Documents, &annora_core::VirtualPath("C:\\Windows".to_string())).is_err());
    assert!(sandbox.resolve_path(StorageScope::Documents, &annora_core::VirtualPath("/etc/shadow".to_string())).is_err());

    let bat = annora_substrate_mobile::BatteryConservationEngine::new(20.0, false);
    assert_eq!(bat.current_tier(), BatteryTier::Conserve);
    assert_eq!(bat.perception_fps(), 1);
    assert!(!bat.should_offload_inference_with_network(false));
    assert!(bat.should_offload_inference_with_network(true));
}

#[test]
fn test_web_conformance_audio_and_webgpu() {
    let config = WebAudioNodeConfig::default();
    let mut bridge = WebAudioBridge::new(config);
    let pcm_in = vec![0.5f32; 480]; // 10ms at 48kHz
    let count = bridge.buffer_mut().ingest_worklet_samples_f32(&pcm_in);
    assert_eq!(count, 160); // 10ms at 16kHz

    let detector = WebGpuDetector::new();
    assert!(detector.capabilities().is_webgpu_supported);
    assert!(detector.can_run_half_precision_gemm());
    assert!(detector.validate_tensor_allocation(256 * 1024 * 1024).is_ok());
    assert!(detector.validate_tensor_allocation(4 * 1024 * 1024 * 1024).is_err());

    let adapter = annora_substrate_web::WebScreenCaptureAdapter::default();
    let rgba = vec![200u8; 50 * 50 * 4];
    let frame = adapter.ingest_image_data(50, 50, &rgba).unwrap();
    assert_eq!(frame.width, 50);
}
