use annora_core::Result;
use annora_inference::SafetensorsModel;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MIMI_FRAME_RATE_HZ: f32 = 12.5;
pub const MIMI_FRAME_DURATION_MS: u32 = 80;
pub const MIMI_NUM_CODEBOOKS: usize = 8;
pub const MIMI_SAMPLE_RATE: u32 = 24000;
pub const SAMPLES_PER_FRAME: usize = (MIMI_SAMPLE_RATE as f32 / MIMI_FRAME_RATE_HZ) as usize; // 1920

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MimiFrameTokens {
    pub codebooks: [u16; MIMI_NUM_CODEBOOKS],
}

impl Default for MimiFrameTokens {
    fn default() -> Self {
        Self {
            codebooks: [0; MIMI_NUM_CODEBOOKS],
        }
    }
}

pub struct MimiCodec {
    frame_counter: u64,
    safetensors_metadata: Option<SafetensorsModel>,
}

impl MimiCodec {
    pub fn new() -> Self {
        Self {
            frame_counter: 0,
            safetensors_metadata: None,
        }
    }

    pub fn with_safetensors_file(path: impl AsRef<Path>) -> Result<Self> {
        let model = SafetensorsModel::read_from_file(path)?;
        Ok(Self::with_safetensors(model))
    }

    pub fn with_safetensors(model: SafetensorsModel) -> Self {
        Self {
            frame_counter: 0,
            safetensors_metadata: Some(model),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.safetensors_metadata.is_some()
    }

    pub fn safetensors_info(&self) -> Option<&SafetensorsModel> {
        self.safetensors_metadata.as_ref()
    }

    pub fn encode_frame(&mut self, samples: &[f32]) -> Result<MimiFrameTokens> {
        self.frame_counter += 1;
        let mut codebooks = [0u16; MIMI_NUM_CODEBOOKS];
        if !samples.is_empty() {
            let chunk_size = samples.len() / MIMI_NUM_CODEBOOKS;
            for i in 0..MIMI_NUM_CODEBOOKS {
                let start = i * chunk_size;
                let end = ((i + 1) * chunk_size).min(samples.len());
                let energy: f32 = samples[start..end].iter().map(|s| s.abs()).sum();
                codebooks[i] = ((energy * 100.0) as u16) % 2048;
            }
        }
        Ok(MimiFrameTokens { codebooks })
    }

    pub fn decode_frame(&mut self, tokens: &MimiFrameTokens) -> Result<Vec<f32>> {
        self.frame_counter += 1;
        let mut samples = vec![0.0f32; SAMPLES_PER_FRAME];
        for (i, &code) in tokens.codebooks.iter().enumerate() {
            let amp = (code as f32) / 2048.0;
            let freq = (i + 1) as f32 * 220.0;
            for (t, s) in samples.iter_mut().enumerate() {
                let phase = (t as f32 / MIMI_SAMPLE_RATE as f32) * freq * 2.0 * std::f32::consts::PI;
                *s += amp * phase.sin() * 0.1;
            }
        }
        Ok(samples)
    }
}

impl Default for MimiCodec {
    fn default() -> Self {
        Self::new()
    }
}
