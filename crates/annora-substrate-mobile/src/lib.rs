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
        assert!(manifest.capabilities_granted.contains(&CapabilityScope::Audio));
        assert!(manifest.capabilities_denied.contains(&CapabilityScope::FullSystem));

        let attestation = substrate.attest(b"mobile-nonce").await.expect("Attestation failed");
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
        assert_eq!(sync.get_value("notes/todo.txt"), Some(b"buy milk and eggs".to_vec()));

        // Stale remote delta with lower version (version 1) must be ignored
        let stale_update = ShadowDelta {
            key: "notes/todo.txt".to_string(),
            value: b"stale text".to_vec(),
            version: 1,
            timestamp: annora_core::Timestamp::now(),
            is_deleted: false,
        };
        sync.push_remote_delta(stale_update);
        assert_eq!(sync.get_value("notes/todo.txt"), Some(b"buy milk and eggs".to_vec()));

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
        let resolved = sandbox.resolve_path(StorageScope::Documents, &path).unwrap();
        assert!(resolved.ends_with("Documents/state.db"));

        // Path traversal attack rejected
        let evil_path = VirtualPath("../../../etc/passwd".to_string());
        assert!(sandbox.resolve_path(StorageScope::Documents, &evil_path).is_err());

        // Windows drive traversal attack rejected
        let win_drive = VirtualPath("C:\\Windows\\System32".to_string());
        assert!(sandbox.resolve_path(StorageScope::Documents, &win_drive).is_err());

        // Absolute unix path rejected
        let abs_path = VirtualPath("/etc/shadow".to_string());
        assert!(sandbox.resolve_path(StorageScope::Documents, &abs_path).is_err());

        // External media without permission rejected
        let media_path = VirtualPath("photos/receipt.jpg".to_string());
        assert!(sandbox.resolve_path(StorageScope::ScopedExternalMedia, &media_path).is_err());

        // Grant permission via picker SAF
        sandbox.grant_media_access("photos/receipt.jpg");
        assert!(sandbox.resolve_path(StorageScope::ScopedExternalMedia, &media_path).is_ok());
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
}
