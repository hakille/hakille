//! Multi-substrate host abstraction and dynamic dispatch for Hakille daemon.
//!
//! Instantiates native OS substrate based on `cfg(target_os)` or explicit runtime override.

use async_trait::async_trait;
use hakille_core::{CapabilityToken, Result, SignedAttestation, VirtualPath};
use hakille_substrate::{
    Compute, Substrate,
    attestation::{
        AudioConfig, AudioStreamHandle, CameraConfig, CameraFrame, CameraStreamHandle,
        ComputeBackend, HttpRequest, HttpResponse, InputAction, PowerState, ProcessHandle,
        ScreenCapture, ThermalState,
    },
    manifest::PlatformManifest,
};

/// Unified host substrate enum wrapping all concrete platform substrates.
pub enum HostSubstrate {
    Linux(hakille_substrate_linux::LinuxSubstrate),
    Windows(hakille_substrate_windows::WindowsSubstrate),
    Macos(hakille_substrate_macos::MacosSubstrate),
    Redox(hakille_substrate_redox::RedoxSubstrate),
    Mobile(hakille_substrate_mobile::MobileSubstrate),
    Web(hakille_substrate_web::WebSubstrate),
}

/// Unified host compute backend enum wrapping all platform compute backends.
pub enum HostComputeBackend {
    Linux(hakille_substrate_linux::LinuxComputeBackend),
    Windows(hakille_substrate_windows::WindowsComputeBackend),
    Macos(hakille_substrate_macos::MacosComputeBackend),
    Redox(hakille_substrate_redox::RedoxComputeBackend),
    Mobile(hakille_substrate_mobile::MobileComputeBackend),
    Web(hakille_substrate_web::WebComputeBackend),
}

impl HostSubstrate {
    /// Pin the Biscuit root key across any underlying substrate.
    #[allow(dead_code)]
    pub fn with_trusted_root(self, root: biscuit_auth::PublicKey) -> Self {
        match self {
            Self::Linux(s) => Self::Linux(s.with_trusted_root(root)),
            Self::Windows(s) => Self::Windows(s.with_trusted_root(root)),
            Self::Macos(s) => Self::Macos(s.with_trusted_root(root)),
            Self::Redox(s) => Self::Redox(s.with_trusted_root(root)),
            Self::Mobile(s) => Self::Mobile(s.with_trusted_root(root)),
            Self::Web(s) => Self::Web(s.with_trusted_root(root)),
        }
    }

    /// Instantiate substrate explicitly by name, or fall back to auto-detection.
    pub fn from_name(
        name: &str,
        root: biscuit_auth::PublicKey,
    ) -> std::result::Result<Self, String> {
        match name.to_ascii_lowercase().as_str() {
            "linux" => Ok(Self::Linux(
                hakille_substrate_linux::LinuxSubstrate::new().with_trusted_root(root),
            )),
            "windows" => Ok(Self::Windows(
                hakille_substrate_windows::WindowsSubstrate::new().with_trusted_root(root),
            )),
            "macos" => Ok(Self::Macos(
                hakille_substrate_macos::MacosSubstrate::new().with_trusted_root(root),
            )),
            "redox" => Ok(Self::Redox(
                hakille_substrate_redox::RedoxSubstrate::new().with_trusted_root(root),
            )),
            "mobile" => Ok(Self::Mobile(
                hakille_substrate_mobile::MobileSubstrate::new().with_trusted_root(root),
            )),
            "web" => Ok(Self::Web(
                hakille_substrate_web::WebSubstrate::new().with_trusted_root(root),
            )),
            "auto" => Ok(detect_substrate(root)),
            other => Err(format!(
                "Unknown substrate '{other}'. Valid options: auto, linux, windows, macos, redox, mobile, web"
            )),
        }
    }
}

impl HostComputeBackend {
    /// Instantiate compute backend explicitly by name.
    pub fn from_name(name: &str) -> std::result::Result<Self, String> {
        match name.to_ascii_lowercase().as_str() {
            "linux" => Ok(Self::Linux(
                hakille_substrate_linux::LinuxComputeBackend::new(),
            )),
            "windows" => Ok(Self::Windows(
                hakille_substrate_windows::WindowsComputeBackend::new(),
            )),
            "macos" => Ok(Self::Macos(
                hakille_substrate_macos::MacosComputeBackend::new(),
            )),
            "redox" => Ok(Self::Redox(
                hakille_substrate_redox::RedoxComputeBackend::new(),
            )),
            "mobile" => Ok(Self::Mobile(hakille_substrate_mobile::MobileComputeBackend)),
            "web" => Ok(Self::Web(hakille_substrate_web::WebComputeBackend::new())),
            "auto" => Ok(detect_compute()),
            other => Err(format!(
                "Unknown compute backend '{other}'. Valid options: auto, linux, windows, macos, redox, mobile, web"
            )),
        }
    }

    /// Total physical RAM installed on host.
    pub fn total_physical_ram(&self) -> u64 {
        match self {
            Self::Linux(c) => c.total_physical_ram(),
            Self::Windows(c) => c.total_physical_ram(),
            Self::Macos(c) => c.total_physical_ram(),
            Self::Redox(c) => c.ram_available_bytes(),
            Self::Mobile(_) | Self::Web(_) => self.ram_available_bytes(),
        }
    }

    /// Available physical RAM on host.
    pub fn available_physical_ram(&self) -> u64 {
        match self {
            Self::Linux(c) => c.available_physical_ram(),
            Self::Windows(c) => c.available_physical_ram(),
            Self::Macos(c) => c.available_physical_ram(),
            Self::Redox(c) => c.ram_available_bytes(),
            Self::Mobile(_) | Self::Web(_) => self.ram_available_bytes(),
        }
    }
}

/// Automatically detect and instantiate native OS substrate based on `target_os`.
pub fn detect_substrate(root: biscuit_auth::PublicKey) -> HostSubstrate {
    #[cfg(target_os = "windows")]
    {
        HostSubstrate::Windows(
            hakille_substrate_windows::WindowsSubstrate::new().with_trusted_root(root),
        )
    }
    #[cfg(target_os = "linux")]
    {
        HostSubstrate::Linux(hakille_substrate_linux::LinuxSubstrate::new().with_trusted_root(root))
    }
    #[cfg(target_os = "macos")]
    {
        HostSubstrate::Macos(hakille_substrate_macos::MacosSubstrate::new().with_trusted_root(root))
    }
    #[cfg(target_os = "redox")]
    {
        HostSubstrate::Redox(hakille_substrate_redox::RedoxSubstrate::new().with_trusted_root(root))
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "linux",
        target_os = "macos",
        target_os = "redox"
    )))]
    {
        HostSubstrate::Linux(hakille_substrate_linux::LinuxSubstrate::new().with_trusted_root(root))
    }
}

/// Automatically detect and instantiate native OS compute backend based on `target_os`.
pub fn detect_compute() -> HostComputeBackend {
    #[cfg(target_os = "windows")]
    {
        HostComputeBackend::Windows(hakille_substrate_windows::WindowsComputeBackend::new())
    }
    #[cfg(target_os = "linux")]
    {
        HostComputeBackend::Linux(hakille_substrate_linux::LinuxComputeBackend::new())
    }
    #[cfg(target_os = "macos")]
    {
        HostComputeBackend::Macos(hakille_substrate_macos::MacosComputeBackend::new())
    }
    #[cfg(target_os = "redox")]
    {
        HostComputeBackend::Redox(hakille_substrate_redox::RedoxComputeBackend::new())
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "linux",
        target_os = "macos",
        target_os = "redox"
    )))]
    {
        HostComputeBackend::Linux(hakille_substrate_linux::LinuxComputeBackend::new())
    }
}

#[async_trait]
impl Substrate for HostSubstrate {
    fn manifest(&self) -> &PlatformManifest {
        match self {
            Self::Linux(s) => s.manifest(),
            Self::Windows(s) => s.manifest(),
            Self::Macos(s) => s.manifest(),
            Self::Redox(s) => s.manifest(),
            Self::Mobile(s) => s.manifest(),
            Self::Web(s) => s.manifest(),
        }
    }

    async fn attest(&self, data: &[u8]) -> Result<SignedAttestation> {
        match self {
            Self::Linux(s) => s.attest(data).await,
            Self::Windows(s) => s.attest(data).await,
            Self::Macos(s) => s.attest(data).await,
            Self::Redox(s) => s.attest(data).await,
            Self::Mobile(s) => s.attest(data).await,
            Self::Web(s) => s.attest(data).await,
        }
    }

    async fn read_file(&self, path: &VirtualPath, cap: &CapabilityToken) -> Result<Vec<u8>> {
        match self {
            Self::Linux(s) => s.read_file(path, cap).await,
            Self::Windows(s) => s.read_file(path, cap).await,
            Self::Macos(s) => s.read_file(path, cap).await,
            Self::Redox(s) => s.read_file(path, cap).await,
            Self::Mobile(s) => s.read_file(path, cap).await,
            Self::Web(s) => s.read_file(path, cap).await,
        }
    }

    async fn write_file(
        &self,
        path: &VirtualPath,
        data: &[u8],
        cap: &CapabilityToken,
    ) -> Result<()> {
        match self {
            Self::Linux(s) => s.write_file(path, data, cap).await,
            Self::Windows(s) => s.write_file(path, data, cap).await,
            Self::Macos(s) => s.write_file(path, data, cap).await,
            Self::Redox(s) => s.write_file(path, data, cap).await,
            Self::Mobile(s) => s.write_file(path, data, cap).await,
            Self::Web(s) => s.write_file(path, data, cap).await,
        }
    }

    async fn list_dir(
        &self,
        path: &VirtualPath,
        cap: &CapabilityToken,
    ) -> Result<Vec<VirtualPath>> {
        match self {
            Self::Linux(s) => s.list_dir(path, cap).await,
            Self::Windows(s) => s.list_dir(path, cap).await,
            Self::Macos(s) => s.list_dir(path, cap).await,
            Self::Redox(s) => s.list_dir(path, cap).await,
            Self::Mobile(s) => s.list_dir(path, cap).await,
            Self::Web(s) => s.list_dir(path, cap).await,
        }
    }

    async fn delete_file(&self, path: &VirtualPath, cap: &CapabilityToken) -> Result<()> {
        match self {
            Self::Linux(s) => s.delete_file(path, cap).await,
            Self::Windows(s) => s.delete_file(path, cap).await,
            Self::Macos(s) => s.delete_file(path, cap).await,
            Self::Redox(s) => s.delete_file(path, cap).await,
            Self::Mobile(s) => s.delete_file(path, cap).await,
            Self::Web(s) => s.delete_file(path, cap).await,
        }
    }

    async fn http_request(&self, req: HttpRequest, cap: &CapabilityToken) -> Result<HttpResponse> {
        match self {
            Self::Linux(s) => s.http_request(req, cap).await,
            Self::Windows(s) => s.http_request(req, cap).await,
            Self::Macos(s) => s.http_request(req, cap).await,
            Self::Redox(s) => s.http_request(req, cap).await,
            Self::Mobile(s) => s.http_request(req, cap).await,
            Self::Web(s) => s.http_request(req, cap).await,
        }
    }

    async fn start_audio_capture(
        &self,
        config: AudioConfig,
        cap: &CapabilityToken,
    ) -> Result<AudioStreamHandle> {
        match self {
            Self::Linux(s) => s.start_audio_capture(config, cap).await,
            Self::Windows(s) => s.start_audio_capture(config, cap).await,
            Self::Macos(s) => s.start_audio_capture(config, cap).await,
            Self::Redox(s) => s.start_audio_capture(config, cap).await,
            Self::Mobile(s) => s.start_audio_capture(config, cap).await,
            Self::Web(s) => s.start_audio_capture(config, cap).await,
        }
    }

    async fn stop_audio_capture(&self, handle: AudioStreamHandle) -> Result<()> {
        match self {
            Self::Linux(s) => s.stop_audio_capture(handle).await,
            Self::Windows(s) => s.stop_audio_capture(handle).await,
            Self::Macos(s) => s.stop_audio_capture(handle).await,
            Self::Redox(s) => s.stop_audio_capture(handle).await,
            Self::Mobile(s) => s.stop_audio_capture(handle).await,
            Self::Web(s) => s.stop_audio_capture(handle).await,
        }
    }

    async fn play_audio(
        &self,
        data: &[u8],
        config: AudioConfig,
        cap: &CapabilityToken,
    ) -> Result<()> {
        match self {
            Self::Linux(s) => s.play_audio(data, config, cap).await,
            Self::Windows(s) => s.play_audio(data, config, cap).await,
            Self::Macos(s) => s.play_audio(data, config, cap).await,
            Self::Redox(s) => s.play_audio(data, config, cap).await,
            Self::Mobile(s) => s.play_audio(data, config, cap).await,
            Self::Web(s) => s.play_audio(data, config, cap).await,
        }
    }

    async fn start_camera_capture(
        &self,
        config: CameraConfig,
        cap: &CapabilityToken,
    ) -> Result<CameraStreamHandle> {
        match self {
            Self::Linux(s) => s.start_camera_capture(config, cap).await,
            Self::Windows(s) => s.start_camera_capture(config, cap).await,
            Self::Macos(s) => s.start_camera_capture(config, cap).await,
            Self::Redox(s) => s.start_camera_capture(config, cap).await,
            Self::Mobile(s) => s.start_camera_capture(config, cap).await,
            Self::Web(s) => s.start_camera_capture(config, cap).await,
        }
    }

    async fn stop_camera_capture(&self, handle: CameraStreamHandle) -> Result<()> {
        match self {
            Self::Linux(s) => s.stop_camera_capture(handle).await,
            Self::Windows(s) => s.stop_camera_capture(handle).await,
            Self::Macos(s) => s.stop_camera_capture(handle).await,
            Self::Redox(s) => s.stop_camera_capture(handle).await,
            Self::Mobile(s) => s.stop_camera_capture(handle).await,
            Self::Web(s) => s.stop_camera_capture(handle).await,
        }
    }

    async fn capture_camera_frame(
        &self,
        handle: CameraStreamHandle,
        cap: &CapabilityToken,
    ) -> Result<CameraFrame> {
        match self {
            Self::Linux(s) => s.capture_camera_frame(handle, cap).await,
            Self::Windows(s) => s.capture_camera_frame(handle, cap).await,
            Self::Macos(s) => s.capture_camera_frame(handle, cap).await,
            Self::Redox(s) => s.capture_camera_frame(handle, cap).await,
            Self::Mobile(s) => s.capture_camera_frame(handle, cap).await,
            Self::Web(s) => s.capture_camera_frame(handle, cap).await,
        }
    }

    async fn capture_screen(&self, cap: &CapabilityToken) -> Result<ScreenCapture> {
        match self {
            Self::Linux(s) => s.capture_screen(cap).await,
            Self::Windows(s) => s.capture_screen(cap).await,
            Self::Macos(s) => s.capture_screen(cap).await,
            Self::Redox(s) => s.capture_screen(cap).await,
            Self::Mobile(s) => s.capture_screen(cap).await,
            Self::Web(s) => s.capture_screen(cap).await,
        }
    }

    async fn inject_input(&self, action: InputAction, cap: &CapabilityToken) -> Result<()> {
        match self {
            Self::Linux(s) => s.inject_input(action, cap).await,
            Self::Windows(s) => s.inject_input(action, cap).await,
            Self::Macos(s) => s.inject_input(action, cap).await,
            Self::Redox(s) => s.inject_input(action, cap).await,
            Self::Mobile(s) => s.inject_input(action, cap).await,
            Self::Web(s) => s.inject_input(action, cap).await,
        }
    }

    async fn spawn_sandboxed(
        &self,
        wasm: &[u8],
        caps: Vec<CapabilityToken>,
    ) -> Result<ProcessHandle> {
        match self {
            Self::Linux(s) => s.spawn_sandboxed(wasm, caps).await,
            Self::Windows(s) => s.spawn_sandboxed(wasm, caps).await,
            Self::Macos(s) => s.spawn_sandboxed(wasm, caps).await,
            Self::Redox(s) => s.spawn_sandboxed(wasm, caps).await,
            Self::Mobile(s) => s.spawn_sandboxed(wasm, caps).await,
            Self::Web(s) => s.spawn_sandboxed(wasm, caps).await,
        }
    }

    fn power_state(&self) -> PowerState {
        match self {
            Self::Linux(s) => s.power_state(),
            Self::Windows(s) => s.power_state(),
            Self::Macos(s) => s.power_state(),
            Self::Redox(s) => s.power_state(),
            Self::Mobile(s) => s.power_state(),
            Self::Web(s) => s.power_state(),
        }
    }

    fn thermal_state(&self) -> ThermalState {
        match self {
            Self::Linux(s) => s.thermal_state(),
            Self::Windows(s) => s.thermal_state(),
            Self::Macos(s) => s.thermal_state(),
            Self::Redox(s) => s.thermal_state(),
            Self::Mobile(s) => s.thermal_state(),
            Self::Web(s) => s.thermal_state(),
        }
    }
}

impl Compute for HostComputeBackend {
    fn available_backends(&self) -> Vec<ComputeBackend> {
        match self {
            Self::Linux(c) => c.available_backends(),
            Self::Windows(c) => c.available_backends(),
            Self::Macos(c) => c.available_backends(),
            Self::Redox(c) => c.available_backends(),
            Self::Mobile(c) => c.available_backends(),
            Self::Web(c) => c.available_backends(),
        }
    }

    fn preferred_backend(&self) -> ComputeBackend {
        match self {
            Self::Linux(c) => c.preferred_backend(),
            Self::Windows(c) => c.preferred_backend(),
            Self::Macos(c) => c.preferred_backend(),
            Self::Redox(c) => c.preferred_backend(),
            Self::Mobile(c) => c.preferred_backend(),
            Self::Web(c) => c.preferred_backend(),
        }
    }

    fn vram_available_bytes(&self) -> u64 {
        match self {
            Self::Linux(c) => c.vram_available_bytes(),
            Self::Windows(c) => c.vram_available_bytes(),
            Self::Macos(c) => c.vram_available_bytes(),
            Self::Redox(c) => c.vram_available_bytes(),
            Self::Mobile(c) => c.vram_available_bytes(),
            Self::Web(c) => c.vram_available_bytes(),
        }
    }

    fn ram_available_bytes(&self) -> u64 {
        match self {
            Self::Linux(c) => c.ram_available_bytes(),
            Self::Windows(c) => c.ram_available_bytes(),
            Self::Macos(c) => c.ram_available_bytes(),
            Self::Redox(c) => c.ram_available_bytes(),
            Self::Mobile(c) => c.ram_available_bytes(),
            Self::Web(c) => c.ram_available_bytes(),
        }
    }

    fn supports_quantization(&self, quant: &str) -> bool {
        match self {
            Self::Linux(c) => c.supports_quantization(quant),
            Self::Windows(c) => c.supports_quantization(quant),
            Self::Macos(c) => c.supports_quantization(quant),
            Self::Redox(c) => c.supports_quantization(quant),
            Self::Mobile(c) => c.supports_quantization(quant),
            Self::Web(c) => c.supports_quantization(quant),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_core::CapabilityAuthority;

    #[tokio::test]
    async fn test_detect_substrate_matches_host() {
        let authority = CapabilityAuthority::new();
        let substrate = detect_substrate(authority.root_public_key());
        let manifest = substrate.manifest();

        #[cfg(target_os = "linux")]
        assert_eq!(manifest.os_name, "Linux");

        #[cfg(target_os = "windows")]
        assert_eq!(manifest.os_name, "Windows");

        #[cfg(target_os = "macos")]
        assert_eq!(manifest.os_name, "macOS");

        #[cfg(target_os = "redox")]
        assert_eq!(manifest.os_name, "Redox");

        let attestation = substrate.attest(b"test-boot").await.expect("attestation");
        assert!(!attestation.platform_id.is_empty());
    }

    #[test]
    fn test_all_substrates_instantiable_by_name() {
        let authority = CapabilityAuthority::new();
        for name in &[
            "linux", "windows", "macos", "redox", "mobile", "web", "auto",
        ] {
            let sub = HostSubstrate::from_name(name, authority.root_public_key());
            assert!(sub.is_ok(), "Failed for {name}: {:?}", sub.err());
            let comp = HostComputeBackend::from_name(name);
            assert!(comp.is_ok(), "Failed compute for {name}: {:?}", comp.err());
        }
    }
}
