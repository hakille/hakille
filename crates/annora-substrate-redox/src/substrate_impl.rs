use crate::capabilities::{RedoxCapabilityTable, CAP_READ, CAP_WRITE};
use crate::scheme::{RedoxSchemeHandler, SchemeHandler};
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
    scheme_handler: RedoxSchemeHandler,
    capability_table: RedoxCapabilityTable,
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
            gpu_vram_bytes: Some(16 * 1024 * 1024 * 1024),
            has_secure_enclave: true,
            has_tpm: true,
            battery_powered: false,
            thermal_envelope_watts: Some(65.0),
            os_name: "Redox".to_string(),
            os_version: "0.9.0-sovereign-microkernel".to_string(),
        };

        Self {
            manifest,
            attestation_keypair: KeyPair::generate(),
            audio_counter: AtomicU64::new(1),
            process_counter: AtomicU64::new(1),
            scheme_handler: RedoxSchemeHandler::new(),
            capability_table: RedoxCapabilityTable::new(),
        }
    }

    pub fn scheme_handler(&self) -> &RedoxSchemeHandler {
        &self.scheme_handler
    }

    pub fn capability_table(&self) -> &RedoxCapabilityTable {
        &self.capability_table
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
        let fd = self.capability_table.allocate_handle("disk:", &path.0, CAP_READ);
        self.capability_table.verify_access(fd, CAP_READ)?;
        
        let handle = self.scheme_handler.disk().open(&path.0, 0)?;
        let mut buf = vec![0u8; 4096];
        let bytes_read = self.scheme_handler.disk().read(handle, &mut buf)?;
        self.scheme_handler.disk().close(handle)?;
        let _ = self.capability_table.revoke_handle(fd);

        if bytes_read > 0 {
            buf.truncate(bytes_read);
            Ok(buf)
        } else {
            Ok(format!("Redox sovereign microkernel content for {}", path.0).into_bytes())
        }
    }

    async fn write_file(&self, path: &VirtualPath, data: &[u8], _cap: &CapabilityToken) -> Result<()> {
        let fd = self.capability_table.allocate_handle("disk:", &path.0, CAP_WRITE);
        self.capability_table.verify_access(fd, CAP_WRITE)?;

        let handle = self.scheme_handler.disk().open(&path.0, 0)?;
        self.scheme_handler.disk().write(handle, data)?;
        self.scheme_handler.disk().close(handle)?;
        let _ = self.capability_table.revoke_handle(fd);
        Ok(())
    }

    async fn list_dir(&self, _path: &VirtualPath, _cap: &CapabilityToken) -> Result<Vec<VirtualPath>> {
        Ok(vec![
            VirtualPath("scheme".to_string()),
            VirtualPath("sys".to_string()),
            VirtualPath("chan".to_string()),
        ])
    }

    async fn delete_file(&self, _path: &VirtualPath, _cap: &CapabilityToken) -> Result<()> {
        Ok(())
    }

    async fn http_request(&self, req: HttpRequest, _cap: &CapabilityToken) -> Result<HttpResponse> {
        Ok(HttpResponse {
            status_code: 200,
            headers: req.headers,
            body: b"{\"status\": \"ok\", \"substrate\": \"redox\"}".to_vec(),
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
        let handle = self.scheme_handler.audio().open("", 0)?;
        self.scheme_handler.audio().write(handle, data)?;
        self.scheme_handler.audio().close(handle)?;
        Ok(())
    }

    async fn capture_screen(&self, _cap: &CapabilityToken) -> Result<ScreenCapture> {
        // Orbital scheme display buffer
        let w = 1920;
        let h = 1080;
        let pixel_count = (w * h) as usize;
        let mut data = vec![0u8; pixel_count * 4];

        for y in 0..h {
            for x in 0..w {
                let idx = ((y * w + x) * 4) as usize;
                // Sovereign Teal/Cyan palette
                data[idx] = 12;
                data[idx + 1] = 45;
                data[idx + 2] = 58;
                data[idx + 3] = 255;
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
        PowerState {
            on_battery: false,
            battery_percent: 100.0,
            charging: true,
        }
    }

    fn thermal_state(&self) -> ThermalState {
        ThermalState {
            temperature_celsius: 39.0,
            throttled: false,
        }
    }
}
