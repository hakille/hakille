//! Moteur TTS neuronal générique via ONNX Runtime (`ort`) pour modèles
//! Kokoro / VITS / Piper exportés en ONNX.
//!
//! Renvoie honnêtement [`MediaError::ModelMissing`] si `model.onnx` ou
//! `vocab.json` est absent.

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
use ort::{session::Session, value::Tensor};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct OnnxNeuralTtsEngine {
    models_dir: PathBuf,
}

impl OnnxNeuralTtsEngine {
    pub fn new(models_dir: impl Into<PathBuf>) -> Self {
        Self {
            models_dir: models_dir.into(),
        }
    }

    fn profile(&self) -> GenerativeModelProfile {
        MediaModelCatalog::by_id("onnx-tts").expect("profil onnx-tts présent dans le catalogue")
    }
}

#[async_trait]
impl AudioGeneratorBackend for OnnxNeuralTtsEngine {
    fn descriptor(&self) -> EngineDescriptor {
        let profile = self.profile();
        let availability = profile.probe_availability(&self.models_dir);
        EngineDescriptor {
            id: profile.id,
            name: profile.name,
            modality: MediaModality::Audio,
            kind: EngineKind::NeuralLocalOnnx,
            data_exit: MediaDataExit::Local,
            availability,
            estimated_ram_mb: profile.expected_ram_mb,
            estimated_vram_mb: profile.expected_vram_mb,
            capabilities: vec!["speech".into(), "onnx_runtime".into()],
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
            run_onnx_tts(
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
            reason: format!("tâche ONNX TTS interrompue : {e}"),
        })?
    }
}

fn run_onnx_tts(
    model_dir: &std::path::Path,
    engine_id: &str,
    model_sha256: Option<String>,
    req: &AudioGenRequest,
    control: &GenerationControl,
    progress: Option<ProgressCallback>,
) -> MediaResult<AudioGenerationOutput> {
    let env = hakille_inference::shared_ort_env().map_err(|e| MediaError::InferenceFailed {
        engine_id: engine_id.into(),
        reason: format!("environnement ORT : {e}"),
    })?;
    control.check()?;

    let vocab_raw = std::fs::read_to_string(model_dir.join("vocab.json"))?;
    let vocab: HashMap<String, i64> = serde_json::from_str(&vocab_raw)?;
    let mut token_ids = Vec::with_capacity(req.prompt.len() + 2);
    token_ids.push(0i64);
    for ch in req.prompt.chars() {
        let key = ch.to_string();
        if let Some(&id) = vocab.get(&key) {
            token_ids.push(id);
        }
    }
    token_ids.push(0i64);

    let speed = match &req.mode {
        AudioGenMode::Speech { speed, .. } => speed.clamp(0.5, 2.0),
        _ => 1.0,
    };

    let builder = Session::builder(&env).map_err(|e| MediaError::InferenceFailed {
        engine_id: engine_id.into(),
        reason: format!("builder ORT : {e}"),
    })?;
    let mut builder = builder
        .with_intra_threads(4)
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("threads ORT : {e}"),
        })?;
    let mut session = builder
        .commit_from_file(model_dir.join("model.onnx"))
        .map_err(|e| MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: format!("session model.onnx : {e}"),
        })?;

    let seq_len = token_ids.len();
    let input_tensor = Tensor::<i64>::from_array(([1usize, seq_len], token_ids)).map_err(|e| {
        MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        }
    })?;
    let speed_tensor = Tensor::<f32>::from_array(([1usize], vec![speed])).map_err(|e| {
        MediaError::InferenceFailed {
            engine_id: engine_id.into(),
            reason: e.to_string(),
        }
    })?;

    let input_names: Vec<String> = session
        .inputs()
        .iter()
        .map(|i| i.name().to_string())
        .collect();
    let outputs = if input_names.iter().any(|n| n == "speed") {
        let first = input_names
            .first()
            .cloned()
            .unwrap_or_else(|| "tokens".to_string());
        session.run(ort::inputs![first.as_str() => input_tensor, "speed" => speed_tensor])
    } else {
        let first = input_names
            .first()
            .cloned()
            .unwrap_or_else(|| "input".to_string());
        session.run(ort::inputs![first.as_str() => input_tensor])
    }
    .map_err(|e| MediaError::InferenceFailed {
        engine_id: engine_id.into(),
        reason: format!("forward ONNX TTS : {e}"),
    })?;

    let (_, pcm_slice) =
        outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| MediaError::InferenceFailed {
                engine_id: engine_id.into(),
                reason: format!("extraction PCM ONNX : {e}"),
            })?;
    let pcm = pcm_slice.to_vec();

    if let Some(cb) = &progress {
        cb(MediaProgressEvent {
            job_id: control.job_id.clone(),
            modality: MediaModality::Audio,
            stage: "inférence_onnx_tts".into(),
            step: 1,
            total_steps: 1,
            fraction: 1.0,
            elapsed_ms: control.elapsed_ms(),
            message: "Inférence ONNX TTS terminée".into(),
            preview: None,
        });
    }

    let mut buffer = AudioBuffer::new(24_000, 1, pcm)?
        .resample(req.sample_rate)
        .to_channels(req.channels);
    buffer.apply_fades(8, 30);
    buffer.normalize_peak(0.93);

    let preview = buffer.render_waveform_preview(200, 96);
    let wav_bytes = buffer.encode_wav(req.format)?;
    let seed = derive_seed(&req.prompt, req.seed);

    Ok(AudioGenerationOutput {
        wav_bytes,
        buffer,
        preview,
        visemes_json: None,
        seed,
        engine_id: engine_id.into(),
        engine_kind: EngineKind::NeuralLocalOnnx,
        data_exit: MediaDataExit::Local,
        model_id: Some(engine_id.into()),
        model_sha256,
    })
}
