pub mod battery_conservation;
pub mod compute_impl;
pub mod scoped_storage;
pub mod shadow;
pub mod substrate_impl;

pub use battery_conservation::*;
pub use compute_impl::*;
pub use scoped_storage::*;
pub use shadow::*;
pub use substrate_impl::*;

#[cfg(test)]
mod tests {
    use super::*;
    use annora_core::{CapabilityScope, Tier, VirtualPath};
    use annora_substrate::{Compute, Substrate};

    #[tokio::test]
    async fn test_mobile_substrate_manifest_and_attestation() {
        let substrate = MobileSubstrate::new();
        let manifest = substrate.manifest();
        assert_eq!(manifest.tier, Tier::Guest);
        assert_eq!(manifest.os_name, "Mobile-Edge");
        assert!(manifest.has_secure_enclave);
        assert!(!manifest.has_tpm);
        assert!(
            manifest
                .capabilities_granted
                .contains(&CapabilityScope::Audio)
        );
        assert!(
            manifest
                .capabilities_denied
                .contains(&CapabilityScope::FullSystem)
        );

        let attestation = substrate
            .attest(b"mobile-nonce")
            .await
            .expect("Attestation failed");
        assert_eq!(attestation.platform_id, "Mobile-Edge-Shadow-Terminal");
        assert!(!attestation.signature.0.is_empty());
    }

    #[test]
    fn test_mobile_compute_backends_and_quantization() {
        let compute = MobileComputeBackend::new();
        let backends = compute.available_backends();
        assert!(backends.contains(&annora_substrate::ComputeBackend::Cpu));
        assert!(backends.contains(&annora_substrate::ComputeBackend::Npu));
        assert!(compute.supports_quantization("q4_0"));
        assert!(compute.supports_quantization("int8"));
        assert!(!compute.supports_quantization("unsupported_quant"));
    }

    #[test]
    fn test_mobile_shadow_sync_state_machine() {
        let mut sync = MobileShadowSync::default();
        assert_eq!(sync.state(), ShadowState::Disconnected);

        // Queue local edits while offline
        sync.queue_local_delta("notes/todo.txt", b"buy milk");
        sync.queue_local_delta("notes/plan.md", b"# Project Annora");

        // Verify local materialized state
        assert_eq!(sync.get_value("notes/todo.txt"), Some(b"buy milk".to_vec()));

        // Connect to TiKV shadow daemon
        assert!(sync.connect().is_ok());
        assert_eq!(sync.state(), ShadowState::ActiveStreaming);

        // Remote delta arriving with higher version
        let remote_update = ShadowDelta {
            key: "notes/todo.txt".to_string(),
            value: b"buy milk and eggs".to_vec(),
            version: 10,
            timestamp: annora_core::Timestamp::now(),
            is_deleted: false,
        };
        sync.push_remote_delta(remote_update);
        assert_eq!(
            sync.get_value("notes/todo.txt"),
            Some(b"buy milk and eggs".to_vec())
        );

        // Stale remote delta with lower version (version 1) must be ignored
        let stale_update = ShadowDelta {
            key: "notes/todo.txt".to_string(),
            value: b"stale text".to_vec(),
            version: 1,
            timestamp: annora_core::Timestamp::now(),
            is_deleted: false,
        };
        sync.push_remote_delta(stale_update);
        assert_eq!(
            sync.get_value("notes/todo.txt"),
            Some(b"buy milk and eggs".to_vec())
        );

        // Deletion tombstone
        sync.queue_local_deletion("notes/todo.txt");
        assert_eq!(sync.get_value("notes/todo.txt"), None);

        // Enter Doze / background
        sync.enter_doze();
        assert_eq!(sync.state(), ShadowState::SuspendedDozing);

        // Wake from APNs / FCM push
        let drained = sync.wake_and_resync();
        assert_eq!(sync.state(), ShadowState::ActiveStreaming);
        assert_eq!(drained, 1); // Synced local deletion tombstone

        // Network failure backoff
        sync.handle_network_disconnect();
        assert_eq!(sync.state(), ShadowState::ReconnectingBackoff);
        assert!(sync.backoff_delay_ms() > 0);
    }

    #[test]
    fn test_mobile_scoped_storage_isolation() {
        let mut sandbox = ScopedStorageSandbox::new("/data/user/0/com.annora.ai/files");

        // Documents access
        let path = VirtualPath("state.db".to_string());
        let resolved = sandbox
            .resolve_path(StorageScope::Documents, &path)
            .unwrap();
        assert!(resolved.ends_with("Documents/state.db"));

        // Path traversal attack rejected
        let evil_path = VirtualPath("../../../etc/passwd".to_string());
        assert!(
            sandbox
                .resolve_path(StorageScope::Documents, &evil_path)
                .is_err()
        );

        // Windows drive traversal attack rejected
        let win_drive = VirtualPath("C:\\Windows\\System32".to_string());
        assert!(
            sandbox
                .resolve_path(StorageScope::Documents, &win_drive)
                .is_err()
        );

        // Absolute unix path rejected
        let abs_path = VirtualPath("/etc/shadow".to_string());
        assert!(
            sandbox
                .resolve_path(StorageScope::Documents, &abs_path)
                .is_err()
        );

        // External media without permission rejected
        let media_path = VirtualPath("photos/receipt.jpg".to_string());
        assert!(
            sandbox
                .resolve_path(StorageScope::ScopedExternalMedia, &media_path)
                .is_err()
        );

        // Grant permission via picker SAF
        sandbox.grant_media_access("photos/receipt.jpg");
        assert!(
            sandbox
                .resolve_path(StorageScope::ScopedExternalMedia, &media_path)
                .is_ok()
        );
    }

    #[test]
    fn test_mobile_battery_conservation_and_profiler() {
        let mut engine = BatteryConservationEngine::new(95.0, false);
        assert_eq!(engine.current_tier(), BatteryTier::Normal);
        assert_eq!(engine.perception_fps(), 30);
        assert!(!engine.should_offload_inference_to_shadow());

        // Low battery -> Conserve
        engine.update_power_state(&annora_substrate::PowerState {
            on_battery: true,
            battery_percent: 22.0,
            charging: false,
        });
        assert_eq!(engine.current_tier(), BatteryTier::Conserve);
        assert_eq!(engine.perception_fps(), 1);
        assert!(engine.should_offload_inference_to_shadow());

        // Offline check: even in Conserve mode, if offline, cannot offload to cloud
        assert!(!engine.should_offload_inference_with_network(false));
        assert!(engine.should_offload_inference_with_network(true));

        // Critical battery
        engine.update_power_state(&annora_substrate::PowerState {
            on_battery: true,
            battery_percent: 8.0,
            charging: false,
        });
        assert_eq!(engine.current_tier(), BatteryTier::Critical);
        assert_eq!(engine.perception_fps(), 0);

        // Plugged into charger -> Charging
        engine.update_power_state(&annora_substrate::PowerState {
            on_battery: false,
            battery_percent: 12.0,
            charging: true,
        });
        assert_eq!(engine.current_tier(), BatteryTier::Charging);
        assert_eq!(engine.perception_fps(), 30);
        assert!(!engine.should_offload_inference_to_shadow());

        // Energy profiler
        engine.record_agent_energy("MailAgent", 1500);
        engine.record_agent_energy("MailAgent", 500);
        assert_eq!(engine.get_agent_energy("MailAgent"), 2000);
    }

    fn future_expiry() -> annora_core::Timestamp {
        annora_core::Timestamp(chrono::Utc::now() + chrono::Duration::hours(1))
    }

    fn network_cap(action: annora_core::CapabilityAction) -> annora_core::CapabilityToken {
        annora_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Network],
                vec![action],
                future_expiry(),
            )
            .unwrap()
    }

    fn get_request(url: String) -> annora_substrate::HttpRequest {
        annora_substrate::HttpRequest {
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
    async fn test_mobile_http_egress_refuses_without_network_cap() {
        let substrate = MobileSubstrate::new();
        let wrong_cap = annora_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![annora_core::CapabilityAction::Read],
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
                annora_core::AnnoraError::CapabilityDenied(_)
                    | annora_core::AnnoraError::CapabilityExpired
            ),
            "unexpected error: {err}"
        );

        let expired_cap = annora_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Network],
                vec![annora_core::CapabilityAction::Read],
                annora_core::Timestamp(chrono::Utc::now() - chrono::Duration::hours(1)),
            )
            .unwrap();
        let err = substrate
            .http_request(get_request("http://127.0.0.1/".to_string()), &expired_cap)
            .await
            .expect_err("egress with expired cap must be refused");
        assert!(
            matches!(err, annora_core::AnnoraError::CapabilityExpired),
            "expired cap must yield CapabilityExpired, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_mobile_http_egress_refuses_host_off_allowlist() {
        let substrate = MobileSubstrate::new().with_egress_policy(
            annora_substrate::EgressPolicy::with_allowlist(["example.com"]),
        );
        let cap = network_cap(annora_core::CapabilityAction::Read);
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
    async fn test_mobile_http_egress_loopback_success() {
        let substrate = MobileSubstrate::new();
        assert!(substrate.egress_policy().is_loopback_only());
        let cap = network_cap(annora_core::CapabilityAction::Read);
        let addr = spawn_local_http_server(r#"{"ok":true}"#).await;
        let response = substrate
            .http_request(get_request(format!("http://{addr}/status")), &cap)
            .await
            .expect("loopback fetch under default policy must succeed");
        assert_eq!(response.status_code, 200);
        assert_eq!(response.body, b"{\"ok\":true}");
    }

    #[tokio::test]
    async fn test_mobile_camera_honest_unimplemented() {
        let substrate = MobileSubstrate::new();
        let cap = annora_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Camera],
                vec![annora_core::CapabilityAction::Read],
                future_expiry(),
            )
            .unwrap();
        let config = annora_substrate::CameraConfig {
            width: 1280,
            height: 720,
            fps: 30,
            facing: annora_substrate::CameraFacing::Front,
        };
        assert!(!config.is_available());
        let err = substrate
            .start_camera_capture(config, &cap)
            .await
            .expect_err("camera without Camera2 binding must be refused");
        let msg = err.to_string();
        assert!(msg.contains("camera not implemented"), "got: {msg}");
        assert!(
            msg.contains("Camera2"),
            "must name the missing backend, got: {msg}"
        );
    }

    #[test]
    fn test_mobile_thermal_and_vram_honest() {
        let substrate = MobileSubstrate::new();
        // No thermal HAL binding linked: explicit Unknown, never invented.
        assert!(substrate.thermal_state().is_unknown());
        // Shared NPU/GPU memory unknown without platform queries.
        assert!(substrate.manifest().gpu_vram_bytes.is_none());
        assert!(substrate.manifest().thermal_envelope_watts.is_none());
        assert_eq!(MobileComputeBackend::new().vram_available_bytes(), 0);
    }
}
