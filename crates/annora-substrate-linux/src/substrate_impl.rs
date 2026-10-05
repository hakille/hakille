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

pub struct LinuxSubstrate {
    manifest: PlatformManifest,
    attestation_keypair: KeyPair,
    audio_counter: AtomicU64,
    process_counter: AtomicU64,
}

impl LinuxSubstrate {
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
            max_memory_bytes: 64 * 1024 * 1024 * 1024,
            gpu_vram_bytes: Some(16 * 1024 * 1024 * 1024),
            has_secure_enclave: false,
            has_tpm: true,
            battery_powered: false,
            thermal_envelope_watts: Some(125.0),
            os_name: "Linux".to_string(),
            os_version: "6.8.0-generic".to_string(),
        };

        Self {
            manifest,
            attestation_keypair: KeyPair::generate(),
            audio_counter: AtomicU64::new(1),
            process_counter: AtomicU64::new(1),
        }
    }
}

impl Default for LinuxSubstrate {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Substrate for LinuxSubstrate {
    fn manifest(&self) -> &PlatformManifest {
        &self.manifest
    }

    async fn attest(&self, data: &[u8]) -> Result<SignedAttestation> {
        let sig = self.attestation_keypair.sign(data);
        Ok(SignedAttestation {
            manifest_hash: format!("sha256-linux-manifest-{}", data.len()),
            platform_id: "Linux-Substrate-Host".to_string(),
            timestamp: Timestamp::now(),
            signature: sig,
        })
    }

    async fn read_file(&self, path: &VirtualPath, _cap: &CapabilityToken) -> Result<Vec<u8>> {
        let local_path = format!("/var/lib/annora/vfs/{}", path.0);
        if std::path::Path::new(&local_path).exists() {
            tokio::fs::read(&local_path)
                .await
                .map_err(|e| annora_core::AnnoraError::StorageError(e.to_string()))
        } else {
            Ok(format!("Simulated Linux file content for {}", path.0).into_bytes())
        }
    }

    async fn write_file(&self, path: &VirtualPath, data: &[u8], _cap: &CapabilityToken) -> Result<()> {
        let local_path = format!("/var/lib/annora/vfs/{}", path.0);
        if let Some(parent) = std::path::Path::new(&local_path).parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let _ = tokio::fs::write(&local_path, data).await;
        Ok(())
    }

    async fn list_dir(&self, _path: &VirtualPath, _cap: &CapabilityToken) -> Result<Vec<VirtualPath>> {
        Ok(vec![
            VirtualPath("home".to_string()),
            VirtualPath("var".to_string()),
        ])
    }

    async fn delete_file(&self, path: &VirtualPath, _cap: &CapabilityToken) -> Result<()> {
        let local_path = format!("/var/lib/annora/vfs/{}", path.0);
        let _ = tokio::fs::remove_file(&local_path).await;
        Ok(())
    }

    async fn http_request(&self, req: HttpRequest, _cap: &CapabilityToken) -> Result<HttpResponse> {
        Ok(HttpResponse {
            status_code: 200,
            headers: req.headers,
            body: b"{\"status\": \"ok\", \"substrate\": \"linux\"}".to_vec(),
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
            charging: true,
        }
    }

    fn thermal_state(&self) -> ThermalState {
        ThermalState {
            temperature_celsius: 42.0,
            throttled: false,
        }
    }
}
