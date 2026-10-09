use crate::audio::capture::AudioChunk;
use annora_inference::SileroVadEngine;
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
    model_path: Option<std::path::PathBuf>,
}

impl VoiceActivityDetector {
    pub fn new(threshold: f32, hangover_frames: usize) -> Self {
        Self {
            threshold,
            hangover_frames,
            active_hangover: 0,
            model_path: None,
        }
    }

    pub fn with_onnx_model(path: impl AsRef<std::path::Path>) -> Self {
        Self {
            threshold: 0.5,
            hangover_frames: 3,
            active_hangover: 0,
            model_path: Some(path.as_ref().to_path_buf()),
        }
    }

    pub fn model_path(&self) -> Option<&std::path::Path> {
        self.model_path.as_deref()
    }

    pub fn default_silero_profile() -> Self {
        Self::new(0.5, 3)
    }

    pub fn silero_engine(&self) -> SileroVadEngine {
        if let Some(ref p) = self.model_path {
            SileroVadEngine::with_model(p, self.threshold)
        } else {
            SileroVadEngine::new(self.threshold)
        }
    }

    pub fn process(&mut self, chunk: &AudioChunk) -> VadResult {
        if chunk.samples.is_empty() {
            return VadResult {
                is_speech: false,
                speech_probability: 0.0,
                energy_rms: 0.0,
            };
        }

        let sum_sq: f32 = chunk.samples.iter().map(|&s| if s.is_finite() { s * s } else { 0.0 }).sum();
        let rms = (sum_sq / chunk.samples.len() as f32).sqrt();
        let rms = if rms.is_finite() { rms } else { 0.0 };

        let engine = self.silero_engine();
        let probability = engine.evaluate_speech_probability(&chunk.samples);

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
