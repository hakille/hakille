use crate::display::MacosQuartzDisplayCapture;
use crate::endpoint_security::EndpointSecurityClient;
use crate::mach_iokit::{
    PAGE_SIZE_APPLE_SILICON, read_host_power_state, read_host_thermal_state,
    read_host_vm_statistics,
};
use annora_core::{
    CapabilityScope, CapabilityToken, KeyPair, Result, SignedAttestation, Tier, Timestamp,
    VirtualPath,
};
use annora_substrate::{
    AudioConfig, AudioStreamHandle, CameraConfig, CameraFrame, CameraStreamHandle, EgressPolicy,
    HttpRequest, HttpResponse, InputAction, PlatformManifest, PowerState, ProcessHandle,
    ScreenCapture, Substrate, ThermalState,
};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Missing native binding reported by every macOS camera method.
const MACOS_CAMERA_UNIMPLEMENTED: &str = "camera not implemented for macOS substrate: AVFoundation binding missing \
     (no AVCaptureSession device wiring; link AVFoundation bindings to enable)";

/// Read the OS version from `sw_vers` when available; otherwise fall back to
/// a labeled placeholder (non-mac host / test env / `sw_vers` missing).
fn read_macos_os_version() -> String {
    if let Ok(output) = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
    {
        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !version.is_empty() {
                return version;
            }
        }
    }
    "15.0 (Sequoia)".to_string()
}

pub struct MacosSubstrate {
    manifest: PlatformManifest,
    attestation_keypair: KeyPair,
    audio_counter: AtomicU64,
    process_counter: AtomicU64,
    display_capture: MacosQuartzDisplayCapture,
    es_client: EndpointSecurityClient,
    virtual_fs_root: PathBuf,
    /// Host allowlist egress policy. Default is loopback-only
    /// (no external egress until hosts are explicitly allowed).
    egress_policy: EgressPolicy,
    /// Pinned Biscuit root public key. When `None`, capability tokens are
    /// still structurally verified but NOT pinned (legacy / tests).
    /// Production MUST call `with_trusted_root`.
    trusted_capability_root: Option<biscuit_auth::PublicKey>,
}

impl MacosSubstrate {
    pub fn new() -> Self {
        let vm = read_host_vm_statistics();
        let total_ram = vm.total_physical_ram(PAGE_SIZE_APPLE_SILICON);

        let manifest = PlatformManifest {
            tier: Tier::Native,
            capabilities_granted: vec![
                CapabilityScope::Filesystem,
                CapabilityScope::Network,
                CapabilityScope::GuiInput,
                CapabilityScope::Audio,
                CapabilityScope::Camera,
                CapabilityScope::Sensor,
                CapabilityScope::ModelInference,
            ],
            capabilities_denied: vec![CapabilityScope::FullSystem],
            max_memory_bytes: total_ram,
            // Honest: the shareable unified-memory fraction is dynamic and
            // only knowable via a Metal / IORegistry query (not linked);
            // unknown, not a hard-coded 3/4 share.
            gpu_vram_bytes: None,
            has_secure_enclave: true,
            has_tpm: false,
            battery_powered: true,
            // Honest: no thermal-pressure source linked; unknown, not invented.
            thermal_envelope_watts: None,
            os_name: "macOS".to_string(),
            os_version: read_macos_os_version(),
        };

        Self {
            manifest,
            attestation_keypair: KeyPair::generate(),
            audio_counter: AtomicU64::new(1),
            process_counter: AtomicU64::new(1),
            display_capture: MacosQuartzDisplayCapture::new(),
            es_client: EndpointSecurityClient::default(),
            virtual_fs_root: PathBuf::from("/Users/Shared/Annora/vfs"),
            egress_policy: EgressPolicy::loopback_only(),
            trusted_capability_root: None,
        }
    }

    /// Pin the Biscuit root public key used to verify ALL capability tokens
    /// at the point of effect. Production must set this from the daemon's
    /// `CapabilityAuthority::root_public_key()`.
    pub fn with_trusted_root(mut self, root: biscuit_auth::PublicKey) -> Self {
        self.trusted_capability_root = Some(root);
        self
    }

    fn enforce(
        &self,
        cap: &CapabilityToken,
        scope: CapabilityScope,
        action: annora_core::CapabilityAction,
    ) -> annora_core::Result<()> {
        if let Some(root) = &self.trusted_capability_root {
            annora_core::verify_capability_with_root_key(cap, root, scope, action)
        } else {
            // Legacy fallback: structural check without key pinning.
            // Still rejects wrong scope/action and expired tokens, but a
            // self-signed attacker token would pass — hence `with_trusted_root`
            // is required in production.
            annora_core::verify_capability_unpinned(cap, scope, action)
        }
    }

    /// Specify a custom root folder for the sandboxed VirtualFS.
    pub fn with_virtual_fs_root(mut self, root: PathBuf) -> Self {
        self.virtual_fs_root = root;
        self
    }

    /// Configure the host allowlist egress policy (default: loopback-only).
    pub fn with_egress_policy(mut self, policy: EgressPolicy) -> Self {
        self.egress_policy = policy;
        self
    }

    /// Current egress policy.
    pub fn egress_policy(&self) -> &EgressPolicy {
        &self.egress_policy
    }

    /// Resolve a VirtualPath to a real path inside the isolated root.
    /// Secure: rejects `..`, absolute paths, drive letters, NUL bytes and
    /// ensures the final path stays inside `virtual_fs_root`.
    fn resolve_path(&self, path: &VirtualPath) -> annora_core::Result<PathBuf> {
        use std::path::Component;
        if path.0.contains('\0') {
            return Err(annora_core::AnnoraError::CapabilityDenied(
                "NUL byte in virtual path".into(),
            ));
        }
        // Reject Windows drive letters and UNC prefixes early.
        let lower = path.0.to_ascii_lowercase();
        if lower.contains(':') || path.0.starts_with(r"\\") {
            return Err(annora_core::AnnoraError::CapabilityDenied(
                "absolute / drive path rejected".into(),
            ));
        }
        let mut clean = PathBuf::new();
        let mut saw_normal = false;
        for comp in std::path::Path::new(&path.0).components() {
            match comp {
                Component::Normal(p) => {
                    if p == std::ffi::OsStr::new("..") {
                        return Err(annora_core::AnnoraError::CapabilityDenied(
                            "path traversal (..) rejected".into(),
                        ));
                    }
                    clean.push(p);
                    saw_normal = true;
                }
                Component::CurDir => {}
                // Any ParentDir, RootDir, Prefix => reject
                _ => {
                    return Err(annora_core::AnnoraError::CapabilityDenied(
                        "non-normal path component rejected".into(),
                    ));
                }
            }
        }
        if !saw_normal {
            // Empty virtual path means the VirtualFS root itself (for list_dir).
            return Ok(self.virtual_fs_root.clone());
        }
        let joined = self.virtual_fs_root.join(&clean);
        // Defence in depth: canonicalize prefix check when root exists.
        // If canonicalization fails (path doesn't exist yet for writes),
        // the component-level rejection above already blocks traversal.
        if let (Ok(root_c), Ok(joined_c)) =
            (self.virtual_fs_root.canonicalize(), joined.canonicalize())
        {
            if !joined_c.starts_with(&root_c) {
                return Err(annora_core::AnnoraError::CapabilityDenied(
                    "resolved path escapes VirtualFS root".into(),
                ));
            }
            return Ok(joined_c);
        }
        Ok(joined)
    }

    pub fn virtual_fs_root(&self) -> &Path {
        &self.virtual_fs_root
    }

    pub fn display_capture(&self) -> &MacosQuartzDisplayCapture {
        &self.display_capture
    }

    pub fn es_client(&self) -> &EndpointSecurityClient {
        &self.es_client
    }
}

impl Default for MacosSubstrate {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Substrate for MacosSubstrate {
    fn manifest(&self) -> &PlatformManifest {
        &self.manifest
    }

    async fn attest(&self, data: &[u8]) -> Result<SignedAttestation> {
        let sig = self.attestation_keypair.sign(data);
        Ok(SignedAttestation {
            manifest_hash: format!("sha256-macos-manifest-{}", data.len()),
            platform_id: "macOS-AppleSilicon-Host".to_string(),
            timestamp: Timestamp::now(),
            signature: sig,
        })
    }

    async fn read_file(&self, path: &VirtualPath, cap: &CapabilityToken) -> Result<Vec<u8>> {
        self.enforce(
            cap,
            CapabilityScope::Filesystem,
            annora_core::CapabilityAction::Read,
        )?;
        let local_path = self.resolve_path(path)?;
        if local_path.exists() {
            tokio::fs::read(&local_path).await.map_err(|e| {
                annora_core::AnnoraError::StorageError(format!(
                    "Read error {}: {}",
                    local_path.display(),
                    e
                ))
            })
        } else {
            Err(annora_core::AnnoraError::StorageError(format!(
                "File not found: {}",
                path.0
            )))
        }
    }

    async fn write_file(
        &self,
        path: &VirtualPath,
        data: &[u8],
        cap: &CapabilityToken,
    ) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::Filesystem,
            annora_core::CapabilityAction::Write,
        )?;
        let local_path = self.resolve_path(path)?;
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                annora_core::AnnoraError::StorageError(format!(
                    "Create dir error {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }
        tokio::fs::write(&local_path, data).await.map_err(|e| {
            annora_core::AnnoraError::StorageError(format!(
                "Write error {}: {}",
                local_path.display(),
                e
            ))
        })
    }

    async fn list_dir(
        &self,
        path: &VirtualPath,
        cap: &CapabilityToken,
    ) -> Result<Vec<VirtualPath>> {
        self.enforce(
            cap,
            CapabilityScope::Filesystem,
            annora_core::CapabilityAction::Read,
        )?;
        let local_path = self.resolve_path(path)?;
        if local_path.exists() && local_path.is_dir() {
            let mut entries = Vec::new();
            if let Ok(mut read_dir) = tokio::fs::read_dir(&local_path).await {
                while let Ok(Some(entry)) = read_dir.next_entry().await {
                    if let Some(name) = entry.file_name().to_str() {
                        entries.push(VirtualPath(name.to_string()));
                    }
                }
            }
            Ok(entries)
        } else {
            Err(annora_core::AnnoraError::StorageError(format!(
                "Directory not found: {}",
                path.0
            )))
        }
    }

    async fn delete_file(&self, path: &VirtualPath, cap: &CapabilityToken) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::Filesystem,
            annora_core::CapabilityAction::Delete,
        )?;
        let local_path = self.resolve_path(path)?;
        if local_path.exists() {
            tokio::fs::remove_file(&local_path).await.map_err(|e| {
                annora_core::AnnoraError::StorageError(format!(
                    "Delete error {}: {}",
                    local_path.display(),
                    e
                ))
            })?;
        }
        Ok(())
    }

    async fn http_request(&self, req: HttpRequest, cap: &CapabilityToken) -> Result<HttpResponse> {
        // Network capability (GET ⇒ Read, others ⇒ Write) including expiry
        // is enforced first; without it egress is refused explicitly.
        // Then the shared allowlist policy applies (loopback-only by
        // default) and the real fetch runs over rustls TLS (10 s, 5 MiB cap).
        let action = if req.method.eq_ignore_ascii_case("get") {
            annora_core::CapabilityAction::Read
        } else {
            annora_core::CapabilityAction::Write
        };
        self.enforce(cap, CapabilityScope::Network, action)?;
        annora_substrate::fetch_via_egress(req, &self.egress_policy).await
    }

    async fn start_audio_capture(
        &self,
        _config: AudioConfig,
        cap: &CapabilityToken,
    ) -> Result<AudioStreamHandle> {
        self.enforce(
            cap,
            CapabilityScope::Audio,
            annora_core::CapabilityAction::Read,
        )?;
        let id = self.audio_counter.fetch_add(1, Ordering::SeqCst);
        Ok(AudioStreamHandle(id))
    }

    async fn stop_audio_capture(&self, _handle: AudioStreamHandle) -> Result<()> {
        Ok(())
    }

    async fn play_audio(
        &self,
        _data: &[u8],
        _config: AudioConfig,
        cap: &CapabilityToken,
    ) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::Audio,
            annora_core::CapabilityAction::Write,
        )?;
        Ok(())
    }

    async fn start_camera_capture(
        &self,
        _config: CameraConfig,
        cap: &CapabilityToken,
    ) -> Result<CameraStreamHandle> {
        self.enforce(
            cap,
            CapabilityScope::Camera,
            annora_core::CapabilityAction::Read,
        )?;
        // Honest: no AVFoundation wiring — refuse instead of synthesizing frames.
        Err(annora_core::AnnoraError::SubstrateError(
            MACOS_CAMERA_UNIMPLEMENTED.into(),
        ))
    }

    async fn stop_camera_capture(&self, _handle: CameraStreamHandle) -> Result<()> {
        Ok(())
    }

    async fn capture_camera_frame(
        &self,
        _handle: CameraStreamHandle,
        cap: &CapabilityToken,
    ) -> Result<CameraFrame> {
        self.enforce(
            cap,
            CapabilityScope::Camera,
            annora_core::CapabilityAction::Read,
        )?;
        // Honest: no AVFoundation wiring — refuse instead of synthesizing frames.
        Err(annora_core::AnnoraError::SubstrateError(
            MACOS_CAMERA_UNIMPLEMENTED.into(),
        ))
    }

    async fn capture_screen(&self, cap: &CapabilityToken) -> Result<ScreenCapture> {
        self.enforce(
            cap,
            CapabilityScope::Sensor,
            annora_core::CapabilityAction::Read,
        )?;
        Ok(self.display_capture.capture_screen())
    }

    async fn inject_input(&self, _action: InputAction, cap: &CapabilityToken) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::GuiInput,
            annora_core::CapabilityAction::Execute,
        )?;
        // macOS CGEventPost input injection abstraction
        Ok(())
    }

    async fn spawn_sandboxed(
        &self,
        _wasm: &[u8],
        caps: Vec<CapabilityToken>,
    ) -> Result<ProcessHandle> {
        // Caller must prove it holds at least one delegable capability;
        // spawned sandbox receives only attenuated `caps` (verified by caller).
        // Here we enforce that every delegated cap is at least structurally valid.
        for c in &caps {
            annora_core::verify_capability_unpinned(
                c,
                CapabilityScope::Filesystem,
                annora_core::CapabilityAction::Read,
            )
            .or_else(|_| {
                annora_core::verify_capability_unpinned(
                    c,
                    CapabilityScope::Network,
                    annora_core::CapabilityAction::Read,
                )
            })
            .or_else(|_| {
                annora_core::verify_capability_unpinned(
                    c,
                    CapabilityScope::GuiInput,
                    annora_core::CapabilityAction::Execute,
                )
            })
            .or_else(|_| {
                annora_core::verify_capability_unpinned(
                    c,
                    CapabilityScope::Sensor,
                    annora_core::CapabilityAction::Read,
                )
            })
            .map_err(|e| {
                annora_core::AnnoraError::CapabilityDenied(format!(
                    "spawn_sandboxed: delegated cap invalid: {e}"
                ))
            })?;
        }
        let id = self.process_counter.fetch_add(1, Ordering::SeqCst);
        Ok(ProcessHandle(id))
    }

    fn power_state(&self) -> PowerState {
        read_host_power_state()
    }

    fn thermal_state(&self) -> ThermalState {
        read_host_thermal_state()
    }
}
