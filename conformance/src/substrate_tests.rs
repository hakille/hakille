use hakille_core::{CapabilityAction, CapabilityScope, Tier, Timestamp, VirtualPath};
use hakille_substrate::{Compute, Substrate};
use hakille_substrate_linux::{LinuxComputeBackend, LinuxSubstrate, SeccompProfile, SyscallArch};
use hakille_substrate_macos::{MacosComputeBackend, MacosSandboxProfile, MacosSubstrate};
use hakille_substrate_mobile::{
    BatteryTier, MobileComputeBackend, MobileShadowSync, MobileSubstrate, ScopedStorageSandbox,
    ShadowState, StorageScope,
};
use hakille_substrate_redox::{
    CAP_READ, CAP_WRITE, RedoxCapabilityTable, RedoxComputeBackend, RedoxSchemeHandler,
    RedoxSubstrate, SchemeHandler,
};
use hakille_substrate_web::{
    WebAudioBridge, WebAudioNodeConfig, WebComputeBackend, WebGpuDetector, WebSubstrate,
};
use hakille_substrate_windows::{WindowsComputeBackend, WindowsSubstrate};

#[tokio::test]
async fn test_all_substrates_manifest_and_attestation() {
    let win = WindowsSubstrate::new();
    assert_eq!(win.manifest().tier, Tier::Native);
    let att = win.attest(b"test").await.unwrap();
    // Attestation HONNÊTE : clé éphémère locale, pas de preuve VBS/TPM — le
    // platform_id porte volontairement le suffixe.
    assert_eq!(att.platform_id, "Windows-Substrate-Host-dev-attestation");

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
    assert!(mob.attest(b"test").await.is_err());
    assert!(!mob.manifest().has_secure_enclave);
    assert!(mob.manifest().max_memory_bytes.is_none());

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
    assert_eq!(
        mob_comp.available_backends(),
        vec![hakille_substrate::ComputeBackend::Cpu]
    );
    assert!(!mob_comp.supports_quantization("int8"));
    assert_eq!(mob_comp.ram_available_bytes(), 0);

    let web_comp = WebComputeBackend::new();
    assert!(!web_comp.available_backends().is_empty());
    assert!(web_comp.supports_quantization("q4_k"));
}

#[tokio::test]
async fn test_cross_substrate_screen_capture_conformance() {
    // Token Sensor+GuiInput / Read+Execute : capture_screen exige Sensor/Read
    // (cohérent Windows). Expiration future pour ne pas expirer mid-test.
    let authority = hakille_core::CapabilityAuthority::new();
    let cap = authority
        .create_root_token(
            vec![CapabilityScope::Sensor, CapabilityScope::GuiInput],
            vec![
                hakille_core::CapabilityAction::Read,
                hakille_core::CapabilityAction::Execute,
            ],
            future_expiry(),
        )
        .unwrap();

    let win = WindowsSubstrate::new();
    // CONTRAT HONNÊTE : le substrat Windows refuse explicitement la capture
    // hors Windows (aucune frame fabriquée n'est acceptable). Sous Windows,
    // la capture est réelle (dimensions non nulles, pixels présents).
    match win.capture_screen(&cap).await {
        Ok(win_screen) => {
            assert!(win_screen.width > 0 && win_screen.height > 0);
            assert!(!win_screen.data.is_empty());
        }
        Err(e) => {
            let message = e.to_string();
            assert!(
                message.contains("capture d'écran") || message.contains("capture d'"),
                "refus explicite attendu, obtenu : {message}"
            );
        }
    }

    // Les backends non raccordés refusent la capture ; leurs générateurs de
    // démonstration ne constituent jamais des pixels du poste utilisateur.
    let substrates: Vec<Box<dyn Substrate>> = vec![
        Box::new(LinuxSubstrate::new()),
        Box::new(MacosSubstrate::new()),
        Box::new(RedoxSubstrate::new()),
        Box::new(MobileSubstrate::new().with_trusted_root(authority.root_public_key())),
        Box::new(WebSubstrate::new()),
    ];
    for substrate in substrates {
        let error = substrate.capture_screen(&cap).await.unwrap_err();
        assert!(
            matches!(&error, hakille_core::HakilleError::SubstrateError(message)
                if message.contains("capture d'écran indisponible")
                    && message.contains("aucune image du poste capturée")),
            "{} : {error}",
            substrate.manifest().os_name,
        );
    }
}

fn future_expiry() -> Timestamp {
    Timestamp(chrono::Utc::now() + chrono::Duration::hours(1))
}

fn fs_write_cap(auth: &hakille_core::CapabilityAuthority) -> hakille_core::CapabilityToken {
    auth.create_root_token(
        vec![CapabilityScope::Filesystem],
        vec![CapabilityAction::Write],
        future_expiry(),
    )
    .unwrap()
}

#[tokio::test]
async fn test_substrate_write_with_wrong_cap_refused() {
    // Token porte le mauvais scope (Network/Read) : write_file doit refuser partout.
    let authority = hakille_core::CapabilityAuthority::new();
    let wrong_cap = authority
        .create_root_token(
            vec![CapabilityScope::Network],
            vec![CapabilityAction::Read],
            future_expiry(),
        )
        .unwrap();
    let path = VirtualPath("should_not_exist.txt".to_string());

    let dir = tempfile::tempdir().unwrap();
    let lin = LinuxSubstrate::new().with_virtual_fs_root(dir.path().join("vfs"));
    assert!(lin.write_file(&path, b"nope", &wrong_cap).await.is_err());

    let mac = MacosSubstrate::new();
    assert!(mac.write_file(&path, b"nope", &wrong_cap).await.is_err());

    let mob = MobileSubstrate::new();
    assert!(mob.write_file(&path, b"nope", &wrong_cap).await.is_err());

    let web = WebSubstrate::new();
    assert!(web.write_file(&path, b"nope", &wrong_cap).await.is_err());

    let redox = RedoxSubstrate::new();
    assert!(redox.write_file(&path, b"nope", &wrong_cap).await.is_err());
}

#[tokio::test]
async fn test_substrate_traversal_evil_refused() {
    // `../evil` doit être rejeté par resolve_path AVANT tout effet.
    let authority = hakille_core::CapabilityAuthority::new();
    let dir = tempfile::tempdir().unwrap();
    let evil = VirtualPath("../evil".to_string());

    let lin = LinuxSubstrate::new().with_virtual_fs_root(dir.path().join("lin"));
    let good = fs_write_cap(&authority);
    assert!(lin.write_file(&evil, b"evil", &good).await.is_err());
    assert!(lin.read_file(&evil, &good).await.is_err());

    let mac = MacosSubstrate::new();
    assert!(mac.write_file(&evil, b"evil", &good).await.is_err());

    let mob = MobileSubstrate::new();
    assert!(mob.write_file(&evil, b"evil", &good).await.is_err());

    let web = WebSubstrate::new();
    assert!(web.write_file(&evil, b"evil", &good).await.is_err());

    let redox = RedoxSubstrate::new();
    assert!(redox.write_file(&evil, b"evil", &good).await.is_err());
}

#[tokio::test]
async fn test_substrate_forged_token_refused_when_pinned() {
    // Substrat pinné sur la racine honnête A : un token auto-signé forgé par
    // l'attaquant B (bon scope/action) doit être refusé ; le token honnête passe.
    let honest = hakille_core::CapabilityAuthority::new();
    let attacker = hakille_core::CapabilityAuthority::new();
    let forged = fs_write_cap(&attacker);
    let legit = fs_write_cap(&honest);
    let path = VirtualPath("pinned.txt".to_string());

    let dir = tempfile::tempdir().unwrap();
    let lin = LinuxSubstrate::new()
        .with_virtual_fs_root(dir.path().join("lin"))
        .with_trusted_root(honest.root_public_key());
    assert!(lin.write_file(&path, b"evil", &forged).await.is_err());
    assert!(lin.write_file(&path, b"legit", &legit).await.is_ok());

    let mac = MacosSubstrate::new().with_trusted_root(honest.root_public_key());
    assert!(mac.write_file(&path, b"evil", &forged).await.is_err());

    let mobile_root = dir.path().join("mobile");
    std::fs::create_dir(&mobile_root).unwrap();
    let mob = MobileSubstrate::new()
        .with_trusted_root(honest.root_public_key())
        .with_app_container(mobile_root)
        .unwrap();
    assert!(mob.write_file(&path, b"evil", &forged).await.is_err());
    assert!(mob.write_file(&path, b"legit", &legit).await.is_ok());

    let web = WebSubstrate::new().with_trusted_root(honest.root_public_key());
    assert!(web.write_file(&path, b"evil", &forged).await.is_err());
    assert!(web.write_file(&path, b"legit", &legit).await.is_ok());

    let redox = RedoxSubstrate::new().with_trusted_root(honest.root_public_key());
    assert!(redox.write_file(&path, b"evil", &forged).await.is_err());
    assert!(redox.write_file(&path, b"legit", &legit).await.is_ok());
}

#[test]
fn test_linux_conformance_seccomp_and_sysfs() {
    let profile = SeccompProfile::wasm_sandbox();
    let bpf = profile.compile_bpf(SyscallArch::X86_64);
    assert!(bpf.len() > 10);

    // Verify BPF simulation
    let allow_res = SeccompProfile::simulate_bpf(&bpf, SyscallArch::X86_64, 9); // mmap
    assert_eq!(allow_res, hakille_substrate_linux::SECCOMP_RET_ALLOW);

    let deny_res = SeccompProfile::simulate_bpf(&bpf, SyscallArch::X86_64, 999);
    assert_eq!(deny_res, hakille_substrate_linux::SECCOMP_RET_ERRNO | 1);

    let lockdown = SeccompProfile::strict_lockdown();
    let lockdown_bpf = lockdown.compile_bpf(SyscallArch::X86_64);
    assert_eq!(
        SeccompProfile::simulate_bpf(&lockdown_bpf, SyscallArch::X86_64, 0),
        hakille_substrate_linux::SECCOMP_RET_KILL_PROCESS
    );

    let meminfo = hakille_substrate_linux::read_host_meminfo();
    assert!(meminfo.mem_total_bytes > 0);
    assert!(meminfo.mem_available_bytes > 0);

    let power = hakille_substrate_linux::read_host_power_state();
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
    let mut es_client = hakille_substrate_macos::EndpointSecurityClient::default();
    es_client.add_mute_pid(9999);
    assert!(es_client.is_pid_muted(9999));

    let vm = hakille_substrate_macos::read_host_vm_statistics();
    assert!(vm.total_physical_ram(16384) > 0);

    let quartz = hakille_substrate_macos::MacosQuartzDisplayCapture::new();
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
    let mut open_pkt = hakille_substrate_redox::RedoxPacket::new(
        10,
        hakille_substrate_redox::SYS_OPEN,
        0,
        0,
        path.len(),
        0,
    );
    let handle = handler.handle_packet(&mut open_pkt, &mut mem_pool).unwrap();
    assert_eq!(open_pkt.a, handle);

    let table = RedoxCapabilityTable::new();
    let fd_test = table.allocate_handle("disk:", "block0", CAP_READ | CAP_WRITE);
    assert!(table.verify_access(fd_test, CAP_READ).is_ok());
    assert!(table.verify_access(fd_test, CAP_WRITE).is_ok());

    // Capability attenuation
    let derived_ro = table
        .derive_restricted_capability(fd_test, CAP_READ)
        .unwrap();
    assert!(table.verify_access(derived_ro, CAP_READ).is_ok());
    assert!(table.verify_access(derived_ro, CAP_WRITE).is_err());
}

#[test]
fn test_mobile_conformance_shadow_and_battery() {
    let mut shadow = MobileShadowSync::default();
    shadow
        .queue_local_delta("draft.txt", b"secret draft")
        .unwrap();
    assert!(shadow.connect().is_err()); // Aucun transport natif intégré.
    assert_eq!(shadow.state(), ShadowState::Disconnected);
    assert_eq!(shadow.pending_count(), 1);
    assert_eq!(
        shadow.get_value("draft.txt"),
        Some(b"secret draft".to_vec())
    );

    // Fusion LWW : le contenu remplacé reste observable dans les conflits.
    let remote_delta = hakille_substrate_mobile::ShadowDelta {
        key: "draft.txt".to_string(),
        value: b"newer draft from TiKV".to_vec(),
        version: 5,
        timestamp: hakille_core::Timestamp::now(),
        is_deleted: false,
    };
    shadow.push_remote_delta(remote_delta).unwrap();
    assert_eq!(shadow.pending_count(), 1);
    assert_eq!(shadow.conflicts().len(), 1);
    assert_eq!(
        shadow.get_value("draft.txt"),
        Some(b"newer draft from TiKV".to_vec())
    );

    let sandbox = ScopedStorageSandbox::default();
    let resolved = sandbox
        .resolve_path(
            StorageScope::Documents,
            &hakille_core::VirtualPath("db.sqlite".to_string()),
        )
        .unwrap();
    assert!(resolved.ends_with("Documents/db.sqlite"));

    // Drive traversal rejection
    assert!(
        sandbox
            .resolve_path(
                StorageScope::Documents,
                &hakille_core::VirtualPath("C:\\Windows".to_string())
            )
            .is_err()
    );
    assert!(
        sandbox
            .resolve_path(
                StorageScope::Documents,
                &hakille_core::VirtualPath("/etc/shadow".to_string())
            )
            .is_err()
    );

    let bat = hakille_substrate_mobile::BatteryConservationEngine::new(20.0, false);
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
    assert!(
        detector
            .validate_tensor_allocation(256 * 1024 * 1024)
            .is_ok()
    );
    assert!(
        detector
            .validate_tensor_allocation(4 * 1024 * 1024 * 1024)
            .is_err()
    );

    let adapter = hakille_substrate_web::WebScreenCaptureAdapter::default();
    let rgba = vec![200u8; 50 * 50 * 4];
    let frame = adapter.ingest_image_data(50, 50, &rgba).unwrap();
    assert_eq!(frame.width, 50);
}
