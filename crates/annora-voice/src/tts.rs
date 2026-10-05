use crate::codec::MIMI_SAMPLE_RATE;
use annora_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsRequest {
    pub text: String,
    pub voice_id: String,
    pub speed_rate: f32, // 1.0 = normal
    pub emotional_tone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsAudioChunk {
    pub sample_rate: u32,
    pub samples: Vec<f32>,
    pub is_final: bool,
}

#[async_trait]
pub trait TtsEngine: Send + Sync {
    async fn synthesize_stream(&self, req: &TtsRequest) -> Result<Vec<TtsAudioChunk>>;
}

pub struct CosyVoice2Engine {
    engine_name: String,
}

impl CosyVoice2Engine {
    pub fn new() -> Self {
        Self {
            engine_name: "CosyVoice2-NeuTTS-Air".to_string(),
        }
    }

    pub fn engine_name(&self) -> &str {
        &self.engine_name
    }
}

impl Default for CosyVoice2Engine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TtsEngine for CosyVoice2Engine {
    async fn synthesize_stream(&self, req: &TtsRequest) -> Result<Vec<TtsAudioChunk>> {
        let chunk_count = (req.text.len() / 20).max(1);
        let mut chunks = Vec::with_capacity(chunk_count);

        let samples_per_chunk = (MIMI_SAMPLE_RATE / 4) as usize; // 250ms chunks
        for i in 0..chunk_count {
            let is_final = i == chunk_count - 1;
            let mut samples = vec![0.0f32; samples_per_chunk];
            for (t, s) in samples.iter_mut().enumerate() {
                let freq = 440.0 * req.speed_rate;
                let phase = (t as f32 / MIMI_SAMPLE_RATE as f32) * freq * 2.0 * std::f32::consts::PI;
                *s = phase.sin() * 0.2;
            }
            chunks.push(TtsAudioChunk {
                sample_rate: MIMI_SAMPLE_RATE,
                samples,
                is_final,
            });
        }

        Ok(chunks)
    }
}
