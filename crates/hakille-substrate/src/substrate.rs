use crate::attestation::{
    AudioConfig, AudioStreamHandle, CameraConfig, CameraFrame, CameraStreamHandle, HttpRequest,
    HttpResponse, InputAction, PowerState, ProcessHandle, ScreenCapture, ThermalState,
};
use crate::manifest::PlatformManifest;
use async_trait::async_trait;
use hakille_core::{CapabilityToken, SignedAttestation, VirtualPath};

#[async_trait]
pub trait Substrate: Send + Sync + 'static {
    fn manifest(&self) -> &PlatformManifest;
    async fn attest(&self, data: &[u8]) -> hakille_core::Result<SignedAttestation>;

    // Filesystem
    async fn read_file(
        &self,
        path: &VirtualPath,
        cap: &CapabilityToken,
    ) -> hakille_core::Result<Vec<u8>>;
    async fn write_file(
        &self,
        path: &VirtualPath,
        data: &[u8],
        cap: &CapabilityToken,
    ) -> hakille_core::Result<()>;
    async fn list_dir(
        &self,
        path: &VirtualPath,
        cap: &CapabilityToken,
    ) -> hakille_core::Result<Vec<VirtualPath>>;
    async fn delete_file(
        &self,
        path: &VirtualPath,
        cap: &CapabilityToken,
    ) -> hakille_core::Result<()>;

    // Network
    async fn http_request(
        &self,
        req: HttpRequest,
        cap: &CapabilityToken,
    ) -> hakille_core::Result<HttpResponse>;

    // Audio
    async fn start_audio_capture(
        &self,
        config: AudioConfig,
        cap: &CapabilityToken,
    ) -> hakille_core::Result<AudioStreamHandle>;
    async fn stop_audio_capture(&self, handle: AudioStreamHandle) -> hakille_core::Result<()>;
    async fn play_audio(
        &self,
        data: &[u8],
        config: AudioConfig,
        cap: &CapabilityToken,
    ) -> hakille_core::Result<()>;

    // Camera — NO synthetic default. Each OS substrate MUST implement these
    // honestly (real MediaFoundation/V4L2/Camera2 wiring) or return
    // `Err(SubstrateError("camera not implemented for this substrate"))`.
    // Returning Ok(fake frames) here would mask missing implementations.
    async fn start_camera_capture(
        &self,
        _config: CameraConfig,
        _cap: &CapabilityToken,
    ) -> hakille_core::Result<CameraStreamHandle> {
        Err(hakille_core::HakilleError::SubstrateError(
            "camera not implemented for this substrate".into(),
        ))
    }
    async fn stop_camera_capture(&self, _handle: CameraStreamHandle) -> hakille_core::Result<()> {
        Err(hakille_core::HakilleError::SubstrateError(
            "camera not implemented for this substrate".into(),
        ))
    }
    async fn capture_camera_frame(
        &self,
        _handle: CameraStreamHandle,
        _cap: &CapabilityToken,
    ) -> hakille_core::Result<CameraFrame> {
        Err(hakille_core::HakilleError::SubstrateError(
            "camera not implemented for this substrate".into(),
        ))
    }

    // Display
    async fn capture_screen(&self, cap: &CapabilityToken) -> hakille_core::Result<ScreenCapture>;
    async fn inject_input(
        &self,
        action: InputAction,
        cap: &CapabilityToken,
    ) -> hakille_core::Result<()>;

    // Process
    async fn spawn_sandboxed(
        &self,
        wasm: &[u8],
        caps: Vec<CapabilityToken>,
    ) -> hakille_core::Result<ProcessHandle>;

    // Power & Thermal
    fn power_state(&self) -> PowerState;
    fn thermal_state(&self) -> ThermalState;
}
