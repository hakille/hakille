use crate::display::LinuxDisplayCaptureEngine;
use crate::input::LinuxInputInjector;
use crate::telemetry::{
    detect_linux_vram_bytes, read_host_meminfo, read_host_power_state, read_host_thermal_state,
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
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct LinuxSubstrate {
    manifest: PlatformManifest,
    attestation_keypair: KeyPair,
    display_engine: LinuxDisplayCaptureEngine,
    input_injector: LinuxInputInjector,
    virtual_fs_root: PathBuf,
    /// Host allowlist egress policy. Default is loopback-only
    /// (no external egress until hosts are explicitly allowed).
    egress_policy: EgressPolicy,
    /// Pinned Biscuit root public key. When `None`, capability tokens are
    /// still structurally verified but NOT pinned (legacy / tests).
    /// Production MUST call `with_trusted_root`.
    trusted_capability_root: Option<biscuit_auth::PublicKey>,
    /// Active V4L2 camera streams (thread per stream, latest-only frames).
    camera_counter: AtomicU64,
    camera_streams: Mutex<std::collections::HashMap<u64, crate::camera_v4l2::CameraStream>>,
}

/// Pure parser for `os-release` style content: `PRETTY_NAME` wins, otherwise
/// `NAME` plus optional `VERSION_ID`. Returns `None` when neither is present.
/// Empty lines, CRLF and unrelated keys are tolerated; quotes are stripped.
pub fn parse_os_release(content: &str) -> Option<String> {
    let mut pretty = String::new();
    let mut name = String::new();
    let mut version_id = String::new();
    for line in content.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("PRETTY_NAME=") {
            pretty = value.trim().trim_matches('"').trim().to_string();
        } else if let Some(value) = line.strip_prefix("NAME=") {
            name = value.trim().trim_matches('"').trim().to_string();
        } else if let Some(value) = line.strip_prefix("VERSION_ID=") {
            version_id = value.trim().trim_matches('"').trim().to_string();
        }
    }
    if !pretty.is_empty() {
        return Some(pretty);
    }
    if name.is_empty() {
        return None;
    }
    if version_id.is_empty() {
        Some(name)
    } else {
        Some(format!("{name} {version_id}"))
    }
}

/// Honest host OS version. On Linux: `/etc/os-release` (`PRETTY_NAME` /
/// `NAME` + `VERSION_ID`), else the real kernel release from
/// `/proc/sys/kernel/osrelease`, else `"unknown"` — never a hard-coded string.
#[cfg(target_os = "linux")]
fn read_linux_os_version() -> String {
    if let Ok(content) = std::fs::read_to_string("/etc/os-release")
        && let Some(version) = parse_os_release(&content)
    {
        return version;
    }
    if let Ok(release) = std::fs::read_to_string("/proc/sys/kernel/osrelease") {
        let release = release.trim();
        if !release.is_empty() {
            return format!("Linux kernel {release}");
        }
    }
    tracing::warn!("OS version unknown: /etc/os-release and /proc/sys/kernel/osrelease unreadable");
    "unknown".to_string()
}

/// Off Linux there is no `/etc/os-release` to read: report an unambiguous
/// unknown instead of a plausible-looking fabricated kernel version.
#[cfg(not(target_os = "linux"))]
fn read_linux_os_version() -> String {
    "unknown (non-Linux host: /etc/os-release unavailable)".to_string()
}

impl LinuxSubstrate {
    pub fn new() -> Self {
        let mem = read_host_meminfo();
        let has_tpm = cfg!(target_os = "linux")
            && (Path::new("/dev/tpm0").exists() || Path::new("/dev/tpmrm0").exists());
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
            max_memory_bytes: (mem.mem_total_bytes > 0).then_some(mem.mem_total_bytes),
            // Honest: discrete VRAM from sysfs DRM when discoverable, else
            // None (shared-memory iGPUs / headless VMs report nothing).
            gpu_vram_bytes: detect_linux_vram_bytes(),
            has_secure_enclave: false,
            has_tpm,
            battery_powered: read_host_power_state().on_battery,
            // Honest: no thermal envelope modeling without RAPL/powercap
            // sampling (see /sys/class/powercap); unknown, not invented.
            thermal_envelope_watts: None,
            os_name: "Linux".to_string(),
            os_version: read_linux_os_version(),
        };

        Self {
            manifest,
            attestation_keypair: KeyPair::generate(),
            display_engine: LinuxDisplayCaptureEngine::new(),
            input_injector: LinuxInputInjector::default(),
            virtual_fs_root: PathBuf::from("/var/lib/hakille/vfs"),
            egress_policy: EgressPolicy::loopback_only(),
            trusted_capability_root: None,
            camera_counter: AtomicU64::new(1),
            camera_streams: Mutex::new(std::collections::HashMap::new()),
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

    pub fn display_engine(&self) -> &LinuxDisplayCaptureEngine {
        &self.display_engine
    }

    pub fn input_injector(&self) -> &LinuxInputInjector {
        &self.input_injector
    }

    /// Capability-checked stop for an audio stream. The trait-level
    /// `stop_audio_capture(handle)` is kept for compatibility but only logs
    /// a deprecation warning (no cap = no ownership proof); new code MUST
    /// call this method with the owner's Audio/Read capability.
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
        let stream = {
            let mut streams = self.camera_streams.lock().map_err(|_| {
                hakille_core::HakilleError::SubstrateError("camera stream registry poisoned".into())
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
}

impl Default for LinuxSubstrate {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Substrate for LinuxSubstrate {
    fn manifest(&self) -> &PlatformManifest {
        &self.manifest
    }

    async fn attest(&self, data: &[u8]) -> Result<SignedAttestation> {
        let sig = self.attestation_keypair.sign(data);
        // Honnête : dev-attestation locale (clé éphémère, pas de quote TPM),
        // empreinte SHA-256 réelle des octets attestés.
        Ok(SignedAttestation {
            manifest_hash: format!("sha256-{}", hakille_substrate::sha256_hex(data)),
            platform_id: "Linux-Substrate-Host-dev-attestation".to_string(),
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
            "capture audio indisponible : backend audio non raccordé au substrat linux; aucun flux démarré".to_string(),
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
            "lecture audio indisponible : backend audio non raccordé au substrat linux; aucun son joué".to_string(),
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
        let stream = crate::camera_v4l2::start(&config)
            .map_err(hakille_core::HakilleError::SubstrateError)?;
        let id = self.camera_counter.fetch_add(1, Ordering::SeqCst);
        self.camera_streams
            .lock()
            .map_err(|_| {
                hakille_core::HakilleError::SubstrateError("camera stream registry poisoned".into())
            })?
            .insert(id, stream);
        Ok(CameraStreamHandle(id))
    }

    async fn stop_camera_capture(&self, handle: CameraStreamHandle) -> Result<()> {
        tracing::warn!(
            "stop_camera_capture(handle) without capability is deprecated; use stop_camera_capture_with_cap(handle, cap) with Camera/Read ownership proof"
        );
        let stream = {
            let mut streams = self.camera_streams.lock().map_err(|_| {
                hakille_core::HakilleError::SubstrateError("camera stream registry poisoned".into())
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
        let receiver = {
            let streams = self.camera_streams.lock().map_err(|_| {
                hakille_core::HakilleError::SubstrateError("camera stream registry poisoned".into())
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
        crate::camera_v4l2::next_frame(receiver, std::time::Duration::from_secs(5))
            .await
            .map_err(hakille_core::HakilleError::SubstrateError)
    }

    async fn capture_screen(&self, cap: &CapabilityToken) -> Result<ScreenCapture> {
        self.enforce(
            cap,
            CapabilityScope::Sensor,
            hakille_core::CapabilityAction::Read,
        )?;
        Err(hakille_core::HakilleError::SubstrateError(
            "capture d'écran indisponible : binding X11/Wayland/DRM non raccordé; aucune image du poste capturée".to_string(),
        ))
    }

    async fn inject_input(&self, action: InputAction, cap: &CapabilityToken) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::GuiInput,
            hakille_core::CapabilityAction::Execute,
        )?;
        self.input_injector
            .inject(&action)
            .map_err(hakille_core::HakilleError::SubstrateError)
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
            "spawn_sandboxed indisponible : moteur WASM non raccordé au substrat linux; aucun processus lancé"
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

        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return Err(hakille_core::HakilleError::SubstrateError(
                "notification Linux indisponible : aucune session graphique active (DISPLAY/WAYLAND_DISPLAY absents)".to_string(),
            ));
        }

        let urgency_str = match request.urgency {
            hakille_substrate::NotificationUrgency::Low => "low",
            hakille_substrate::NotificationUrgency::Normal => "normal",
            hakille_substrate::NotificationUrgency::High
            | hakille_substrate::NotificationUrgency::Critical => "critical",
        };
        let title = request.title.clone();
        let body = request.body.clone();
        let timeout_ms = request.timeout_ms;

        let status = tokio::task::spawn_blocking(move || {
            let mut cmd = std::process::Command::new("notify-send");
            cmd.arg("-u").arg(urgency_str).arg("-a").arg("Hakille");
            if let Some(timeout) = timeout_ms {
                cmd.arg("-t").arg(timeout.to_string());
            }
            // Séparateur `--` obligatoire : empêche l'injection d'options CLI via titre/corps.
            cmd.arg("--").arg(title).arg(body);
            cmd.status()
        })
        .await
        .map_err(|e| {
            hakille_core::HakilleError::SubstrateError(format!(
                "notification Linux échouée (tâche notify-send) : {e}"
            ))
        })?
        .map_err(|e| {
            hakille_core::HakilleError::SubstrateError(format!(
                "notification Linux indisponible (notify-send) : {e}"
            ))
        })?;

        if !status.success() {
            return Err(hakille_core::HakilleError::SubstrateError(format!(
                "notification Linux refusée par notify-send (code {status})"
            )));
        }

        Ok(hakille_substrate::NotificationHandle {
            id: request.id,
            channel: "freedesktop_dbus".to_string(),
            delivered_at_ms: chrono::Utc::now().timestamp_millis() as u64,
        })
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
            "fermeture de notification Linux indisponible : aucun binding D-Bus org.freedesktop.Notifications raccordé".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_release_prefers_pretty_name() {
        let fixture = "NAME=\"Ubuntu\"\nVERSION_ID=\"24.04\"\nPRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\nID=ubuntu\n";
        assert_eq!(
            parse_os_release(fixture).as_deref(),
            Some("Ubuntu 24.04.1 LTS")
        );
    }

    #[test]
    fn os_release_falls_back_to_name_and_version_id() {
        let fixture = "NAME=\"Debian GNU/Linux\"\nVERSION_ID=\"12\"\nID=debian\n";
        assert_eq!(
            parse_os_release(fixture).as_deref(),
            Some("Debian GNU/Linux 12")
        );
        let no_version = "NAME=Alpine\nID=alpine\n";
        assert_eq!(parse_os_release(no_version).as_deref(), Some("Alpine"));
    }

    #[test]
    fn os_release_handles_crlf_junk_unquoted_and_empty_values() {
        let fixture = "\r\n# comment\r\nPRETTY_NAME=\r\nNAME=Fedora Linux\r\nVERSION_ID=40\r\nUNRELATED=x y\r\n";
        assert_eq!(
            parse_os_release(fixture).as_deref(),
            Some("Fedora Linux 40")
        );
    }

    #[test]
    fn os_release_returns_none_without_name_or_pretty_name() {
        assert_eq!(parse_os_release(""), None);
        assert_eq!(parse_os_release("ID=ubuntu\nVERSION_ID=\"24.04\"\n"), None);
        assert_eq!(parse_os_release("PRETTY_NAME=\"  \"\n"), None);
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn os_version_off_linux_is_explicitly_unknown() {
        let version = read_linux_os_version();
        assert!(
            version.contains("unknown"),
            "non-Linux OS version must be explicitly unknown, got: {version}"
        );
        assert!(
            !version.contains("6.8.0-generic"),
            "hard-coded kernel version must be gone, got: {version}"
        );
    }

    #[tokio::test]
    async fn test_linux_substrate_notification_lifecycle_and_security() {
        let substrate = LinuxSubstrate::new();
        let authority = hakille_core::CapabilityAuthority::new();
        let substrate = substrate.with_trusted_root(authority.root_public_key());
        let exp = Timestamp(chrono::Utc::now() + chrono::Duration::hours(1));

        // 1. Token avec CapabilityScope::Notification (Write + Delete)
        let notif_token = authority
            .create_root_token(
                vec![CapabilityScope::Notification],
                vec![
                    hakille_core::CapabilityAction::Write,
                    hakille_core::CapabilityAction::Delete,
                ],
                exp.clone(),
            )
            .expect("token");

        let request = hakille_substrate::NotificationRequest::new(
            "notif-123",
            "Alerte de test",
            "Ceci est une notification souveraine Linux",
        )
        .with_urgency(hakille_substrate::NotificationUrgency::Normal)
        .with_category(hakille_substrate::NotificationCategory::System);

        // 2. Émission autorisée : sans session graphique ou sans notify-send,
        // le contrat d'honnêteté impose un SubstrateError explicite (jamais un faux Ok).
        let res = substrate.send_notification(request, &notif_token).await;
        match res {
            Ok(handle) => assert_eq!(handle.id, "notif-123"),
            Err(hakille_core::HakilleError::SubstrateError(_)) => {}
            Err(other) => panic!("erreur inattendue avec capability Notification valide : {other}"),
        }

        // 3. Fermeture autorisée : sans binding D-Bus, retourne honnêtement SubstrateError.
        let dummy_handle = hakille_substrate::NotificationHandle {
            id: "notif-123".to_string(),
            channel: "freedesktop_dbus".to_string(),
            delivered_at_ms: 0,
        };
        assert!(matches!(
            substrate
                .dismiss_notification(&dummy_handle, &notif_token)
                .await,
            Err(hakille_core::HakilleError::SubstrateError(_))
        ));

        // 4. Token frauduleux (Scope Camera uniquement) : REFUS STRICT (CapabilityDenied)
        let camera_token = authority
            .create_root_token(
                vec![CapabilityScope::Camera],
                vec![hakille_core::CapabilityAction::Write],
                exp,
            )
            .expect("camera token");
        let unauthorized_req = hakille_substrate::NotificationRequest::new("bad", "Titre", "Corps");
        let err = substrate
            .send_notification(unauthorized_req, &camera_token)
            .await;
        assert!(
            matches!(err, Err(hakille_core::HakilleError::CapabilityDenied(_))),
            "Must reject notification without CapabilityScope::Notification"
        );
    }
}
