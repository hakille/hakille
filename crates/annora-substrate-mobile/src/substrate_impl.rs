use annora_core::{
    CapabilityScope, CapabilityToken, KeyPair, Result, SignedAttestation, Tier,
    Timestamp, VirtualPath,
};
use annora_substrate::{
    AudioConfig, AudioStreamHandle, HttpRequest, HttpResponse, ImageFormat, InputAction,
    PlatformManifest, PowerState, ProcessHandle, ScreenCapture, Substrate, ThermalState,
};
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct MobileSubstrate {
    manifest: PlatformManifest,
    attestation_keypair: KeyPair,
    audio_counter: AtomicU64,
}

impl MobileSubstrate {
    pub fn new() -> Self {
        let manifest = PlatformManifest {
            tier: Tier::Guest,
            capabilities_granted: vec![
                CapabilityScope::Audio,
                CapabilityScope::Camera,
                CapabilityScope::Sensor,
                CapabilityScope::Network,
            ],
            capabilities_denied: vec![
                CapabilityScope::FullSystem,
                CapabilityScope::Filesystem,
                CapabilityScope::GuiInput,
            ],
            max_memory_bytes: 8 * 1024 * 1024 * 1024,
            gpu_vram_bytes: Some(2 * 1024 * 1024 * 1024),
            has_secure_enclave: true,
            has_tpm: false,
            battery_powered: true,
            thermal_envelope_watts: Some(5.0),
            os_name: "Mobile (Android/iOS)".to_string(),
            os_version: "Scoped Sandbox".to_string(),
        };

        Self {
            manifest,
            attestation_keypair: KeyPair::generate(),
            audio_counter: AtomicU64::new(1),
        }
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
            manifest_hash: format!("sha256-mobile-strongbox-{}", data.len()),
            platform_id: "Mobile-Guest-Device".to_string(),
            timestamp: Timestamp::now(),
            signature: sig,
        })
    }

    async fn read_file(&self, path: &VirtualPath, _cap: &CapabilityToken) -> Result<Vec<u8>> {
        // Scoped storage sandbox
        Ok(format!("Scoped storage: {}", path.0).into_bytes())
    }

    async fn write_file(&self, _path: &VirtualPath, _data: &[u8], _cap: &CapabilityToken) -> Result<()> {
        Ok(())
    }

    async fn list_dir(&self, _path: &VirtualPath, _cap: &CapabilityToken) -> Result<Vec<VirtualPath>> {
        Ok(vec![VirtualPath("scoped_documents".to_string())])
    }

    async fn delete_file(&self, _path: &VirtualPath, _cap: &CapabilityToken) -> Result<()> {
        Ok(())
    }

    async fn http_request(&self, req: HttpRequest, _cap: &CapabilityToken) -> Result<HttpResponse> {
        Ok(HttpResponse {
            status_code: 200,
            headers: req.headers,
            body: b"{\"status\": \"ok\", \"substrate\": \"mobile-guest\"}".to_vec(),
        })
    }

    async fn start_audio_capture(&self, _config: AudioConfig, _cap: &CapabilityToken) -> Result<AudioStreamHandle> {
        let id = self.audio_counter.fetch_add(1, Ordering::SeqCst);
        Ok(AudioStreamHandle(id))
    }

    async fn stop_audio_capture(&self, _handle: AudioStreamHandle) -> Result<()> {
        Ok(())
    }

    async fn play_audio(&self, _data: &[u8], _config: AudioConfig, _cap: &CapabilityToken) -> Result<()> {
        Ok(())
    }

    async fn capture_screen(&self, _cap: &CapabilityToken) -> Result<ScreenCapture> {
        Ok(ScreenCapture {
            width: 1080,
            height: 2400,
            data: vec![0u8; 1080 * 2400 * 4],
            format: ImageFormat::Rgba8,
        })
    }

    async fn inject_input(&self, _action: InputAction, _cap: &CapabilityToken) -> Result<()> {
        Err(annora_core::AnnoraError::CapabilityDenied(
            "Mobile OS strictly prohibits arbitrary input injection".into(),
        ))
    }

    async fn spawn_sandboxed(&self, _wasm: &[u8], _caps: Vec<CapabilityToken>) -> Result<ProcessHandle> {
        Err(annora_core::AnnoraError::CapabilityDenied(
            "Process spawn is denied on mobile substrate".into(),
        ))
    }

    fn power_state(&self) -> PowerState {
        PowerState {
            on_battery: true,
            battery_percent: 74.0,
            charging: false,
        }
    }

    fn thermal_state(&self) -> ThermalState {
        ThermalState {
            temperature_celsius: 36.5,
            throttled: false,
        }
    }
}
