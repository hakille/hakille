//! Moteur TTS neuronal local Parler-TTS + codec DAC 100 % Rust pur via Candle.
//!
//! Utilise `candle_transformers::models::parler_tts` (T5 + décodeur
//! multi-codebooks + codec DAC). Renvoie honnêtement [`MediaError::ModelMissing`]
//! lorsque les poids locaux ne sont pas provisionnés.

use crate::audio::mastering::AudioBuffer;
use crate::audio::{AudioGenerationOutput, AudioGeneratorBackend};
use crate::catalog::{GenerativeModelProfile, MediaModelCatalog};
use crate::error::{MediaError, MediaResult};
use crate::types::{
    AudioGenMode, AudioGenRequest, EngineAvailability, EngineDescriptor, EngineKind,
    GenerationControl, MediaDataExit, MediaModality, MediaProgressEvent, ProgressCallback,
    derive_seed,
};
use async_trait::async_trait;
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::parler_tts;
use std::path::PathBuf;
use tokenizers::Tokenizer;

pub struct CandleParlerTtsEngine {
    models_dir: PathBuf,
}

impl CandleParlerTtsEngine {
    pub fn new(models_dir: impl Into<PathBuf>) -> Self {
        Self {
            models_dir: models_dir.into(),
        }
    }

    fn profile(&self) -> GenerativeModelProfile {
        MediaModelCatalog::by_id("parler-tts-candle")
            .expect("profil parler-tts-candle présent dans le catalogue")
    }
}

#[async_trait]
impl AudioGeneratorBackend for CandleParlerTtsEngine {
    fn descriptor(&self) -> EngineDescriptor {
        let profile = self.profile();
        let availability = profile.probe_availability(&self.models_dir);
        EngineDescriptor {
            id: profile.id,
            name: profile.name,
            modality: MediaModality::Audio,
            kind: EngineKind::NeuralLocalCandle,
            data_exit: MediaDataExit::Local,
            availability,
            estimated_ram_mb: profile.expected_ram_mb,
            estimated_vram_mb: profile.expected_vram_mb,
            capabilities: vec![
                "speech".into(),
                "style_description".into(),
                "dac_neural_codec".into(),
            ],
            description: profile.description,
        }
    }

    async fn generate(
        &self,
        req: &AudioGenRequest,
        control: &GenerationControl,
        progress: Option<ProgressCallback>,
    ) -> MediaResult<AudioGenerationOutput> {
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
            run_parler_tts(
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
            reason: format!("tâche Parler-TTS interrompue : {e}"),
        })?
    }
}

fn run_parler_tts(
    model_dir: &std::path::Path,
    engine_id: &str,
    model_sha256: Option<String>,
    req: &AudioGenRequest,
    control: &GenerationControl,
    progress: Option<ProgressCallback>,
) -> MediaResult<AudioGenerationOutput> {
    let device = Device::Cpu;
    let dtype = DType::F32;
    let seed = derive_seed(&req.prompt, req.seed);
    control.check()?;

    let cfg_bytes = std::fs::read(model_dir.join("config.json"))?;
    let cfg: parler_tts::Config = serde_json::from_slice(&cfg_bytes)?;
    let tokenizer = Tokenizer::from_file(model_dir.join("tokenizer.json")).map_err(|e| {
        MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("tokenizer Parler-TTS : {e}"),
        }
    })?;

    let description = match &req.mode {
        AudioGenMode::Speech {
            style_description: Some(desc),
            ..
        } => desc.clone(),
        AudioGenMode::Speech { voice, emotion, .. } => format!(
            "A clear studio voice ({voice}) speaking with a {emotion} tone, high audio quality."
        ),
        _ => "A calm, clear studio voice with high audio quality.".to_string(),
    };

    let desc_ids = tokenizer
        .encode(description.as_str(), true)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        })?
        .get_ids()
        .to_vec();
    let prompt_ids = tokenizer
        .encode(req.prompt.as_str(), true)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        })?
        .get_ids()
        .to_vec();

    let desc_tensor = Tensor::new(desc_ids.as_slice(), &device)
        .and_then(|t| t.unsqueeze(0))
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        })?;
    let prompt_tensor = Tensor::new(prompt_ids.as_slice(), &device)
        .and_then(|t| t.unsqueeze(0))
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        })?;

    let vb = unsafe {
        VarBuilder::from_mmaped_safetensors(&[model_dir.join("model.safetensors")], dtype, &device)
    }
    .map_err(|e| MediaError::InferenceFailed {
        engine_id: engine_id.into(),
        reason: format!("chargement Parler-TTS safetensors : {e}"),
    })?;
    let mut model = parler_tts::Model::new(&cfg, vb).map_err(|e| MediaError::InferenceFailed {
        engine_id: engine_id.into(),
        reason: format!("construction Parler-TTS : {e}"),
    })?;

    if let Some(cb) = &progress {
        cb(MediaProgressEvent {
            job_id: control.job_id.clone(),
            modality: MediaModality::Audio,
            stage: "décodage_parler_tts".into(),
            step: 1,
            total_steps: 2,
            fraction: 0.4,
            elapsed_ms: control.elapsed_ms(),
            message: "Génération des codes acoustiques DAC (Parler-TTS)".into(),
            preview: None,
        });
    }

    let max_steps = ((req.duration_secs * 86.0) as usize).clamp(32, 2048);
    let lp = LogitsProcessor::new(seed, Some(0.8), Some(0.95));
    let codes = model
        .generate(&prompt_tensor, &desc_tensor, lp, max_steps)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("génération Parler-TTS : {e}"),
        })?;
    control.check()?;

    let codes = codes
        .unsqueeze(0)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        })?;
    let pcm = model
        .audio_encoder
        .decode_codes(&codes)
        .and_then(|t| t.flatten_all())
        .and_then(|t| t.to_vec1::<f32>())
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("décodage codec DAC : {e}"),
        })?;

    let native_sr = cfg.audio_encoder.sampling_rate as u32;
    let mut buffer = AudioBuffer::new(native_sr.max(16_000), 1, pcm)?
        .resample(req.sample_rate)
        .to_channels(req.channels);
    buffer.apply_fades(10, 40);
    buffer.normalize_peak(0.93);

    if let Some(cb) = &progress {
        cb(MediaProgressEvent {
            job_id: control.job_id.clone(),
            modality: MediaModality::Audio,
            stage: "décodage_parler_tts".into(),
            step: 2,
            total_steps: 2,
            fraction: 1.0,
            elapsed_ms: control.elapsed_ms(),
            message: "Audio neural Parler-TTS encodé".into(),
            preview: None,
        });
    }

    let preview = buffer.render_waveform_preview(200, 96);
    let wav_bytes = buffer.encode_wav(req.format)?;

    Ok(AudioGenerationOutput {
        wav_bytes,
        buffer,
        preview,
        visemes_json: None,
        seed,
        engine_id: engine_id.into(),
        engine_kind: EngineKind::NeuralLocalCandle,
        data_exit: MediaDataExit::Local,
        model_id: Some(engine_id.into()),
        model_sha256,
    })
}
