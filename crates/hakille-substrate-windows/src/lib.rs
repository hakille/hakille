pub mod appcontainer;
#[cfg(windows)]
pub mod camera_mediafoundation;
pub mod compute_impl;
pub mod input_injector;
pub mod screen_capture;
pub mod service;
pub mod substrate_impl;
pub mod tray_and_hotkey;
pub mod win32_ffi;

pub use appcontainer::*;
pub use compute_impl::*;
pub use input_injector::*;
pub use screen_capture::*;
pub use service::*;
pub use substrate_impl::*;
pub use tray_and_hotkey::*;

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_core::{CapabilityScope, Tier, VirtualPath};
    use hakille_substrate::{
        CameraFacing, Compute, ImageFormat, InputAction, MouseButton, ScrollDirection, Substrate,
    };

    #[tokio::test]
    async fn test_windows_substrate_manifest_and_attestation() {
        let substrate = WindowsSubstrate::new();
        let manifest = substrate.manifest();
        assert_eq!(manifest.tier, Tier::Native);
        assert_eq!(manifest.os_name, "Windows");
        assert!(manifest.has_tpm);
        assert!(
            manifest
                .capabilities_granted
                .contains(&CapabilityScope::Filesystem)
        );
        assert!(
            manifest
                .capabilities_denied
                .contains(&CapabilityScope::FullSystem)
        );

        let attestation = substrate
            .attest(b"test-nonce")
            .await
            .expect("Attestation failed");
        assert_eq!(
            attestation.platform_id,
            "Windows-Substrate-Host-dev-attestation"
        );
        assert!(!attestation.signature.0.is_empty());
    }

    #[test]
    fn test_windows_compute_backends_and_quantization() {
        let compute = WindowsComputeBackend::new();
        let backends = compute.available_backends();
        assert!(backends.contains(&hakille_substrate::ComputeBackend::Cpu));
        assert!(backends.contains(&hakille_substrate::ComputeBackend::Cuda));
        assert!(compute.supports_quantization("q4_k"));
        assert!(compute.supports_quantization("int8"));
        assert!(!compute.supports_quantization("unsupported_quant"));

        // Verify real physical RAM query (Windows only : le fallback
        // non-Windows renvoie volontairement 0, jamais une valeur inventée).
        let total_ram = compute.total_physical_ram();
        let avail_ram = compute.available_physical_ram();
        if cfg!(windows) {
            assert!(total_ram > 0, "Total RAM must be greater than 0");
            assert!(avail_ram > 0, "Available RAM must be greater than 0");
        } else {
            assert_eq!(total_ram, 0);
            assert_eq!(avail_ram, 0);
        }
    }

    #[test]
    fn test_appcontainer_profile_creation() {
        let temp_dir = std::env::temp_dir().join("hakille_ac_test");
        let mut profile = AppContainerSandbox::create_profile(
            "MailClient",
            vec![CapabilityScope::Network, CapabilityScope::Filesystem],
            temp_dir.to_str().unwrap(),
        )
        .expect("Profile creation failed");

        assert_eq!(profile.package_name, "Hakille.AppContainer.MailClient");
        assert!(
            profile
                .allowed_capabilities
                .contains(&CapabilityScope::Network)
        );
        assert!(profile.capability_sids.contains(&"S-1-15-3-1".to_string()));
        assert!(profile.is_active);

        AppContainerSandbox::teardown(&mut profile).unwrap();
        assert!(!profile.is_active);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_windows_real_screen_capture_and_input_injection() {
        let (w, h) = WindowsScreenCapturer::display_dimensions();
        assert!(
            w > 0 && h > 0,
            "Display dimensions must be positive integers"
        );

        let capture = WindowsScreenCapturer::capture_primary_display();
        assert_eq!(capture.width, w as u32);
        assert_eq!(capture.height, h as u32);
        assert_eq!(capture.data.len(), (w * h * 4) as usize);

        // Test input injection
        let click = InputAction::Click {
            x: 500,
            y: 400,
            button: MouseButton::Left,
        };
        let res = WindowsInputInjector::inject(&click);
        assert!(res.is_ok(), "Input injection should succeed");

        // Test vertical and horizontal scroll injection
        let scroll_up = InputAction::Scroll {
            direction: ScrollDirection::Up,
            amount: 2,
        };
        assert!(WindowsInputInjector::inject(&scroll_up).is_ok());

        let scroll_right = InputAction::Scroll {
            direction: ScrollDirection::Right,
            amount: 2,
        };
        assert!(WindowsInputInjector::inject(&scroll_right).is_ok());

        let scroll_left = InputAction::Scroll {
            direction: ScrollDirection::Left,
            amount: 2,
        };
        assert!(WindowsInputInjector::inject(&scroll_left).is_ok());
    }

    #[tokio::test]
    async fn test_windows_filesystem_isolation_and_power() {
        // Dossier unique par processus : les binaires de test tournent en
        // parallèle sous `cargo test --workspace` (pas de collision).
        let temp_dir =
            std::env::temp_dir().join(format!("hakille_vfs_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);

        let substrate = WindowsSubstrate::new().with_virtual_fs_root(temp_dir.clone());
        // Expiration future : Timestamp::now() expirerait à la seconde suivante
        // maintenant que l'enforcement vérifie réellement l'expiration.
        let expiry = hakille_core::Timestamp(chrono::Utc::now() + chrono::Duration::hours(1));
        let cap = hakille_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![
                    hakille_core::CapabilityAction::Read,
                    hakille_core::CapabilityAction::Write,
                    hakille_core::CapabilityAction::Delete,
                ],
                expiry,
            )
            .unwrap();

        let vpath = VirtualPath("test_notes.txt".to_string());
        substrate
            .write_file(&vpath, b"Hakille Windows Native Substrate", &cap)
            .await
            .unwrap();

        let read_data = substrate.read_file(&vpath, &cap).await.unwrap();
        assert_eq!(read_data, b"Hakille Windows Native Substrate");

        let list = substrate
            .list_dir(&VirtualPath("".to_string()), &cap)
            .await
            .unwrap();
        assert!(list.iter().any(|p| p.0 == "test_notes.txt"));

        substrate.delete_file(&vpath, &cap).await.unwrap();

        // Check power state
        let power = substrate.power_state();
        assert!(power.battery_percent >= 0.0 && power.battery_percent <= 100.0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_windows_tray_and_hotkey() {
        let mut tray = WindowsPresenceTray::new();
        assert_eq!(tray.state(), WindowsPresenceState::Idle);

        tray.set_state(WindowsPresenceState::Acting);
        assert_eq!(tray.state(), WindowsPresenceState::Acting);
        assert!(tray.tooltip.contains("Acting"));

        let mut hotkey = WindowsGlobalHotkey::new();
        assert_eq!(hotkey.key_combination, "Alt+Space");
        assert!(hotkey.register());
        assert!(hotkey.is_registered);
        hotkey.unregister();
        assert!(!hotkey.is_registered);
    }

    #[tokio::test]
    async fn test_windows_capability_enforced_and_traversal_rejected() {
        use hakille_core::CapabilityAction;
        let temp_dir =
            std::env::temp_dir().join(format!("hakille_vfs_sec_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let substrate = WindowsSubstrate::new().with_virtual_fs_root(temp_dir.clone());
        let authority = hakille_core::CapabilityAuthority::new();

        // Token with only Network/Read must NOT allow filesystem writes.
        let wrong_cap = authority
            .create_root_token(
                vec![CapabilityScope::Network],
                vec![CapabilityAction::Read],
                hakille_core::Timestamp::now(),
            )
            .unwrap();
        let res = substrate
            .write_file(&VirtualPath("evil.txt".to_string()), b"x", &wrong_cap)
            .await;
        assert!(
            res.is_err(),
            "filesystem write with network-only cap must be denied"
        );

        // Correct-scoped token but traversal path must be rejected.
        let fs_cap = authority
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Write, CapabilityAction::Read],
                hakille_core::Timestamp::now(),
            )
            .unwrap();
        for evil in [
            "../evil.txt",
            "..\\evil.txt",
            "/abs",
            "C:\\evil.txt",
            "a/../../b",
        ] {
            let r = substrate
                .write_file(&VirtualPath(evil.to_string()), b"x", &fs_cap)
                .await;
            assert!(r.is_err(), "traversal path '{evil}' must be rejected");
        }
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_windows_pinned_root_rejects_forged_token() {
        use hakille_core::CapabilityAction;
        let temp_dir =
            std::env::temp_dir().join(format!("hakille_vfs_pin_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let authority = hakille_core::CapabilityAuthority::new();
        let substrate = WindowsSubstrate::new()
            .with_virtual_fs_root(temp_dir.clone())
            .with_trusted_root(authority.root_public_key());

        // Attacker self-signed token (different root key) must be rejected
        // even though it claims Filesystem/Write. Expiration future des deux
        // tokens : le rejet du forgé doit venir de la clé épinglée, jamais
        // d'une expiration (Timestamp::now() = déjà expiré à la création).
        let attacker = hakille_core::CapabilityAuthority::new();
        let forged = attacker
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Write],
                future_expiry(),
            )
            .unwrap();
        let res = substrate
            .write_file(&VirtualPath("forged.txt".to_string()), b"x", &forged)
            .await;
        assert!(
            res.is_err(),
            "forged self-signed token must be rejected when root is pinned"
        );

        // Legit token from the pinned authority must succeed.
        let legit = authority
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Write],
                future_expiry(),
            )
            .unwrap();
        substrate
            .write_file(&VirtualPath("ok.txt".to_string()), b"ok", &legit)
            .await
            .expect("pinned legit token must succeed");
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    fn future_expiry() -> hakille_core::Timestamp {
        hakille_core::Timestamp(chrono::Utc::now() + chrono::Duration::hours(1))
    }

    fn network_cap(action: hakille_core::CapabilityAction) -> hakille_core::CapabilityToken {
        hakille_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Network],
                vec![action],
                future_expiry(),
            )
            .unwrap()
    }

    fn get_request(url: String) -> hakille_substrate::HttpRequest {
        hakille_substrate::HttpRequest {
            method: "GET".to_string(),
            url,
            headers: std::collections::HashMap::new(),
            body: Vec::new(),
        }
    }

    /// Minimal loopback HTTP server (raw TCP, no extra deps).
    async fn spawn_local_http_server(body: &'static str) -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback for egress test");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = vec![0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        addr
    }

    #[tokio::test]
    async fn test_windows_http_egress_refuses_without_network_cap() {
        let substrate = WindowsSubstrate::new();
        let wrong_cap = hakille_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![hakille_core::CapabilityAction::Read],
                future_expiry(),
            )
            .unwrap();
        let err = substrate
            .http_request(get_request("http://127.0.0.1/".to_string()), &wrong_cap)
            .await
            .expect_err("egress without Network cap must be refused");
        assert!(
            matches!(
                err,
                hakille_core::HakilleError::CapabilityDenied(_)
                    | hakille_core::HakilleError::CapabilityExpired
            ),
            "unexpected error: {err}"
        );

        let expired_cap = hakille_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Network],
                vec![hakille_core::CapabilityAction::Read],
                hakille_core::Timestamp(chrono::Utc::now() - chrono::Duration::hours(1)),
            )
            .unwrap();
        let err = substrate
            .http_request(get_request("http://127.0.0.1/".to_string()), &expired_cap)
            .await
            .expect_err("egress with expired cap must be refused");
        assert!(
            matches!(err, hakille_core::HakilleError::CapabilityExpired),
            "expired cap must yield CapabilityExpired, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_windows_http_egress_refuses_host_off_allowlist() {
        let substrate = WindowsSubstrate::new().with_egress_policy(
            hakille_substrate::EgressPolicy::with_allowlist(["example.com"]),
        );
        let cap = network_cap(hakille_core::CapabilityAction::Read);
        let addr = spawn_local_http_server(r#"{"ok":true}"#).await;
        let err = substrate
            .http_request(get_request(format!("http://{addr}/")), &cap)
            .await
            .expect_err("host off allowlist must be refused");
        assert!(
            err.to_string().contains("not in egress allowlist"),
            "refusal must name the allowlist, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_windows_http_egress_loopback_success() {
        let substrate = WindowsSubstrate::new();
        assert!(substrate.egress_policy().is_loopback_only());
        let cap = network_cap(hakille_core::CapabilityAction::Read);
        let addr = spawn_local_http_server(r#"{"ok":true}"#).await;
        let response = substrate
            .http_request(get_request(format!("http://{addr}/status")), &cap)
            .await
            .expect("loopback fetch under default policy must succeed");
        assert_eq!(response.status_code, 200);
        assert_eq!(response.body, b"{\"ok\":true}");
    }

    #[tokio::test]
    async fn test_windows_camera_mediafoundation_or_honest_refusal() {
        let substrate = WindowsSubstrate::new();
        let cap = hakille_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Camera],
                vec![hakille_core::CapabilityAction::Read],
                future_expiry(),
            )
            .unwrap();
        let config = hakille_substrate::CameraConfig {
            width: 1280,
            height: 720,
            fps: 30,
            facing: CameraFacing::Front,
        };
        assert!(!config.is_available());
        match substrate.start_camera_capture(config.clone(), &cap).await {
            Ok(handle) => {
                // Backend MediaFoundation réellement branché : la frame doit
                // être une vraie capture BGRA de la taille annoncée.
                let frame = substrate
                    .capture_camera_frame(handle, &cap)
                    .await
                    .expect("real MediaFoundation capture must yield a frame");
                assert!(frame.width > 0 && frame.height > 0);
                assert!(matches!(frame.format, ImageFormat::Bgra8));
                assert_eq!(
                    frame.data.len(),
                    (frame.width as usize) * (frame.height as usize) * 4
                );
                substrate
                    .stop_camera_capture_with_cap(handle, &cap)
                    .await
                    .unwrap();
            }
            Err(hakille_core::HakilleError::SubstrateError(msg)) => {
                assert!(
                    msg.contains("camera unavailable") || msg.contains("camera not implemented"),
                    "got: {msg}"
                );
                assert!(
                    msg.contains("MediaFoundation"),
                    "must name the missing backend, got: {msg}"
                );
            }
            Err(other) => panic!("unexpected camera error: {other:?}"),
        }
    }

    #[test]
    fn test_windows_thermal_and_vram_honest() {
        let substrate = WindowsSubstrate::new();
        // No thermal binding linked: explicit Unknown, never invented.
        assert!(substrate.thermal_state().is_unknown());
        // Dedicated VRAM unknown without DXGI; envelope unknown without WMI.
        assert!(substrate.manifest().gpu_vram_bytes.is_none());
        assert!(substrate.manifest().thermal_envelope_watts.is_none());
        let compute = WindowsComputeBackend::new();
        assert_eq!(compute.vram_available_bytes(), 0);
        // RAM itself is honestly measured via GlobalMemoryStatusEx sur Windows ;
        // hors Windows, le fallback explicite renvoie 0.
        if cfg!(windows) {
            assert!(compute.total_physical_ram() > 0);
        } else {
            assert_eq!(compute.total_physical_ram(), 0);
        }
    }
}
