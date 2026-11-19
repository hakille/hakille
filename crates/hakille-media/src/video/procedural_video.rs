//! Moteur vidéo procédural et cinématique pur Rust (`procedural-dsp`).
//!
//! Contrat d'honnêteté : se déclare explicitement [`EngineKind::ProceduralDsp`].
//! Supporte les 4 modes vidéo :
//! - **TextToVideo** : scène procédurale animée dans le temps + mouvement caméra ;
//! - **ImageToVideo** : animation fluide d'une image locale via [`CameraMotion`]
//!   (KenBurns, ZoomIn/Out, PanLeft/Right, Orbit) ;
//! - **KeyframeInterpolation** : interpolation croisée fluide entre palettes/scènes ;
//! - **AudioReactive** : réactivité temps réel pilotée par l'enveloppe RMS d'une
//!   piste WAV locale.

use crate::audio::mastering::read_wav_file;
use crate::error::{MediaError, MediaResult};
use crate::image::encode::{RgbaImageBuf, load_rgba_image, resize_rgba_bilinear};
use crate::image::procedural::{palette_from_prompt, render_scene_frame};
use crate::types::{
    EngineAvailability, EngineDescriptor, EngineKind, GenerationControl, MediaDataExit,
    MediaModality, MediaProgressEvent, ProgressCallback, VideoContainer, VideoGenMode,
    VideoGenRequest, derive_seed,
};
use crate::video::{
    VideoGenerationOutput, VideoGeneratorBackend, apply_camera_motion, encode_video_container,
};
use async_trait::async_trait;

pub struct ProceduralMotionVideoEngine;

impl ProceduralMotionVideoEngine {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ProceduralMotionVideoEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VideoGeneratorBackend for ProceduralMotionVideoEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            id: "procedural-dsp".into(),
            name: "Moteur Vidéo Procédural & Cinématique DSP (Rust pur)".into(),
            modality: MediaModality::Video,
            kind: EngineKind::ProceduralDsp,
            data_exit: MediaDataExit::Local,
            availability: EngineAvailability::Ready {
                device: "CPU (DSP Vidéo Rust pur)".into(),
                model_sha256: None,
            },
            estimated_ram_mb: 48,
            estimated_vram_mb: 0,
            capabilities: vec![
                "text_to_video".into(),
                "image_to_video".into(),
                "keyframe_interpolation".into(),
                "audio_reactive".into(),
                "camera_motion".into(),
                "apng".into(),
                "y4m".into(),
                "mp4_via_ffmpeg".into(),
            ],
            description: "Synthèse vidéo algorithmique pure Rust (animation fBm, mouvements caméra Ken Burns/Orbit, visualiseur audio-réactif RMS). Explicite DSP.".into(),
        }
    }

    async fn generate(
        &self,
        req: &VideoGenRequest,
        audio_wav_path: Option<&std::path::Path>,
        control: &GenerationControl,
        progress: Option<ProgressCallback>,
    ) -> MediaResult<VideoGenerationOutput> {
        let req = req.clone();
        let audio_wav = audio_wav_path.map(std::path::PathBuf::from);
        let control = control.clone();
        tokio::task::spawn_blocking(move || {
            render_procedural_video(&req, audio_wav.as_deref(), &control, progress)
        })
        .await
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: "procedural-dsp".into(),
            reason: format!("tâche vidéo interrompue : {e}"),
        })?
    }
}

fn render_procedural_video(
    req: &VideoGenRequest,
    external_audio_wav: Option<&std::path::Path>,
    control: &GenerationControl,
    progress: Option<ProgressCallback>,
) -> MediaResult<VideoGenerationOutput> {
    let seed = derive_seed(&req.prompt, req.seed);
    let total = req.num_frames.max(1);
    let w = req.width;
    let h = req.height;

    // Enveloppe audio RMS si mode AudioReactive ou piste audio externe présente.
    let audio_env: Vec<f32> = match &req.mode {
        VideoGenMode::AudioReactive { audio_wav_path } => {
            let wav = read_wav_file(audio_wav_path)?;
            wav.rms_envelope_for_frames(total as usize)
        }
        _ => {
            if let Some(wav_path) = external_audio_wav {
                read_wav_file(wav_path)
                    .map(|wav| wav.rms_envelope_for_frames(total as usize))
                    .unwrap_or_else(|_| vec![0.0; total as usize])
            } else {
                vec![0.0; total as usize]
            }
        }
    };

    let mut frames = Vec::with_capacity(total as usize);

    match &req.mode {
        VideoGenMode::ImageToVideo { init_image_path } => {
            let base = load_rgba_image(init_image_path)?;
            let resized = resize_rgba_bilinear(&base.pixels, base.width, base.height, w, h);
            let base_buf = RgbaImageBuf::new(w, h, resized)?;
            for i in 0..total {
                control.check()?;
                let t = if total > 1 {
                    i as f32 / (total - 1) as f32
                } else {
                    0.0
                };
                let warped = apply_camera_motion(&base_buf, req.camera_motion, t);
                frames.push(warped);
                emit_video_progress(
                    progress.as_ref(),
                    control,
                    i + 1,
                    total,
                    "camera_motion_i2v",
                );
            }
        }
        VideoGenMode::KeyframeInterpolation { keyframes } if keyframes.len() >= 2 => {
            let mut key_imgs = Vec::with_capacity(keyframes.len());
            for (k_idx, kf) in keyframes.iter().enumerate() {
                control.check()?;
                let pal = palette_from_prompt(&kf.prompt, seed.wrapping_add(k_idx as u64 * 997));
                let px = render_scene_frame(
                    w,
                    h,
                    seed.wrapping_add(k_idx as u64 * 997),
                    &pal,
                    k_idx as f32 * 0.7,
                    0.2,
                    1,
                    control,
                    None,
                )?;
                key_imgs.push(RgbaImageBuf::new(w, h, px)?);
            }
            for i in 0..total {
                control.check()?;
                let u = if total > 1 {
                    (i as f32 / (total - 1) as f32) * (key_imgs.len() - 1) as f32
                } else {
                    0.0
                };
                let idx0 = (u.floor() as usize).min(key_imgs.len() - 1);
                let idx1 = (idx0 + 1).min(key_imgs.len() - 1);
                let frac = smoothstep(u - idx0 as f32);
                let blended = blend_frames(&key_imgs[idx0], &key_imgs[idx1], frac);
                let t = if total > 1 {
                    i as f32 / (total - 1) as f32
                } else {
                    0.0
                };
                frames.push(apply_camera_motion(&blended, req.camera_motion, t));
                emit_video_progress(
                    progress.as_ref(),
                    control,
                    i + 1,
                    total,
                    "interpolation_keyframes",
                );
            }
        }
        _ => {
            let palette = palette_from_prompt(&req.prompt, seed);
            for i in 0..total {
                control.check()?;
                let t = if total > 1 {
                    i as f32 / (total - 1) as f32
                } else {
                    0.0
                };
                let phase = i as f32 / f32::from(req.fps.max(1));
                let energy = audio_env.get(i as usize).copied().unwrap_or(0.0);
                let px = render_scene_frame(w, h, seed, &palette, phase, energy, 1, control, None)?;
                let raw_frame = RgbaImageBuf::new(w, h, px)?;
                frames.push(apply_camera_motion(&raw_frame, req.camera_motion, t));
                emit_video_progress(
                    progress.as_ref(),
                    control,
                    i + 1,
                    total,
                    "synthèse_vidéo_dsp",
                );
            }
        }
    }

    let preview = frames
        .get(frames.len() / 2)
        .or_else(|| frames.first())
        .map(|f| f.make_preview(200, 120))
        .unwrap_or(crate::types::PreviewImage {
            width: 0,
            height: 0,
            rgba: Vec::new(),
        });

    let effective_audio = match &req.mode {
        VideoGenMode::AudioReactive { audio_wav_path } => Some(audio_wav_path.as_path()),
        _ => external_audio_wav,
    };
    let (encoded_bytes, container) =
        encode_video_container(&frames, req.fps, req.container, effective_audio)?;

    Ok(VideoGenerationOutput {
        encoded_bytes,
        container,
        preview,
        seed,
        engine_id: "procedural-dsp".into(),
        engine_kind: EngineKind::ProceduralDsp,
        data_exit: MediaDataExit::Local,
        model_id: None,
        model_sha256: None,
    })
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn blend_frames(a: &RgbaImageBuf, b: &RgbaImageBuf, t: f32) -> RgbaImageBuf {
    let mut out = vec![0u8; a.pixels.len()];
    for i in 0..a.pixels.len() {
        let va = f32::from(a.pixels[i]);
        let vb = f32::from(b.pixels[i]);
        out[i] = (va * (1.0 - t) + vb * t).round().clamp(0.0, 255.0) as u8;
    }
    RgbaImageBuf {
        width: a.width,
        height: a.height,
        pixels: out,
    }
}

fn emit_video_progress(
    progress: Option<&ProgressCallback>,
    control: &GenerationControl,
    frame: u32,
    total: u32,
    stage: &str,
) {
    if let Some(cb) = progress {
        cb(MediaProgressEvent {
            job_id: control.job_id.clone(),
            modality: MediaModality::Video,
            stage: stage.into(),
            step: frame,
            total_steps: total,
            fraction: frame as f32 / total.max(1) as f32,
            elapsed_ms: control.elapsed_ms(),
            message: format!("Rendu vidéo frame {frame}/{total}"),
            preview: None,
        });
    }
}

/// Vérifie explicitement qu'un conteneur nécessitant `ffmpeg` signale clairement
/// son état.
pub fn container_requires_ffmpeg(c: VideoContainer) -> bool {
    matches!(c, VideoContainer::Mp4H264 | VideoContainer::WebmVp9)
}
