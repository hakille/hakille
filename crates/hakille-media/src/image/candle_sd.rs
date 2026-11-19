//! Moteur neuronal local Stable Diffusion 1.5 / Turbo 100 % Rust pur via Candle.
//!
//! Utilise directement `candle_transformers::models::stable_diffusion` :
//! - `build_clip_transformer` (CLIP ViT-L/14) ;
//! - `StableDiffusionConfig::v1_5` / `v2_1` / `sdxl_turbo` ;
//! - `UNet2DConditionModel`, `AutoEncoderKL`, ordonnanceurs DDIM / Euler-A / UniPC ;
//! - vérification d'annulation et d'échéance dure à chaque pas de débruitage ;
//! - si un fichier safetensors ou tokenizer manque, renvoie honnêtement
//!   [`MediaError::ModelMissing`] sans jamais simuler de résultat.

use crate::catalog::{GenerativeModelProfile, MediaModelCatalog};
use crate::error::{MediaError, MediaResult};
use crate::image::encode::{RgbaImageBuf, load_rgba_image, resize_rgba_bilinear};
use crate::image::{ImageGenerationOutput, ImageGeneratorBackend};
use crate::types::{
    DiffusionScheduler, EngineAvailability, EngineDescriptor, EngineKind, GenerationControl,
    ImageGenMode, ImageGenRequest, MediaDataExit, MediaModality, MediaProgressEvent,
    ProgressCallback, derive_seed,
};
use async_trait::async_trait;
use candle_core::{DType, Device, Module, Tensor};
use candle_transformers::models::stable_diffusion::{
    StableDiffusionConfig, build_clip_transformer, ddim::DDIMSchedulerConfig,
    euler_ancestral_discrete::EulerAncestralDiscreteSchedulerConfig, schedulers::SchedulerConfig,
    uni_pc::UniPCSchedulerConfig,
};
use std::path::PathBuf;
use tokenizers::Tokenizer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdVariant {
    V1_5,
    Turbo,
}

pub struct CandleStableDiffusionEngine {
    models_dir: PathBuf,
    variant: SdVariant,
}

impl CandleStableDiffusionEngine {
    pub fn new(models_dir: impl Into<PathBuf>, variant: SdVariant) -> Self {
        Self {
            models_dir: models_dir.into(),
            variant,
        }
    }

    fn profile(&self) -> GenerativeModelProfile {
        let id = match self.variant {
            SdVariant::V1_5 => "sd-1.5-candle",
            SdVariant::Turbo => "sd-turbo-candle",
        };
        MediaModelCatalog::by_id(id).expect("profil Stable Diffusion présent dans le catalogue")
    }
}

#[async_trait]
impl ImageGeneratorBackend for CandleStableDiffusionEngine {
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
                "image_to_image".into(),
                "ddim".into(),
                "euler_ancestral".into(),
                "uni_pc".into(),
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
        let availability = profile.probe_availability(&self.models_dir);
        let model_sha256 = match availability {
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
        let variant = self.variant;
        let engine_id = profile.id.clone();

        tokio::task::spawn_blocking(move || {
            run_sd_inference(
                &model_dir,
                &engine_id,
                variant,
                model_sha256,
                &req,
                &control,
                progress,
            )
        })
        .await
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: profile.id,
            reason: format!("tâche Stable Diffusion interrompue : {e}"),
        })?
    }
}

fn run_sd_inference(
    model_dir: &std::path::Path,
    engine_id: &str,
    variant: SdVariant,
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
            reason: format!("initialisation graine Candle : {e}"),
        })?;

    control.check()?;

    let sd_config = match variant {
        SdVariant::V1_5 => StableDiffusionConfig::v1_5(
            Some(1),
            Some(req.height as usize),
            Some(req.width as usize),
        ),
        SdVariant::Turbo => StableDiffusionConfig::v2_1(
            Some(1),
            Some(req.height as usize),
            Some(req.width as usize),
        ),
    };

    let tokenizer = Tokenizer::from_file(model_dir.join("tokenizer.json")).map_err(|e| {
        MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("chargement tokenizer CLIP : {e}"),
        }
    })?;

    let pad_id = match &sd_config.clip.pad_with {
        Some(padding) => *tokenizer
            .get_vocab(true)
            .get(padding.as_str())
            .unwrap_or(&0),
        None => *tokenizer.get_vocab(true).get("<|endoftext|>").unwrap_or(&0),
    };

    let encode_tokens = |text: &str| -> MediaResult<Tensor> {
        let mut tokens = tokenizer
            .encode(text, true)
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: format!("tokenisation CLIP : {e}"),
            })?
            .get_ids()
            .to_vec();
        let max_len = sd_config.clip.max_position_embeddings;
        if tokens.len() > max_len {
            tokens.truncate(max_len);
        }
        while tokens.len() < max_len {
            tokens.push(pad_id);
        }
        Tensor::new(tokens.as_slice(), &device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: format!("tenseur tokens CLIP : {e}"),
            })
    };

    // 1. Encodage texte CLIP.
    let clip_model = build_clip_transformer(
        &sd_config.clip,
        model_dir.join("clip.safetensors"),
        &device,
        dtype,
    )
    .map_err(|e| MediaError::InferenceFailed {
        engine_id: engine_id.into(),
        reason: format!("chargement CLIP safetensors : {e}"),
    })?;

    let prompt_ids = encode_tokens(&req.prompt)?;
    let text_embeddings =
        clip_model
            .forward(&prompt_ids)
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: format!("forward CLIP prompt : {e}"),
            })?;

    let use_cfg = req.guidance_scale > 1.0 && variant != SdVariant::Turbo;
    let text_embeddings = if use_cfg {
        let neg_ids = encode_tokens(req.negative_prompt.as_deref().unwrap_or(""))?;
        let uncond = clip_model
            .forward(&neg_ids)
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: format!("forward CLIP négatif : {e}"),
            })?;
        Tensor::cat(&[uncond, text_embeddings], 0).map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("concat CFG : {e}"),
        })?
    } else {
        text_embeddings
    };
    drop(clip_model);

    control.check()?;

    // 2. Ordonnanceur de débruitage choisi.
    let n_steps = req.steps as usize;
    let mut scheduler = match req.scheduler {
        DiffusionScheduler::Ddim | DiffusionScheduler::RectifiedFlow => {
            DDIMSchedulerConfig::default().build(n_steps)
        }
        DiffusionScheduler::EulerAncestral => {
            EulerAncestralDiscreteSchedulerConfig::default().build(n_steps)
        }
        DiffusionScheduler::UniPc => UniPCSchedulerConfig::default().build(n_steps),
    }
    .map_err(|e| MediaError::InferenceFailed {
        engine_id: engine_id.into(),
        reason: format!("construction ordonnanceur diffusion : {e}"),
    })?;

    // 3. Initialisation latente (T2I ou I2I via VAE encode).
    let vae = sd_config
        .build_vae(model_dir.join("vae.safetensors"), &device, dtype)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("chargement VAE safetensors : {e}"),
        })?;

    let timesteps = scheduler.timesteps().to_vec();
    let (mut latents, start_step) = match &req.mode {
        ImageGenMode::TextToImage => {
            let noise = Tensor::randn(
                0f32,
                1f32,
                (1, 4, sd_config.height / 8, sd_config.width / 8),
                &device,
            )
            .and_then(|t| t * scheduler.init_noise_sigma())
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: format!("bruit latent initial : {e}"),
            })?;
            (noise, 0usize)
        }
        ImageGenMode::ImageToImage {
            init_image_path,
            strength,
        }
        | ImageGenMode::Inpaint {
            init_image_path,
            strength,
            ..
        } => {
            let loaded = load_rgba_image(init_image_path)?;
            let resized = resize_rgba_bilinear(
                &loaded.pixels,
                loaded.width,
                loaded.height,
                req.width,
                req.height,
            );
            let rgb_norm: Vec<f32> = resized
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|px| {
                    [
                        (px[0] as f32 / 127.5) - 1.0,
                        (px[1] as f32 / 127.5) - 1.0,
                        (px[2] as f32 / 127.5) - 1.0,
                    ]
                })
                .collect();
            let img_tensor = Tensor::from_vec(
                rgb_norm,
                (1, req.height as usize, req.width as usize, 3),
                &device,
            )
            .and_then(|t| t.permute((0, 3, 1, 2)))
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: format!("préparation tenseur I2I : {e}"),
            })?;
            let dist = vae
                .encode(&img_tensor)
                .map_err(|e| MediaError::InferenceFailed {
                    engine_id: engine_id.into(),
                    reason: format!("encodage VAE I2I : {e}"),
                })?;
            let init_latents = (dist.sample().map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: format!("échantillonnage VAE I2I : {e}"),
            })? * 0.18215)
                .map_err(|e| MediaError::InferenceFailed {
                    engine_id: engine_id.into(),
                    reason: format!("mise à l'échelle latente : {e}"),
                })?;
            let t_start = ((timesteps.len() as f32) * (1.0 - strength.clamp(0.05, 1.0))) as usize;
            let t_start = t_start.min(timesteps.len().saturating_sub(1));
            let noise =
                init_latents
                    .randn_like(0.0, 1.0)
                    .map_err(|e| MediaError::InferenceFailed {
                        engine_id: engine_id.into(),
                        reason: format!("bruit I2I : {e}"),
                    })?;
            let noisy = scheduler
                .add_noise(&init_latents, noise, timesteps[t_start])
                .map_err(|e| MediaError::InferenceFailed {
                    engine_id: engine_id.into(),
                    reason: format!("injection bruit I2I : {e}"),
                })?;
            (noisy, t_start)
        }
    };

    // 4. Boucle UNet de débruitage avec annulation coopérative à chaque pas.
    let unet = sd_config
        .build_unet(model_dir.join("unet.safetensors"), &device, 4, false, dtype)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("chargement UNet safetensors : {e}"),
        })?;

    let active_steps = &timesteps[start_step..];
    let total_active = active_steps.len().max(1) as u32;
    for (idx, &timestep) in active_steps.iter().enumerate() {
        control.check()?;
        let latent_input = if use_cfg {
            Tensor::cat(&[&latents, &latents], 0)
        } else {
            Ok(latents.clone())
        }
        .and_then(|t| scheduler.scale_model_input(t, timestep))
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("entrée UNet étape {idx} : {e}"),
        })?;

        let noise_pred = unet
            .forward(&latent_input, timestep as f64, &text_embeddings)
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: format!("forward UNet étape {idx} : {e}"),
            })?;

        let noise_pred = if use_cfg {
            let chunks = noise_pred
                .chunk(2, 0)
                .map_err(|e| MediaError::InferenceFailed {
                    engine_id: engine_id.into(),
                    reason: format!("chunk CFG : {e}"),
                })?;
            let (uncond, cond) = (&chunks[0], &chunks[1]);
            (uncond
                + ((cond - uncond).map_err(|e| MediaError::InferenceFailed {
                    engine_id: engine_id.into(),
                    reason: e.to_string(),
                })? * f64::from(req.guidance_scale))
                .map_err(|e| MediaError::InferenceFailed {
                    engine_id: engine_id.into(),
                    reason: e.to_string(),
                })?)
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            })?
        } else {
            noise_pred
        };

        latents = scheduler
            .step(&noise_pred, timestep, &latents)
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: format!("pas ordonnanceur {idx} : {e}"),
            })?;

        if let Some(cb) = &progress {
            let step = (idx + 1) as u32;
            cb(MediaProgressEvent {
                job_id: control.job_id.clone(),
                modality: MediaModality::Image,
                stage: "débruitage_unet".into(),
                step,
                total_steps: total_active,
                fraction: step as f32 / total_active as f32,
                elapsed_ms: control.elapsed_ms(),
                message: format!("Stable Diffusion pas {step}/{total_active}"),
                preview: None,
            });
        }
    }
    drop(unet);

    control.check()?;

    // 5. Décodage VAE → RGB8 → RGBA8 → PNG/JPEG.
    let decoded = vae
        .decode(
            &(latents / 0.18215).map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            })?,
        )
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("décodage VAE : {e}"),
        })?;
    let image_u8 = ((decoded / 2.0)
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
        reason: format!("conversion tenseur image : {e}"),
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
