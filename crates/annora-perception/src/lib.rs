pub mod audio;
pub mod vision;
pub mod world_model;

pub use audio::{
    asr::*, capture::AudioChunk, capture::ContinuousAudioRingBuffer, diarization::*, speaker_id::*,
    vad::*, wake_word::*,
};
pub use vision::*;
pub use world_model::*;


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vad_silence_vs_speech() {
        let mut vad = VoiceActivityDetector::default_silero_profile();

        // 1. Silent chunk
        let silent_chunk = AudioChunk {
            samples: vec![0.0f32; 1600],
            sample_rate: 16_000,
            channels: 1,
            timestamp: annora_core::Timestamp::now(),
        };
        let res_silence = vad.process(&silent_chunk);
        assert!(!res_silence.is_speech);
        assert_eq!(res_silence.energy_rms, 0.0);

        // 2. Synthesized speech-like chunk (150 Hz sine wave with high energy)
        let mut speech_samples = Vec::new();
        for t in 0..1600 {
            let s = (t as f32 / 16000.0 * 150.0 * 2.0 * std::f32::consts::PI).sin() * 0.4;
            speech_samples.push(s);
        }
        let speech_chunk = AudioChunk {
            samples: speech_samples,
            sample_rate: 16_000,
            channels: 1,
            timestamp: annora_core::Timestamp::now(),
        };
        let res_speech = vad.process(&speech_chunk);
        assert!(res_speech.is_speech);
        assert!(res_speech.speech_probability > 0.5);
    }

    #[test]
    fn test_wake_word_trigger_and_cooldown() {
        let mut detector = WakeWordDetector::default_annora();

        // Silence: not detected
        let silent_chunk = AudioChunk {
            samples: vec![0.0f32; 1600],
            sample_rate: 16_000,
            channels: 1,
            timestamp: annora_core::Timestamp::now(),
        };
        assert!(!detector.process(&silent_chunk).detected);

        // High-energy voice chunk: detected
        let energetic_chunk = AudioChunk {
            samples: vec![0.3f32; 1600],
            sample_rate: 16_000,
            channels: 1,
            timestamp: annora_core::Timestamp::now(),
        };
        let det = detector.process(&energetic_chunk);
        assert!(det.detected);
        assert_eq!(det.keyword, "Dis Annora");

        // Immediately following chunk: must be blocked by cooldown
        let follow_up = detector.process(&energetic_chunk);
        assert!(!follow_up.detected, "Cooldown must prevent consecutive wake word triggers");
    }

    #[test]
    fn test_vad_nan_and_inf_resilience() {
        let mut vad = VoiceActivityDetector::default_silero_profile();

        let corrupted_chunk = AudioChunk {
            samples: vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.2, 0.3],
            sample_rate: 16_000,
            channels: 1,
            timestamp: annora_core::Timestamp::now(),
        };

        let res = vad.process(&corrupted_chunk);
        assert!(!res.energy_rms.is_nan(), "RMS must never be NaN");
        assert!(!res.speech_probability.is_nan(), "Speech probability must never be NaN");
        assert!(res.speech_probability >= 0.0 && res.speech_probability <= 1.0);
    }

    #[test]
    fn test_camera_pipeline_and_world_model() {
        let config = annora_substrate::CameraConfig {
            width: 640,
            height: 480,
            fps: 30,
            facing: annora_substrate::CameraFacing::Front,
        };
        let mut pipeline = CameraPipeline::new(config.clone());
        assert_eq!(pipeline.frames_processed(), 0);

        // 1. Process dark empty frame
        let dark_frame = annora_substrate::CameraFrame {
            width: 640,
            height: 480,
            data: vec![5u8; 640 * 480 * 3],
            format: annora_substrate::ImageFormat::Rgb8,
            timestamp_ms: 1000,
        };
        let ctx_dark = pipeline.process_frame(&dark_frame);
        assert_eq!(ctx_dark.lighting, LightingCondition::Dark);
        assert_eq!(ctx_dark.user_presence, UserPresence::Absent);
        assert_eq!(pipeline.frames_processed(), 1);

        // 2. Process daylight frame with centered subject
        let mut bright_data = vec![140u8; 640 * 480 * 3];
        // Inject high center contrast (representing a face/silhouette)
        for y in 160..320 {
            for x in 200..440 {
                let idx = (y * 640 + x) * 3;
                bright_data[idx] = 230;
                bright_data[idx + 1] = 190;
                bright_data[idx + 2] = 170;
            }
        }
        let bright_frame = annora_substrate::CameraFrame {
            width: 640,
            height: 480,
            data: bright_data,
            format: annora_substrate::ImageFormat::Rgb8,
            timestamp_ms: 2000,
        };
        let ctx_bright = pipeline.process_frame(&bright_frame);
        assert_eq!(ctx_bright.lighting, LightingCondition::IndoorAdequate);
        assert_eq!(ctx_bright.user_presence, UserPresence::Present);
        assert!(ctx_bright.user_gaze_focused_on_device);
        assert_eq!(pipeline.frames_processed(), 2);

        // 3. Update WorldModel with camera perception
        let mut world_model = WorldModel::new("HomeOffice");
        pipeline.update_world_model(&ctx_bright, &mut world_model);

        assert!(world_model.state().user_present);
        assert!(world_model.state().user_attentive);
        assert!(!world_model.state().third_parties_present);
        assert!(world_model.is_private_environment());
    }

    #[test]
    fn test_environmental_privacy_arbitration() {
        let mut world_model = WorldModel::new("ConferenceRoom");
        assert!(world_model.is_private_environment());

        // Introduce multiple people in room
        world_model.update_visual_presence(true, false, 3, 2.5, "Daylight");
        assert!(world_model.state().third_parties_present);
        assert!(!world_model.is_private_environment());
        assert!(world_model.should_suppress_loud_audio());

        // In meeting
        world_model.set_meeting_status(true);
        assert!(world_model.should_suppress_loud_audio());
    }
}

