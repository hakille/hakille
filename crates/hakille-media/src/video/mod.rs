//! Sous-système de génération vidéo (ONNX 5D, Candle SLERP, Procédural &
//! Caméra virtuelle, encodeurs APNG/Y4M pur Rust et muxer explicite `ffmpeg`).

pub mod candle_keyframe;
pub mod encoder;
pub mod muxer;
pub mod onnx_video;
pub mod procedural_video;

use crate::error::{MediaError, MediaResult};
use crate::image::encode::RgbaImageBuf;
use crate::types::{
    CameraMotion, EngineDescriptor, EngineKind, GenerationControl, MediaDataExit, PreviewImage,
    ProgressCallback, VideoContainer, VideoGenRequest,
};
use async_trait::async_trait;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct VideoGenerationOutput {
    pub encoded_bytes: Vec<u8>,
    pub container: VideoContainer,
    pub preview: PreviewImage,
    pub seed: u64,
    pub engine_id: String,
    pub engine_kind: EngineKind,
    pub data_exit: MediaDataExit,
    pub model_id: Option<String>,
    pub model_sha256: Option<String>,
}

#[async_trait]
pub trait VideoGeneratorBackend: Send + Sync {
    fn descriptor(&self) -> EngineDescriptor;

    async fn generate(
        &self,
        req: &VideoGenRequest,
        audio_wav_path: Option<&Path>,
        control: &GenerationControl,
        progress: Option<ProgressCallback>,
    ) -> MediaResult<VideoGenerationOutput>;
}

/// Encode une séquence de frames dans le conteneur demandé.
///
/// Aucun repli silencieux : `Mp4H264` et `WebmVp9` exigent un binaire `ffmpeg`
/// fonctionnel et renvoient [`MediaError::MuxerUnavailable`] s'il est absent.
pub fn encode_video_container(
    frames: &[RgbaImageBuf],
    fps: u16,
    container: VideoContainer,
    audio_wav_path: Option<&Path>,
) -> MediaResult<(Vec<u8>, VideoContainer)> {
    match container {
        VideoContainer::Apng => Ok((encoder::encode_apng(frames, fps)?, VideoContainer::Apng)),
        VideoContainer::Y4m => Ok((encoder::encode_y4m(frames, fps)?, VideoContainer::Y4m)),
        VideoContainer::FrameSequence => Ok((
            encoder::encode_frame_sequence_bundle(frames, fps)?,
            VideoContainer::FrameSequence,
        )),
        VideoContainer::Mp4H264 | VideoContainer::WebmVp9 => {
            let muxer = muxer::FfmpegMuxer::detect().ok_or_else(|| {
                MediaError::MuxerUnavailable {
                    binary: "ffmpeg".into(),
                    reason: "installer ffmpeg ou utiliser un conteneur natif Rust (--format apng | y4m | frames)".into(),
                }
            })?;
            let bytes = muxer.mux_frames(frames, fps, container, audio_wav_path)?;
            Ok((bytes, container))
        }
    }
}

/// Applique un mouvement caméra virtuel bilinéaire doux à une frame à l'instant
/// normalisé `t ∈ [0.0, 1.0]`.
pub fn apply_camera_motion(src: &RgbaImageBuf, motion: CameraMotion, t: f32) -> RgbaImageBuf {
    if matches!(motion, CameraMotion::Static) {
        return src.clone();
    }
    let t = t.clamp(0.0, 1.0);
    let (scale, shift_x, shift_y) = match motion {
        CameraMotion::Static => (1.0f32, 0.0f32, 0.0f32),
        CameraMotion::ZoomIn => (1.0 + 0.16 * t, 0.0, 0.0),
        CameraMotion::ZoomOut => (1.16 - 0.16 * t, 0.0, 0.0),
        CameraMotion::PanLeft => (1.12, 0.06 - 0.12 * t, 0.0),
        CameraMotion::PanRight => (1.12, -0.06 + 0.12 * t, 0.0),
        CameraMotion::Orbit => {
            let angle = t * std::f32::consts::TAU;
            (1.10, 0.04 * angle.cos(), 0.03 * angle.sin())
        }
        CameraMotion::KenBurns => (1.02 + 0.14 * t, -0.04 + 0.08 * t, 0.03 - 0.06 * t),
    };

    let w = src.width;
    let h = src.height;
    let mut dst = vec![0u8; src.pixels.len()];
    let cx = (w as f32) * 0.5;
    let cy = (h as f32) * 0.5;
    let inv_scale = 1.0 / scale.max(0.01);

    for y in 0..h {
        let sy = ((y as f32 - cy) * inv_scale + cy + shift_y * h as f32).clamp(0.0, (h - 1) as f32);
        let y0 = sy.floor() as u32;
        let y1 = (y0 + 1).min(h - 1);
        let fy = sy - y0 as f32;
        for x in 0..w {
            let sx =
                ((x as f32 - cx) * inv_scale + cx + shift_x * w as f32).clamp(0.0, (w - 1) as f32);
            let x0 = sx.floor() as u32;
            let x1 = (x0 + 1).min(w - 1);
            let fx = sx - x0 as f32;

            let i00 = ((y0 * w + x0) * 4) as usize;
            let i10 = ((y0 * w + x1) * 4) as usize;
            let i01 = ((y1 * w + x0) * 4) as usize;
            let i11 = ((y1 * w + x1) * 4) as usize;
            let out_i = ((y * w + x) * 4) as usize;

            for c in 0..4 {
                let top = f32::from(src.pixels[i00 + c]) * (1.0 - fx)
                    + f32::from(src.pixels[i10 + c]) * fx;
                let bot = f32::from(src.pixels[i01 + c]) * (1.0 - fx)
                    + f32::from(src.pixels[i11 + c]) * fx;
                dst[out_i + c] = (top * (1.0 - fy) + bot * fy).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    RgbaImageBuf {
        width: w,
        height: h,
        pixels: dst,
    }
}
