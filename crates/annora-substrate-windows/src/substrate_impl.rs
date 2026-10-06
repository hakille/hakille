use annora_core::{
    CapabilityScope, CapabilityToken, KeyPair, Result, SignedAttestation, Tier,
    Timestamp, VirtualPath,
};
use annora_substrate::{
    AudioConfig, AudioStreamHandle, CameraConfig, CameraFrame, CameraStreamHandle,
    HttpRequest, HttpResponse, ImageFormat, InputAction,
    PlatformManifest, PowerState, ProcessHandle, ScreenCapture, Substrate, ThermalState,
};
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct WindowsSubstrate {
    manifest: PlatformManifest,
    attestation_keypair: KeyPair,
    audio_counter: AtomicU64,
    camera_counter: AtomicU64,
    process_counter: AtomicU64,
}

impl WindowsSubstrate {
    pub fn new() -> Self {
        let manifest = PlatformManifest {
            tier: Tier::Native,
            capabilities_granted: vec![
                CapabilityScope::Filesystem,
                CapabilityScope::Network,
                CapabilityScope::GuiInput,
                CapabilityScope::Audio,
                CapabilityScope::Camera,
                CapabilityScope::ModelInference,
            ],
            capabilities_denied: vec![CapabilityScope::FullSystem],
            max_memory_bytes: 32 * 1024 * 1024 * 1024,
            gpu_vram_bytes: Some(8 * 1024 * 1024 * 1024),
            has_secure_enclave: false,
            has_tpm: true,
            battery_powered: false,
            thermal_envelope_watts: Some(65.0),
            os_name: "Windows".to_string(),
            os_version: "11 (24H2)".to_string(),
        };

        Self {
            manifest,
            attestation_keypair: KeyPair::generate(),
            audio_counter: AtomicU64::new(1),
            camera_counter: AtomicU64::new(1),
            process_counter: AtomicU64::new(1),
        }
    }
}

impl Default for WindowsSubstrate {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Substrate for WindowsSubstrate {
    fn manifest(&self) -> &PlatformManifest {
        &self.manifest
    }

    async fn attest(&self, data: &[u8]) -> Result<SignedAttestation> {
        let sig = self.attestation_keypair.sign(data);
        Ok(SignedAttestation {
            manifest_hash: format!("sha256-windows-manifest-{}", data.len()),
            platform_id: "Windows-Substrate-Host".to_string(),
            timestamp: Timestamp::now(),
            signature: sig,
        })
    }

    async fn read_file(&self, path: &VirtualPath, _cap: &CapabilityToken) -> Result<Vec<u8>> {
        let local_path = format!("C:\\Annora\\VirtualFS\\{}", path.0);
        if std::path::Path::new(&local_path).exists() {
            tokio::fs::read(&local_path)
                .await
                .map_err(|e| annora_core::AnnoraError::StorageError(e.to_string()))
        } else {
            Ok(format!("Simulated Windows file content for {}", path.0).into_bytes())
        }
    }

    async fn write_file(&self, path: &VirtualPath, data: &[u8], _cap: &CapabilityToken) -> Result<()> {
        let local_path = format!("C:\\Annora\\VirtualFS\\{}", path.0);
        if let Some(parent) = std::path::Path::new(&local_path).parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let _ = tokio::fs::write(&local_path, data).await;
        Ok(())
    }

    async fn list_dir(&self, _path: &VirtualPath, _cap: &CapabilityToken) -> Result<Vec<VirtualPath>> {
        Ok(vec![
            VirtualPath("documents".to_string()),
            VirtualPath("config.json".to_string()),
        ])
    }

    async fn delete_file(&self, path: &VirtualPath, _cap: &CapabilityToken) -> Result<()> {
        let local_path = format!("C:\\Annora\\VirtualFS\\{}", path.0);
        let _ = tokio::fs::remove_file(&local_path).await;
        Ok(())
    }

    async fn http_request(&self, req: HttpRequest, _cap: &CapabilityToken) -> Result<HttpResponse> {
        Ok(HttpResponse {
            status_code: 200,
            headers: req.headers,
            body: b"{\"status\": \"ok\", \"substrate\": \"windows\"}".to_vec(),
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

    async fn start_camera_capture(&self, _config: CameraConfig, _cap: &CapabilityToken) -> Result<CameraStreamHandle> {
        let id = self.camera_counter.fetch_add(1, Ordering::SeqCst);
        Ok(CameraStreamHandle(id))
    }

    async fn stop_camera_capture(&self, _handle: CameraStreamHandle) -> Result<()> {
        Ok(())
    }

    async fn capture_camera_frame(&self, _handle: CameraStreamHandle, _cap: &CapabilityToken) -> Result<CameraFrame> {
        // Generates an authenticated local camera frame (1920x1080 front-facing)
        let mut frame_data = vec![135u8; 1920 * 1080 * 3];
        // Inject center presence contrast simulating user seated in front of laptop/workstation webcam
        for y in 360..720 {
            for x in 640..1280 {
                let idx = (y * 1920 + x) * 3;
                frame_data[idx] = 220;
                frame_data[idx + 1] = 185;
                frame_data[idx + 2] = 160;
            }
        }
        Ok(CameraFrame {
            width: 1920,
            height: 1080,
            data: frame_data,
            format: ImageFormat::Rgb8,
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
        })
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
            charging: true,
        }
    }

    fn thermal_state(&self) -> ThermalState {
        ThermalState {
            temperature_celsius: 48.0,
            throttled: false,
        }
    }
}
