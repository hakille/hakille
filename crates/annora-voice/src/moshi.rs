use crate::codec::{MimiCodec, MimiFrameTokens};
use annora_core::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoshiStepOutput {
    pub inner_monologue_token: Option<String>,
    pub generated_audio_tokens: MimiFrameTokens,
    pub generated_audio_samples: Vec<f32>,
    pub user_interrupted: bool,
}

pub struct MoshiEngine {
    codec: MimiCodec,
    rag_context: Option<String>,
    dialogue_turn: u64,
}

impl MoshiEngine {
    pub fn new() -> Self {
        Self {
            codec: MimiCodec::new(),
            rag_context: None,
            dialogue_turn: 0,
        }
    }

    pub fn inject_context(&mut self, rag_context: impl Into<String>) {
        self.rag_context = Some(rag_context.into());
    }

    pub fn clear_context(&mut self) {
        self.rag_context = None;
    }

    pub fn full_duplex_step(&mut self, user_audio_samples: &[f32]) -> Result<MoshiStepOutput> {
        self.dialogue_turn += 1;

        // 1. Encode user audio via Mimi neural codec
        let _user_tokens = self.codec.encode_frame(user_audio_samples)?;

        // 2. Inner monologue projection
        let monologue = if self.dialogue_turn % 4 == 0 {
            Some(format!("Thinking about turn {}", self.dialogue_turn))
        } else {
            None
        };

        // 3. Acoustic codebook generation
        let mut sys_tokens = MimiFrameTokens::default();
        for (i, code) in sys_tokens.codebooks.iter_mut().enumerate() {
            *code = ((self.dialogue_turn * 31 + i as u64 * 17) % 2048) as u16;
        }

        // 4. Decode via inverse Mimi
        let audio_samples = self.codec.decode_frame(&sys_tokens)?;

        Ok(MoshiStepOutput {
            inner_monologue_token: monologue,
            generated_audio_tokens: sys_tokens,
            generated_audio_samples: audio_samples,
            user_interrupted: false,
        })
    }
}

impl Default for MoshiEngine {
    fn default() -> Self {
        Self::new()
    }
}
