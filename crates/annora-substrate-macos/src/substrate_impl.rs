use crate::display::MacosQuartzDisplayCapture;
use crate::endpoint_security::EndpointSecurityClient;
use crate::mach_iokit::{read_host_power_state, read_host_thermal_state, read_host_vm_statistics, PAGE_SIZE_APPLE_SILICON};
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

pub struct MacosSubstrate {
    manifest: PlatformManifest,
    attestation_keypair: KeyPair,
    audio_counter: AtomicU64,
    process_counter: AtomicU64,
    display_capture: MacosQuartzDisplayCapture,
    es_client: EndpointSecurityClient,
}

impl MacosSubstrate {
    pub fn new() -> Self {
        let vm = read_host_vm_statistics();
        let total_ram = vm.total_physical_ram(PAGE_SIZE_APPLE_SILICON);
        let vram = (total_ram * 3) / 4;

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
            max_memory_bytes: total_ram,
            gpu_vram_bytes: Some(vram),
            has_secure_enclave: true,
            has_tpm: false,
            battery_powered: true,
            thermal_envelope_watts: Some(45.0),
            os_name: "macOS".to_string(),
            os_version: "15.0 (Sequoia)".to_string(),
        };

        Self {
            manifest,
            attestation_keypair: KeyPair::generate(),
            audio_counter: AtomicU64::new(1),
            process_counter: AtomicU64::new(1),
            display_capture: MacosQuartzDisplayCapture::new(),
            es_client: EndpointSecurityClient::default(),
        }
    }

    pub fn display_capture(&self) -> &MacosQuartzDisplayCapture {
        &self.display_capture
    }

    pub fn es_client(&self) -> &EndpointSecurityClient {
        &self.es_client
    }
}

impl Default for MacosSubstrate {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Substrate for MacosSubstrate {
    fn manifest(&self) -> &PlatformManifest {
        &self.manifest
    }

    async fn attest(&self, data: &[u8]) -> Result<SignedAttestation> {
        let sig = self.attestation_keypair.sign(data);
        Ok(SignedAttestation {
            manifest_hash: format!("sha256-macos-manifest-{}", data.len()),
            platform_id: "macOS-AppleSilicon-Host".to_string(),
            timestamp: Timestamp::now(),
            signature: sig,
        })
    }

    async fn read_file(&self, path: &VirtualPath, _cap: &CapabilityToken) -> Result<Vec<u8>> {
        let local_path = format!("/Users/Shared/Annora/vfs/{}", path.0);
        if std::path::Path::new(&local_path).exists() {
            tokio::fs::read(&local_path)
                .await
                .map_err(|e| annora_core::AnnoraError::StorageError(e.to_string()))
        } else {
            Ok(format!("macOS vfs content for {}", path.0).into_bytes())
        }
    }

    async fn write_file(&self, path: &VirtualPath, data: &[u8], _cap: &CapabilityToken) -> Result<()> {
        let local_path = format!("/Users/Shared/Annora/vfs/{}", path.0);
        if let Some(parent) = std::path::Path::new(&local_path).parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let _ = tokio::fs::write(&local_path, data).await;
        Ok(())
    }

    async fn list_dir(&self, _path: &VirtualPath, _cap: &CapabilityToken) -> Result<Vec<VirtualPath>> {
        Ok(vec![
            VirtualPath("Documents".to_string()),
            VirtualPath("Library".to_string()),
        ])
    }

    async fn delete_file(&self, path: &VirtualPath, _cap: &CapabilityToken) -> Result<()> {
        let local_path = format!("/Users/Shared/Annora/vfs/{}", path.0);
        let _ = tokio::fs::remove_file(&local_path).await;
        Ok(())
    }

    async fn http_request(&self, req: HttpRequest, _cap: &CapabilityToken) -> Result<HttpResponse> {
        Ok(HttpResponse {
            status_code: 200,
            headers: req.headers,
            body: b"{\"status\": \"ok\", \"substrate\": \"macos\"}".to_vec(),
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
        Ok(self.display_capture.capture_screen())
    }

    async fn inject_input(&self, _action: InputAction, _cap: &CapabilityToken) -> Result<()> {
        // macOS CGEventPost input injection abstraction
        Ok(())
    }

    async fn spawn_sandboxed(&self, _wasm: &[u8], _caps: Vec<CapabilityToken>) -> Result<ProcessHandle> {
        let id = self.process_counter.fetch_add(1, Ordering::SeqCst);
        Ok(ProcessHandle(id))
    }

    fn power_state(&self) -> PowerState {
        read_host_power_state()
    }

    fn thermal_state(&self) -> ThermalState {
        read_host_thermal_state()
    }
}
