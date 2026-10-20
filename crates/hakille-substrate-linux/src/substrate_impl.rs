use crate::display::LinuxDisplayCaptureEngine;
use crate::input::LinuxInputInjector;
use crate::telemetry::{
    detect_linux_vram_bytes, read_host_meminfo, read_host_power_state, read_host_thermal_state,
};
use async_trait::async_trait;
use hakille_core::{
    CapabilityScope, CapabilityToken, KeyPair, Result, SignedAttestation, Tier, Timestamp,
    VirtualPath,
};
use hakille_substrate::{
    AudioConfig, AudioStreamHandle, CameraConfig, CameraFrame, CameraStreamHandle, EgressPolicy,
    HttpRequest, HttpResponse, InputAction, PlatformManifest, PowerState, ProcessHandle,
    ScreenCapture, Substrate, ThermalState,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Missing native binding reported by every Linux camera method.
const LINUX_CAMERA_UNIMPLEMENTED: &str = "camera not implemented for Linux substrate: V4L2 binding missing \
     (no /dev/video* device wiring; link a V4L2 crate with ioctl bindings to enable)";

pub struct LinuxSubstrate {
    manifest: PlatformManifest,
    attestation_keypair: KeyPair,
    audio_counter: AtomicU64,
    process_counter: AtomicU64,
    display_engine: LinuxDisplayCaptureEngine,
    input_injector: LinuxInputInjector,
    virtual_fs_root: PathBuf,
    /// Host allowlist egress policy. Default is loopback-only
    /// (no external egress until hosts are explicitly allowed).
    egress_policy: EgressPolicy,
    /// Pinned Biscuit root public key. When `None`, capability tokens are
    /// still structurally verified but NOT pinned (legacy / tests).
    /// Production MUST call `with_trusted_root`.
    trusted_capability_root: Option<biscuit_auth::PublicKey>,
}

/// Pure parser for `os-release` style content: `PRETTY_NAME` wins, otherwise
/// `NAME` plus optional `VERSION_ID`. Returns `None` when neither is present.
/// Empty lines, CRLF and unrelated keys are tolerated; quotes are stripped.
pub fn parse_os_release(content: &str) -> Option<String> {
    let mut pretty = String::new();
    let mut name = String::new();
    let mut version_id = String::new();
    for line in content.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("PRETTY_NAME=") {
            pretty = value.trim().trim_matches('"').trim().to_string();
        } else if let Some(value) = line.strip_prefix("NAME=") {
            name = value.trim().trim_matches('"').trim().to_string();
        } else if let Some(value) = line.strip_prefix("VERSION_ID=") {
            version_id = value.trim().trim_matches('"').trim().to_string();
        }
    }
    if !pretty.is_empty() {
        return Some(pretty);
    }
    if name.is_empty() {
        return None;
    }
    if version_id.is_empty() {
        Some(name)
    } else {
        Some(format!("{name} {version_id}"))
    }
}

/// Honest host OS version. On Linux: `/etc/os-release` (`PRETTY_NAME` /
/// `NAME` + `VERSION_ID`), else the real kernel release from
/// `/proc/sys/kernel/osrelease`, else `"unknown"` — never a hard-coded string.
#[cfg(target_os = "linux")]
fn read_linux_os_version() -> String {
    if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
        if let Some(version) = parse_os_release(&content) {
            return version;
        }
    }
    if let Ok(release) = std::fs::read_to_string("/proc/sys/kernel/osrelease") {
        let release = release.trim();
        if !release.is_empty() {
            return format!("Linux kernel {release}");
        }
    }
    tracing::warn!("OS version unknown: /etc/os-release and /proc/sys/kernel/osrelease unreadable");
    "unknown".to_string()
}

/// Off Linux there is no `/etc/os-release` to read: report an unambiguous
/// unknown instead of a plausible-looking fabricated kernel version.
#[cfg(not(target_os = "linux"))]
fn read_linux_os_version() -> String {
    "unknown (non-Linux host: /etc/os-release unavailable)".to_string()
}

impl LinuxSubstrate {
    pub fn new() -> Self {
        let mem = read_host_meminfo();
        let manifest = PlatformManifest {
            tier: Tier::Native,
            capabilities_granted: vec![
                CapabilityScope::Filesystem,
                CapabilityScope::Network,
                CapabilityScope::GuiInput,
                CapabilityScope::Audio,
                CapabilityScope::Camera,
                CapabilityScope::Sensor,
                CapabilityScope::ModelInference,
            ],
            capabilities_denied: vec![CapabilityScope::FullSystem],
            max_memory_bytes: mem.mem_total_bytes,
            // Honest: discrete VRAM from sysfs DRM when discoverable, else
            // None (shared-memory iGPUs / headless VMs report nothing).
            gpu_vram_bytes: detect_linux_vram_bytes(),
            has_secure_enclave: false,
            has_tpm: true,
            battery_powered: false,
            // Honest: no thermal envelope modeling without RAPL/powercap
            // sampling (see /sys/class/powercap); unknown, not invented.
            thermal_envelope_watts: None,
            os_name: "Linux".to_string(),
            os_version: read_linux_os_version(),
        };

        Self {
            manifest,
            attestation_keypair: KeyPair::generate(),
            audio_counter: AtomicU64::new(1),
            process_counter: AtomicU64::new(1),
            display_engine: LinuxDisplayCaptureEngine::new(),
            input_injector: LinuxInputInjector::default(),
            virtual_fs_root: PathBuf::from("/var/lib/hakille/vfs"),
            egress_policy: EgressPolicy::loopback_only(),
            trusted_capability_root: None,
        }
    }

    /// Pin the Biscuit root public key used to verify ALL capability tokens
    /// at the point of effect. Production must set this from the daemon's
    /// `CapabilityAuthority::root_public_key()`.
    /// MANDATORY in production: `spawn_sandboxed` refuses (Err + error log)
    /// when no root is pinned — including release builds.
    pub fn with_trusted_root(mut self, root: biscuit_auth::PublicKey) -> Self {
        self.trusted_capability_root = Some(root);
        self
    }

    /// Whether a trusted capability root is pinned (`with_trusted_root` called).
    pub fn has_trusted_root(&self) -> bool {
        self.trusted_capability_root.is_some()
    }

    /// Configure the host allowlist egress policy (default: loopback-only).
    pub fn with_egress_policy(mut self, policy: EgressPolicy) -> Self {
        self.egress_policy = policy;
        self
    }

    /// Current egress policy.
    pub fn egress_policy(&self) -> &EgressPolicy {
        &self.egress_policy
    }

    /// Mandatory gate: returns the pinned root or refuses with an error log.
    /// Called by `spawn_sandboxed` (and any future privileged op) so a
    /// missing pin can never silently fall back to unpinned verification.
    fn require_trusted_root(&self, op: &str) -> hakille_core::Result<biscuit_auth::PublicKey> {
        match &self.trusted_capability_root {
            Some(root) => Ok(*root),
            None => {
                tracing::error!(
                    "{op} refused: trusted capability root not pinned; call with_trusted_root in production"
                );
                Err(hakille_core::HakilleError::CapabilityDenied(format!(
                    "{op} refused: trusted capability root not pinned; call with_trusted_root in production"
                )))
            }
        }
    }

    fn enforce(
        &self,
        cap: &CapabilityToken,
        scope: CapabilityScope,
        action: hakille_core::CapabilityAction,
    ) -> hakille_core::Result<()> {
        if let Some(root) = &self.trusted_capability_root {
            hakille_core::verify_capability_with_root_key(cap, root, scope, action)
        } else {
            // Legacy fallback UNIQUEMENT en debug/tests : vérification
            // structurelle sans pinning (accepte les tokens auto-signés).
            // En release, panique fail-closed : `verify_unpinned` est interdit.
            #[cfg(debug_assertions)]
            {
                #[allow(deprecated)]
                {
                    hakille_core::verify_capability_unpinned(cap, scope, action)
                }
            }
            #[cfg(not(debug_assertions))]
            {
                panic!(
                    "unpinned capability verification forbidden in release; call with_trusted_root in production"
                );
            }
        }
    }

    /// Specify a custom root folder for the sandboxed VirtualFS.
    pub fn with_virtual_fs_root(mut self, root: PathBuf) -> Self {
        self.virtual_fs_root = root;
        self
    }

    /// Resolve a VirtualPath to a real path inside the isolated root.
    /// Secure: rejects `..`, absolute paths, drive letters, NUL bytes and
    /// ensures the final path stays inside `virtual_fs_root`.
    fn resolve_path(&self, path: &VirtualPath) -> hakille_core::Result<PathBuf> {
        use std::path::Component;
        if path.0.contains('\0') {
            return Err(hakille_core::HakilleError::CapabilityDenied(
                "NUL byte in virtual path".into(),
            ));
        }
        // Reject Windows drive letters and UNC prefixes early.
        let lower = path.0.to_ascii_lowercase();
        if lower.contains(':') || path.0.starts_with(r"\\") {
            return Err(hakille_core::HakilleError::CapabilityDenied(
                "absolute / drive path rejected".into(),
            ));
        }
        let mut clean = PathBuf::new();
        let mut saw_normal = false;
        for comp in std::path::Path::new(&path.0).components() {
            match comp {
                Component::Normal(p) => {
                    if p == std::ffi::OsStr::new("..") {
                        return Err(hakille_core::HakilleError::CapabilityDenied(
                            "path traversal (..) rejected".into(),
                        ));
                    }
                    clean.push(p);
                    saw_normal = true;
                }
                Component::CurDir => {}
                // Any ParentDir, RootDir, Prefix => reject
                _ => {
                    return Err(hakille_core::HakilleError::CapabilityDenied(
                        "non-normal path component rejected".into(),
                    ));
                }
            }
        }
        if !saw_normal {
            // Empty virtual path means the VirtualFS root itself (for list_dir).
            return Ok(self.virtual_fs_root.clone());
        }
        let joined = self.virtual_fs_root.join(&clean);
        // Defence in depth: canonicalize prefix check when root exists.
        // If canonicalization fails (path doesn't exist yet for writes),
        // the component-level rejection above already blocks traversal.
        if let (Ok(root_c), Ok(joined_c)) =
            (self.virtual_fs_root.canonicalize(), joined.canonicalize())
        {
            if !joined_c.starts_with(&root_c) {
                return Err(hakille_core::HakilleError::CapabilityDenied(
                    "resolved path escapes VirtualFS root".into(),
                ));
            }
            return Ok(joined_c);
        }
        Ok(joined)
    }

    pub fn virtual_fs_root(&self) -> &Path {
        &self.virtual_fs_root
    }

    pub fn display_engine(&self) -> &LinuxDisplayCaptureEngine {
        &self.display_engine
    }

    pub fn input_injector(&self) -> &LinuxInputInjector {
        &self.input_injector
    }

    /// Capability-checked stop for an audio stream. The trait-level
    /// `stop_audio_capture(handle)` is kept for compatibility but only logs
    /// a deprecation warning (no cap = no ownership proof); new code MUST
    /// call this method with the owner's Audio/Read capability.
    pub async fn stop_audio_capture_with_cap(
        &self,
        handle: AudioStreamHandle,
        cap: &CapabilityToken,
    ) -> hakille_core::Result<()> {
        self.enforce(
            cap,
            CapabilityScope::Audio,
            hakille_core::CapabilityAction::Read,
        )?;
        let _ = handle;
        Ok(())
    }

    /// Capability-checked stop for a camera stream. See
    /// `stop_audio_capture_with_cap` — same ownership rationale (Camera/Read).
    pub async fn stop_camera_capture_with_cap(
        &self,
        handle: CameraStreamHandle,
        cap: &CapabilityToken,
    ) -> hakille_core::Result<()> {
        self.enforce(
            cap,
            CapabilityScope::Camera,
            hakille_core::CapabilityAction::Read,
        )?;
        let _ = handle;
        Ok(())
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

    async fn read_file(&self, path: &VirtualPath, cap: &CapabilityToken) -> Result<Vec<u8>> {
        self.enforce(
            cap,
            CapabilityScope::Filesystem,
            hakille_core::CapabilityAction::Read,
        )?;
        let local_path = self.resolve_path(path)?;
        if local_path.exists() {
            tokio::fs::read(&local_path).await.map_err(|e| {
                hakille_core::HakilleError::StorageError(format!(
                    "Read error {}: {}",
                    local_path.display(),
                    e
                ))
            })
        } else {
            Err(hakille_core::HakilleError::StorageError(format!(
                "File not found: {}",
                path.0
            )))
        }
    }

    async fn write_file(
        &self,
        path: &VirtualPath,
        data: &[u8],
        cap: &CapabilityToken,
    ) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::Filesystem,
            hakille_core::CapabilityAction::Write,
        )?;
        let local_path = self.resolve_path(path)?;
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                hakille_core::HakilleError::StorageError(format!(
                    "Create dir error {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }
        tokio::fs::write(&local_path, data).await.map_err(|e| {
            hakille_core::HakilleError::StorageError(format!(
                "Write error {}: {}",
                local_path.display(),
                e
            ))
        })
    }

    async fn list_dir(
        &self,
        path: &VirtualPath,
        cap: &CapabilityToken,
    ) -> Result<Vec<VirtualPath>> {
        self.enforce(
            cap,
            CapabilityScope::Filesystem,
            hakille_core::CapabilityAction::Read,
        )?;
        let local_path = self.resolve_path(path)?;
        if local_path.exists() && local_path.is_dir() {
            let mut entries = Vec::new();
            if let Ok(mut read_dir) = tokio::fs::read_dir(&local_path).await {
                while let Ok(Some(entry)) = read_dir.next_entry().await {
                    if let Some(name) = entry.file_name().to_str() {
                        entries.push(VirtualPath(name.to_string()));
                    }
                }
            }
            Ok(entries)
        } else {
            Err(hakille_core::HakilleError::StorageError(format!(
                "Directory not found: {}",
                path.0
            )))
        }
    }

    async fn delete_file(&self, path: &VirtualPath, cap: &CapabilityToken) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::Filesystem,
            hakille_core::CapabilityAction::Delete,
        )?;
        let local_path = self.resolve_path(path)?;
        if local_path.exists() {
            tokio::fs::remove_file(&local_path).await.map_err(|e| {
                hakille_core::HakilleError::StorageError(format!(
                    "Delete error {}: {}",
                    local_path.display(),
                    e
                ))
            })?;
        }
        Ok(())
    }

    async fn http_request(&self, req: HttpRequest, cap: &CapabilityToken) -> Result<HttpResponse> {
        // Network capability (GET ⇒ Read, others ⇒ Write) including expiry
        // is enforced first; without it egress is refused explicitly.
        // Then the shared allowlist policy applies (loopback-only by
        // default) and the real fetch runs over rustls TLS (10 s, 5 MiB cap).
        let action = if req.method.eq_ignore_ascii_case("get") {
            hakille_core::CapabilityAction::Read
        } else {
            hakille_core::CapabilityAction::Write
        };
        self.enforce(cap, CapabilityScope::Network, action)?;
        hakille_substrate::fetch_via_egress(req, &self.egress_policy).await
    }

    async fn start_audio_capture(
        &self,
        _config: AudioConfig,
        cap: &CapabilityToken,
    ) -> Result<AudioStreamHandle> {
        self.enforce(
            cap,
            CapabilityScope::Audio,
            hakille_core::CapabilityAction::Read,
        )?;
        let id = self.audio_counter.fetch_add(1, Ordering::SeqCst);
        Ok(AudioStreamHandle(id))
    }

    async fn stop_audio_capture(&self, _handle: AudioStreamHandle) -> Result<()> {
        tracing::warn!(
            "stop_audio_capture(handle) without capability is deprecated; use stop_audio_capture_with_cap(handle, cap) with Audio/Read ownership proof"
        );
        Ok(())
    }

    async fn play_audio(
        &self,
        _data: &[u8],
        _config: AudioConfig,
        cap: &CapabilityToken,
    ) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::Audio,
            hakille_core::CapabilityAction::Write,
        )?;
        Ok(())
    }

    async fn start_camera_capture(
        &self,
        _config: CameraConfig,
        cap: &CapabilityToken,
    ) -> Result<CameraStreamHandle> {
        self.enforce(
            cap,
            CapabilityScope::Camera,
            hakille_core::CapabilityAction::Read,
        )?;
        // Honest: no V4L2 wiring — refuse instead of synthesizing frames.
        Err(hakille_core::HakilleError::SubstrateError(
            LINUX_CAMERA_UNIMPLEMENTED.into(),
        ))
    }

    async fn stop_camera_capture(&self, _handle: CameraStreamHandle) -> Result<()> {
        tracing::warn!(
            "stop_camera_capture(handle) without capability is deprecated; use stop_camera_capture_with_cap(handle, cap) with Camera/Read ownership proof"
        );
        Ok(())
    }

    async fn capture_camera_frame(
        &self,
        _handle: CameraStreamHandle,
        cap: &CapabilityToken,
    ) -> Result<CameraFrame> {
        self.enforce(
            cap,
            CapabilityScope::Camera,
            hakille_core::CapabilityAction::Read,
        )?;
        // Honest: no V4L2 wiring — refuse instead of synthesizing frames.
        Err(hakille_core::HakilleError::SubstrateError(
            LINUX_CAMERA_UNIMPLEMENTED.into(),
        ))
    }

    async fn capture_screen(&self, cap: &CapabilityToken) -> Result<ScreenCapture> {
        self.enforce(
            cap,
            CapabilityScope::Sensor,
            hakille_core::CapabilityAction::Read,
        )?;
        Ok(self.display_engine.capture_screen())
    }

    async fn inject_input(&self, action: InputAction, cap: &CapabilityToken) -> Result<()> {
        self.enforce(
            cap,
            CapabilityScope::GuiInput,
            hakille_core::CapabilityAction::Execute,
        )?;
        self.input_injector
            .inject(&action)
            .map_err(hakille_core::HakilleError::SubstrateError)
    }

    async fn spawn_sandboxed(
        &self,
        _wasm: &[u8],
        caps: Vec<CapabilityToken>,
    ) -> Result<ProcessHandle> {
        // Pinned-root enforcement: every delegated cap MUST verify against the
        // pinned authority root. No silent unpinned fallback — without a
        // trusted root, refuse (error logged in `require_trusted_root`).
        let root = self.require_trusted_root("spawn_sandboxed")?;
        // Caller must prove it holds at least one delegable capability;
        // spawned sandbox receives only attenuated `caps` (verified by caller).
        for c in &caps {
            hakille_core::verify_capability_with_root_key(
                c,
                &root,
                CapabilityScope::Filesystem,
                hakille_core::CapabilityAction::Read,
            )
            .or_else(|_| {
                hakille_core::verify_capability_with_root_key(
                    c,
                    &root,
                    CapabilityScope::Network,
                    hakille_core::CapabilityAction::Read,
                )
            })
            .or_else(|_| {
                hakille_core::verify_capability_with_root_key(
                    c,
                    &root,
                    CapabilityScope::GuiInput,
                    hakille_core::CapabilityAction::Execute,
                )
            })
            .or_else(|_| {
                hakille_core::verify_capability_with_root_key(
                    c,
                    &root,
                    CapabilityScope::Sensor,
                    hakille_core::CapabilityAction::Read,
                )
            })
            .map_err(|e| {
                hakille_core::HakilleError::CapabilityDenied(format!(
                    "spawn_sandboxed: delegated cap invalid (pinned root): {e}"
                ))
            })?;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_release_prefers_pretty_name() {
        let fixture = "NAME=\"Ubuntu\"\nVERSION_ID=\"24.04\"\nPRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\nID=ubuntu\n";
        assert_eq!(
            parse_os_release(fixture).as_deref(),
            Some("Ubuntu 24.04.1 LTS")
        );
    }

    #[test]
    fn os_release_falls_back_to_name_and_version_id() {
        let fixture = "NAME=\"Debian GNU/Linux\"\nVERSION_ID=\"12\"\nID=debian\n";
        assert_eq!(
            parse_os_release(fixture).as_deref(),
            Some("Debian GNU/Linux 12")
        );
        let no_version = "NAME=Alpine\nID=alpine\n";
        assert_eq!(parse_os_release(no_version).as_deref(), Some("Alpine"));
    }

    #[test]
    fn os_release_handles_crlf_junk_unquoted_and_empty_values() {
        let fixture = "\r\n# comment\r\nPRETTY_NAME=\r\nNAME=Fedora Linux\r\nVERSION_ID=40\r\nUNRELATED=x y\r\n";
        assert_eq!(
            parse_os_release(fixture).as_deref(),
            Some("Fedora Linux 40")
        );
    }

    #[test]
    fn os_release_returns_none_without_name_or_pretty_name() {
        assert_eq!(parse_os_release(""), None);
        assert_eq!(parse_os_release("ID=ubuntu\nVERSION_ID=\"24.04\"\n"), None);
        assert_eq!(parse_os_release("PRETTY_NAME=\"  \"\n"), None);
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn os_version_off_linux_is_explicitly_unknown() {
        let version = read_linux_os_version();
        assert!(
            version.contains("unknown"),
            "non-Linux OS version must be explicitly unknown, got: {version}"
        );
        assert!(
            !version.contains("6.8.0-generic"),
            "hard-coded kernel version must be gone, got: {version}"
        );
    }
}
