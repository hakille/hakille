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
#[cfg(windows)]
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
            max_memory_bytes: (total_ram > 0).then_some(total_ram),
            // Honest: dedicated VRAM is unknown without a DXGI adapter query
            // (win32_ffi intentionally exposes no DXGI); not an 8 GiB claim.
            gpu_vram_bytes: None,
            has_secure_enclave: false,
            // Honnête : aucun binding TBS (TPM Base Services) n'est interrogé.
            has_tpm: false,
            battery_powered: on_battery,
            // Honest: no thermal envelope modeling without a WMI /
            // MSACPI thermal-zone binding; unknown, not invented.
            thermal_envelope_watts: None,
            os_name: "Windows".to_string(),
            os_version: "Windows (version OS non sondée)".to_string(),
        };

        let virtual_fs_root = std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./hakille_data"))
            .join("Hakille")
            .join("VirtualFS");

        Self {
            manifest,
            attestation_keypair: KeyPair::generate(),
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

    /// Specify a custom root folder for the sandboxed VirtualFS.
    pub fn with_virtual_fs_root(mut self, root: PathBuf) -> Self {
        self.virtual_fs_root = root;
        self
    }

    pub fn virtual_fs_root(&self) -> &Path {
        &self.virtual_fs_root
    }

    /// Capability-checked stop for an audio stream.
    pub async fn stop_audio_capture_with_cap(
        &self,
        _handle: AudioStreamHandle,
        cap: &CapabilityToken,
    ) -> hakille_core::Result<()> {
        self.enforce(
            cap,
            CapabilityScope::Audio,
            hakille_core::CapabilityAction::Read,
        )?;
        Err(hakille_core::HakilleError::SubstrateError(
            "arrêt audio indisponible : aucun flux audio natif géré par ce substrat".to_string(),
        ))
    }

    /// Capability-checked stop for a camera stream.
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
        self.stop_camera_stream_inner(handle).await
    }

    async fn stop_camera_stream_inner(
        &self,
        handle: CameraStreamHandle,
    ) -> hakille_core::Result<()> {
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
            Ok(())
        }
        #[cfg(not(windows))]
        {
            Err(hakille_core::HakilleError::SubstrateError(
                WINDOWS_CAMERA_UNIMPLEMENTED.into(),
            ))
        }
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
        hakille_substrate::read_confined_file(&self.virtual_fs_root, path).await
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
        hakille_substrate::write_confined_file(&self.virtual_fs_root, path, data).await
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
        hakille_substrate::list_confined_dir(&self.virtual_fs_root, path).await
    }

    async fn delete_file(&self, path: &VirtualPath, cap: &CapabilityToken) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::Filesystem,
            hakille_core::CapabilityAction::Delete,
        )?;
        hakille_substrate::delete_confined_file(&self.virtual_fs_root, path).await
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
            "capture audio indisponible : backend audio non raccordé au substrat windows; aucun flux démarré".to_string(),
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
            "lecture audio indisponible : backend audio non raccordé au substrat windows; aucun son joué".to_string(),
        ))
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

    async fn stop_camera_capture(&self, handle: CameraStreamHandle) -> Result<()> {
        tracing::warn!(
            "stop_camera_capture(handle) without capability is deprecated; use stop_camera_capture_with_cap(handle, cap) with Camera/Read ownership proof"
        );
        self.stop_camera_stream_inner(handle).await
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
        tokio::task::spawn_blocking(WindowsScreenCapturer::capture_primary_display)
            .await
            .map_err(|e| {
                hakille_core::HakilleError::SubstrateError(format!(
                    "tâche de capture d'écran Windows échouée : {e}"
                ))
            })?
    }

    async fn inject_input(&self, action: InputAction, cap: &CapabilityToken) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::GuiInput,
            hakille_core::CapabilityAction::Execute,
        )?;
        // Déporte sur thread bloquant pour ne jamais geler les workers Tokio (sleep sur drag/double-click).
        tokio::task::spawn_blocking(move || WindowsInputInjector::inject(&action))
            .await
            .map_err(|e| {
                hakille_core::HakilleError::SubstrateError(format!(
                    "tâche d'injection d'entrée Windows échouée : {e}"
                ))
            })?
    }

    async fn spawn_sandboxed(
        &self,
        _wasm: &[u8],
        caps: Vec<CapabilityToken>,
    ) -> Result<ProcessHandle> {
        let root = self.require_trusted_root("spawn_sandboxed")?;
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
        Err(hakille_core::HakilleError::SubstrateError(
            "spawn_sandboxed indisponible : moteur WASM non raccordé au substrat windows; aucun processus lancé"
                .to_string(),
        ))
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
        // HONNÊTE : aucun binding Shell_NotifyIcon / WinRT ToastNotification n'est raccordé.
        Err(hakille_core::HakilleError::SubstrateError(
            "notification Windows indisponible : binding Shell_NotifyIcon/WinRT Toast non raccordé ; aucune notification affichée".to_string(),
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
            "fermeture de notification Windows indisponible : binding Shell_NotifyIcon/WinRT Toast non raccordé".to_string(),
        ))
    }
}
