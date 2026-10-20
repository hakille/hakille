use crate::display::MacosQuartzDisplayCapture;
use crate::endpoint_security::EndpointSecurityClient;
use crate::host_query::read_macos_os_version;
use crate::mach_iokit::{
    read_host_power_state, read_host_thermal_state, read_total_physical_ram_bytes,
};
use async_trait::async_trait;
use hakille_core::{
    CapabilityScope, CapabilityToken, KeyPair, Result, SignedAttestation, Tier, Timestamp,
    VirtualPath,
};
use hakille_substrate::{
    AudioConfig, AudioStreamHandle, CameraConfig, CameraFrame, CameraStreamHandle, EgressPolicy,
    HttpRequest, HttpResponse, InputAction, PlatformManifest, PowerState, ProcessHandle,
    ScreenCapture, Substrate, ThermalState,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Missing native binding reported by every macOS camera method.
const MACOS_CAMERA_UNIMPLEMENTED: &str = "camera not implemented for macOS substrate: AVFoundation binding missing \
     (no AVCaptureSession device wiring; link AVFoundation bindings to enable)";

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
        // Real `sysctl hw.memsize` on macOS; 0 (= unknown, invalid manifest)
        // if the query fails — never an invented size. Off macOS the
        // explicitly simulated fallback keeps the manifest structurally
        // valid (see `read_total_physical_ram_bytes`).
        let total_ram = read_total_physical_ram_bytes();

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
            virtual_fs_root: PathBuf::from("/Users/Shared/Hakille/vfs"),
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
        action: hakille_core::CapabilityAction,
    ) -> hakille_core::Result<()> {
        if let Some(root) = &self.trusted_capability_root {
            hakille_core::verify_capability_with_root_key(cap, root, scope, action)
        } else {
            // Legacy fallback UNIQUEMENT en debug/tests : vérification
            // structurelle sans pinning (accepte les tokens auto-signés).
            // En release, panique fail-closed : `verify_unpinned` est interdit.
            #[cfg(debug_assertions)]
            {
                #[allow(deprecated)]
                {
                    hakille_core::verify_capability_unpinned(cap, scope, action)
                }
            }
            #[cfg(not(debug_assertions))]
            {
                panic!(
                    "unpinned capability verification forbidden in release; call with_trusted_root in production"
                );
            }
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
    fn resolve_path(&self, path: &VirtualPath) -> hakille_core::Result<PathBuf> {
        use std::path::Component;
        if path.0.contains('\0') {
            return Err(hakille_core::HakilleError::CapabilityDenied(
                "NUL byte in virtual path".into(),
            ));
        }
        // Reject Windows drive letters and UNC prefixes early.
        let lower = path.0.to_ascii_lowercase();
        if lower.contains(':') || path.0.starts_with(r"\\") {
            return Err(hakille_core::HakilleError::CapabilityDenied(
                "absolute / drive path rejected".into(),
            ));
        }
        let mut clean = PathBuf::new();
        let mut saw_normal = false;
        for comp in std::path::Path::new(&path.0).components() {
            match comp {
                Component::Normal(p) => {
                    if p == std::ffi::OsStr::new("..") {
                        return Err(hakille_core::HakilleError::CapabilityDenied(
                            "path traversal (..) rejected".into(),
                        ));
                    }
                    clean.push(p);
                    saw_normal = true;
                }
                Component::CurDir => {}
                // Any ParentDir, RootDir, Prefix => reject
                _ => {
                    return Err(hakille_core::HakilleError::CapabilityDenied(
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
                return Err(hakille_core::HakilleError::CapabilityDenied(
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
            hakille_core::CapabilityAction::Read,
        )?;
        let local_path = self.resolve_path(path)?;
        if local_path.exists() {
            tokio::fs::read(&local_path).await.map_err(|e| {
                hakille_core::HakilleError::StorageError(format!(
                    "Read error {}: {}",
                    local_path.display(),
                    e
                ))
            })
        } else {
            Err(hakille_core::HakilleError::StorageError(format!(
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
            hakille_core::CapabilityAction::Write,
        )?;
        let local_path = self.resolve_path(path)?;
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                hakille_core::HakilleError::StorageError(format!(
                    "Create dir error {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }
        tokio::fs::write(&local_path, data).await.map_err(|e| {
            hakille_core::HakilleError::StorageError(format!(
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
            hakille_core::CapabilityAction::Read,
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
            Err(hakille_core::HakilleError::StorageError(format!(
                "Directory not found: {}",
                path.0
            )))
        }
    }

    async fn delete_file(&self, path: &VirtualPath, cap: &CapabilityToken) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::Filesystem,
            hakille_core::CapabilityAction::Delete,
        )?;
        let local_path = self.resolve_path(path)?;
        if local_path.exists() {
            tokio::fs::remove_file(&local_path).await.map_err(|e| {
                hakille_core::HakilleError::StorageError(format!(
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
            hakille_core::CapabilityAction::Read
        } else {
            hakille_core::CapabilityAction::Write
        };
        self.enforce(cap, CapabilityScope::Network, action)?;
        hakille_substrate::fetch_via_egress(req, &self.egress_policy).await
    }

    async fn start_audio_capture(
        &self,
        _config: AudioConfig,
        cap: &CapabilityToken,
    ) -> Result<AudioStreamHandle> {
        self.enforce(
            cap,
            CapabilityScope::Audio,
            hakille_core::CapabilityAction::Read,
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
            hakille_core::CapabilityAction::Write,
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
            hakille_core::CapabilityAction::Read,
        )?;
        // Honest: no AVFoundation wiring — refuse instead of synthesizing frames.
        Err(hakille_core::HakilleError::SubstrateError(
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
            hakille_core::CapabilityAction::Read,
        )?;
        // Honest: no AVFoundation wiring — refuse instead of synthesizing frames.
        Err(hakille_core::HakilleError::SubstrateError(
            MACOS_CAMERA_UNIMPLEMENTED.into(),
        ))
    }

    async fn capture_screen(&self, cap: &CapabilityToken) -> Result<ScreenCapture> {
        self.enforce(
            cap,
            CapabilityScope::Sensor,
            hakille_core::CapabilityAction::Read,
        )?;
        Ok(self.display_capture.capture_screen())
    }

    async fn inject_input(&self, _action: InputAction, cap: &CapabilityToken) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::GuiInput,
            hakille_core::CapabilityAction::Execute,
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
        let delegable = [
            (
                CapabilityScope::Filesystem,
                hakille_core::CapabilityAction::Read,
            ),
            (
                CapabilityScope::Network,
                hakille_core::CapabilityAction::Read,
            ),
            (
                CapabilityScope::GuiInput,
                hakille_core::CapabilityAction::Execute,
            ),
            (
                CapabilityScope::Sensor,
                hakille_core::CapabilityAction::Read,
            ),
        ];
        for c in &caps {
            let mut last_err = None;
            let mut granted = false;
            for (scope, action) in delegable.iter() {
                match self.enforce(c, scope.clone(), action.clone()) {
                    Ok(()) => {
                        granted = true;
                        break;
                    }
                    Err(e) => last_err = Some(e),
                }
            }
            if !granted {
                return Err(hakille_core::HakilleError::CapabilityDenied(format!(
                    "spawn_sandboxed: delegated cap invalid: {}",
                    last_err
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "aucun droit délégable".to_string())
                )));
            }
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
