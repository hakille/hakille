use annora_core::{
    CapabilityScope, CapabilityToken, KeyPair, Result, SignedAttestation, Tier,
    Timestamp, VirtualPath,
};
use annora_substrate::{
    AudioConfig, AudioStreamHandle, HttpRequest, HttpResponse, InputAction,
    PlatformManifest, PowerState, ProcessHandle, ScreenCapture, Substrate, ThermalState,
};
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct WebSubstrate {
    manifest: PlatformManifest,
    attestation_keypair: KeyPair,
    audio_counter: AtomicU64,
}

impl WebSubstrate {
    pub fn new() -> Self {
        let manifest = PlatformManifest {
            tier: Tier::Guest,
            capabilities_granted: vec![
                CapabilityScope::Audio,
                CapabilityScope::Network,
            ],
            capabilities_denied: vec![
                CapabilityScope::FullSystem,
                CapabilityScope::Filesystem,
                CapabilityScope::GuiInput,
                CapabilityScope::Camera,
            ],
            max_memory_bytes: 2 * 1024 * 1024 * 1024,
            gpu_vram_bytes: Some(1024 * 1024 * 1024),
            has_secure_enclave: false,
            has_tpm: false,
            battery_powered: false,
            thermal_envelope_watts: None,
            os_name: "Web Browser".to_string(),
            os_version: "WASM / WebGPU Sandbox".to_string(),
        };

        Self {
            manifest,
            attestation_keypair: KeyPair::generate(),
            audio_counter: AtomicU64::new(1),
        }
    }
}

impl Default for WebSubstrate {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Substrate for WebSubstrate {
    fn manifest(&self) -> &PlatformManifest {
        &self.manifest
    }

    async fn attest(&self, data: &[u8]) -> Result<SignedAttestation> {
        let sig = self.attestation_keypair.sign(data);
        Ok(SignedAttestation {
            manifest_hash: format!("sha256-web-crypto-{}", data.len()),
            platform_id: "Browser-Wasm-Guest".to_string(),
            timestamp: Timestamp::now(),
            signature: sig,
        })
    }

    async fn read_file(&self, path: &VirtualPath, _cap: &CapabilityToken) -> Result<Vec<u8>> {
        Ok(format!("localStorage[{}]", path.0).into_bytes())
    }

    async fn write_file(&self, _path: &VirtualPath, _data: &[u8], _cap: &CapabilityToken) -> Result<()> {
        Ok(())
    }

    async fn list_dir(&self, _path: &VirtualPath, _cap: &CapabilityToken) -> Result<Vec<VirtualPath>> {
        Ok(vec![VirtualPath("indexed_db".to_string())])
    }

    async fn delete_file(&self, _path: &VirtualPath, _cap: &CapabilityToken) -> Result<()> {
        Ok(())
    }

    async fn http_request(&self, req: HttpRequest, _cap: &CapabilityToken) -> Result<HttpResponse> {
        Ok(HttpResponse {
            status_code: 200,
            headers: req.headers,
            body: b"{\"status\": \"ok\", \"substrate\": \"web-fetch\"}".to_vec(),
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
        Err(annora_core::AnnoraError::CapabilityDenied(
            "Screen capture is denied in browser sandbox".into(),
        ))
    }

    async fn inject_input(&self, _action: InputAction, _cap: &CapabilityToken) -> Result<()> {
        Err(annora_core::AnnoraError::CapabilityDenied(
            "Input injection is impossible in web sandbox".into(),
        ))
    }

    async fn spawn_sandboxed(&self, _wasm: &[u8], _caps: Vec<CapabilityToken>) -> Result<ProcessHandle> {
        Err(annora_core::AnnoraError::CapabilityDenied(
            "Process spawn is unsupported in web browser sandbox".into(),
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
        ThermalState {
            temperature_celsius: 25.0,
            throttled: false,
        }
    }
}
