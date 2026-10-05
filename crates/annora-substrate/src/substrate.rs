use async_trait::async_trait;
use annora_core::{CapabilityToken, SignedAttestation, VirtualPath};
use crate::manifest::PlatformManifest;
use crate::attestation::{
    AudioConfig, AudioStreamHandle, HttpRequest, HttpResponse, InputAction, ProcessHandle,
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
    
    // Display
    async fn capture_screen(&self, cap: &CapabilityToken) -> annora_core::Result<ScreenCapture>;
    async fn inject_input(&self, action: InputAction, cap: &CapabilityToken) -> annora_core::Result<()>;
    
    // Process
    async fn spawn_sandboxed(&self, wasm: &[u8], caps: Vec<CapabilityToken>) -> annora_core::Result<ProcessHandle>;
    
    // Power & Thermal
    fn power_state(&self) -> PowerState;
    fn thermal_state(&self) -> ThermalState;
}
