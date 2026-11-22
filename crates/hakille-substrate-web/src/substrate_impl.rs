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

/// Missing native binding reported by every web camera method.
const WEB_CAMERA_UNIMPLEMENTED: &str = "camera not implemented for web substrate: getUserMedia binding missing \
     (no navigator.mediaDevices wiring; connect the Web API bridge to enable)";

pub struct WebSubstrate {
    manifest: PlatformManifest,
    attestation_keypair: KeyPair,
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

/// Plafonds mémoire anti-DoS du magasin OPFS en mémoire.
const MAX_OPFS_FILES: usize = 1024;
const MAX_OPFS_TOTAL_BYTES: usize = 16 * 1024 * 1024;

impl WebSubstrate {
    pub fn new() -> Self {
        let manifest = PlatformManifest {
            tier: Tier::Guest,
            capabilities_granted: vec![
                CapabilityScope::Filesystem,
                CapabilityScope::Network,
                CapabilityScope::Audio,
                CapabilityScope::Camera,
                CapabilityScope::GuiInput,
            ],
            capabilities_denied: vec![CapabilityScope::FullSystem, CapabilityScope::ModelInference],
            // Honest: tab memory budget for the WASM sandbox (the live quota
            // comes from navigator.storage.estimate() — no browser binding
            // linked on native targets); labeled budget, not a measurement.
            max_memory_bytes: Some(4 * 1024 * 1024 * 1024),
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
            // Repli legacy UNIQUEMENT en debug/tests ; en release, refus fail-closed
            // propre (`Err`) sans jamais paniquer le processus.
            #[cfg(debug_assertions)]
            {
                #[allow(deprecated)]
                {
                    hakille_core::verify_capability_unpinned(cap, scope, action)
                }
            }
            #[cfg(not(debug_assertions))]
            {
                Err(hakille_core::HakilleError::CapabilityDenied(
                    "vérification non épinglée interdite en release : appeler with_trusted_root en production".into(),
                ))
            }
        }
    }

    /// Resolve a VirtualPath to a logical OPFS key.
    fn resolve_path(&self, path: &VirtualPath) -> hakille_core::Result<PathBuf> {
        let segments = hakille_substrate::validate_virtual_components(path)?;
        let mut clean = PathBuf::new();
        for seg in segments {
            clean.push(seg);
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
        // Honnête : dev-attestation locale (clé éphémère, aucune enclave navigateur),
        // empreinte SHA-256 réelle des octets attestés.
        Ok(SignedAttestation {
            manifest_hash: format!("sha256-{}", hakille_substrate::sha256_hex(data)),
            platform_id: "Wasm-Browser-Client-dev-attestation".to_string(),
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
        let resolved = self.resolve_path(path)?;
        let key = Self::opfs_key(&resolved);
        let store = self.opfs_store.lock().map_err(|_| {
            hakille_core::HakilleError::StorageError("verrou OPFS empoisonné".into())
        })?;
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
        let resolved = self.resolve_path(path)?;
        let key = Self::opfs_key(&resolved);
        let mut store = self.opfs_store.lock().map_err(|_| {
            hakille_core::HakilleError::StorageError("verrou OPFS empoisonné".into())
        })?;
        let is_new = !store.contains_key(&key);
        if is_new && store.len() >= MAX_OPFS_FILES {
            return Err(hakille_core::HakilleError::StorageError(format!(
                "quota OPFS atteint ({MAX_OPFS_FILES} fichiers)"
            )));
        }
        let existing_len = store.get(&key).map_or(0, Vec::len);
        let current_total: usize = store.values().map(Vec::len).sum();
        let next_total = current_total
            .saturating_sub(existing_len)
            .saturating_add(data.len());
        if next_total > MAX_OPFS_TOTAL_BYTES {
            return Err(hakille_core::HakilleError::StorageError(format!(
                "quota mémoire OPFS dépassé ({next_total} > {MAX_OPFS_TOTAL_BYTES} octets)"
            )));
        }
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
        let store = self.opfs_store.lock().map_err(|_| {
            hakille_core::HakilleError::StorageError("verrou OPFS empoisonné".into())
        })?;
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
        let mut store = self.opfs_store.lock().map_err(|_| {
            hakille_core::HakilleError::StorageError("verrou OPFS empoisonné".into())
        })?;
        store.remove(&key);
        Ok(())
    }

    async fn http_request(&self, req: HttpRequest, cap: &CapabilityToken) -> Result<HttpResponse> {
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
        Err(hakille_core::HakilleError::SubstrateError(
            "capture audio indisponible : backend audio non raccordé au substrat web; aucun flux démarré".to_string(),
        ))
    }

    async fn stop_audio_capture(&self, _handle: AudioStreamHandle) -> Result<()> {
        Err(hakille_core::HakilleError::SubstrateError(
            "arrêt audio indisponible : aucun flux audio natif géré par ce substrat".to_string(),
        ))
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
        Err(hakille_core::HakilleError::SubstrateError(
            "lecture audio indisponible : backend audio non raccordé au substrat web; aucun son joué".to_string(),
        ))
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
        Err(hakille_core::HakilleError::SubstrateError(
            WEB_CAMERA_UNIMPLEMENTED.into(),
        ))
    }

    async fn stop_camera_capture(&self, _handle: CameraStreamHandle) -> Result<()> {
        Err(hakille_core::HakilleError::SubstrateError(
            WEB_CAMERA_UNIMPLEMENTED.into(),
        ))
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
        Err(hakille_core::HakilleError::SubstrateError(
            "capture d'écran indisponible : binding getDisplayMedia non raccordé; aucune image du poste capturée".to_string(),
        ))
    }

    async fn inject_input(&self, _action: InputAction, cap: &CapabilityToken) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::GuiInput,
            hakille_core::CapabilityAction::Execute,
        )?;
        Err(hakille_core::HakilleError::SubstrateError(
            "injection d'entrée indisponible : binding événements navigateur non raccordé; aucun événement émis".to_string(),
        ))
    }

    async fn spawn_sandboxed(
        &self,
        _wasm: &[u8],
        caps: Vec<CapabilityToken>,
    ) -> Result<ProcessHandle> {
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
        Err(hakille_core::HakilleError::SubstrateError(
            "spawn_sandboxed indisponible : moteur WASM non raccordé au substrat web; aucun processus lancé"
                .to_string(),
        ))
    }

    fn power_state(&self) -> PowerState {
        PowerState {
            on_battery: false,
            battery_percent: 100.0,
            charging: true,
        }
    }

    fn thermal_state(&self) -> ThermalState {
        ThermalState::Unknown
    }

    async fn send_notification(
        &self,
        _request: hakille_substrate::NotificationRequest,
        cap: &CapabilityToken,
    ) -> Result<hakille_substrate::NotificationHandle> {
        self.enforce(
            cap,
            CapabilityScope::Notification,
            hakille_core::CapabilityAction::Write,
        )?;
        // HONNÊTE : aucun binding JS `Notification` (web-sys) n'est raccordé.
        Err(hakille_core::HakilleError::SubstrateError(
            "notification Web indisponible : binding HTML5 Notification API non raccordé ; aucune notification affichée".to_string(),
        ))
    }

    async fn dismiss_notification(
        &self,
        _handle: &hakille_substrate::NotificationHandle,
        cap: &CapabilityToken,
    ) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::Notification,
            hakille_core::CapabilityAction::Delete,
        )?;
        Err(hakille_core::HakilleError::SubstrateError(
            "fermeture de notification Web indisponible : binding HTML5 Notification API non raccordé".to_string(),
        ))
    }
}
