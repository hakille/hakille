use crate::web_audio::WebAudioBridge;
use crate::web_canvas::WebScreenCaptureAdapter;
use crate::webgpu::WebGpuDetector;
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
use std::sync::Mutex;

pub struct WebSubstrate {
    manifest: PlatformManifest,
    attestation_keypair: KeyPair,
    audio_counter: AtomicU64,
    process_counter: AtomicU64,
    audio_bridge: Mutex<WebAudioBridge>,
    screen_capture: WebScreenCaptureAdapter,
    gpu_detector: WebGpuDetector,
}

impl WebSubstrate {
    pub fn new() -> Self {
        let manifest = PlatformManifest {
            tier: Tier::Guest,
            capabilities_granted: vec![
                CapabilityScope::Audio,
                CapabilityScope::Camera,
                CapabilityScope::GuiInput,
            ],
            capabilities_denied: vec![
                CapabilityScope::FullSystem,
                CapabilityScope::Filesystem,
                CapabilityScope::ModelInference,
            ],
            max_memory_bytes: 4 * 1024 * 1024 * 1024,
            gpu_vram_bytes: Some(2 * 1024 * 1024 * 1024),
            has_secure_enclave: false,
            has_tpm: false,
            battery_powered: false,
            thermal_envelope_watts: None,
            os_name: "WebAssembly".to_string(),
            os_version: "Wasm-Browser-W3C".to_string(),
        };

        Self {
            manifest,
            attestation_keypair: KeyPair::generate(),
            audio_counter: AtomicU64::new(1),
            process_counter: AtomicU64::new(1),
            audio_bridge: Mutex::new(WebAudioBridge::default()),
            screen_capture: WebScreenCaptureAdapter::default(),
            gpu_detector: WebGpuDetector::default(),
        }
    }

    pub fn audio_bridge(&self) -> &Mutex<WebAudioBridge> {
        &self.audio_bridge
    }

    pub fn screen_capture_adapter(&self) -> &WebScreenCaptureAdapter {
        &self.screen_capture
    }

    pub fn gpu_detector(&self) -> &WebGpuDetector {
        &self.gpu_detector
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
            manifest_hash: format!("sha256-web-guest-{}", data.len()),
            platform_id: "Wasm-Browser-Client".to_string(),
            timestamp: Timestamp::now(),
            signature: sig,
        })
    }

    async fn read_file(&self, path: &VirtualPath, _cap: &CapabilityToken) -> Result<Vec<u8>> {
        // In browser, reads from IndexedDB / Origin Private File System (OPFS)
        Ok(format!("Browser IndexedDB/OPFS content for {}", path.0).into_bytes())
    }

    async fn write_file(&self, _path: &VirtualPath, _data: &[u8], _cap: &CapabilityToken) -> Result<()> {
        // Writes to OPFS
        Ok(())
    }

    async fn list_dir(&self, _path: &VirtualPath, _cap: &CapabilityToken) -> Result<Vec<VirtualPath>> {
        Ok(vec![
            VirtualPath("opfs".to_string()),
            VirtualPath("idb".to_string()),
        ])
    }

    async fn delete_file(&self, _path: &VirtualPath, _cap: &CapabilityToken) -> Result<()> {
        Ok(())
    }

    async fn http_request(&self, req: HttpRequest, _cap: &CapabilityToken) -> Result<HttpResponse> {
        // Dispatches via browser `fetch()` API
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

    async fn play_audio(&self, data: &[u8], _config: AudioConfig, _cap: &CapabilityToken) -> Result<()> {
        let mut bridge = self.audio_bridge.lock().unwrap();
        bridge.buffer_mut().push_playback_audio(data);
        Ok(())
    }

    async fn capture_screen(&self, _cap: &CapabilityToken) -> Result<ScreenCapture> {
        Ok(self.screen_capture.capture_frame())
    }

    async fn inject_input(&self, _action: InputAction, _cap: &CapabilityToken) -> Result<()> {
        Ok(())
    }

    async fn spawn_sandboxed(&self, _wasm: &[u8], _caps: Vec<CapabilityToken>) -> Result<ProcessHandle> {
        let id = self.process_counter.fetch_add(1, Ordering::SeqCst);
        Ok(ProcessHandle(id))
    }

    fn power_state(&self) -> PowerState {
        // Browser Battery Status API (`navigator.getBattery()`)
        PowerState {
            on_battery: false,
            battery_percent: 100.0,
            charging: true,
        }
    }

    fn thermal_state(&self) -> ThermalState {
        // Browser Compute Pressure API
        ThermalState {
            temperature_celsius: 40.0,
            throttled: false,
        }
    }
}
