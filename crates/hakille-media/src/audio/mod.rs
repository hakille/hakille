//! Sous-système de génération audio (Parler-TTS Candle, ONNX TTS, Formant Klatt,
//! Synthétiseur DSP polyphonique, Mastering & Sidechain Ducking).

pub mod candle_parler;
pub mod formant_adapter;
pub mod mastering;
pub mod onnx_tts;
pub mod procedural_synth;

use crate::audio::mastering::AudioBuffer;
use crate::error::MediaResult;
use crate::types::{
    AudioGenRequest, EngineDescriptor, EngineKind, GenerationControl, MediaDataExit, PreviewImage,
    ProgressCallback,
};
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct AudioGenerationOutput {
    pub wav_bytes: Vec<u8>,
    pub buffer: AudioBuffer,
    pub preview: PreviewImage,
    pub visemes_json: Option<Vec<u8>>,
    pub seed: u64,
    pub engine_id: String,
    pub engine_kind: EngineKind,
    pub data_exit: MediaDataExit,
    pub model_id: Option<String>,
    pub model_sha256: Option<String>,
}

#[async_trait]
pub trait AudioGeneratorBackend: Send + Sync {
    fn descriptor(&self) -> EngineDescriptor;

    async fn generate(
        &self,
        req: &AudioGenRequest,
        control: &GenerationControl,
        progress: Option<ProgressCallback>,
    ) -> MediaResult<AudioGenerationOutput>;
}
