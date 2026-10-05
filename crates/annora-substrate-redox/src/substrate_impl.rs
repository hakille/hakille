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

pub struct RedoxSubstrate {
    manifest: PlatformManifest,
    attestation_keypair: KeyPair,
    audio_counter: AtomicU64,
    process_counter: AtomicU64,
    hardware_killswitch_active: bool,
}

impl RedoxSubstrate {
    pub fn new() -> Self {
        let manifest = PlatformManifest {
            tier: Tier::Sovereign,
            capabilities_granted: vec![
                CapabilityScope::Filesystem,
                CapabilityScope::Network,
                CapabilityScope::GuiInput,
                CapabilityScope::Audio,
                CapabilityScope::Camera,
                CapabilityScope::ModelInference,
                CapabilityScope::FullSystem,
            ],
            capabilities_denied: vec![],
            max_memory_bytes: 64 * 1024 * 1024 * 1024,
            gpu_vram_bytes: None,
            has_secure_enclave: true,
            has_tpm: true,
            battery_powered: false,
            thermal_envelope_watts: Some(250.0),
            os_name: "Redox OS".to_string(),
            os_version: "0.9.0-microkernel".to_string(),
        };

        Self {
            manifest,
            attestation_keypair: KeyPair::generate(),
            audio_counter: AtomicU64::new(1),
            process_counter: AtomicU64::new(1),
            hardware_killswitch_active: false,
        }
    }

    pub fn set_killswitch(&mut self, active: bool) {
        self.hardware_killswitch_active = active;
    }

    pub fn is_killswitch_active(&self) -> bool {
        self.hardware_killswitch_active
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
        Ok(SignedAttestation {
            manifest_hash: format!("sha256-redox-sovereign-{}", data.len()),
            platform_id: "Redox-Sovereign-Dedicated-Node".to_string(),
            timestamp: Timestamp::now(),
            signature: sig,
        })
    }

    async fn read_file(&self, path: &VirtualPath, _cap: &CapabilityToken) -> Result<Vec<u8>> {
        if self.hardware_killswitch_active {
            return Err(annora_core::AnnoraError::SubstrateError("Hardware killswitch engaged".into()));
        }
        Ok(format!("Redox scheme:file:{}", path.0).into_bytes())
    }

    async fn write_file(&self, path: &VirtualPath, _data: &[u8], _cap: &CapabilityToken) -> Result<()> {
        if self.hardware_killswitch_active {
            return Err(annora_core::AnnoraError::SubstrateError("Hardware killswitch engaged".into()));
        }
        tracing::info!("Written to Redox scheme:file:{}", path.0);
        Ok(())
    }

    async fn list_dir(&self, _path: &VirtualPath, _cap: &CapabilityToken) -> Result<Vec<VirtualPath>> {
        Ok(vec![
            VirtualPath("scheme:file".to_string()),
            VirtualPath("scheme:annora".to_string()),
        ])
    }

    async fn delete_file(&self, _path: &VirtualPath, _cap: &CapabilityToken) -> Result<()> {
        Ok(())
    }

    async fn http_request(&self, req: HttpRequest, _cap: &CapabilityToken) -> Result<HttpResponse> {
        Ok(HttpResponse {
            status_code: 200,
            headers: req.headers,
            body: b"{\"status\": \"ok\", \"substrate\": \"redox-native\"}".to_vec(),
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
            width: 1920,
            height: 1080,
            data: vec![0u8; 1920 * 1080 * 4],
            format: ImageFormat::Rgba8,
        })
    }

    async fn inject_input(&self, _action: InputAction, _cap: &CapabilityToken) -> Result<()> {
        Ok(())
    }

    async fn spawn_sandboxed(&self, _wasm: &[u8], _caps: Vec<CapabilityToken>) -> Result<ProcessHandle> {
        let id = self.process_counter.fetch_add(1, Ordering::SeqCst);
        Ok(ProcessHandle(id))
    }

    fn power_state(&self) -> PowerState {
        PowerState {
            on_battery: false,
            battery_percent: 100.0,
            charging: false,
        }
    }

    fn thermal_state(&self) -> ThermalState {
        ThermalState {
            temperature_celsius: 32.0,
            throttled: false,
        }
    }
}
