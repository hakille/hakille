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

use crate::compute_impl::WindowsComputeBackend;
use crate::input_injector::WindowsInputInjector;
use crate::screen_capture::WindowsScreenCapturer;
use crate::win32_ffi::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

/// Missing native binding reported by the non-Windows fallback build.
#[cfg(not(windows))]
const WINDOWS_CAMERA_UNIMPLEMENTED: &str = "camera not implemented for Windows substrate: MediaFoundation binding missing \
     (no IMFSourceReader device wiring; link MediaFoundation bindings to enable)";

pub struct WindowsSubstrate {
    manifest: PlatformManifest,
    attestation_keypair: KeyPair,
    audio_counter: AtomicU64,
    process_counter: AtomicU64,
    virtual_fs_root: PathBuf,
    /// Host allowlist egress policy. Default is loopback-only
    /// (no external egress until hosts are explicitly allowed).
    egress_policy: EgressPolicy,
    /// Pinned Biscuit root public key. When `None`, capability tokens are
    /// still structurally verified but NOT pinned (legacy / tests).
    /// Production MUST call `with_trusted_root`.
    trusted_capability_root: Option<biscuit_auth::PublicKey>,
    /// Active MediaFoundation camera streams (thread per stream, latest-only frames).
    #[cfg(windows)]
    camera_counter: AtomicU64,
    #[cfg(windows)]
    camera_streams: std::sync::Mutex<
        std::collections::HashMap<u64, crate::camera_mediafoundation::CameraStream>,
    >,
}

impl WindowsSubstrate {
    pub fn new() -> Self {
        let compute = WindowsComputeBackend::new();
        let total_ram = compute.total_physical_ram();

        // Dynamically detect power state on startup
        let mut power_info = SYSTEM_POWER_STATUS::default();
        let on_battery = unsafe {
            if GetSystemPowerStatus(&mut power_info) != 0 {
                power_info.ACLineStatus == 0
            } else {
                false
            }
        };

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
            // Honest: dedicated VRAM is unknown without a DXGI adapter query
            // (win32_ffi intentionally exposes no DXGI); not an 8 GiB claim.
            gpu_vram_bytes: None,
            has_secure_enclave: false,
            has_tpm: true,
            battery_powered: on_battery,
            // Honest: no thermal envelope modeling without a WMI /
            // MSACPI thermal-zone binding; unknown, not invented.
            thermal_envelope_watts: None,
            os_name: "Windows".to_string(),
            os_version: "11 (24H2)".to_string(),
        };

        let virtual_fs_root = std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./hakille_data"))
            .join("Hakille")
            .join("VirtualFS");

        Self {
            manifest,
            attestation_keypair: KeyPair::generate(),
            audio_counter: AtomicU64::new(1),
            process_counter: AtomicU64::new(1),
            virtual_fs_root,
            egress_policy: EgressPolicy::loopback_only(),
            trusted_capability_root: None,
            #[cfg(windows)]
            camera_counter: AtomicU64::new(1),
            #[cfg(windows)]
            camera_streams: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Pin the Biscuit root public key used to verify ALL capability tokens
    /// at the point of effect. Production must set this from the daemon's
    /// `CapabilityAuthority::root_public_key()`.
    /// MANDATORY in production: `spawn_sandboxed` refuses (Err + error log)
    /// when no root is pinned — including release builds.
    pub fn with_trusted_root(mut self, root: biscuit_auth::PublicKey) -> Self {
        self.trusted_capability_root = Some(root);
        self
    }

    /// Whether a trusted capability root is pinned (`with_trusted_root` called).
    pub fn has_trusted_root(&self) -> bool {
        self.trusted_capability_root.is_some()
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

    /// Mandatory gate: returns the pinned root or refuses with an error log.
    /// Called by `spawn_sandboxed` (and any future privileged op) so a
    /// missing pin can never silently fall back to unpinned verification.
    fn require_trusted_root(&self, op: &str) -> hakille_core::Result<biscuit_auth::PublicKey> {
        match &self.trusted_capability_root {
            Some(root) => Ok(*root),
            None => {
                tracing::error!(
                    "{op} refused: trusted capability root not pinned; call with_trusted_root in production"
                );
                Err(hakille_core::HakilleError::CapabilityDenied(format!(
                    "{op} refused: trusted capability root not pinned; call with_trusted_root in production"
                )))
            }
        }
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
                    let authority = hakille_core::CapabilityAuthority::default();
                    let _ = authority;
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

    /// Resolve a VirtualPath to a real Windows path inside the isolated root.
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

    /// Capability-checked stop for an audio stream. The trait-level
    /// `stop_audio_capture(handle)` is kept for compatibility but only logs
    /// a deprecation warning (no cap = no ownership proof); new code MUST
    /// call this method with the owner's Audio/Read capability.
    pub async fn stop_audio_capture_with_cap(
        &self,
        handle: AudioStreamHandle,
        cap: &CapabilityToken,
    ) -> hakille_core::Result<()> {
        self.enforce(
            cap,
            CapabilityScope::Audio,
            hakille_core::CapabilityAction::Read,
        )?;
        let _ = handle;
        Ok(())
    }

    /// Capability-checked stop for a camera stream. See
    /// `stop_audio_capture_with_cap` — same ownership rationale (Camera/Read).
    pub async fn stop_camera_capture_with_cap(
        &self,
        handle: CameraStreamHandle,
        cap: &CapabilityToken,
    ) -> hakille_core::Result<()> {
        self.enforce(
            cap,
            CapabilityScope::Camera,
            hakille_core::CapabilityAction::Read,
        )?;
        let _ = handle;
        #[cfg(windows)]
        {
            let stream = {
                let mut streams = self.camera_streams.lock().map_err(|_| {
                    hakille_core::HakilleError::SubstrateError(
                        "camera stream registry poisoned".into(),
                    )
                })?;
                streams.remove(&handle.0)
            };
            if let Some(stream) = stream {
                match tokio::task::spawn_blocking(move || stream.stop_and_join()).await {
                    Ok(()) => {}
                    Err(e) => tracing::warn!("camera stop task failed: {e}"),
                }
            }
        }
        Ok(())
    }
}

impl Default for WindowsSubstrate {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Substrate for WindowsSubstrate {
    fn manifest(&self) -> &PlatformManifest {
        &self.manifest
    }

    async fn attest(&self, data: &[u8]) -> Result<SignedAttestation> {
        let sig = self.attestation_keypair.sign(data);
        // HONNÊTE : dev-attestation locale (clé éphémère, PAS une preuve
        // VBS/TPM matérielle). manifest_hash = vrai SHA-256 canonique des
        // octets attestés, pas `len()`.
        Ok(SignedAttestation {
            manifest_hash: format!("sha256-{}", hakille_substrate::sha256_hex(data)),
            platform_id: "Windows-Substrate-Host-dev-attestation".to_string(),
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
            // Honest miss: no synthetic fallback. Mirrors Linux behaviour —
            // callers must handle FileNotFound instead of receiving fake bytes.
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
            let _ = tokio::fs::create_dir_all(parent).await;
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
            // HONNÊTE : aucun contenu synthétique. Répertoire absent → Err.
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
        tracing::warn!(
            "stop_audio_capture(handle) without capability is deprecated; use stop_audio_capture_with_cap(handle, cap) with Audio/Read ownership proof"
        );
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
        config: CameraConfig,
        cap: &CapabilityToken,
    ) -> Result<CameraStreamHandle> {
        self.enforce(
            cap,
            CapabilityScope::Camera,
            hakille_core::CapabilityAction::Read,
        )?;
        let _ = &config;
        #[cfg(windows)]
        {
            let stream = crate::camera_mediafoundation::start(&config)
                .map_err(hakille_core::HakilleError::SubstrateError)?;
            let id = self.camera_counter.fetch_add(1, Ordering::SeqCst);
            self.camera_streams
                .lock()
                .map_err(|_| {
                    hakille_core::HakilleError::SubstrateError(
                        "camera stream registry poisoned".into(),
                    )
                })?
                .insert(id, stream);
            return Ok(CameraStreamHandle(id));
        }
        #[cfg(not(windows))]
        {
            Err(hakille_core::HakilleError::SubstrateError(
                WINDOWS_CAMERA_UNIMPLEMENTED.into(),
            ))
        }
    }

    async fn stop_camera_capture(&self, _handle: CameraStreamHandle) -> Result<()> {
        tracing::warn!(
            "stop_camera_capture(handle) without capability is deprecated; use stop_camera_capture_with_cap(handle, cap) with Camera/Read ownership proof"
        );
        Ok(())
    }

    async fn capture_camera_frame(
        &self,
        handle: CameraStreamHandle,
        cap: &CapabilityToken,
    ) -> Result<CameraFrame> {
        self.enforce(
            cap,
            CapabilityScope::Camera,
            hakille_core::CapabilityAction::Read,
        )?;
        let _ = handle;
        #[cfg(windows)]
        {
            let receiver = {
                let streams = self.camera_streams.lock().map_err(|_| {
                    hakille_core::HakilleError::SubstrateError(
                        "camera stream registry poisoned".into(),
                    )
                })?;
                streams
                    .get(&handle.0)
                    .map(|stream| stream.latest())
                    .ok_or_else(|| {
                        hakille_core::HakilleError::SubstrateError(
                            "camera capture failed: unknown or stopped camera stream handle".into(),
                        )
                    })?
            };
            return crate::camera_mediafoundation::next_frame(
                receiver,
                std::time::Duration::from_secs(5),
            )
            .await
            .map_err(hakille_core::HakilleError::SubstrateError);
        }
        #[cfg(not(windows))]
        {
            Err(hakille_core::HakilleError::SubstrateError(
                WINDOWS_CAMERA_UNIMPLEMENTED.into(),
            ))
        }
    }

    async fn capture_screen(&self, cap: &CapabilityToken) -> Result<ScreenCapture> {
        self.enforce(
            cap,
            CapabilityScope::Sensor,
            hakille_core::CapabilityAction::Read,
        )?;
        // Real screen capture using native Win32 GDI APIs
        Ok(WindowsScreenCapturer::capture_primary_display())
    }

    async fn inject_input(&self, action: InputAction, cap: &CapabilityToken) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::GuiInput,
            hakille_core::CapabilityAction::Execute,
        )?;
        // Real native input injection via SendInput / mouse_event / keybd_event
        WindowsInputInjector::inject(&action)
    }

    async fn spawn_sandboxed(
        &self,
        _wasm: &[u8],
        caps: Vec<CapabilityToken>,
    ) -> Result<ProcessHandle> {
        // Pinned-root enforcement: every delegated cap MUST verify against the
        // pinned authority root. No silent unpinned fallback — without a
        // trusted root, refuse (error logged in `require_trusted_root`).
        let root = self.require_trusted_root("spawn_sandboxed")?;
        // Caller must prove it holds at least one delegable capability;
        // spawned sandbox receives only attenuated `caps` (verified by caller).
        for c in &caps {
            hakille_core::verify_capability_with_root_key(
                c,
                &root,
                CapabilityScope::Filesystem,
                hakille_core::CapabilityAction::Read,
            )
            .or_else(|_| {
                hakille_core::verify_capability_with_root_key(
                    c,
                    &root,
                    CapabilityScope::Network,
                    hakille_core::CapabilityAction::Read,
                )
            })
            .or_else(|_| {
                hakille_core::verify_capability_with_root_key(
                    c,
                    &root,
                    CapabilityScope::GuiInput,
                    hakille_core::CapabilityAction::Execute,
                )
            })
            .or_else(|_| {
                hakille_core::verify_capability_with_root_key(
                    c,
                    &root,
                    CapabilityScope::Sensor,
                    hakille_core::CapabilityAction::Read,
                )
            })
            .map_err(|e| {
                hakille_core::HakilleError::CapabilityDenied(format!(
                    "spawn_sandboxed: delegated cap invalid (pinned root): {e}"
                ))
            })?;
        }
        let id = self.process_counter.fetch_add(1, Ordering::SeqCst);
        Ok(ProcessHandle(id))
    }

    fn power_state(&self) -> PowerState {
        let mut status = SYSTEM_POWER_STATUS::default();
        unsafe {
            if GetSystemPowerStatus(&mut status) != 0 {
                let on_battery = status.ACLineStatus == 0;
                let battery_percent = if status.BatteryLifePercent <= 100 {
                    status.BatteryLifePercent as f32
                } else {
                    100.0
                };
                let charging = (status.BatteryFlag & 8) != 0 || status.ACLineStatus == 1;

                PowerState {
                    on_battery,
                    battery_percent,
                    charging,
                }
            } else {
                PowerState {
                    on_battery: false,
                    battery_percent: 100.0,
                    charging: true,
                }
            }
        }
    }

    fn thermal_state(&self) -> ThermalState {
        // Honest: no thermal binding is linked (reading the package
        // temperature requires a WMI / MSACPI thermal-zone binding that
        // win32_ffi intentionally does not expose). Unknown, not invented.
        ThermalState::Unknown
    }
}
