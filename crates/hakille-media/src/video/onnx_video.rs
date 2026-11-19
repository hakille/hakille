//! Moteur vidéo neuronal spatio-temporel via ONNX Runtime (`onnx-video`).
//!
//! Pilote les tenseurs 5D `[B, C, T, H, W]` (`video_unet.onnx` +
//! `vae_decoder.onnx` + `text_encoder.onnx` + `tokenizer.json`) avec contrôle
//! d'annulation par pas. Renvoie honnêtement [`MediaError::ModelMissing`] si
//! les artefacts locaux sont absents.

use crate::catalog::{GenerativeModelProfile, MediaModelCatalog};
use crate::error::{MediaError, MediaResult};
use crate::image::encode::RgbaImageBuf;
use crate::types::{
    EngineAvailability, EngineDescriptor, EngineKind, GenerationControl, MediaDataExit,
    MediaModality, MediaProgressEvent, ProgressCallback, VideoGenRequest, derive_seed,
};
use crate::video::{VideoGenerationOutput, VideoGeneratorBackend, encode_video_container};
use async_trait::async_trait;
use ort::{session::Session, value::Tensor};
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};
use std::path::{Path, PathBuf};
use tokenizers::Tokenizer;

fn open_ort_session(
    env: &std::sync::Arc<ort::environment::Environment>,
    path: &Path,
    engine_id: &str,
) -> MediaResult<Session> {
    let builder = Session::builder(env).map_err(|e| MediaError::InferenceFailed {
        engine_id: engine_id.into(),
        reason: e.to_string(),
    })?;
    let mut builder = builder
        .with_intra_threads(4)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        })?;
    builder
        .commit_from_file(path)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        })
}

pub struct OnnxVideoDiffusionEngine {
    models_dir: PathBuf,
}

impl OnnxVideoDiffusionEngine {
    pub fn new(models_dir: impl Into<PathBuf>) -> Self {
        Self {
            models_dir: models_dir.into(),
        }
    }

    fn profile(&self) -> GenerativeModelProfile {
        MediaModelCatalog::by_id("onnx-video").expect("profil onnx-video présent")
    }
}

#[async_trait]
impl VideoGeneratorBackend for OnnxVideoDiffusionEngine {
    fn descriptor(&self) -> EngineDescriptor {
        let profile = self.profile();
        let availability = profile.probe_availability(&self.models_dir);
        EngineDescriptor {
            id: profile.id,
            name: profile.name,
            modality: MediaModality::Video,
            kind: EngineKind::NeuralLocalOnnx,
            data_exit: MediaDataExit::Local,
            availability,
            estimated_ram_mb: profile.expected_ram_mb,
            estimated_vram_mb: profile.expected_vram_mb,
            capabilities: vec![
                "text_to_video".into(),
                "spatiotemporal_5d".into(),
                "onnx_runtime".into(),
            ],
            description: profile.description,
        }
    }

    async fn generate(
        &self,
        req: &VideoGenRequest,
        audio_wav_path: Option<&Path>,
        control: &GenerationControl,
        progress: Option<ProgressCallback>,
    ) -> MediaResult<VideoGenerationOutput> {
        let profile = self.profile();
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
                    engine_id: profile.id,
                    reason: env_hint,
                });
            }
            EngineAvailability::Unavailable { reason } => {
                return Err(MediaError::BackendUnavailable {
                    engine_id: profile.id,
                    reason,
                });
            }
        };

        let model_dir = profile.model_dir(&self.models_dir);
        let req = req.clone();
        let audio = audio_wav_path.map(PathBuf::from);
        let control = control.clone();
        let engine_id = profile.id.clone();

        tokio::task::spawn_blocking(move || {
            run_onnx_video(
                &model_dir,
                &engine_id,
                model_sha256,
                &req,
                audio.as_deref(),
                &control,
                progress,
            )
        })
        .await
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: profile.id,
            reason: format!("tâche ONNX vidéo interrompue : {e}"),
        })?
    }
}

fn run_onnx_video(
    model_dir: &Path,
    engine_id: &str,
    model_sha256: Option<String>,
    req: &VideoGenRequest,
    audio_wav_path: Option<&Path>,
    control: &GenerationControl,
    progress: Option<ProgressCallback>,
) -> MediaResult<VideoGenerationOutput> {
    let env = hakille_inference::shared_ort_env().map_err(|e| MediaError::InferenceFailed {
        engine_id: engine_id.into(),
        reason: e.to_string(),
    })?;
    let seed = derive_seed(&req.prompt, req.seed);
    control.check()?;

    let tokenizer = Tokenizer::from_file(model_dir.join("tokenizer.json")).map_err(|e| {
        MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        }
    })?;
    let mut ids: Vec<i64> = tokenizer
        .encode(req.prompt.as_str(), true)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        })?
        .get_ids()
        .iter()
        .map(|&v| i64::from(v))
        .collect();
    ids.resize(77, 0);

    let mut text_enc = open_ort_session(&env, &model_dir.join("text_encoder.onnx"), engine_id)?;
    let ids_t = Tensor::<i64>::from_array(([1usize, 77], ids)).map_err(|e| {
        MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        }
    })?;
    let txt_out = text_enc
        .run(ort::inputs!["input_ids" => ids_t])
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        })?;
    let (emb_shape, emb_vals) =
        txt_out[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            })?;
    let emb_dims: Vec<usize> = emb_shape.iter().map(|&d| d as usize).collect();
    let emb_vec = emb_vals.to_vec();
    drop(txt_out);
    drop(text_enc);

    let mut unet = open_ort_session(&env, &model_dir.join("video_unet.onnx"), engine_id)?;

    let t_frames = req.num_frames.max(1) as usize;
    let lh = (req.height / 8).max(4) as usize;
    let lw = (req.width / 8).max(4) as usize;
    let mut rng = StdRng::seed_from_u64(seed);
    let mut latents: Vec<f32> = (0..(4 * t_frames * lh * lw))
        .map(|_| {
            let u1 = ((rng.next_u32() as f32) / (u32::MAX as f32)).max(1e-7);
            let u2 = (rng.next_u32() as f32) / (u32::MAX as f32);
            (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
        })
        .collect();

    let steps = 12u32;
    for step_idx in 0..steps {
        control.check()?;
        let t_val = ((steps - step_idx) as f32 / steps as f32) * 999.0;
        let sample_t = Tensor::<f32>::from_array(([1usize, 4, t_frames, lh, lw], latents.clone()))
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            })?;
        let ts_t = Tensor::<f32>::from_array(([1usize], vec![t_val])).map_err(|e| {
            MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            }
        })?;
        let emb_t =
            Tensor::<f32>::from_array((emb_dims.as_slice(), emb_vec.clone())).map_err(|e| {
                MediaError::InferenceFailed {
                    engine_id: engine_id.into(),
                    reason: e.to_string(),
                }
            })?;
        let out = unet
            .run(ort::inputs![
                "sample" => sample_t,
                "timestep" => ts_t,
                "encoder_hidden_states" => emb_t,
            ])
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            })?;
        let (_, pred) =
            out[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| MediaError::InferenceFailed {
                    engine_id: engine_id.into(),
                    reason: e.to_string(),
                })?;
        let dt = 1.0 / steps as f32;
        for (l, &p) in latents.iter_mut().zip(pred.iter()) {
            *l -= p * dt;
        }
        if let Some(cb) = &progress {
            cb(MediaProgressEvent {
                job_id: control.job_id.clone(),
                modality: MediaModality::Video,
                stage: "débruitage_onnx_5d".into(),
                step: step_idx + 1,
                total_steps: steps,
                fraction: (step_idx + 1) as f32 / steps as f32,
                elapsed_ms: control.elapsed_ms(),
                message: format!("ONNX Vidéo 5D pas {}/{steps}", step_idx + 1),
                preview: None,
            });
        }
    }
    drop(unet);

    let mut vae = open_ort_session(&env, &model_dir.join("vae_decoder.onnx"), engine_id)?;

    let mut frames = Vec::with_capacity(t_frames);
    let hw = (req.width as usize) * (req.height as usize);
    for f in 0..t_frames {
        control.check()?;
        let mut frame_lat = vec![0.0f32; 4 * lh * lw];
        for c in 0..4 {
            let src_off = (c * t_frames + f) * lh * lw;
            let dst_off = c * lh * lw;
            for j in 0..(lh * lw) {
                frame_lat[dst_off + j] = latents[src_off + j] / 0.18215;
            }
        }
        let vae_in = Tensor::<f32>::from_array(([1usize, 4, lh, lw], frame_lat)).map_err(|e| {
            MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            }
        })?;
        let vae_out = vae
            .run(ort::inputs!["latent_sample" => vae_in])
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            })?;
        let (_, chw) =
            vae_out[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| MediaError::InferenceFailed {
                    engine_id: engine_id.into(),
                    reason: e.to_string(),
                })?;
        let mut rgb = vec![0u8; 3 * hw];
        for i in 0..hw.min(chw.len() / 3) {
            for c in 0..3 {
                rgb[i * 3 + c] =
                    ((chw[c * hw + i] * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
        frames.push(RgbaImageBuf::from_rgb(req.width, req.height, &rgb)?);
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
        engine_kind: EngineKind::NeuralLocalOnnx,
        data_exit: MediaDataExit::Local,
        model_id: Some(engine_id.into()),
        model_sha256,
    })
}
