use crate::battery_conservation::BatteryConservationEngine;
use crate::scoped_storage::{ScopedStorageSandbox, StorageScope};
use crate::shadow::MobileShadowSync;
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
use std::sync::Mutex;

pub struct MobileSubstrate {
    manifest: PlatformManifest,
    attestation_keypair: KeyPair,
    audio_counter: AtomicU64,
    process_counter: AtomicU64,
    shadow_sync: Mutex<MobileShadowSync>,
    storage_sandbox: ScopedStorageSandbox,
    battery_engine: Mutex<BatteryConservationEngine>,
}

impl MobileSubstrate {
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
            gpu_vram_bytes: None,
            has_secure_enclave: true,
            has_tpm: false,
            battery_powered: true,
            thermal_envelope_watts: Some(5.0),
            os_name: "Mobile-Edge".to_string(),
            os_version: "iOS/Android-Shadow".to_string(),
        };

        Self {
            manifest,
            attestation_keypair: KeyPair::generate(),
            audio_counter: AtomicU64::new(1),
            process_counter: AtomicU64::new(1),
            shadow_sync: Mutex::new(MobileShadowSync::default()),
            storage_sandbox: ScopedStorageSandbox::default(),
            battery_engine: Mutex::new(BatteryConservationEngine::default()),
        }
    }

    pub fn storage_sandbox(&self) -> &ScopedStorageSandbox {
        &self.storage_sandbox
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
            manifest_hash: format!("sha256-mobile-shadow-{}", data.len()),
            platform_id: "Mobile-Edge-Shadow-Terminal".to_string(),
            timestamp: Timestamp::now(),
            signature: sig,
        })
    }

    async fn read_file(&self, path: &VirtualPath, _cap: &CapabilityToken) -> Result<Vec<u8>> {
        let _resolved = self.storage_sandbox.resolve_path(StorageScope::Documents, path)?;
        Ok(format!("Mobile scoped document content for {}", path.0).into_bytes())
    }

    async fn write_file(&self, path: &VirtualPath, data: &[u8], _cap: &CapabilityToken) -> Result<()> {
        let _resolved = self.storage_sandbox.resolve_path(StorageScope::Documents, path)?;
        let mut sync = self.shadow_sync.lock().unwrap();
        sync.queue_local_delta(&path.0, data);
        Ok(())
    }

    async fn list_dir(&self, _path: &VirtualPath, _cap: &CapabilityToken) -> Result<Vec<VirtualPath>> {
        Ok(vec![
            VirtualPath("Documents".to_string()),
            VirtualPath("Cache".to_string()),
        ])
    }

    async fn delete_file(&self, path: &VirtualPath, _cap: &CapabilityToken) -> Result<()> {
        let _resolved = self.storage_sandbox.resolve_path(StorageScope::Documents, path)?;
        Ok(())
    }

    async fn http_request(&self, req: HttpRequest, _cap: &CapabilityToken) -> Result<HttpResponse> {
        Ok(HttpResponse {
            status_code: 200,
            headers: req.headers,
            body: b"{\"status\": \"ok\", \"substrate\": \"mobile-shadow\"}".to_vec(),
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
        // Mobile portrait framebuffer (1170 x 2532)
        let w = 1170;
        let h = 2532;
        let pixel_count = (w * h) as usize;
        let mut data = vec![0u8; pixel_count * 4];

        for y in 0..h {
            for x in 0..w {
                let idx = ((y * w + x) * 4) as usize;
                // Mobile OLED true black base with soft indicator
                if y < 80 {
                    // Status bar notch / dynamic island
                    data[idx] = 10;
                    data[idx + 1] = 10;
                    data[idx + 2] = 12;
                    data[idx + 3] = 255;
                } else {
                    data[idx] = 18;
                    data[idx + 1] = 18;
                    data[idx + 2] = 22;
                    data[idx + 3] = 255;
                }
            }
        }

        Ok(ScreenCapture {
            width: w,
            height: h,
            data,
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
        let eng = self.battery_engine.lock().unwrap();
        PowerState {
            on_battery: true,
            battery_percent: 85.0,
            charging: eng.current_tier() == crate::battery_conservation::BatteryTier::Charging,
        }
    }

    fn thermal_state(&self) -> ThermalState {
        ThermalState {
            temperature_celsius: 36.5,
            throttled: false,
        }
    }
}
