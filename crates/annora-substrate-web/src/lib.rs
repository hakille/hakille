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
        assert!(manifest.capabilities_granted.contains(&CapabilityScope::Audio));
        assert!(manifest.capabilities_denied.contains(&CapabilityScope::FullSystem));

        let attestation = substrate.attest(b"web-nonce").await.expect("Attestation failed");
        assert_eq!(attestation.platform_id, "Wasm-Browser-Client");
        assert!(!attestation.signature.0.is_empty());
    }

    #[test]
    fn test_web_compute_backends_and_quantization() {
        let compute = WebComputeBackend::new();
        let backends = compute.available_backends();
        assert!(backends.contains(&annora_substrate::ComputeBackend::WebGpu));
        assert!(backends.contains(&annora_substrate::ComputeBackend::Cpu));
        assert_eq!(compute.preferred_backend(), annora_substrate::ComputeBackend::WebGpu);
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
        assert!(bridge.buffer().overrun_count() > 0, "Buffer overrun must be detected and counted");
        assert!(bridge.buffer().queued_samples_count() <= 3200);

        // Test zero sample rate safety
        let zero_cfg = WebAudioNodeConfig {
            input_sample_rate: 48000,
            target_sample_rate: 0,
            buffer_capacity_samples: 1000,
            is_muted: false,
        };
        let mut zero_bridge = WebAudioBridge::new(zero_cfg);
        assert_eq!(zero_bridge.buffer_mut().ingest_worklet_samples_f32(&samples), 0);
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
        assert_eq!(detector.optimal_compute_backend(), annora_substrate::ComputeBackend::WebGpu);

        // Allocation validation
        assert!(detector.validate_tensor_allocation(512 * 1024 * 1024).is_ok());
        assert!(detector.validate_tensor_allocation(4 * 1024 * 1024 * 1024).is_err()); // Exceeds 2GB

        // Workgroup dispatch validation
        assert!(detector.validate_workgroup_dispatch(256, 16 * 1024).is_ok());
        assert!(detector.validate_workgroup_dispatch(512, 16 * 1024).is_err()); // Exceeds 256 invocations

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
        assert_eq!(fallback_detector.optimal_compute_backend(), annora_substrate::ComputeBackend::Cpu);
        assert!(!fallback_detector.can_run_half_precision_gemm());
        assert!(fallback_detector.validate_tensor_allocation(1024).is_err());
    }

    #[tokio::test]
    async fn test_web_canvas_screen_capture() {
        let substrate = WebSubstrate::new();
        let cap = annora_core::CapabilityAuthority::new()
            .create_root_token(
                vec![CapabilityScope::GuiInput],
                vec![annora_core::CapabilityAction::Read],
                annora_core::Timestamp::now(),
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
}
