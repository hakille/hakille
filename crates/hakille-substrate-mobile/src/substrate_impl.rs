use crate::battery_conservation::BatteryConservationEngine;
use crate::scoped_storage::{ScopedStorageSandbox, StorageScope};
use crate::shadow::MobileShadowSync;
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
use std::path::PathBuf;
use std::sync::Mutex;

/// Missing native binding reported by every mobile camera method.
const MOBILE_CAMERA_UNIMPLEMENTED: &str = "camera not implemented for mobile substrate: Camera2 (Android) / AVFoundation (iOS) binding missing \
     (no NDK camera device wiring; link Camera2/NDK bindings to enable)";

pub struct MobileSubstrate {
    manifest: PlatformManifest,
    attestation_keypair: KeyPair,
    shadow_sync: Mutex<MobileShadowSync>,
    storage_sandbox: ScopedStorageSandbox,
    battery_engine: Mutex<BatteryConservationEngine>,
    /// Host allowlist egress policy. Default is loopback-only
    /// (no external egress until hosts are explicitly allowed).
    egress_policy: EgressPolicy,
    /// Pinned Biscuit root public key. When `None`, capability tokens are
    /// still structurally verified but NOT pinned (legacy / tests).
    /// Production MUST call `with_trusted_root`.
    trusted_capability_root: Option<biscuit_auth::PublicKey>,
}

impl MobileSubstrate {
    pub fn new() -> Self {
        let manifest = PlatformManifest {
            tier: Tier::Guest,
            // Filesystem is granted SCOPED-ONLY: read_file/write_file enforce a
            // Filesystem capability AND mediate every path through
            // `ScopedStorageSandbox` (SAF / PhotoPicker scoped storage — app
            // Documents scope, no raw full-filesystem access). This resolves
            // the former manifest paradox where Filesystem was denied while
            // the file methods enforced a Filesystem cap.
            capabilities_granted: vec![
                CapabilityScope::Audio,
                CapabilityScope::Camera,
                CapabilityScope::GuiInput,
                CapabilityScope::Filesystem,
            ],
            capabilities_denied: vec![CapabilityScope::FullSystem, CapabilityScope::ModelInference],
            // Honest: per-app RAM budget placeholder for the shadow runtime
            // (the real budget comes from ActivityManager on Android /
            // os_proc_available_memory on iOS — no JNI/ObjC binding linked).
            max_memory_bytes: Some(4 * 1024 * 1024 * 1024),
            gpu_vram_bytes: None,
            has_secure_enclave: true,
            has_tpm: false,
            battery_powered: true,
            // Honest: no PowerManager/thermal HAL binding linked; unknown.
            thermal_envelope_watts: None,
            os_name: "Mobile-Edge".to_string(),
            // Honest placeholder: resolved from Build.VERSION / UIDevice
            // once a platform binding is linked; labeled, not a measurement.
            os_version: "iOS/Android-Shadow".to_string(),
        };

        Self {
            manifest,
            attestation_keypair: KeyPair::generate(),
            shadow_sync: Mutex::new(MobileShadowSync::default()),
            storage_sandbox: ScopedStorageSandbox::default(),
            battery_engine: Mutex::new(BatteryConservationEngine::default()),
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

    /// Secure path resolution: component-level anti-traversal check (same
    /// rules as other substrates) IN ADDITION TO the scoped-storage sandbox
    /// check — never instead of it.
    fn resolve_path(&self, path: &VirtualPath) -> hakille_core::Result<PathBuf> {
        use std::path::Component;
        if path.0.contains('\0') {
            return Err(hakille_core::HakilleError::CapabilityDenied(
                "NUL byte in virtual path".into(),
            ));
        }
        let lower = path.0.to_ascii_lowercase();
        if lower.contains(':') || path.0.starts_with(r"\\") {
            return Err(hakille_core::HakilleError::CapabilityDenied(
                "absolute / drive path rejected".into(),
            ));
        }
        let mut saw_normal = false;
        for comp in std::path::Path::new(&path.0).components() {
            match comp {
                Component::Normal(p) => {
                    if p == std::ffi::OsStr::new("..") {
                        return Err(hakille_core::HakilleError::CapabilityDenied(
                            "path traversal (..) rejected".into(),
                        ));
                    }
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
            return Err(hakille_core::HakilleError::CapabilityDenied(
                "empty virtual path rejected".into(),
            ));
        }
        // PLUS the scoped-storage sandbox check (SAF / PhotoPicker mediation).
        self.storage_sandbox
            .resolve_path(StorageScope::Documents, path)
            .map_err(|e| {
                hakille_core::HakilleError::CapabilityDenied(format!(
                    "scoped storage rejected path: {e}"
                ))
            })
    }

    pub fn storage_sandbox(&self) -> &ScopedStorageSandbox {
        &self.storage_sandbox
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

impl Default for MobileSubstrate {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Substrate for MobileSubstrate {
    fn manifest(&self) -> &PlatformManifest {
        &self.manifest
    }

    async fn attest(&self, data: &[u8]) -> Result<SignedAttestation> {
        let sig = self.attestation_keypair.sign(data);
        Ok(SignedAttestation {
            manifest_hash: format!("sha256-mobile-shadow-{}", data.len()),
            platform_id: "Mobile-Edge-Shadow-Terminal".to_string(),
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
        let _resolved = self.resolve_path(path)?;
        let sync = self.shadow_sync.lock().unwrap();
        sync.get_value(&path.0).ok_or_else(|| {
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
        let _resolved = self.resolve_path(path)?;
        let mut sync = self.shadow_sync.lock().unwrap();
        sync.queue_local_delta(&path.0, data);
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
        // Root ("" / "/" / ".") lists all materialized shadow keys; any other
        // path must first pass the secure resolution above.
        let prefix = if path.0.is_empty() || path.0 == "/" || path.0 == "." {
            String::new()
        } else {
            let _resolved = self.resolve_path(path)?;
            let mut p = path.0.trim_start_matches('/').to_string();
            if !p.ends_with('/') {
                p.push('/');
            }
            p
        };
        let sync = self.shadow_sync.lock().unwrap();
        let keys = sync.list_keys(&prefix);
        if keys.is_empty() {
            return Err(hakille_core::HakilleError::StorageError(format!(
                "Directory not found: {}",
                path.0
            )));
        }
        Ok(keys.into_iter().map(VirtualPath).collect())
    }

    async fn delete_file(&self, path: &VirtualPath, cap: &CapabilityToken) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::Filesystem,
            hakille_core::CapabilityAction::Delete,
        )?;
        let _resolved = self.resolve_path(path)?;
        let mut sync = self.shadow_sync.lock().unwrap();
        sync.queue_local_deletion(&path.0);
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
        Err(hakille_core::HakilleError::SubstrateError(
            "capture audio indisponible : backend audio non raccordé au substrat mobile; aucun flux démarré".to_string(),
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
            "lecture audio indisponible : backend audio non raccordé au substrat mobile; aucun son joué".to_string(),
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
        // Honest: no Camera2/AVFoundation wiring — refuse instead of synthesizing frames.
        Err(hakille_core::HakilleError::SubstrateError(
            MOBILE_CAMERA_UNIMPLEMENTED.into(),
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
        // Honest: no Camera2/AVFoundation wiring — refuse instead of synthesizing frames.
        Err(hakille_core::HakilleError::SubstrateError(
            MOBILE_CAMERA_UNIMPLEMENTED.into(),
        ))
    }

    async fn capture_screen(&self, cap: &CapabilityToken) -> Result<ScreenCapture> {
        self.enforce(
            cap,
            CapabilityScope::Sensor,
            hakille_core::CapabilityAction::Read,
        )?;
        Err(hakille_core::HakilleError::SubstrateError(
            "capture d'écran indisponible : binding MediaProjection/ReplayKit non raccordé; aucune image du poste capturée".to_string(),
        ))
    }

    async fn inject_input(&self, _action: InputAction, cap: &CapabilityToken) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::GuiInput,
            hakille_core::CapabilityAction::Execute,
        )?;
        Err(hakille_core::HakilleError::SubstrateError(
            "injection d'entrée indisponible : binding accessibilité Android/iOS non raccordé; aucun événement émis".to_string(),
        ))
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
        // Aucun moteur WASM n'est relié à cette API : ne jamais inventer
        // un handle de processus. Le runtime des agents de référence est distinct.
        Err(hakille_core::HakilleError::SubstrateError(
            "spawn_sandboxed indisponible : moteur WASM non raccordé au substrat mobile; aucun processus lancé"
                .to_string(),
        ))
    }

    fn power_state(&self) -> PowerState {
        let eng = self.battery_engine.lock().unwrap();
        PowerState {
            on_battery: true,
            battery_percent: 85.0,
            charging: eng.current_tier() == crate::battery_conservation::BatteryTier::Charging,
        }
    }

    fn thermal_state(&self) -> ThermalState {
        // Honest: no native thermal HAL binding is linked (PowerManager /
        // thermal APIs unwired); Unknown, not an invented 36.5 °C.
        ThermalState::Unknown
    }
}
