use crate::capabilities::{CAP_READ, CAP_WRITE, RedoxCapabilityTable};
use crate::scheme::{RedoxSchemeHandler, SchemeHandler};
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

/// Missing native binding reported by every Redox camera method.
const REDOX_CAMERA_UNIMPLEMENTED: &str = "camera not implemented for Redox substrate: orbital camera scheme binding missing \
     (no orbital:/camera scheme wiring; expose the camera via an orbital scheme to enable)";

pub struct RedoxSubstrate {
    manifest: PlatformManifest,
    attestation_keypair: KeyPair,
    scheme_handler: RedoxSchemeHandler,
    capability_table: RedoxCapabilityTable,
    virtual_fs_root: PathBuf,
    /// Host allowlist egress policy. Default is loopback-only
    /// (no external egress until hosts are explicitly allowed).
    egress_policy: EgressPolicy,
    /// Pinned Biscuit root public key. When `None`, capability tokens are
    /// still structurally verified but NOT pinned (legacy / tests).
    /// Production MUST call `with_trusted_root`.
    trusted_capability_root: Option<biscuit_auth::PublicKey>,
}

impl RedoxSubstrate {
    pub fn new() -> Self {
        // NOTE(honnêteté): `FullSystem` est REFUSÉ (pas accordé) : l'enforcement
        // Biscuit de bout en bout n'est pas complet — les handles de la
        // `capability_table` sont des jetons porteurs non encore liés
        // cryptographiquement au `CapabilityToken` Biscuit d'entrée
        // (voir TODO ci-dessous). Accorder FullSystem serait mensonger.
        let manifest = PlatformManifest {
            tier: Tier::Sovereign,
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
            // Sovereign-node quota mirroring the `sys:memory` scheme report
            // (MemTotal 64 GiB); the scheme is the source of truth, not a probe.
            max_memory_bytes: Some(64 * 1024 * 1024 * 1024),
            // Honest: no orbital/sys VRAM discovery binding is linked;
            // unknown, not a 16 GiB claim.
            gpu_vram_bytes: None,
            // Honnête : aucun pilote d'enclave matérielle ou TPM raccordé.
            has_secure_enclave: false,
            has_tpm: false,
            battery_powered: false,
            // Honest: no `sys:` thermal envelope source linked; unknown.
            thermal_envelope_watts: None,
            os_name: "Redox".to_string(),
            os_version: "0.9.0-sovereign-microkernel".to_string(),
        };

        Self {
            manifest,
            attestation_keypair: KeyPair::generate(),
            scheme_handler: RedoxSchemeHandler::new(),
            capability_table: RedoxCapabilityTable::new(),
            virtual_fs_root: PathBuf::from("disk:/hakille/vfs"),
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

    /// Bind a Biscuit `CapabilityToken` to a `RedoxCapabilityTable` handle.
    fn bind_table_handle(
        &self,
        cap: &CapabilityToken,
        scope: CapabilityScope,
        action: hakille_core::CapabilityAction,
        redox_rights: u32,
        resource: &str,
    ) -> hakille_core::Result<usize> {
        self.enforce(cap, scope, action)?;
        let fd = self
            .capability_table
            .allocate_handle("disk:", resource, redox_rights);
        if let Err(e) = self.capability_table.verify_access(fd, redox_rights) {
            let _ = self.capability_table.revoke_handle(fd);
            return Err(e);
        }
        Ok(fd)
    }

    /// Specify a custom root for the sandboxed VirtualFS.
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

    /// Resolve a VirtualPath to a clean `disk:`-relative key.
    fn resolve_key(&self, path: &VirtualPath) -> hakille_core::Result<String> {
        let segments = hakille_substrate::validate_virtual_components(path)?;
        Ok(segments.join("/"))
    }

    pub fn virtual_fs_root(&self) -> &Path {
        &self.virtual_fs_root
    }

    pub fn scheme_handler(&self) -> &RedoxSchemeHandler {
        &self.scheme_handler
    }

    pub fn capability_table(&self) -> &RedoxCapabilityTable {
        &self.capability_table
    }
}

impl Default for RedoxSubstrate {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Substrate for RedoxSubstrate {
    fn manifest(&self) -> &PlatformManifest {
        &self.manifest
    }

    async fn attest(&self, data: &[u8]) -> Result<SignedAttestation> {
        let sig = self.attestation_keypair.sign(data);
        // Honnête : dev-attestation locale (clé éphémère, pas de TPM/enclave),
        // empreinte SHA-256 réelle des octets attestés.
        Ok(SignedAttestation {
            manifest_hash: format!("sha256-{}", hakille_substrate::sha256_hex(data)),
            platform_id: "Redox-Substrate-Host-dev-attestation".to_string(),
            timestamp: Timestamp::now(),
            signature: sig,
        })
    }

    async fn read_file(&self, path: &VirtualPath, cap: &CapabilityToken) -> Result<Vec<u8>> {
        let key = self.resolve_key(path)?;
        let fd = self.bind_table_handle(
            cap,
            CapabilityScope::Filesystem,
            hakille_core::CapabilityAction::Read,
            CAP_READ,
            &key,
        )?;
        let res = self.scheme_handler.disk().read_all(&key);
        let _ = self.capability_table.revoke_handle(fd);

        match res? {
            Some(buf) => Ok(buf),
            None => Err(hakille_core::HakilleError::StorageError(format!(
                "File not found: {}",
                path.0
            ))),
        }
    }

    async fn write_file(
        &self,
        path: &VirtualPath,
        data: &[u8],
        cap: &CapabilityToken,
    ) -> Result<()> {
        let key = self.resolve_key(path)?;
        let fd = self.bind_table_handle(
            cap,
            CapabilityScope::Filesystem,
            hakille_core::CapabilityAction::Write,
            CAP_WRITE,
            &key,
        )?;

        let op_res = (|| -> Result<()> {
            let handle = self.scheme_handler.disk().open(&key, 0)?;
            let write_res = self.scheme_handler.disk().write(handle, data);
            let close_res = self.scheme_handler.disk().close(handle);
            write_res?;
            close_res?;
            Ok(())
        })();
        let _ = self.capability_table.revoke_handle(fd);
        op_res
    }

    async fn list_dir(
        &self,
        path: &VirtualPath,
        cap: &CapabilityToken,
    ) -> Result<Vec<VirtualPath>> {
        let key = self.resolve_key(path)?;
        let fd = self.bind_table_handle(
            cap,
            CapabilityScope::Filesystem,
            hakille_core::CapabilityAction::Read,
            CAP_READ,
            &key,
        )?;
        let keys = self.scheme_handler.disk().list_keys(&key);
        let _ = self.capability_table.revoke_handle(fd);
        if keys.is_empty() {
            return Err(hakille_core::HakilleError::StorageError(format!(
                "Directory not found: {}",
                path.0
            )));
        }
        Ok(keys.into_iter().map(VirtualPath).collect())
    }

    async fn delete_file(&self, path: &VirtualPath, cap: &CapabilityToken) -> Result<()> {
        let key = self.resolve_key(path)?;
        let fd = self.bind_table_handle(
            cap,
            CapabilityScope::Filesystem,
            hakille_core::CapabilityAction::Delete,
            CAP_WRITE,
            &key,
        )?;
        self.scheme_handler.disk().remove(&key);
        let _ = self.capability_table.revoke_handle(fd);
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
            "capture audio indisponible : backend audio non raccordé au substrat redox; aucun flux démarré".to_string(),
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
            "lecture audio indisponible : backend audio non raccordé au substrat redox; aucun son joué".to_string(),
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
            REDOX_CAMERA_UNIMPLEMENTED.into(),
        ))
    }

    async fn stop_camera_capture(&self, _handle: CameraStreamHandle) -> Result<()> {
        Err(hakille_core::HakilleError::SubstrateError(
            REDOX_CAMERA_UNIMPLEMENTED.into(),
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
            REDOX_CAMERA_UNIMPLEMENTED.into(),
        ))
    }

    async fn capture_screen(&self, cap: &CapabilityToken) -> Result<ScreenCapture> {
        self.enforce(
            cap,
            CapabilityScope::Sensor,
            hakille_core::CapabilityAction::Read,
        )?;
        Err(hakille_core::HakilleError::SubstrateError(
            "capture d'écran indisponible : binding Orbital non raccordé; aucune image du poste capturée".to_string(),
        ))
    }

    async fn inject_input(&self, _action: InputAction, cap: &CapabilityToken) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::GuiInput,
            hakille_core::CapabilityAction::Execute,
        )?;
        Err(hakille_core::HakilleError::SubstrateError(
            "injection d'entrée indisponible : binding Orbital non raccordé; aucun événement émis"
                .to_string(),
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
            "spawn_sandboxed indisponible : moteur WASM non raccordé au substrat redox; aucun processus lancé"
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
        // HONNÊTE : aucun scheme noyau `orbital:notify` n'est raccordé.
        Err(hakille_core::HakilleError::SubstrateError(
            "notification Redox indisponible : scheme orbital:notify non raccordé ; aucune notification émise".to_string(),
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
            "fermeture de notification Redox indisponible : scheme orbital:notify non raccordé"
                .to_string(),
        ))
    }
}
