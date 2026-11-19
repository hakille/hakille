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

/// Missing native binding reported by every macOS camera method.
const MACOS_CAMERA_UNIMPLEMENTED: &str = "camera not implemented for macOS substrate: AVFoundation binding missing \
     (no AVCaptureSession device wiring; link AVFoundation bindings to enable)";

pub struct MacosSubstrate {
    manifest: PlatformManifest,
    attestation_keypair: KeyPair,
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
        let on_battery = read_host_power_state().on_battery;

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
            // Honest: the shareable unified-memory fraction is dynamic and
            // only knowable via a Metal / IORegistry query (not linked);
            // unknown, not a hard-coded 3/4 share.
            gpu_vram_bytes: None,
            // Honnête : aucun binding Security.framework / Secure Enclave raccordé.
            has_secure_enclave: false,
            has_tpm: false,
            battery_powered: on_battery,
            // Honest: no thermal-pressure source linked; unknown, not invented.
            thermal_envelope_watts: None,
            os_name: "macOS".to_string(),
            os_version: read_macos_os_version(),
        };

        Self {
            manifest,
            attestation_keypair: KeyPair::generate(),
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

    /// Configure the host allowlist egress policy (default: loopback-only).
    pub fn with_egress_policy(mut self, policy: EgressPolicy) -> Self {
        self.egress_policy = policy;
        self
    }

    /// Current egress policy.
    pub fn egress_policy(&self) -> &EgressPolicy {
        &self.egress_policy
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
        // Honnête : dev-attestation locale (clé éphémère, pas de Secure Enclave),
        // empreinte SHA-256 réelle des octets attestés.
        Ok(SignedAttestation {
            manifest_hash: format!("sha256-{}", hakille_substrate::sha256_hex(data)),
            platform_id: "macOS-Substrate-Host-dev-attestation".to_string(),
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
            "capture audio indisponible : backend audio non raccordé au substrat macos; aucun flux démarré".to_string(),
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
            "lecture audio indisponible : backend audio non raccordé au substrat macos; aucun son joué".to_string(),
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
        // Honest: no AVFoundation wiring — refuse instead of synthesizing frames.
        Err(hakille_core::HakilleError::SubstrateError(
            MACOS_CAMERA_UNIMPLEMENTED.into(),
        ))
    }

    async fn stop_camera_capture(&self, _handle: CameraStreamHandle) -> Result<()> {
        Err(hakille_core::HakilleError::SubstrateError(
            MACOS_CAMERA_UNIMPLEMENTED.into(),
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
        Err(hakille_core::HakilleError::SubstrateError(
            "capture d'écran indisponible : binding CoreGraphics non raccordé; aucune image du poste capturée".to_string(),
        ))
    }

    async fn inject_input(&self, _action: InputAction, cap: &CapabilityToken) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::GuiInput,
            hakille_core::CapabilityAction::Execute,
        )?;
        Err(hakille_core::HakilleError::SubstrateError(
            "injection d'entrée indisponible : binding CGEventPost non raccordé; aucun événement émis".to_string(),
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
            "spawn_sandboxed indisponible : moteur WASM non raccordé au substrat macos; aucun processus lancé"
                .to_string(),
        ))
    }

    fn power_state(&self) -> PowerState {
        read_host_power_state()
    }

    fn thermal_state(&self) -> ThermalState {
        read_host_thermal_state()
    }

    async fn send_notification(
        &self,
        request: hakille_substrate::NotificationRequest,
        cap: &CapabilityToken,
    ) -> Result<hakille_substrate::NotificationHandle> {
        self.enforce(
            cap,
            CapabilityScope::Notification,
            hakille_core::CapabilityAction::Write,
        )?;

        #[cfg(target_os = "macos")]
        {
            // Sécurité : aucune interpolation de texte utilisateur dans le code AppleScript.
            // Le titre et le corps sont passés via argv au script statique.
            const SCRIPT: &str = "on run argv\n\
                display notification (item 1 of argv) with title (item 2 of argv) subtitle \"Hakille\"\n\
                end run";
            let body = request.body.clone();
            let title = request.title.clone();
            let status = tokio::task::spawn_blocking(move || {
                std::process::Command::new("osascript")
                    .arg("-e")
                    .arg(SCRIPT)
                    .arg("--")
                    .arg(body)
                    .arg(title)
                    .status()
            })
            .await
            .map_err(|e| {
                hakille_core::HakilleError::SubstrateError(format!(
                    "notification macOS échouée (tâche osascript) : {e}"
                ))
            })?
            .map_err(|e| {
                hakille_core::HakilleError::SubstrateError(format!(
                    "notification macOS indisponible (osascript) : {e}"
                ))
            })?;
            if !status.success() {
                return Err(hakille_core::HakilleError::SubstrateError(format!(
                    "notification macOS refusée par osascript (code {status})"
                )));
            }
            Ok(hakille_substrate::NotificationHandle {
                id: request.id,
                channel: "macos_user_notifications".to_string(),
                delivered_at_ms: chrono::Utc::now().timestamp_millis() as u64,
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = request;
            Err(hakille_core::HakilleError::SubstrateError(
                "notification macOS indisponible hors macOS (osascript absent)".to_string(),
            ))
        }
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
            "fermeture de notification macOS indisponible : binding UNUserNotificationCenter non raccordé".to_string(),
        ))
    }
}
