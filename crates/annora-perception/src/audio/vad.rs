use crate::audio::capture::AudioChunk;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VadResult {
    pub is_speech: bool,
    pub speech_probability: f32,
    pub energy_rms: f32,
}

pub struct VoiceActivityDetector {
    threshold: f32,
    hangover_frames: usize,
    active_hangover: usize,
}

impl VoiceActivityDetector {
    pub fn new(threshold: f32, hangover_frames: usize) -> Self {
        Self {
            threshold,
            hangover_frames,
            active_hangover: 0,
        }
    }

    pub fn default_silero_profile() -> Self {
        Self::new(0.5, 3)
    }

    pub fn process(&mut self, chunk: &AudioChunk) -> VadResult {
        if chunk.samples.is_empty() {
            return VadResult {
                is_speech: false,
                speech_probability: 0.0,
                energy_rms: 0.0,
            };
        }

        let sum_sq: f32 = chunk.samples.iter().map(|&s| s * s).sum();
        let rms = (sum_sq / chunk.samples.len() as f32).sqrt();

        // Calculate zero-crossing rate
        let mut zero_crossings = 0;
        for i in 1..chunk.samples.len() {
            if (chunk.samples[i] >= 0.0 && chunk.samples[i - 1] < 0.0)
                || (chunk.samples[i] < 0.0 && chunk.samples[i - 1] >= 0.0)
            {
                zero_crossings += 1;
            }
        }
        let zcr = zero_crossings as f32 / chunk.samples.len() as f32;

        // Speech probability model (combines RMS energy and voice-band ZCR)
        let energy_score = (rms * 25.0).clamp(0.0, 1.0);
        let zcr_score = if (0.02..0.35).contains(&zcr) { 1.0 } else { 0.2 };
        let probability = (energy_score * 0.7 + zcr_score * 0.3).clamp(0.0, 1.0);

        let instantaneous_speech = probability >= self.threshold;

        if instantaneous_speech {
            self.active_hangover = self.hangover_frames;
        } else if self.active_hangover > 0 {
            self.active_hangover -= 1;
        }

        let is_speech = instantaneous_speech || self.active_hangover > 0;

        VadResult {
            is_speech,
            speech_probability: probability,
            energy_rms: rms,
        }
    }
}
