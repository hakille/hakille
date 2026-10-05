pub mod audio;
pub mod vision;
pub mod world_model;

pub use audio::{
    asr::*, capture::AudioChunk, capture::ContinuousAudioRingBuffer, diarization::*, speaker_id::*,
    vad::*, wake_word::*,
};
pub use vision::{
    capture::FrameFormat, capture::VideoFrame, grounding::*, scene::*,
};
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
}
