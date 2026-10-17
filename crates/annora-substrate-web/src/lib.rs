pub mod compute_impl;
pub mod substrate_impl;
pub mod web_audio;
pub mod web_canvas;
pub mod webgpu;

pub use compute_impl::*;
pub use substrate_impl::*;
pub use web_audio::*;
pub use web_canvas::*;
pub use webgpu::*;

#[cfg(test)]
mod tests {
    use super::*;
    use annora_core::{CapabilityScope, Tier};
    use annora_substrate::{Compute, Substrate};

    #[tokio::test]
    async fn test_web_substrate_manifest_and_attestation() {
        let substrate = WebSubstrate::new();
        let manifest = substrate.manifest();
        assert_eq!(manifest.tier, Tier::Guest);
        assert_eq!(manifest.os_name, "WebAssembly");
        assert!(!manifest.has_secure_enclave);
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
            .attest(b"web-nonce")
            .await
            .expect("Attestation failed");
        assert_eq!(attestation.platform_id, "Wasm-Browser-Client");
        assert!(!attestation.signature.0.is_empty());
    }

    #[test]
    fn test_web_compute_backends_and_quantization() {
        let compute = WebComputeBackend::new();
        let backends = compute.available_backends();
        assert!(backends.contains(&annora_substrate::ComputeBackend::WebGpu));
        assert!(backends.contains(&annora_substrate::ComputeBackend::Cpu));
        assert_eq!(
            compute.preferred_backend(),
            annora_substrate::ComputeBackend::WebGpu
        );
        assert!(compute.supports_quantization("q4_k"));
        assert!(compute.supports_quantization("int8"));
        assert!(!compute.supports_quantization("unsupported_quant"));
    }

    #[test]
    fn test_web_audio_pcm_streaming_buffer() {
        let config = WebAudioNodeConfig {
            input_sample_rate: 48000,
            target_sample_rate: 16000,
            buffer_capacity_samples: 3200, // 200ms buffer
            is_muted: false,
        };
        let mut bridge = WebAudioBridge::new(config);

        // Feed 4800 samples at 48kHz (100ms of audio: 440 Hz sine wave)
        let samples: Vec<f32> = (0..4800)
            .map(|i| (i as f32 * 440.0 * 2.0 * std::f32::consts::PI / 48000.0).sin())
            .collect();

        let ingested = bridge.buffer_mut().ingest_worklet_samples_f32(&samples);
        assert_eq!(ingested, 1600); // 4800 / 3 = 1600 samples at 16kHz
        assert_eq!(bridge.buffer().queued_samples_count(), 1600);

        // Pop 320 samples (20ms chunk for STT)
        let chunk = bridge.buffer_mut().pop_pcm_chunk(320);
        assert!(chunk.is_some());
        assert_eq!(chunk.unwrap().len(), 320);
        assert_eq!(bridge.buffer().queued_samples_count(), 1280);

        // Test muting
        bridge.set_muted(true);
        let muted_ingested = bridge.buffer_mut().ingest_worklet_samples_f32(&samples);
        assert_eq!(muted_ingested, 0);

        // Test TTS playback buffer
        bridge.buffer_mut().push_playback_audio(b"PCM_SPEECH_DATA");
        let popped = bridge.buffer_mut().pop_playback_audio(15);
        assert_eq!(&popped, b"PCM_SPEECH_DATA");

        // Test overrun protection (fill beyond 3200 capacity)
        let large_burst = vec![0.8f32; 15000]; // 5000 samples after 3:1 resampling
        bridge.set_muted(false);
        bridge.buffer_mut().ingest_worklet_samples_f32(&large_burst);
        assert!(
            bridge.buffer().overrun_count() > 0,
            "Buffer overrun must be detected and counted"
        );
        assert!(bridge.buffer().queued_samples_count() <= 3200);

        // Test zero sample rate safety
        let zero_cfg = WebAudioNodeConfig {
            input_sample_rate: 48000,
            target_sample_rate: 0,
            buffer_capacity_samples: 1000,
            is_muted: false,
        };
        let mut zero_bridge = WebAudioBridge::new(zero_cfg);
        assert_eq!(
            zero_bridge
                .buffer_mut()
                .ingest_worklet_samples_f32(&samples),
            0
        );
    }

    #[test]
    fn test_webgpu_compute_capability_detection() {
        let detector = WebGpuDetector::new();
        assert!(detector.capabilities().is_webgpu_supported);
        assert!(detector.capabilities().has_shader_f16);
        assert!(detector.capabilities().has_subgroups);
        assert!(detector.can_run_half_precision_gemm());

        // Limits test
        assert!(detector.capabilities().limits.max_buffer_size >= 2 * 1024 * 1024 * 1024);
        assert_eq!(
            detector.optimal_compute_backend(),
            annora_substrate::ComputeBackend::WebGpu
        );

        // Allocation validation
        assert!(
            detector
                .validate_tensor_allocation(512 * 1024 * 1024)
                .is_ok()
        );
        assert!(
            detector
                .validate_tensor_allocation(4 * 1024 * 1024 * 1024)
                .is_err()
        ); // Exceeds 2GB

        // Workgroup dispatch validation
        assert!(detector.validate_workgroup_dispatch(256, 16 * 1024).is_ok());
        assert!(
            detector
                .validate_workgroup_dispatch(512, 16 * 1024)
                .is_err()
        ); // Exceeds 256 invocations

        // Fallback test when WebGPU is unavailable
        let fallback_caps = WebGpuCapabilities {
            is_webgpu_supported: false,
            adapter_name: "Fallback CPU SIMD".to_string(),
            has_shader_f16: false,
            has_subgroups: false,
            has_timestamp_query: false,
            limits: WebGpuLimits::default(),
        };
        let fallback_detector = WebGpuDetector::with_capabilities(fallback_caps);
        assert_eq!(
            fallback_detector.optimal_compute_backend(),
            annora_substrate::ComputeBackend::Cpu
        );
        assert!(!fallback_detector.can_run_half_precision_gemm());
        assert!(fallback_detector.validate_tensor_allocation(1024).is_err());
    }

    #[tokio::test]
    async fn test_web_canvas_screen_capture() {
        let substrate = WebSubstrate::new();
        // Enforcement: capture_screen exige Sensor/Read (pas GuiInput).
        // Expiration future : Timestamp::now() expirerait à la seconde suivante.
        let cap = annora_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::Sensor],
                vec![annora_core::CapabilityAction::Read],
                annora_core::Timestamp(chrono::Utc::now() + chrono::Duration::hours(1)),
            )
            .unwrap();

        let screen = substrate.capture_screen(&cap).await.unwrap();
        assert_eq!(screen.width, 1920);
        assert_eq!(screen.height, 1080);
        assert_eq!(screen.data.len(), 1920 * 1080 * 4);

        // Test direct ImageData ingestion from browser canvas
        let adapter = WebScreenCaptureAdapter::default();
        let sample_rgba = vec![255u8; 100 * 100 * 4];
        let ingested = adapter.ingest_image_data(100, 100, &sample_rgba).unwrap();
        assert_eq!(ingested.width, 100);
        assert_eq!(ingested.height, 100);
        assert_eq!(ingested.data.len(), 100 * 100 * 4);

        // Test invalid image data length
        assert!(adapter.ingest_image_data(100, 100, &vec![0u8; 50]).is_err());
        // Test zero dimensions
        assert!(adapter.ingest_image_data(0, 100, &[]).is_err());
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
    async fn test_web_http_egress_refuses_without_network_cap() {
        let substrate = WebSubstrate::new();
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
    async fn test_web_http_egress_refuses_host_off_allowlist() {
        let substrate = WebSubstrate::new().with_egress_policy(
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
    async fn test_web_http_egress_loopback_success() {
        let substrate = WebSubstrate::new();
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
    async fn test_web_camera_honest_unimplemented() {
        let substrate = WebSubstrate::new();
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
            .expect_err("camera without getUserMedia binding must be refused");
        let msg = err.to_string();
        assert!(msg.contains("camera not implemented"), "got: {msg}");
        assert!(
            msg.contains("getUserMedia"),
            "must name the missing backend, got: {msg}"
        );
    }

    #[test]
    fn test_web_thermal_and_vram_honest() {
        let substrate = WebSubstrate::new();
        // Compute Pressure API not wired: explicit Unknown, never invented.
        assert!(substrate.thermal_state().is_unknown());
        // VRAM only knowable from the live WebGPU adapter.limits.
        assert!(substrate.manifest().gpu_vram_bytes.is_none());
        assert!(substrate.manifest().thermal_envelope_watts.is_none());
    }
}
