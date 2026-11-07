use crate::scoped_storage::{AppContainerStorage, ScopedStorageSandbox, StorageScope};
use async_trait::async_trait;
use hakille_core::{
    CapabilityScope, CapabilityToken, HakilleError, Result, SignedAttestation, Tier, VirtualPath,
};
use hakille_substrate::{
    AudioConfig, AudioStreamHandle, CameraConfig, CameraFrame, CameraStreamHandle, EgressPolicy,
    HttpRequest, HttpResponse, InputAction, PlatformManifest, PowerState, ProcessHandle,
    ScreenCapture, Substrate, ThermalState,
};
use std::path::PathBuf;
use std::sync::Arc;

/// Missing native binding reported by every mobile camera method.
const MOBILE_CAMERA_UNIMPLEMENTED: &str = "camera not implemented for mobile substrate: Camera2 (Android) / AVFoundation (iOS) binding missing \
     (no NDK camera device wiring; link Camera2/NDK bindings to enable)";

/// Pilote audio installé par l'hôte Android/iOS. Chaque opération doit vérifier
/// la permission OS courante et ne renvoyer un succès qu'après un effet réel.
#[async_trait]
pub trait MobileAudioDriver: Send + Sync {
    async fn start_capture(&self, config: AudioConfig) -> Result<AudioStreamHandle>;
    async fn stop_capture(&self, handle: AudioStreamHandle) -> Result<()>;
    async fn play(&self, data: &[u8], config: AudioConfig) -> Result<()>;
}

/// Caméra native : le pilote possède les sessions et refuse les handles inconnus.
#[async_trait]
pub trait MobileCameraDriver: Send + Sync {
    async fn start_capture(&self, config: CameraConfig) -> Result<CameraStreamHandle>;
    async fn stop_capture(&self, handle: CameraStreamHandle) -> Result<()>;
    async fn frame(&self, handle: CameraStreamHandle) -> Result<CameraFrame>;
}

#[async_trait]
pub trait MobileInputDriver: Send + Sync {
    async fn inject(&self, action: InputAction) -> Result<()>;
}

#[async_trait]
pub trait MobileScreenDriver: Send + Sync {
    async fn capture(&self) -> Result<ScreenCapture>;
}

/// Attestation native explicite : aucune clé éphémère ne simule une preuve OS.
#[async_trait]
pub trait MobileAttestationDriver: Send + Sync {
    fn has_secure_enclave(&self) -> bool;
    async fn attest(&self, data: &[u8], manifest: &PlatformManifest) -> Result<SignedAttestation>;
}

/// Observation du système, jamais une estimation issue du profil d'économie.
pub trait MobilePowerDriver: Send + Sync {
    fn power_state(&self) -> Result<PowerState>;
    fn thermal_state(&self) -> Result<ThermalState>;
}

pub struct MobileSubstrate {
    manifest: PlatformManifest,
    storage: Option<AppContainerStorage>,
    audio: Option<Arc<dyn MobileAudioDriver>>,
    camera: Option<Arc<dyn MobileCameraDriver>>,
    input: Option<Arc<dyn MobileInputDriver>>,
    screen: Option<Arc<dyn MobileScreenDriver>>,
    attestation: Option<Arc<dyn MobileAttestationDriver>>,
    power: Option<Arc<dyn MobilePowerDriver>>,
    egress_policy: EgressPolicy,
    trusted_capability_root: Option<biscuit_auth::PublicKey>,
}

impl MobileSubstrate {
    /// État initial non configuré : aucune identité OS, RAM ou permission native
    /// n'est inventée. Le réseau est disponible après épinglage d'une autorité.
    pub fn new() -> Self {
        let mut substrate = Self {
            manifest: PlatformManifest {
                tier: Tier::Guest,
                capabilities_granted: Vec::new(),
                capabilities_denied: Vec::new(),
                max_memory_bytes: None,
                gpu_vram_bytes: None,
                has_secure_enclave: false,
                has_tpm: false,
                battery_powered: cfg!(any(target_os = "android", target_os = "ios")),
                thermal_envelope_watts: None,
                os_name: if cfg!(target_os = "android") {
                    "Android"
                } else if cfg!(target_os = "ios") {
                    "iOS"
                } else {
                    "Mobile — hôte non configuré"
                }
                .into(),
                os_version: "inconnue — non observée".into(),
            },
            storage: None,
            audio: None,
            camera: None,
            input: None,
            screen: None,
            attestation: None,
            power: None,
            egress_policy: EgressPolicy::loopback_only(),
            trusted_capability_root: None,
        };
        substrate.refresh_manifest();
        substrate
    }

    fn refresh_manifest(&mut self) {
        self.manifest.capabilities_granted.clear();
        self.manifest.capabilities_denied.clear();
        let pinned = self.trusted_capability_root.is_some();
        for (scope, implemented) in [
            (CapabilityScope::Network, true),
            (CapabilityScope::Filesystem, self.storage.is_some()),
            (CapabilityScope::Audio, self.audio.is_some()),
            (CapabilityScope::Camera, self.camera.is_some()),
            (CapabilityScope::GuiInput, self.input.is_some()),
            (CapabilityScope::Sensor, self.screen.is_some()),
            (CapabilityScope::ModelInference, false),
            (CapabilityScope::Wallet, false),
            (CapabilityScope::FullSystem, false),
        ] {
            if pinned && implemented {
                self.manifest.capabilities_granted.push(scope);
            } else {
                self.manifest.capabilities_denied.push(scope);
            }
        }
        self.manifest.has_secure_enclave = self
            .attestation
            .as_ref()
            .is_some_and(|driver| driver.has_secure_enclave());
    }

    /// L'autorité doit être explicitement épinglée, y compris dans les tests.
    pub fn with_trusted_root(mut self, root: biscuit_auth::PublicKey) -> Self {
        self.trusted_capability_root = Some(root);
        self.refresh_manifest();
        self
    }

    /// Conteneur existant fourni par l'OS (ou un répertoire temporaire de test).
    /// Les effets FS utilisent des handles confinés, jamais le shadow en mémoire.
    pub fn with_app_container(mut self, root: impl Into<PathBuf>) -> Result<Self> {
        self.storage = Some(AppContainerStorage::open(root.into())?);
        self.refresh_manifest();
        Ok(self)
    }

    pub fn storage_sandbox(&self) -> Option<&ScopedStorageSandbox> {
        self.storage.as_ref().map(AppContainerStorage::sandbox)
    }

    fn storage(&self) -> Result<&AppContainerStorage> {
        self.storage.as_ref().ok_or_else(|| HakilleError::StorageError(
            "conteneur applicatif absent : configurer with_app_container avec le répertoire fourni par l'OS".into()
        ))
    }

    /// Le manifeste décrit les pilotes raccordés. Les permissions OS peuvent
    /// être révoquées : chaque pilote doit encore les vérifier à chaque effet.
    pub fn with_audio_driver(mut self, driver: Arc<dyn MobileAudioDriver>) -> Self {
        self.audio = Some(driver);
        self.refresh_manifest();
        self
    }
    pub fn with_camera_driver(mut self, driver: Arc<dyn MobileCameraDriver>) -> Self {
        self.camera = Some(driver);
        self.refresh_manifest();
        self
    }
    pub fn with_input_driver(mut self, driver: Arc<dyn MobileInputDriver>) -> Self {
        self.input = Some(driver);
        self.refresh_manifest();
        self
    }
    pub fn with_screen_driver(mut self, driver: Arc<dyn MobileScreenDriver>) -> Self {
        self.screen = Some(driver);
        self.refresh_manifest();
        self
    }
    pub fn with_attestation_driver(mut self, driver: Arc<dyn MobileAttestationDriver>) -> Self {
        self.attestation = Some(driver);
        self.refresh_manifest();
        self
    }
    pub fn with_power_driver(mut self, driver: Arc<dyn MobilePowerDriver>) -> Self {
        self.power = Some(driver);
        self
    }

    /// None signifie absence de binding ; une erreur de mesure reste explicite.
    pub fn power_observation(&self) -> Result<Option<PowerState>> {
        let Some(driver) = &self.power else {
            return Ok(None);
        };
        let state = driver.power_state()?;
        if !state.battery_percent.is_finite() || !(0.0..=100.0).contains(&state.battery_percent) {
            return Err(HakilleError::SubstrateError(
                "mesure de batterie mobile invalide".into(),
            ));
        }
        Ok(Some(state))
    }

    pub fn thermal_observation(&self) -> Result<Option<ThermalState>> {
        let Some(driver) = &self.power else {
            return Ok(None);
        };
        let state = driver.thermal_state()?;
        if !state.temperature_celsius.is_finite() {
            return Err(HakilleError::SubstrateError(
                "mesure thermique mobile invalide".into(),
            ));
        }
        Ok(Some(state))
    }

    fn enforce(
        &self,
        cap: &CapabilityToken,
        scope: CapabilityScope,
        action: hakille_core::CapabilityAction,
    ) -> Result<()> {
        let root = self.trusted_capability_root.as_ref().ok_or_else(|| {
            HakilleError::CapabilityDenied(
                "autorité Biscuit non configurée : appeler with_trusted_root avant tout effet"
                    .into(),
            )
        })?;
        hakille_core::verify_capability_with_root_key(cap, root, scope, action)
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
        let driver = self.attestation.as_ref().ok_or_else(|| HakilleError::SubstrateError(
            "attestation mobile indisponible : aucun pilote KeyStore/App Attest raccordé ; aucune identité matérielle prouvée".into()
        ))?;
        driver.attest(data, &self.manifest).await
    }

    async fn read_file(&self, path: &VirtualPath, cap: &CapabilityToken) -> Result<Vec<u8>> {
        self.enforce(
            cap,
            CapabilityScope::Filesystem,
            hakille_core::CapabilityAction::Read,
        )?;
        self.storage()?.read_file(StorageScope::Documents, path)
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
        self.storage()?
            .write_file_atomic(StorageScope::Documents, path, data)
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
        self.storage()?.list_dir(StorageScope::Documents, path)
    }

    async fn delete_file(&self, path: &VirtualPath, cap: &CapabilityToken) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::Filesystem,
            hakille_core::CapabilityAction::Delete,
        )?;
        self.storage()?.delete_file(StorageScope::Documents, path)
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
        config: AudioConfig,
        cap: &CapabilityToken,
    ) -> Result<AudioStreamHandle> {
        self.enforce(
            cap,
            CapabilityScope::Audio,
            hakille_core::CapabilityAction::Read,
        )?;
        if let Some(driver) = &self.audio {
            return driver.start_capture(config).await;
        }
        Err(hakille_core::HakilleError::SubstrateError(
            "capture audio indisponible : backend audio non raccordé au substrat mobile; aucun flux démarré".to_string(),
        ))
    }

    async fn stop_audio_capture(&self, handle: AudioStreamHandle) -> Result<()> {
        if let Some(driver) = &self.audio {
            return driver.stop_capture(handle).await;
        }
        Err(hakille_core::HakilleError::SubstrateError(
            "arrêt audio indisponible : aucun flux audio natif géré par ce substrat".to_string(),
        ))
    }

    async fn play_audio(
        &self,
        data: &[u8],
        config: AudioConfig,
        cap: &CapabilityToken,
    ) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::Audio,
            hakille_core::CapabilityAction::Write,
        )?;
        if let Some(driver) = &self.audio {
            return driver.play(data, config).await;
        }
        Err(hakille_core::HakilleError::SubstrateError(
            "lecture audio indisponible : backend audio non raccordé au substrat mobile; aucun son joué".to_string(),
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
        if let Some(driver) = &self.camera {
            return driver.start_capture(config).await;
        }
        // Honest: no Camera2/AVFoundation wiring — refuse instead of synthesizing frames.
        Err(hakille_core::HakilleError::SubstrateError(
            MOBILE_CAMERA_UNIMPLEMENTED.into(),
        ))
    }

    async fn stop_camera_capture(&self, handle: CameraStreamHandle) -> Result<()> {
        if let Some(driver) = &self.camera {
            return driver.stop_capture(handle).await;
        }
        Err(HakilleError::SubstrateError(
            MOBILE_CAMERA_UNIMPLEMENTED.into(),
        ))
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
        if let Some(driver) = &self.camera {
            return driver.frame(handle).await;
        }
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
        if let Some(driver) = &self.screen {
            return driver.capture().await;
        }
        Err(hakille_core::HakilleError::SubstrateError(
            "capture d'écran indisponible : binding MediaProjection/ReplayKit non raccordé; aucune image du poste capturée".to_string(),
        ))
    }

    async fn inject_input(&self, action: InputAction, cap: &CapabilityToken) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::GuiInput,
            hakille_core::CapabilityAction::Execute,
        )?;
        if let Some(driver) = &self.input {
            return driver.inject(action).await;
        }
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
        match self.power_observation() {
            Ok(Some(state)) => state,
            other => {
                if let Err(error) = other {
                    tracing::warn!(%error, "observation batterie indisponible");
                }
                // Limite du contrat legacy sans Option : NaN signifie inconnu.
                // Les appelants mobiles doivent préférer power_observation().
                PowerState {
                    on_battery: false,
                    battery_percent: f32::NAN,
                    charging: false,
                }
            }
        }
    }

    fn thermal_state(&self) -> ThermalState {
        match self.thermal_observation() {
            Ok(Some(state)) => state,
            other => {
                if let Err(error) = other {
                    tracing::warn!(%error, "observation thermique indisponible");
                }
                ThermalState::Unknown
            }
        }
    }
}

#[cfg(test)]
mod configured_mobile_tests {
    use super::*;
    use hakille_core::{CapabilityAction, CapabilityAuthority, Timestamp};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestContainer(PathBuf);
    impl TestContainer {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("hakille-mobile-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for TestContainer {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn token(
        authority: &CapabilityAuthority,
        scope: CapabilityScope,
        actions: Vec<CapabilityAction>,
    ) -> CapabilityToken {
        authority
            .create_root_token(
                vec![scope],
                actions,
                Timestamp(chrono::Utc::now() + chrono::Duration::hours(1)),
            )
            .unwrap()
    }
    fn fs_token(authority: &CapabilityAuthority) -> CapabilityToken {
        token(
            authority,
            CapabilityScope::Filesystem,
            vec![
                CapabilityAction::Read,
                CapabilityAction::Write,
                CapabilityAction::Delete,
            ],
        )
    }

    #[tokio::test]
    async fn file_effects_require_pinned_authority_and_explicit_container() {
        let authority = CapabilityAuthority::new();
        let cap = fs_token(&authority);
        let path = VirtualPath("note.txt".into());
        let unconfigured = MobileSubstrate::new();
        assert!(matches!(
            unconfigured.write_file(&path, b"no", &cap).await,
            Err(HakilleError::CapabilityDenied(_))
        ));
        let pinned = MobileSubstrate::new().with_trusted_root(authority.root_public_key());
        assert!(matches!(
            pinned.write_file(&path, b"no", &cap).await,
            Err(HakilleError::StorageError(_))
        ));
        assert!(
            !pinned
                .manifest()
                .capabilities_granted
                .contains(&CapabilityScope::Filesystem)
        );
        assert!(
            pinned
                .manifest()
                .capabilities_granted
                .contains(&CapabilityScope::Network)
        );
        assert!(pinned.read_file(&path, &cap).await.is_err());
        assert!(
            pinned
                .list_dir(&VirtualPath(String::new()), &cap)
                .await
                .is_err()
        );
        assert!(pinned.delete_file(&path, &cap).await.is_err());
    }

    #[tokio::test]
    async fn files_persist_across_substrate_recreation_and_delete_is_real() {
        let dir = TestContainer::new();
        let authority = CapabilityAuthority::new();
        let cap = fs_token(&authority);
        let path = VirtualPath("notes/todo.txt".into());
        let substrate = MobileSubstrate::new()
            .with_trusted_root(authority.root_public_key())
            .with_app_container(&dir.0)
            .unwrap();
        assert!(
            substrate
                .manifest()
                .capabilities_granted
                .contains(&CapabilityScope::Filesystem)
        );
        assert!(substrate.storage_sandbox().is_some());
        assert!(
            substrate
                .list_dir(&VirtualPath(String::new()), &cap)
                .await
                .unwrap()
                .is_empty()
        );
        substrate
            .write_file(&path, b"premiere version", &cap)
            .await
            .unwrap();
        substrate
            .write_file(&path, b"version durable", &cap)
            .await
            .unwrap();
        drop(substrate);
        assert_eq!(
            std::fs::read(dir.0.join("Documents/notes/todo.txt")).unwrap(),
            b"version durable"
        );
        let reopened = MobileSubstrate::new()
            .with_trusted_root(authority.root_public_key())
            .with_app_container(&dir.0)
            .unwrap();
        assert_eq!(
            reopened.read_file(&path, &cap).await.unwrap(),
            b"version durable"
        );
        let listed = reopened
            .list_dir(&VirtualPath("notes".into()), &cap)
            .await
            .unwrap();
        assert!(listed.iter().any(|entry| entry.0.ends_with("todo.txt")));
        reopened.delete_file(&path, &cap).await.unwrap();
        assert!(!dir.0.join("Documents/notes/todo.txt").exists());
        assert!(reopened.read_file(&path, &cap).await.is_err());
        assert!(reopened.delete_file(&path, &cap).await.is_err());
    }

    #[tokio::test]
    async fn foreign_expired_wrong_scope_and_read_only_tokens_cannot_mutate_storage() {
        let dir = TestContainer::new();
        let authority = CapabilityAuthority::new();
        let substrate = MobileSubstrate::new()
            .with_trusted_root(authority.root_public_key())
            .with_app_container(&dir.0)
            .unwrap();
        let path = VirtualPath("protected.txt".into());
        let forged = fs_token(&CapabilityAuthority::new());
        let wrong = token(
            &authority,
            CapabilityScope::Network,
            vec![CapabilityAction::Write],
        );
        let read_only = token(
            &authority,
            CapabilityScope::Filesystem,
            vec![CapabilityAction::Read],
        );
        let expired = authority
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Write],
                Timestamp(chrono::Utc::now() - chrono::Duration::hours(1)),
            )
            .unwrap();
        for cap in [&forged, &wrong, &read_only, &expired] {
            assert!(
                substrate
                    .write_file(&path, b"forbidden", cap)
                    .await
                    .is_err()
            );
        }
        assert!(!dir.0.join("Documents/protected.txt").exists());
        let cap = fs_token(&authority);
        substrate
            .write_file(&path, b"original", &cap)
            .await
            .unwrap();
        assert!(substrate.delete_file(&path, &read_only).await.is_err());
        assert_eq!(substrate.read_file(&path, &cap).await.unwrap(), b"original");
    }

    #[tokio::test]
    async fn container_rejects_traversal_and_missing_root() {
        let dir = TestContainer::new();
        assert!(
            MobileSubstrate::new()
                .with_app_container(dir.0.join("missing"))
                .is_err()
        );
        let authority = CapabilityAuthority::new();
        let substrate = MobileSubstrate::new()
            .with_trusted_root(authority.root_public_key())
            .with_app_container(&dir.0)
            .unwrap();
        let cap = fs_token(&authority);
        for path in [
            "../escape",
            "/absolute",
            "C:\\escape",
            "a/../../escape",
            "a\\..\\escape",
            "nul\0name",
        ] {
            let path = VirtualPath(path.into());
            assert!(substrate.write_file(&path, b"escape", &cap).await.is_err());
            assert!(substrate.read_file(&path, &cap).await.is_err());
            assert!(substrate.delete_file(&path, &cap).await.is_err());
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn container_does_not_read_or_mutate_outside_symlink_target() {
        let dir = TestContainer::new();
        let outside = TestContainer::new();
        std::fs::write(outside.0.join("secret"), b"original secret").unwrap();
        let authority = CapabilityAuthority::new();
        let substrate = MobileSubstrate::new()
            .with_trusted_root(authority.root_public_key())
            .with_app_container(&dir.0)
            .unwrap();
        std::os::unix::fs::symlink(&outside.0, dir.0.join("Documents/escape")).unwrap();
        let path = VirtualPath("escape/secret".into());
        let cap = fs_token(&authority);
        assert!(substrate.read_file(&path, &cap).await.is_err());
        assert!(substrate.write_file(&path, b"changed", &cap).await.is_err());
        assert!(substrate.delete_file(&path, &cap).await.is_err());
        assert_eq!(
            std::fs::read(outside.0.join("secret")).unwrap(),
            b"original secret"
        );
    }

    struct DeniedInput(AtomicUsize);
    #[async_trait]
    impl MobileInputDriver for DeniedInput {
        async fn inject(&self, _action: InputAction) -> Result<()> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(HakilleError::SubstrateError(
                "permission native refusée (pilote de test)".into(),
            ))
        }
    }

    #[tokio::test]
    async fn configured_driver_still_checks_token_and_preserves_native_failure() {
        let authority = CapabilityAuthority::new();
        let driver = Arc::new(DeniedInput(AtomicUsize::new(0)));
        let substrate = MobileSubstrate::new().with_input_driver(driver.clone());
        assert!(
            !substrate
                .manifest()
                .capabilities_granted
                .contains(&CapabilityScope::GuiInput)
        );
        let substrate = substrate.with_trusted_root(authority.root_public_key());
        assert!(
            substrate
                .manifest()
                .capabilities_granted
                .contains(&CapabilityScope::GuiInput)
        );
        let wrong = fs_token(&authority);
        let action = || InputAction::Type {
            text: "test".into(),
        };
        assert!(matches!(
            substrate.inject_input(action(), &wrong).await,
            Err(HakilleError::CapabilityDenied(_))
        ));
        assert_eq!(driver.0.load(Ordering::SeqCst), 0);
        let cap = token(
            &authority,
            CapabilityScope::GuiInput,
            vec![CapabilityAction::Execute],
        );
        assert!(
            substrate
                .inject_input(action(), &cap)
                .await
                .unwrap_err()
                .to_string()
                .contains("permission native refusée")
        );
        assert_eq!(driver.0.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn camera_stop_without_driver_does_not_acknowledge_unknown_handle() {
        assert!(
            MobileSubstrate::new()
                .stop_camera_capture(CameraStreamHandle(42))
                .await
                .is_err()
        );
    }

    struct ObservedPower(f32);
    impl MobilePowerDriver for ObservedPower {
        fn power_state(&self) -> Result<PowerState> {
            Ok(PowerState {
                on_battery: true,
                battery_percent: self.0,
                charging: false,
            })
        }
        fn thermal_state(&self) -> Result<ThermalState> {
            Err(HakilleError::SubstrateError(
                "capteur thermique indisponible (test)".into(),
            ))
        }
    }

    #[test]
    fn power_observations_are_explicit_and_invalid_measurements_are_not_defaults() {
        let observed = MobileSubstrate::new().with_power_driver(Arc::new(ObservedPower(17.0)));
        assert_eq!(
            observed
                .power_observation()
                .unwrap()
                .unwrap()
                .battery_percent,
            17.0
        );
        assert_eq!(observed.power_state().battery_percent, 17.0);
        assert!(observed.thermal_observation().is_err());
        assert!(observed.thermal_state().is_unknown());
        for invalid in [f32::NAN, f32::INFINITY, -1.0, 101.0] {
            let substrate =
                MobileSubstrate::new().with_power_driver(Arc::new(ObservedPower(invalid)));
            assert!(substrate.power_observation().is_err());
            assert!(substrate.power_state().battery_percent.is_nan());
        }
    }
}
