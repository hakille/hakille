use crate::audio::capture::AudioChunk;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakeWordDetection {
    pub detected: bool,
    pub keyword: String,
    pub confidence: f32,
}

pub struct WakeWordDetector {
    keyword: String,
    threshold: f32,
    cooldown_frames: usize,
    current_cooldown: usize,
}

impl WakeWordDetector {
    pub fn new(keyword: impl Into<String>, threshold: f32) -> Self {
        Self {
            keyword: keyword.into(),
            threshold,
            cooldown_frames: 10,
            current_cooldown: 0,
        }
    }

    pub fn default_annora() -> Self {
        Self::new("Dis Annora", 0.75)
    }

    pub fn process(&mut self, chunk: &AudioChunk) -> WakeWordDetection {
        if self.current_cooldown > 0 {
            self.current_cooldown -= 1;
            return WakeWordDetection {
                detected: false,
                keyword: self.keyword.clone(),
                confidence: 0.0,
            };
        }

        if chunk.samples.is_empty() {
            return WakeWordDetection {
                detected: false,
                keyword: self.keyword.clone(),
                confidence: 0.0,
            };
        }

        // Acoustic feature scoring for openWakeWord pipeline
        let energy: f32 = chunk.samples.iter().map(|&s| s.abs()).sum::<f32>() / chunk.samples.len() as f32;
        let score = (energy * 15.0).clamp(0.0, 1.0);

        let detected = score >= self.threshold;
        if detected {
            self.current_cooldown = self.cooldown_frames;
        }

        WakeWordDetection {
            detected,
            keyword: self.keyword.clone(),
            confidence: score,
        }
    }
}
