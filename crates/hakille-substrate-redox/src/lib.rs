pub mod capabilities;
pub mod compute_impl;
pub mod scheme;
pub mod substrate_impl;

pub use capabilities::*;
pub use compute_impl::*;
pub use scheme::*;
pub use substrate_impl::*;

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_core::{CapabilityScope, Tier, VirtualPath};
    use hakille_substrate::Substrate;

    #[tokio::test]
    async fn test_redox_substrate_manifest_and_attestation() {
        let substrate = RedoxSubstrate::new();
        let manifest = substrate.manifest();
        assert_eq!(manifest.tier, Tier::Sovereign);
        assert_eq!(manifest.os_name, "Redox");
        assert!(manifest.has_tpm);
        assert!(manifest.has_secure_enclave);
        // Honnêteté manifeste: FullSystem REFUSÉ tant que l'enforcement Biscuit
        // de bout en bout n'est pas complet (handles de table = jetons porteurs).
        assert!(
            manifest
                .capabilities_denied
                .contains(&CapabilityScope::FullSystem)
        );
        assert!(
            manifest
                .capabilities_granted
                .contains(&CapabilityScope::Sensor)
        );

        let attestation = substrate
            .attest(b"redox-nonce")
            .await
            .expect("Attestation failed");
        assert_eq!(attestation.platform_id, "Redox-Sovereign-Dedicated-Node");
        assert!(!attestation.signature.0.is_empty());
    }

    #[test]
    fn test_redox_native_schemes() {
        let handler = RedoxSchemeHandler::new();
        assert!(handler.is_scheme_supported("chan:"));
        assert!(handler.is_scheme_supported("orbital:"));
        assert!(handler.is_scheme_supported("disk:"));
        assert!(handler.is_scheme_supported("sys:"));
        assert!(handler.is_scheme_supported("audio:"));

        // Test chan: scheme communication between agent and kernel
        let chan_handle = handler.dispatch_open("chan:kernel/inbox", 0).unwrap();
        let payload = b"ping from sovereign agent";
        assert_eq!(
            handler.chan().write(chan_handle, payload).unwrap(),
            payload.len()
        );

        let mut read_buf = vec![0u8; 64];
        let bytes_read = handler.chan().read(chan_handle, &mut read_buf).unwrap();
        assert_eq!(&read_buf[..bytes_read], payload);

        // Test sys: telemetry scheme
        let sys_handle = handler.dispatch_open("sys:memory", 0).unwrap();
        let mut sys_buf = vec![0u8; 128];
        let sys_bytes = handler.sys().read(sys_handle, &mut sys_buf).unwrap();
        let sys_str = std::str::from_utf8(&sys_buf[..sys_bytes]).unwrap();
        assert!(sys_str.contains("MemTotal:"));

        // Test orbital: scheme window surface creation
        let orb_handle = handler.dispatch_open("orbital:Hakille_Overlay", 0).unwrap();
        let surface = handler.orbital().get_surface(orb_handle);
        assert!(surface.is_some());
        let (w, h, fb) = surface.unwrap();
        assert_eq!(w, 1920);
        assert_eq!(h, 1080);
        assert_eq!(fb.len(), 1920 * 1080 * 4);

        // Test RedoxPacket microkernel IPC handling
        let mut mem_pool = vec![0u8; 1024];
        let path = b"chan:agent/ipc_packet";
        mem_pool[..path.len()].copy_from_slice(path);

        let mut open_pkt = RedoxPacket::new(1, SYS_OPEN, 0, 0, path.len(), 0);
        let hdl = handler.handle_packet(&mut open_pkt, &mut mem_pool).unwrap();
        assert_eq!(open_pkt.a, hdl);

        let msg = b"Microkernel packet IPC message";
        mem_pool[100..100 + msg.len()].copy_from_slice(msg);
        let mut write_pkt = RedoxPacket::new(2, SYS_WRITE, hdl, 100, msg.len(), 0);
        let written = handler
            .handle_packet(&mut write_pkt, &mut mem_pool)
            .unwrap();
        assert_eq!(written, msg.len());

        let mut read_pkt = RedoxPacket::new(3, SYS_READ, hdl, 200, 64, 0);
        let read_len = handler.handle_packet(&mut read_pkt, &mut mem_pool).unwrap();
        assert_eq!(read_len, msg.len());
        assert_eq!(&mem_pool[200..200 + read_len], msg);

        let mut close_pkt = RedoxPacket::new(4, SYS_CLOSE, hdl, 0, 0, 0);
        assert_eq!(
            handler
                .handle_packet(&mut close_pkt, &mut mem_pool)
                .unwrap(),
            0
        );
    }

    #[test]
    fn test_redox_capability_based_access() {
        let table = RedoxCapabilityTable::new();
        let fd_read = table.allocate_handle("disk:", "ledger.dat", CAP_READ);
        let fd_full = table.allocate_handle("disk:", "state.db", CAP_READ | CAP_WRITE);

        // Verify granted access
        assert!(table.verify_access(fd_read, CAP_READ).is_ok());
        // Verify denied ungranted write access
        assert!(table.verify_access(fd_read, CAP_WRITE).is_err());

        // Full access fd
        assert!(table.verify_access(fd_full, CAP_READ).is_ok());
        assert!(table.verify_access(fd_full, CAP_WRITE).is_ok());

        // Capability derivation / attenuation
        let derived_ro = table
            .derive_restricted_capability(fd_full, CAP_READ)
            .unwrap();
        assert!(table.verify_access(derived_ro, CAP_READ).is_ok());
        assert!(table.verify_access(derived_ro, CAP_WRITE).is_err());

        // Attempting to derive privilege parent lacks must fail
        assert!(
            table
                .derive_restricted_capability(fd_full, CAP_EXEC)
                .is_err()
        );

        // Transfer / delegation
        let delegated = table
            .transfer_capability(derived_ro, "agent-subtask-view")
            .unwrap();
        assert!(table.verify_access(delegated, CAP_READ).is_ok());

        // Revocation
        assert!(table.revoke_handle(fd_read).is_ok());
        assert!(table.verify_access(fd_read, CAP_READ).is_err());
    }

    #[tokio::test]
    async fn test_redox_substrate_io_and_screen() {
        let substrate = RedoxSubstrate::new();
        // Enforcement: write exige Filesystem/Write, read exige Filesystem/Read,
        // capture_screen exige Sensor/Read.
        // Expiration future : Timestamp::now() expirerait à la seconde suivante.
        let expiry = hakille_core::Timestamp(chrono::Utc::now() + chrono::Duration::hours(1));
        let fs_cap = hakille_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![
                    hakille_core::CapabilityAction::Read,
                    hakille_core::CapabilityAction::Write,
                ],
                expiry.clone(),
            )
            .unwrap();
        let sensor_cap = hakille_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Sensor],
                vec![hakille_core::CapabilityAction::Read],
                expiry,
            )
            .unwrap();

        let path = VirtualPath("test_sovereign.txt".to_string());
        substrate
            .write_file(&path, b"Sovereign Redox Microkernel", &fs_cap)
            .await
            .unwrap();

        let data = substrate.read_file(&path, &fs_cap).await.unwrap();
        assert_eq!(data, b"Sovereign Redox Microkernel");

        let screen = substrate.capture_screen(&sensor_cap).await.unwrap();
        assert_eq!(screen.width, 1920);
        assert_eq!(screen.height, 1080);
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
    async fn test_redox_http_egress_refuses_without_network_cap() {
        let substrate = RedoxSubstrate::new();
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
    async fn test_redox_http_egress_refuses_host_off_allowlist() {
        let substrate = RedoxSubstrate::new().with_egress_policy(
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
    async fn test_redox_http_egress_loopback_success() {
        let substrate = RedoxSubstrate::new();
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
    async fn test_redox_camera_honest_unimplemented() {
        let substrate = RedoxSubstrate::new();
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
            facing: hakille_substrate::CameraFacing::Front,
        };
        assert!(!config.is_available());
        let err = substrate
            .start_camera_capture(config, &cap)
            .await
            .expect_err("camera without orbital scheme binding must be refused");
        let msg = err.to_string();
        assert!(msg.contains("camera not implemented"), "got: {msg}");
        assert!(
            msg.contains("orbital"),
            "must name the missing backend, got: {msg}"
        );
    }

    #[test]
    fn test_redox_thermal_and_vram_honest() {
        let substrate = RedoxSubstrate::new();
        // No sys: thermal binding linked: explicit Unknown, never invented.
        assert!(substrate.thermal_state().is_unknown());
        // VRAM undiscoverable without an orbital/sys query; envelope unknown.
        assert!(substrate.manifest().gpu_vram_bytes.is_none());
        assert!(substrate.manifest().thermal_envelope_watts.is_none());
    }
}
