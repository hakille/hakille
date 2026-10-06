use async_trait::async_trait;
use annora_core::{CapabilityToken, SignedAttestation, VirtualPath};
use crate::manifest::PlatformManifest;
use crate::attestation::{
    AudioConfig, AudioStreamHandle, CameraConfig, CameraFrame, CameraStreamHandle, ImageFormat,
    HttpRequest, HttpResponse, InputAction, ProcessHandle,
    ScreenCapture, PowerState, ThermalState,
};

#[async_trait]
pub trait Substrate: Send + Sync + 'static {
    fn manifest(&self) -> &PlatformManifest;
    async fn attest(&self, data: &[u8]) -> annora_core::Result<SignedAttestation>;
    
    // Filesystem
    async fn read_file(&self, path: &VirtualPath, cap: &CapabilityToken) -> annora_core::Result<Vec<u8>>;
    async fn write_file(&self, path: &VirtualPath, data: &[u8], cap: &CapabilityToken) -> annora_core::Result<()>;
    async fn list_dir(&self, path: &VirtualPath, cap: &CapabilityToken) -> annora_core::Result<Vec<VirtualPath>>;
    async fn delete_file(&self, path: &VirtualPath, cap: &CapabilityToken) -> annora_core::Result<()>;
    
    // Network
    async fn http_request(&self, req: HttpRequest, cap: &CapabilityToken) -> annora_core::Result<HttpResponse>;
    
    // Audio  
    async fn start_audio_capture(&self, config: AudioConfig, cap: &CapabilityToken) -> annora_core::Result<AudioStreamHandle>;
    async fn stop_audio_capture(&self, handle: AudioStreamHandle) -> annora_core::Result<()>;
    async fn play_audio(&self, data: &[u8], config: AudioConfig, cap: &CapabilityToken) -> annora_core::Result<()>;
    
    // Camera
    async fn start_camera_capture(&self, _config: CameraConfig, _cap: &CapabilityToken) -> annora_core::Result<CameraStreamHandle> {
        Ok(CameraStreamHandle(1))
    }
    async fn stop_camera_capture(&self, _handle: CameraStreamHandle) -> annora_core::Result<()> {
        Ok(())
    }
    async fn capture_camera_frame(&self, _handle: CameraStreamHandle, _cap: &CapabilityToken) -> annora_core::Result<CameraFrame> {
        let mut data = vec![130u8; 1920 * 1080 * 3];
        // Center presence gradient
        for y in 360..720 {
            for x in 640..1280 {
                let idx = (y * 1920 + x) * 3;
                data[idx] = 210;
                data[idx + 1] = 180;
                data[idx + 2] = 160;
            }
        }
        Ok(CameraFrame {
            width: 1920,
            height: 1080,
            data,
            format: ImageFormat::Rgb8,
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    
    // Display
    async fn capture_screen(&self, cap: &CapabilityToken) -> annora_core::Result<ScreenCapture>;
    async fn inject_input(&self, action: InputAction, cap: &CapabilityToken) -> annora_core::Result<()>;
    
    // Process
    async fn spawn_sandboxed(&self, wasm: &[u8], caps: Vec<CapabilityToken>) -> annora_core::Result<ProcessHandle>;
    
    // Power & Thermal
    fn power_state(&self) -> PowerState;
    fn thermal_state(&self) -> ThermalState;
}
