//! Moteur neuronal local FLUX.1 Schnell 100 % Rust pur via Candle (`flux`).
//!
//! Utilise `candle_transformers::models::flux::{model, autoencoder, sampling}`
//! avec boucle Rectified Flow contrôlée pas à pas (annulation coopérative +
//! échéance dure). Renvoie honnêtement [`MediaError::ModelMissing`] lorsque les
//! poids safetensors locaux ne sont pas provisionnés.

use crate::catalog::{GenerativeModelProfile, MediaModelCatalog};
use crate::error::{MediaError, MediaResult};
use crate::image::encode::RgbaImageBuf;
use crate::image::{ImageGenerationOutput, ImageGeneratorBackend};
use crate::types::{
    EngineAvailability, EngineDescriptor, EngineKind, GenerationControl, ImageGenRequest,
    MediaDataExit, MediaModality, MediaProgressEvent, ProgressCallback, derive_seed,
};
use async_trait::async_trait;
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::flux::{WithForward, autoencoder, model as flux_model, sampling};
use candle_transformers::models::{stable_diffusion::clip, t5};
use std::path::PathBuf;
use tokenizers::Tokenizer;

pub struct CandleFluxEngine {
    models_dir: PathBuf,
}

impl CandleFluxEngine {
    pub fn new(models_dir: impl Into<PathBuf>) -> Self {
        Self {
            models_dir: models_dir.into(),
        }
    }

    fn profile(&self) -> GenerativeModelProfile {
        MediaModelCatalog::by_id("flux-schnell-candle")
            .expect("profil FLUX.1 Schnell présent dans le catalogue")
    }
}

#[async_trait]
impl ImageGeneratorBackend for CandleFluxEngine {
    fn descriptor(&self) -> EngineDescriptor {
        let profile = self.profile();
        let availability = profile.probe_availability(&self.models_dir);
        EngineDescriptor {
            id: profile.id,
            name: profile.name,
            modality: MediaModality::Image,
            kind: EngineKind::NeuralLocalCandle,
            data_exit: MediaDataExit::Local,
            availability,
            estimated_ram_mb: profile.expected_ram_mb,
            estimated_vram_mb: profile.expected_vram_mb,
            capabilities: vec![
                "text_to_image".into(),
                "rectified_flow".into(),
                "deterministic_seed".into(),
            ],
            description: profile.description,
        }
    }

    async fn generate(
        &self,
        req: &ImageGenRequest,
        control: &GenerationControl,
        progress: Option<ProgressCallback>,
    ) -> MediaResult<ImageGenerationOutput> {
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
        let control = control.clone();
        let engine_id = profile.id.clone();

        tokio::task::spawn_blocking(move || {
            run_flux_inference(
                &model_dir,
                &engine_id,
                model_sha256,
                &req,
                &control,
                progress,
            )
        })
        .await
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: profile.id,
            reason: format!("tâche FLUX interrompue : {e}"),
        })?
    }
}

fn run_flux_inference(
    model_dir: &std::path::Path,
    engine_id: &str,
    model_sha256: Option<String>,
    req: &ImageGenRequest,
    control: &GenerationControl,
    progress: Option<ProgressCallback>,
) -> MediaResult<ImageGenerationOutput> {
    let device = Device::Cpu;
    let dtype = DType::F32;
    let seed = derive_seed(&req.prompt, req.seed);
    device
        .set_seed(seed)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("graine FLUX : {e}"),
        })?;
    control.check()?;

    // 1. Encodage T5-XXL (256 tokens pour FLUX.1 Schnell).
    let t5_tok = Tokenizer::from_file(model_dir.join("t5_tokenizer.json")).map_err(|e| {
        MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("tokenizer T5 : {e}"),
        }
    })?;
    let mut t5_ids = t5_tok
        .encode(req.prompt.as_str(), true)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("encodage T5 : {e}"),
        })?
        .get_ids()
        .to_vec();
    t5_ids.resize(256, 0);
    let t5_input = Tensor::new(t5_ids.as_slice(), &device)
        .and_then(|t| t.unsqueeze(0))
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        })?;

    let t5_cfg_path = model_dir.join("t5_config.json");
    let t5_cfg: t5::Config = if t5_cfg_path.is_file() {
        let raw = std::fs::read(&t5_cfg_path)?;
        serde_json::from_slice(&raw)?
    } else {
        return Err(MediaError::ModelMissing {
            model_id: engine_id.into(),
            missing: vec!["t5_config.json".into()],
            hint: format!("fournir t5_config.json dans {}", model_dir.display()),
        });
    };
    let vb_t5 = unsafe {
        VarBuilder::from_mmaped_safetensors(&[model_dir.join("t5.safetensors")], dtype, &device)
    }
    .map_err(|e| MediaError::InferenceFailed {
        engine_id: engine_id.into(),
        reason: format!("chargement T5 : {e}"),
    })?;
    let mut t5_model =
        t5::T5EncoderModel::load(vb_t5, &t5_cfg).map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("construction T5 : {e}"),
        })?;
    let t5_emb = t5_model
        .forward(&t5_input)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("forward T5 : {e}"),
        })?;
    drop(t5_model);
    control.check()?;

    // 2. Encodage CLIP pooled (77 tokens).
    let clip_tok = Tokenizer::from_file(model_dir.join("clip_tokenizer.json")).map_err(|e| {
        MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("tokenizer CLIP : {e}"),
        }
    })?;
    let mut clip_ids = clip_tok
        .encode(req.prompt.as_str(), true)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("encodage CLIP : {e}"),
        })?
        .get_ids()
        .to_vec();
    clip_ids.resize(77, 0);
    let clip_input = Tensor::new(clip_ids.as_slice(), &device)
        .and_then(|t| t.unsqueeze(0))
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        })?;
    let clip_cfg = clip::Config::v1_5();
    let clip_model = candle_transformers::models::stable_diffusion::build_clip_transformer(
        &clip_cfg,
        model_dir.join("clip.safetensors"),
        &device,
        dtype,
    )
    .map_err(|e| MediaError::InferenceFailed {
        engine_id: engine_id.into(),
        reason: format!("chargement CLIP : {e}"),
    })?;
    let clip_emb =
        clip_model
            .forward_with_mask(&clip_input, 76)
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: format!("forward CLIP : {e}"),
            })?;
    drop(clip_model);
    control.check()?;

    // 3. Initialisation Rectified Flow + boucle pas à pas annulable.
    let img_noise = sampling::get_noise(1, req.height as usize, req.width as usize, &device)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("bruit FLUX : {e}"),
        })?;
    let state = sampling::State::new(&t5_emb, &clip_emb, &img_noise).map_err(|e| {
        MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("état FLUX : {e}"),
        }
    })?;

    let vb_flux = unsafe {
        VarBuilder::from_mmaped_safetensors(&[model_dir.join("flux.safetensors")], dtype, &device)
    }
    .map_err(|e| MediaError::InferenceFailed {
        engine_id: engine_id.into(),
        reason: format!("chargement FLUX safetensors : {e}"),
    })?;
    let model = flux_model::Flux::new(&flux_model::Config::schnell(), vb_flux).map_err(|e| {
        MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("construction FLUX : {e}"),
        }
    })?;

    let steps = req.steps.clamp(1, 16) as usize;
    let schedule = sampling::get_schedule(steps, None);
    let mut img = state.img.clone();
    let total_steps = schedule.len().saturating_sub(1).max(1) as u32;
    for (idx, window) in schedule.windows(2).enumerate() {
        control.check()?;
        let (t_curr, t_prev) = (window[0], window[1]);
        let t_vec = Tensor::full(t_curr as f32, 1usize, &device).map_err(|e| {
            MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            }
        })?;
        let pred = model
            .forward(
                &img,
                &state.img_ids,
                &state.txt,
                &state.txt_ids,
                &t_vec,
                &state.vec,
                None,
            )
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: format!("forward FLUX pas {idx} : {e}"),
            })?;
        img = (img
            + (pred * (t_prev - t_curr)).map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            })?)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        })?;

        if let Some(cb) = &progress {
            let step = (idx + 1) as u32;
            cb(MediaProgressEvent {
                job_id: control.job_id.clone(),
                modality: MediaModality::Image,
                stage: "rectified_flow_flux".into(),
                step,
                total_steps,
                fraction: step as f32 / total_steps as f32,
                elapsed_ms: control.elapsed_ms(),
                message: format!("FLUX.1 Schnell pas {step}/{total_steps}"),
                preview: None,
            });
        }
    }
    drop(model);
    control.check()?;

    // 4. Décodage AutoEncoder FLUX.
    let unpacked =
        sampling::unpack(&img, req.height as usize, req.width as usize).map_err(|e| {
            MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: format!("unpack FLUX : {e}"),
            }
        })?;
    let vb_ae = unsafe {
        VarBuilder::from_mmaped_safetensors(&[model_dir.join("ae.safetensors")], dtype, &device)
    }
    .map_err(|e| MediaError::InferenceFailed {
        engine_id: engine_id.into(),
        reason: format!("chargement AutoEncoder FLUX : {e}"),
    })?;
    let ae =
        autoencoder::AutoEncoder::new(&autoencoder::Config::schnell(), vb_ae).map_err(|e| {
            MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: format!("construction AutoEncoder FLUX : {e}"),
            }
        })?;
    let decoded = ae
        .decode(&unpacked)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("décodage AutoEncoder FLUX : {e}"),
        })?;
    let image_u8 = ((decoded.clamp(-1f32, 1f32))
        .and_then(|t| t + 1.0)
        .and_then(|t| t * 127.5)
        .and_then(|t| t.to_dtype(DType::U8))
        .and_then(|t| t.squeeze(0))
        .and_then(|t| t.permute((1, 2, 0)))
        .and_then(|t| t.flatten_all())
        .and_then(|t| t.to_vec1::<u8>()))
    .map_err(|e| MediaError::InferenceFailed {
        engine_id: engine_id.into(),
        reason: format!("conversion pixels FLUX : {e}"),
    })?;

    let rgba = RgbaImageBuf::from_rgb(req.width, req.height, &image_u8)?;
    let preview = rgba.make_preview(200, 120);
    let encoded_bytes = rgba.encode(req.format)?;

    Ok(ImageGenerationOutput {
        encoded_bytes,
        rgba,
        preview,
        seed,
        engine_id: engine_id.into(),
        engine_kind: EngineKind::NeuralLocalCandle,
        data_exit: MediaDataExit::Local,
        model_id: Some(engine_id.into()),
        model_sha256,
    })
}
