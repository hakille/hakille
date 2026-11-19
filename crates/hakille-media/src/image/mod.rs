//! Sous-système de génération d'images (Candle SD 1.5/Turbo, FLUX.1 Schnell,
//! ONNX Diffusion, Procédural DSP, Loopback & Cloud opt-in).

pub mod candle_flux;
pub mod candle_sd;
pub mod encode;
pub mod onnx_diffusion;
pub mod procedural;

use crate::error::MediaResult;
use crate::image::encode::RgbaImageBuf;
use crate::types::{
    EngineDescriptor, EngineKind, GenerationControl, ImageGenRequest, MediaDataExit, PreviewImage,
    ProgressCallback,
};
use async_trait::async_trait;

/// Sortie brute produite par un backend image avant persistance & signature Ed25519.
#[derive(Debug, Clone)]
pub struct ImageGenerationOutput {
    pub encoded_bytes: Vec<u8>,
    pub rgba: RgbaImageBuf,
    pub preview: PreviewImage,
    pub seed: u64,
    pub engine_id: String,
    pub engine_kind: EngineKind,
    pub data_exit: MediaDataExit,
    pub model_id: Option<String>,
    pub model_sha256: Option<String>,
}

#[async_trait]
pub trait ImageGeneratorBackend: Send + Sync {
    fn descriptor(&self) -> EngineDescriptor;

    async fn generate(
        &self,
        req: &ImageGenRequest,
        control: &GenerationControl,
        progress: Option<ProgressCallback>,
    ) -> MediaResult<ImageGenerationOutput>;
}
