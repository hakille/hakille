//! Moteur vidéo neuronal par interpolation latente sphérique (SLERP) dans
//! l'espace latent Stable Diffusion (`candle-keyframe`).
//!
//! Principe :
//! 1. Génère les latents clés via Stable Diffusion Candle (UNet + CLIP) ;
//! 2. Interpole les vecteurs latents sur la sphère unitaire (SLERP) pour des
//!    transitions organiques sans flou fantôme ;
//! 3. Décode chaque latent interpolé via le vrai `AutoEncoderKL` ;
//! 4. Applique le mouvement caméra virtuel (`CameraMotion`) et encode le flux.
//!
//! Renvoie honnêtement [`MediaError::ModelMissing`] si les poids SD 1.5 ne sont
//! pas provisionnés localement.

use crate::catalog::{GenerativeModelProfile, MediaModelCatalog};
use crate::error::{MediaError, MediaResult};
use crate::image::encode::RgbaImageBuf;
use crate::types::{
    EngineAvailability, EngineDescriptor, EngineKind, GenerationControl, MediaDataExit,
    MediaModality, MediaProgressEvent, ProgressCallback, VideoGenMode, VideoGenRequest,
    derive_seed,
};
use crate::video::{
    VideoGenerationOutput, VideoGeneratorBackend, apply_camera_motion, encode_video_container,
};
use async_trait::async_trait;
use candle_core::{DType, Device, Module, Tensor};
use candle_transformers::models::stable_diffusion::{
    StableDiffusionConfig, build_clip_transformer, ddim::DDIMSchedulerConfig,
    schedulers::SchedulerConfig,
};
use std::path::{Path, PathBuf};
use tokenizers::Tokenizer;

pub struct CandleKeyframeVideoEngine {
    models_dir: PathBuf,
}

impl CandleKeyframeVideoEngine {
    pub fn new(models_dir: impl Into<PathBuf>) -> Self {
        Self {
            models_dir: models_dir.into(),
        }
    }

    fn base_profile(&self) -> GenerativeModelProfile {
        MediaModelCatalog::by_id("sd-1.5-candle").expect("profil sd-1.5-candle présent")
    }
}

#[async_trait]
impl VideoGeneratorBackend for CandleKeyframeVideoEngine {
    fn descriptor(&self) -> EngineDescriptor {
        let profile = self.base_profile();
        let availability = profile.probe_availability(&self.models_dir);
        EngineDescriptor {
            id: "candle-keyframe".into(),
            name: "Vidéo SLERP Latente Candle (SD 1.5 VAE)".into(),
            modality: MediaModality::Video,
            kind: EngineKind::NeuralLocalCandle,
            data_exit: MediaDataExit::Local,
            availability,
            estimated_ram_mb: 3_800,
            estimated_vram_mb: 3_400,
            capabilities: vec![
                "text_to_video".into(),
                "keyframe_interpolation".into(),
                "latent_slerp".into(),
                "camera_motion".into(),
            ],
            description: "Génération vidéo neurale 100 % Rust via interpolation latente sphérique (SLERP) + décodage VAE Candle + caméra virtuelle.".into(),
        }
    }

    async fn generate(
        &self,
        req: &VideoGenRequest,
        audio_wav_path: Option<&Path>,
        control: &GenerationControl,
        progress: Option<ProgressCallback>,
    ) -> MediaResult<VideoGenerationOutput> {
        let profile = self.base_profile();
        let model_sha256 = match profile.probe_availability(&self.models_dir) {
            EngineAvailability::Ready { model_sha256, .. } => model_sha256,
            EngineAvailability::ModelMissing {
                model_id,
                missing_files,
                hint,
            } => {
                return Err(MediaError::ModelMissing {
                    model_id,
                    missing: missing_files,
                    hint,
                });
            }
            EngineAvailability::Unconfigured { env_hint } => {
                return Err(MediaError::BackendUnavailable {
                    engine_id: "candle-keyframe".into(),
                    reason: env_hint,
                });
            }
            EngineAvailability::Unavailable { reason } => {
                return Err(MediaError::BackendUnavailable {
                    engine_id: "candle-keyframe".into(),
                    reason,
                });
            }
        };

        let model_dir = profile.model_dir(&self.models_dir);
        let req = req.clone();
        let audio = audio_wav_path.map(PathBuf::from);
        let control = control.clone();

        tokio::task::spawn_blocking(move || {
            run_slerp_video(
                &model_dir,
                model_sha256,
                &req,
                audio.as_deref(),
                &control,
                progress,
            )
        })
        .await
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: "candle-keyframe".into(),
            reason: format!("tâche vidéo Candle SLERP interrompue : {e}"),
        })?
    }
}

/// Interpolation sphérique (SLERP) entre deux vecteurs latents aplatis.
pub fn slerp_latents(a: &[f32], b: &[f32], t: f32) -> Vec<f32> {
    let t = t.clamp(0.0, 1.0);
    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for (&x, &y) in a.iter().zip(b.iter()) {
        let xf = f64::from(x);
        let yf = f64::from(y);
        dot += xf * yf;
        norm_a += xf * xf;
        norm_b += yf * yf;
    }
    let denom = (norm_a.sqrt() * norm_b.sqrt()).max(1e-12);
    let cos_omega = (dot / denom).clamp(-1.0, 1.0);
    let omega = cos_omega.acos();
    if omega.abs() < 1e-4 {
        return a
            .iter()
            .zip(b.iter())
            .map(|(&x, &y)| x * (1.0 - t) + y * t)
            .collect();
    }
    let sin_omega = omega.sin();
    let s0 = ((1.0 - f64::from(t)) * omega).sin() / sin_omega;
    let s1 = (f64::from(t) * omega).sin() / sin_omega;
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| (s0 * f64::from(x) + s1 * f64::from(y)) as f32)
        .collect()
}

fn run_slerp_video(
    model_dir: &Path,
    model_sha256: Option<String>,
    req: &VideoGenRequest,
    audio_wav_path: Option<&Path>,
    control: &GenerationControl,
    progress: Option<ProgressCallback>,
) -> MediaResult<VideoGenerationOutput> {
    let engine_id = "candle-keyframe";
    let device = Device::Cpu;
    let dtype = DType::F32;
    let seed = derive_seed(&req.prompt, req.seed);

    let width = (req.width / 8).max(8) * 8;
    let height = (req.height / 8).max(8) * 8;
    let sd_config =
        StableDiffusionConfig::v1_5(Some(1), Some(height as usize), Some(width as usize));
    let tokenizer = Tokenizer::from_file(model_dir.join("tokenizer.json")).map_err(|e| {
        MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        }
    })?;
    let clip_model = build_clip_transformer(
        &sd_config.clip,
        model_dir.join("clip.safetensors"),
        &device,
        dtype,
    )
    .map_err(|e| MediaError::InferenceFailed {
        engine_id: engine_id.into(),
        reason: e.to_string(),
    })?;

    let prompts: Vec<String> = match &req.mode {
        VideoGenMode::KeyframeInterpolation { keyframes } if keyframes.len() >= 2 => {
            keyframes.iter().map(|k| k.prompt.clone()).collect()
        }
        _ => vec![req.prompt.clone(), format!("{}, subtle motion", req.prompt)],
    };

    let mut embeddings = Vec::with_capacity(prompts.len());
    for p in &prompts {
        control.check()?;
        let mut ids = tokenizer
            .encode(p.as_str(), true)
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            })?
            .get_ids()
            .to_vec();
        ids.resize(sd_config.clip.max_position_embeddings, 0);
        let tok = Tensor::new(ids.as_slice(), &device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            })?;
        let emb = clip_model
            .forward(&tok)
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            })?;
        embeddings.push(emb);
    }
    drop(clip_model);

    // Débruitage UNet des keyframes en latents.
    let unet = sd_config
        .build_unet(model_dir.join("unet.safetensors"), &device, 4, false, dtype)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        })?;
    let mut key_latents: Vec<Vec<f32>> = Vec::with_capacity(embeddings.len());
    let lh = (height / 8) as usize;
    let lw = (width / 8) as usize;

    for (k, emb) in embeddings.iter().enumerate() {
        control.check()?;
        let _ = device.set_seed(seed.wrapping_add(k as u64 * 17));
        let mut scheduler =
            DDIMSchedulerConfig::default()
                .build(12)
                .map_err(|e| MediaError::InferenceFailed {
                    engine_id: engine_id.into(),
                    reason: e.to_string(),
                })?;
        let mut lat = Tensor::randn(0f32, 1f32, (1, 4, lh, lw), &device).map_err(|e| {
            MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            }
        })?;
        for &ts in scheduler.timesteps().to_vec().iter() {
            control.check()?;
            let inp = scheduler.scale_model_input(lat.clone(), ts).map_err(|e| {
                MediaError::InferenceFailed {
                    engine_id: engine_id.into(),
                    reason: e.to_string(),
                }
            })?;
            let pred =
                unet.forward(&inp, ts as f64, emb)
                    .map_err(|e| MediaError::InferenceFailed {
                        engine_id: engine_id.into(),
                        reason: e.to_string(),
                    })?;
            lat = scheduler
                .step(&pred, ts, &lat)
                .map_err(|e| MediaError::InferenceFailed {
                    engine_id: engine_id.into(),
                    reason: e.to_string(),
                })?;
        }
        let flat = lat
            .flatten_all()
            .and_then(|t| t.to_vec1::<f32>())
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            })?;
        key_latents.push(flat);
    }
    drop(unet);

    // Décodage VAE des latents interpolés par SLERP.
    let vae = sd_config
        .build_vae(model_dir.join("vae.safetensors"), &device, dtype)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        })?;
    let total = req.num_frames.max(1);
    let mut frames = Vec::with_capacity(total as usize);

    for i in 0..total {
        control.check()?;
        let t_global = if total > 1 {
            i as f32 / (total - 1) as f32
        } else {
            0.0
        };
        let pos = t_global * (key_latents.len() - 1) as f32;
        let k0 = (pos.floor() as usize).min(key_latents.len() - 1);
        let k1 = (k0 + 1).min(key_latents.len() - 1);
        let interp = slerp_latents(&key_latents[k0], &key_latents[k1], pos - k0 as f32);
        let lat_t = Tensor::from_vec(interp, (1, 4, lh, lw), &device)
            .and_then(|t| t / 0.18215)
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            })?;
        let decoded = vae
            .decode(&lat_t)
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            })?;
        let rgb = ((decoded / 2.0)
            .and_then(|t| t + 0.5)
            .and_then(|t| t.clamp(0f32, 1f32))
            .and_then(|t| t * 255.0)
            .and_then(|t| t.to_dtype(DType::U8))
            .and_then(|t| t.squeeze(0))
            .and_then(|t| t.permute((1, 2, 0)))
            .and_then(|t| t.flatten_all())
            .and_then(|t| t.to_vec1::<u8>()))
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        })?;
        let frame_rgba = RgbaImageBuf::from_rgb(width, height, &rgb)?;
        frames.push(apply_camera_motion(
            &frame_rgba,
            req.camera_motion,
            t_global,
        ));

        if let Some(cb) = &progress {
            cb(MediaProgressEvent {
                job_id: control.job_id.clone(),
                modality: MediaModality::Video,
                stage: "décodage_slerp_vae".into(),
                step: i + 1,
                total_steps: total,
                fraction: (i + 1) as f32 / total as f32,
                elapsed_ms: control.elapsed_ms(),
                message: format!("Vidéo Candle SLERP frame {}/{total}", i + 1),
                preview: None,
            });
        }
    }

    let preview =
        frames
            .first()
            .map(|f| f.make_preview(200, 120))
            .unwrap_or(crate::types::PreviewImage {
                width: 0,
                height: 0,
                rgba: Vec::new(),
            });
    let (encoded_bytes, container) =
        encode_video_container(&frames, req.fps, req.container, audio_wav_path)?;

    Ok(VideoGenerationOutput {
        encoded_bytes,
        container,
        preview,
        seed,
        engine_id: engine_id.into(),
        engine_kind: EngineKind::NeuralLocalCandle,
        data_exit: MediaDataExit::Local,
        model_id: Some("sd-1.5-candle".into()),
        model_sha256,
    })
}
