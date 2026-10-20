use crate::web_audio::WebAudioBridge;
use crate::web_canvas::WebScreenCaptureAdapter;
use crate::webgpu::WebGpuDetector;
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
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Missing native binding reported by every web camera method.
const WEB_CAMERA_UNIMPLEMENTED: &str = "camera not implemented for web substrate: getUserMedia binding missing \
     (no navigator.mediaDevices wiring; connect the Web API bridge to enable)";

pub struct WebSubstrate {
    manifest: PlatformManifest,
    attestation_keypair: KeyPair,
    audio_counter: AtomicU64,
    process_counter: AtomicU64,
    audio_bridge: Mutex<WebAudioBridge>,
    screen_capture: WebScreenCaptureAdapter,
    gpu_detector: WebGpuDetector,
    /// In-memory stand-in for the Origin Private File System (OPFS).
    /// Absent keys are reported via `StorageError` — never simulated content.
    opfs_store: Mutex<HashMap<String, Vec<u8>>>,
    /// Host allowlist egress policy. Default is loopback-only
    /// (no external egress until hosts are explicitly allowed).
    /// In a real browser build this maps to `fetch()` under the same
    /// allowlist; on native test targets the shared reqwest fetch applies.
    egress_policy: EgressPolicy,
    /// Pinned Biscuit root public key. When `None`, capability tokens are
    /// still structurally verified but NOT pinned (legacy / tests).
    /// Production MUST call `with_trusted_root`.
    trusted_capability_root: Option<biscuit_auth::PublicKey>,
}

impl WebSubstrate {
    pub fn new() -> Self {
        let manifest = PlatformManifest {
            tier: Tier::Guest,
            capabilities_granted: vec![
                CapabilityScope::Audio,
                CapabilityScope::Camera,
                CapabilityScope::GuiInput,
            ],
            capabilities_denied: vec![
                CapabilityScope::FullSystem,
                CapabilityScope::Filesystem,
                CapabilityScope::ModelInference,
            ],
            // Honest: tab memory budget for the WASM sandbox (the live quota
            // comes from navigator.storage.estimate() — no browser binding
            // linked on native targets); labeled budget, not a measurement.
            max_memory_bytes: 4 * 1024 * 1024 * 1024,
            // Honest: VRAM is only knowable from the live WebGPU
            // adapter.limits (see WebGpuDetector at runtime); unknown here.
            gpu_vram_bytes: None,
            has_secure_enclave: false,
            has_tpm: false,
            battery_powered: false,
            thermal_envelope_watts: None,
            os_name: "WebAssembly".to_string(),
            os_version: "Wasm-Browser-W3C".to_string(),
        };

        Self {
            manifest,
            attestation_keypair: KeyPair::generate(),
            audio_counter: AtomicU64::new(1),
            process_counter: AtomicU64::new(1),
            audio_bridge: Mutex::new(WebAudioBridge::default()),
            screen_capture: WebScreenCaptureAdapter::default(),
            gpu_detector: WebGpuDetector::default(),
            opfs_store: Mutex::new(HashMap::new()),
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

    /// Resolve a VirtualPath to a logical OPFS key.
    /// Secure: rejects `..`, absolute paths, drive letters, NUL bytes and
    /// non-normal components (same rules as native substrates; there is no
    /// host path to canonicalize in the browser sandbox).
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
            // Empty virtual path means the OPFS root itself (for list_dir).
            return Ok(PathBuf::new());
        }
        Ok(clean)
    }

    /// Canonical OPFS key string for the store (`a/b/c` with `/` separators).
    fn opfs_key(resolved: &Path) -> String {
        resolved
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/")
    }

    pub fn audio_bridge(&self) -> &Mutex<WebAudioBridge> {
        &self.audio_bridge
    }

    pub fn screen_capture_adapter(&self) -> &WebScreenCaptureAdapter {
        &self.screen_capture
    }

    pub fn gpu_detector(&self) -> &WebGpuDetector {
        &self.gpu_detector
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
}

impl Default for WebSubstrate {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Substrate for WebSubstrate {
    fn manifest(&self) -> &PlatformManifest {
        &self.manifest
    }

    async fn attest(&self, data: &[u8]) -> Result<SignedAttestation> {
        let sig = self.attestation_keypair.sign(data);
        Ok(SignedAttestation {
            manifest_hash: format!("sha256-web-guest-{}", data.len()),
            platform_id: "Wasm-Browser-Client".to_string(),
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
        // In browser, reads from IndexedDB / Origin Private File System (OPFS).
        let resolved = self.resolve_path(path)?;
        let key = Self::opfs_key(&resolved);
        let store = self.opfs_store.lock().unwrap();
        store.get(&key).cloned().ok_or_else(|| {
            hakille_core::HakilleError::StorageError(format!("File not found: {}", path.0))
        })
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
        // Writes to OPFS.
        let resolved = self.resolve_path(path)?;
        let key = Self::opfs_key(&resolved);
        let mut store = self.opfs_store.lock().unwrap();
        store.insert(key, data.to_vec());
        Ok(())
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
        let resolved = self.resolve_path(path)?;
        let prefix = Self::opfs_key(&resolved);
        let store = self.opfs_store.lock().unwrap();
        let mut children: Vec<String> = Vec::new();
        for key in store.keys() {
            let rest = if prefix.is_empty() {
                key.as_str()
            } else {
                match key.strip_prefix(prefix.as_str()) {
                    Some(r) if r.starts_with('/') => &r[1..],
                    _ => continue,
                }
            };
            if rest.is_empty() {
                continue;
            }
            let child = rest.split('/').next().unwrap_or(rest).to_string();
            if !child.is_empty() && !children.contains(&child) {
                children.push(child);
            }
        }
        if children.is_empty() {
            return Err(hakille_core::HakilleError::StorageError(format!(
                "Directory not found: {}",
                path.0
            )));
        }
        children.sort();
        Ok(children.into_iter().map(VirtualPath).collect())
    }

    async fn delete_file(&self, path: &VirtualPath, cap: &CapabilityToken) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::Filesystem,
            hakille_core::CapabilityAction::Delete,
        )?;
        let resolved = self.resolve_path(path)?;
        let key = Self::opfs_key(&resolved);
        let mut store = self.opfs_store.lock().unwrap();
        store.remove(&key);
        Ok(())
    }

    async fn http_request(&self, req: HttpRequest, cap: &CapabilityToken) -> Result<HttpResponse> {
        // Network capability (GET ⇒ Read, others ⇒ Write) including expiry
        // is enforced first; without it egress is refused explicitly.
        // Then the shared allowlist policy applies (loopback-only by
        // default) and the real fetch runs — `fetch()` under the same
        // allowlist in a browser build, the shared reqwest fetch (rustls
        // TLS, 10 s, 5 MiB cap) on native targets.
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
        data: &[u8],
        _config: AudioConfig,
        cap: &CapabilityToken,
    ) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::Audio,
            hakille_core::CapabilityAction::Write,
        )?;
        let mut bridge = self.audio_bridge.lock().unwrap();
        bridge.buffer_mut().push_playback_audio(data);
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
        // Honest: no getUserMedia wiring — refuse instead of synthesizing frames.
        Err(hakille_core::HakilleError::SubstrateError(
            WEB_CAMERA_UNIMPLEMENTED.into(),
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
        // Honest: no getUserMedia wiring — refuse instead of synthesizing frames.
        Err(hakille_core::HakilleError::SubstrateError(
            WEB_CAMERA_UNIMPLEMENTED.into(),
        ))
    }

    async fn capture_screen(&self, cap: &CapabilityToken) -> Result<ScreenCapture> {
        self.enforce(
            cap,
            CapabilityScope::Sensor,
            hakille_core::CapabilityAction::Read,
        )?;
        Ok(self.screen_capture.capture_frame())
    }

    async fn inject_input(&self, _action: InputAction, cap: &CapabilityToken) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::GuiInput,
            hakille_core::CapabilityAction::Execute,
        )?;
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
        // Browser Battery Status API (`navigator.getBattery()`) is not
        // wired on native targets; this mains-powered default is a
        // documented placeholder, not a claimed reading.
        PowerState {
            on_battery: false,
            battery_percent: 100.0,
            charging: true,
        }
    }

    fn thermal_state(&self) -> ThermalState {
        // Honest: the Compute Pressure API is not wired; Unknown, not invented.
        ThermalState::Unknown
    }
}
