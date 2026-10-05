use crate::audio::capture::AudioChunk;
use annora_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerTurn {
    pub speaker_id: String,
    pub start_time: f32,
    pub end_time: f32,
    pub confidence: f32,
}

#[async_trait]
pub trait Diarizer: Send + Sync {
    async fn diarize(&self, chunk: &AudioChunk) -> Result<Vec<SpeakerTurn>>;
}

pub struct NemotronDiarizer {
    max_speakers: usize,
}

impl NemotronDiarizer {
    pub fn new(max_speakers: usize) -> Self {
        Self { max_speakers }
    }

    pub fn max_speakers(&self) -> usize {
        self.max_speakers
    }
}

impl Default for NemotronDiarizer {
    fn default() -> Self {
        Self::new(4)
    }
}

#[async_trait]
impl Diarizer for NemotronDiarizer {
    async fn diarize(&self, chunk: &AudioChunk) -> Result<Vec<SpeakerTurn>> {
        let dur = chunk.duration_seconds();
        if dur < 0.1 {
            return Ok(vec![]);
        }

        Ok(vec![SpeakerTurn {
            speaker_id: "Speaker_0".to_string(),
            start_time: 0.0,
            end_time: dur,
            confidence: 0.91,
        }])
    }
}
