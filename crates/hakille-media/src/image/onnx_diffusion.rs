//! Moteur neuronal générique de diffusion latente via ONNX Runtime (`ort`).
//!
//! Exécute un pipeline ONNX exporté (`text_encoder.onnx` + `unet.onnx` +
//! `vae_decoder.onnx` + `tokenizer.json`) avec contrôle d'annulation à chaque pas.
//! Renvoie honnêtement [`MediaError::ModelMissing`] en l'absence des artefacts.

use crate::catalog::{GenerativeModelProfile, MediaModelCatalog};
use crate::error::{MediaError, MediaResult};
use crate::image::encode::RgbaImageBuf;
use crate::image::{ImageGenerationOutput, ImageGeneratorBackend};
use crate::types::{
    EngineAvailability, EngineDescriptor, EngineKind, GenerationControl, ImageGenRequest,
    MediaDataExit, MediaModality, MediaProgressEvent, ProgressCallback, derive_seed,
};
use async_trait::async_trait;
use ort::{session::Session, value::Tensor};
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};
use std::path::PathBuf;
use tokenizers::Tokenizer;

fn open_ort_session(
    env: &std::sync::Arc<ort::environment::Environment>,
    path: &std::path::Path,
    engine_id: &str,
) -> MediaResult<Session> {
    let builder = Session::builder(env).map_err(|e| MediaError::InferenceFailed {
        engine_id: engine_id.into(),
        reason: format!("builder ORT : {e}"),
    })?;
    let mut builder = builder
        .with_intra_threads(4)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("threads ORT : {e}"),
        })?;
    builder
        .commit_from_file(path)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("session {} : {e}", path.display()),
        })
}

pub struct OnnxImageDiffusionEngine {
    models_dir: PathBuf,
}

impl OnnxImageDiffusionEngine {
    pub fn new(models_dir: impl Into<PathBuf>) -> Self {
        Self {
            models_dir: models_dir.into(),
        }
    }

    fn profile(&self) -> GenerativeModelProfile {
        MediaModelCatalog::by_id("onnx-diffusion")
            .expect("profil onnx-diffusion présent dans le catalogue")
    }
}

#[async_trait]
impl ImageGeneratorBackend for OnnxImageDiffusionEngine {
    fn descriptor(&self) -> EngineDescriptor {
        let profile = self.profile();
        let availability = profile.probe_availability(&self.models_dir);
        EngineDescriptor {
            id: profile.id,
            name: profile.name,
            modality: MediaModality::Image,
            kind: EngineKind::NeuralLocalOnnx,
            data_exit: MediaDataExit::Local,
            availability,
            estimated_ram_mb: profile.expected_ram_mb,
            estimated_vram_mb: profile.expected_vram_mb,
            capabilities: vec!["text_to_image".into(), "onnx_runtime".into()],
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
            run_onnx_diffusion(
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
            reason: format!("tâche ONNX diffusion interrompue : {e}"),
        })?
    }
}

fn run_onnx_diffusion(
    model_dir: &std::path::Path,
    engine_id: &str,
    model_sha256: Option<String>,
    req: &ImageGenRequest,
    control: &GenerationControl,
    progress: Option<ProgressCallback>,
) -> MediaResult<ImageGenerationOutput> {
    let env = hakille_inference::shared_ort_env().map_err(|e| MediaError::InferenceFailed {
        engine_id: engine_id.into(),
        reason: format!("environnement ORT : {e}"),
    })?;
    let seed = derive_seed(&req.prompt, req.seed);
    control.check()?;

    let tokenizer = Tokenizer::from_file(model_dir.join("tokenizer.json")).map_err(|e| {
        MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("tokenizer ONNX : {e}"),
        }
    })?;
    let mut ids: Vec<i64> = tokenizer
        .encode(req.prompt.as_str(), true)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("encodage tokenizer ONNX : {e}"),
        })?
        .get_ids()
        .iter()
        .map(|&v| i64::from(v))
        .collect();
    ids.resize(77, 0);

    // 1. Encodeur texte ONNX.
    let mut text_enc = open_ort_session(&env, &model_dir.join("text_encoder.onnx"), engine_id)?;
    let ids_tensor = Tensor::<i64>::from_array(([1usize, 77], ids)).map_err(|e| {
        MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        }
    })?;
    let text_out = text_enc
        .run(ort::inputs!["input_ids" => ids_tensor])
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("forward text_encoder.onnx : {e}"),
        })?;
    let (emb_shape, emb_vals) =
        text_out[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: format!("extraction embedding texte : {e}"),
            })?;
    let emb_dims: Vec<usize> = emb_shape.iter().map(|&d| d as usize).collect();
    let emb_vec = emb_vals.to_vec();
    drop(text_out);
    drop(text_enc);

    // 2. Débruitage UNet ONNX (schéma Euler simple sur grille linéaire de sigmas).
    let mut unet = open_ort_session(&env, &model_dir.join("unet.onnx"), engine_id)?;

    let lh = (req.height / 8) as usize;
    let lw = (req.width / 8) as usize;
    let latent_len = 4 * lh * lw;
    let mut rng = StdRng::seed_from_u64(seed);
    let mut latents: Vec<f32> = (0..latent_len)
        .map(|_| sample_standard_normal(&mut rng))
        .collect();

    let steps = req.steps.max(1);
    for step_idx in 0..steps {
        control.check()?;
        let t_val = ((steps - step_idx) as f32 / steps as f32) * 999.0;
        let sample_tensor = Tensor::<f32>::from_array(([1usize, 4, lh, lw], latents.clone()))
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            })?;
        let t_tensor = Tensor::<f32>::from_array(([1usize], vec![t_val])).map_err(|e| {
            MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            }
        })?;
        let emb_tensor = Tensor::<f32>::from_array((emb_dims.as_slice(), emb_vec.clone()))
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            })?;

        let out = unet
            .run(ort::inputs![
                "sample" => sample_tensor,
                "timestep" => t_tensor,
                "encoder_hidden_states" => emb_tensor,
            ])
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: format!("forward unet.onnx pas {step_idx} : {e}"),
            })?;
        let (_, noise_pred) =
            out[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| MediaError::InferenceFailed {
                    engine_id: engine_id.into(),
                    reason: e.to_string(),
                })?;
        let dt = 1.0 / steps as f32;
        for (l, &p) in latents.iter_mut().zip(noise_pred.iter()) {
            *l -= p * dt;
        }

        if let Some(cb) = &progress {
            let step = step_idx + 1;
            cb(MediaProgressEvent {
                job_id: control.job_id.clone(),
                modality: MediaModality::Image,
                stage: "débruitage_onnx".into(),
                step,
                total_steps: steps,
                fraction: step as f32 / steps as f32,
                elapsed_ms: control.elapsed_ms(),
                message: format!("ONNX Diffusion pas {step}/{steps}"),
                preview: None,
            });
        }
    }
    drop(unet);
    control.check()?;

    // 3. Décodeur VAE ONNX.
    let mut vae = open_ort_session(&env, &model_dir.join("vae_decoder.onnx"), engine_id)?;
    let scaled: Vec<f32> = latents.iter().map(|&v| v / 0.18215).collect();
    let vae_in = Tensor::<f32>::from_array(([1usize, 4, lh, lw], scaled)).map_err(|e| {
        MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        }
    })?;
    let vae_out = vae
        .run(ort::inputs!["latent_sample" => vae_in])
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("forward vae_decoder.onnx : {e}"),
        })?;
    let (_, chw) =
        vae_out[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: e.to_string(),
            })?;
    let hw = (req.width as usize) * (req.height as usize);
    if chw.len() < 3 * hw {
        return Err(MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("sortie VAE ONNX trop courte : {} < {}", chw.len(), 3 * hw),
        });
    }
    let mut rgb = vec![0u8; 3 * hw];
    for i in 0..hw {
        for c in 0..3 {
            let v = ((chw[c * hw + i] * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8;
            rgb[i * 3 + c] = v;
        }
    }

    let rgba = RgbaImageBuf::from_rgb(req.width, req.height, &rgb)?;
    let preview = rgba.make_preview(200, 120);
    let encoded_bytes = rgba.encode(req.format)?;

    Ok(ImageGenerationOutput {
        encoded_bytes,
        rgba,
        preview,
        seed,
        engine_id: engine_id.into(),
        engine_kind: EngineKind::NeuralLocalOnnx,
        data_exit: MediaDataExit::Local,
        model_id: Some(engine_id.into()),
        model_sha256,
    })
}

fn sample_standard_normal(rng: &mut StdRng) -> f32 {
    let u1 = ((rng.next_u32() as f32) / (u32::MAX as f32)).max(1e-7);
    let u2 = (rng.next_u32() as f32) / (u32::MAX as f32);
    (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
}
