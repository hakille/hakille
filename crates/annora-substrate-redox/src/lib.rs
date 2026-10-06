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
    use annora_core::{CapabilityScope, Tier, VirtualPath};
    use annora_substrate::Substrate;

    #[tokio::test]
    async fn test_redox_substrate_manifest_and_attestation() {
        let substrate = RedoxSubstrate::new();
        let manifest = substrate.manifest();
        assert_eq!(manifest.tier, Tier::Sovereign);
        assert_eq!(manifest.os_name, "Redox");
        assert!(manifest.has_tpm);
        assert!(manifest.has_secure_enclave);
        assert!(manifest.capabilities_granted.contains(&CapabilityScope::FullSystem));

        let attestation = substrate.attest(b"redox-nonce").await.expect("Attestation failed");
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
        assert_eq!(handler.chan().write(chan_handle, payload).unwrap(), payload.len());

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
        let orb_handle = handler.dispatch_open("orbital:Annora_Overlay", 0).unwrap();
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
        let written = handler.handle_packet(&mut write_pkt, &mut mem_pool).unwrap();
        assert_eq!(written, msg.len());

        let mut read_pkt = RedoxPacket::new(3, SYS_READ, hdl, 200, 64, 0);
        let read_len = handler.handle_packet(&mut read_pkt, &mut mem_pool).unwrap();
        assert_eq!(read_len, msg.len());
        assert_eq!(&mem_pool[200..200 + read_len], msg);

        let mut close_pkt = RedoxPacket::new(4, SYS_CLOSE, hdl, 0, 0, 0);
        assert_eq!(handler.handle_packet(&mut close_pkt, &mut mem_pool).unwrap(), 0);
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
        let derived_ro = table.derive_restricted_capability(fd_full, CAP_READ).unwrap();
        assert!(table.verify_access(derived_ro, CAP_READ).is_ok());
        assert!(table.verify_access(derived_ro, CAP_WRITE).is_err());

        // Attempting to derive privilege parent lacks must fail
        assert!(table.derive_restricted_capability(fd_full, CAP_EXEC).is_err());

        // Transfer / delegation
        let delegated = table.transfer_capability(derived_ro, "agent-subtask-view").unwrap();
        assert!(table.verify_access(delegated, CAP_READ).is_ok());

        // Revocation
        assert!(table.revoke_handle(fd_read).is_ok());
        assert!(table.verify_access(fd_read, CAP_READ).is_err());
    }

    #[tokio::test]
    async fn test_redox_substrate_io_and_screen() {
        let substrate = RedoxSubstrate::new();
        let cap = annora_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![annora_core::CapabilityAction::Write],
                annora_core::Timestamp::now(),
            )
            .unwrap();

        let path = VirtualPath("test_sovereign.txt".to_string());
        substrate.write_file(&path, b"Sovereign Redox Microkernel", &cap).await.unwrap();

        let data = substrate.read_file(&path, &cap).await.unwrap();
        assert_eq!(data, b"Sovereign Redox Microkernel");

        let screen = substrate.capture_screen(&cap).await.unwrap();
        assert_eq!(screen.width, 1920);
        assert_eq!(screen.height, 1080);
    }
}
