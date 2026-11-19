//! Gestionnaire de jobs multimodaux (`MediaJobManager`) :
//! - inventaire honnête de tous les moteurs (`list_engines`) ;
//! - routage strict sans dégradation silencieuse d'un moteur neuronal explicite ;
//! - concurrence bornée par `tokio::sync::Semaphore` ;
//! - registre d'annulation coopérative (`cancel_job`) + diffusion temps réel
//!   des événements [`MediaProgressEvent`] via `tokio::sync::broadcast` ;
//! - exécution complète des requêtes unitaires (Image, Audio, Vidéo) et des
//!   graphes [`MediaPipelineDag`] avec signature Ed25519 dans [`MediaStore`].

use crate::audio::AudioGeneratorBackend;
use crate::audio::candle_parler::CandleParlerTtsEngine;
use crate::audio::formant_adapter::FormantSpeechEngine;
use crate::audio::mastering::{mix_with_sidechain_ducking, read_wav_file};
use crate::audio::onnx_tts::OnnxNeuralTtsEngine;
use crate::audio::procedural_synth::ProceduralAudioSynthEngine;
use crate::backends::cloud::CloudMediaBackend;
use crate::backends::loopback::LoopbackMediaServerBackend;
use crate::dag::{DagExecutionResult, DagNodeKind, MediaPipelineDag, implicit_dependencies};
use crate::error::{MediaError, MediaResult};
use crate::guard::{MediaAuthContext, MediaConstitutionGuard};
use crate::image::ImageGeneratorBackend;
use crate::image::candle_flux::CandleFluxEngine;
use crate::image::candle_sd::{CandleStableDiffusionEngine, SdVariant};
use crate::image::onnx_diffusion::OnnxImageDiffusionEngine;
use crate::image::procedural::ProceduralCanvasEngine;
use crate::policy::MediaPolicy;
use crate::store::{MediaStore, StoreArtifactParams};
use crate::types::{
    AudioFormat, AudioGenMode, AudioGenRequest, EngineAvailability, EngineDescriptor, EngineKind,
    GenerationControl, ImageGenRequest, MediaArtifact, MediaDataExit, MediaModality,
    MediaProgressEvent, MediaRequest, ProgressCallback, VideoGenMode, VideoGenRequest,
};
use crate::video::VideoGeneratorBackend;
use crate::video::candle_keyframe::CandleKeyframeVideoEngine;
use crate::video::muxer::FfmpegMuxer;
use crate::video::onnx_video::OnnxVideoDiffusionEngine;
use crate::video::procedural_video::ProceduralMotionVideoEngine;
use ed25519_dalek::{SigningKey, VerifyingKey};
use hakille_core::{CapabilityAuthority, CapabilityToken};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::{Semaphore, broadcast};
use uuid::Uuid;

/// Configuration d'initialisation du [`MediaJobManager`].
#[derive(Debug, Clone)]
pub struct MediaManagerConfig {
    pub store_dir: PathBuf,
    pub models_dir: PathBuf,
    pub policy: MediaPolicy,
}

impl MediaManagerConfig {
    pub fn new(store_dir: impl Into<PathBuf>, models_dir: impl Into<PathBuf>) -> Self {
        Self {
            store_dir: store_dir.into(),
            models_dir: models_dir.into(),
            policy: MediaPolicy::default(),
        }
    }
}

/// Orchestrateur souverain de tous les jobs Image, Vidéo, Audio et DAG.
pub struct MediaJobManager {
    store: MediaStore,
    guard: MediaConstitutionGuard,
    signing_key: SigningKey,
    semaphore: Arc<Semaphore>,
    progress_tx: broadcast::Sender<MediaProgressEvent>,
    active_cancels: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    image_backends: Vec<Arc<dyn ImageGeneratorBackend>>,
    audio_backends: Vec<Arc<dyn AudioGeneratorBackend>>,
    video_backends: Vec<Arc<dyn VideoGeneratorBackend>>,
}

impl MediaJobManager {
    pub fn new(config: MediaManagerConfig, signing_key: SigningKey) -> MediaResult<Self> {
        let store = MediaStore::new(&config.store_dir, config.policy.max_store_bytes)?;
        let max_jobs = config.policy.max_concurrent_jobs.max(1);
        let guard = MediaConstitutionGuard::new(config.policy);
        let (progress_tx, _) = broadcast::channel(256);

        let loopback = Arc::new(LoopbackMediaServerBackend::from_env());
        let cloud = Arc::new(CloudMediaBackend::from_env());

        let image_backends: Vec<Arc<dyn ImageGeneratorBackend>> = vec![
            Arc::new(CandleStableDiffusionEngine::new(
                &config.models_dir,
                SdVariant::Turbo,
            )),
            Arc::new(CandleStableDiffusionEngine::new(
                &config.models_dir,
                SdVariant::V1_5,
            )),
            Arc::new(CandleFluxEngine::new(&config.models_dir)),
            Arc::new(OnnxImageDiffusionEngine::new(&config.models_dir)),
            loopback.clone(),
            cloud.clone(),
            Arc::new(ProceduralCanvasEngine::new()),
        ];

        let audio_backends: Vec<Arc<dyn AudioGeneratorBackend>> = vec![
            Arc::new(CandleParlerTtsEngine::new(&config.models_dir)),
            Arc::new(OnnxNeuralTtsEngine::new(&config.models_dir)),
            loopback.clone(),
            cloud,
            Arc::new(FormantSpeechEngine::new()),
            Arc::new(ProceduralAudioSynthEngine::new()),
        ];

        let video_backends: Vec<Arc<dyn VideoGeneratorBackend>> = vec![
            Arc::new(OnnxVideoDiffusionEngine::new(&config.models_dir)),
            Arc::new(CandleKeyframeVideoEngine::new(&config.models_dir)),
            loopback,
            Arc::new(ProceduralMotionVideoEngine::new()),
        ];

        Ok(Self {
            store,
            guard,
            signing_key,
            semaphore: Arc::new(Semaphore::new(max_jobs)),
            progress_tx,
            active_cancels: Arc::new(Mutex::new(HashMap::new())),
            image_backends,
            audio_backends,
            video_backends,
        })
    }

    pub fn store(&self) -> &MediaStore {
        &self.store
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    pub fn subscribe_progress(&self) -> broadcast::Receiver<MediaProgressEvent> {
        self.progress_tx.subscribe()
    }

    /// Annule un job en cours par son `job_id`. Renvoie `true` si trouvé.
    pub fn cancel_job(&self, job_id: &str) -> bool {
        if let Ok(map) = self.active_cancels.lock()
            && let Some(flag) = map.get(job_id)
        {
            flag.store(true, Ordering::SeqCst);
            return true;
        }
        false
    }

    /// Inventaire honnête de tous les moteurs disponibles/absents/non configurés.
    pub fn list_engines(&self) -> Vec<EngineDescriptor> {
        let mut out = Vec::new();
        for b in &self.image_backends {
            out.push(b.descriptor());
        }
        for b in &self.audio_backends {
            out.push(b.descriptor());
        }
        for b in &self.video_backends {
            out.push(b.descriptor());
        }
        out
    }

    /// Indique si le muxer externe `ffmpeg` est détecté sur cette machine.
    pub fn ffmpeg_available(&self) -> bool {
        FfmpegMuxer::detect().is_some()
    }

    /// Génère une image de bout en bout (garde Constitution/Biscuit + backend +
    /// signature Ed25519 + chunk PNG `iTXt` + persistance `MediaStore`).
    pub async fn generate_image(
        &self,
        req: ImageGenRequest,
        ctx: &MediaAuthContext,
        authority: Option<&CapabilityAuthority>,
        token: Option<&CapabilityToken>,
        parent_artifacts: Vec<String>,
    ) -> MediaResult<MediaArtifact> {
        let backend = self.select_image_backend(&req.engine)?;
        let desc = backend.descriptor();
        let media_req = MediaRequest::Image(req.clone());
        self.guard
            .authorize(&media_req, &desc.data_exit, ctx, authority, token)?;

        let _permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| MediaError::Storage("sémaphore média fermé".into()))?;

        let artifact_id = Uuid::new_v4();
        let job_id = artifact_id.to_string();
        let (control, _guard_cancel) = self.register_job_control(&job_id, req.deadline_ms);
        let progress = self.make_progress_callback();
        let started = Instant::now();

        let output = backend.generate(&req, &control, Some(progress)).await?;
        let elapsed_ms = started.elapsed().as_millis() as u64;

        let (artifact, _) = self.store.store_bytes(
            output.encoded_bytes,
            StoreArtifactParams {
                artifact_id,
                modality: MediaModality::Image,
                extension: req.format.extension().into(),
                mime_type: req.format.mime_type().into(),
                width: Some(output.rgba.width),
                height: Some(output.rgba.height),
                duration_secs: None,
                fps: None,
                sample_rate: None,
                channels: None,
                seed: output.seed,
                prompt: req.prompt,
                engine_id: output.engine_id,
                engine_kind: output.engine_kind,
                data_exit: output.data_exit,
                model_id: output.model_id,
                model_sha256: output.model_sha256,
                parent_artifacts,
                visemes_json: None,
                preview: Some(output.preview),
                elapsed_ms,
            },
            &self.signing_key,
        )?;
        Ok(artifact)
    }

    /// Génère un artefact audio (Voix, Musique, Soundscape, Foley) avec
    /// provenance signée Ed25519 et export visèmes JSON si applicable.
    pub async fn generate_audio(
        &self,
        req: AudioGenRequest,
        ctx: &MediaAuthContext,
        authority: Option<&CapabilityAuthority>,
        token: Option<&CapabilityToken>,
        parent_artifacts: Vec<String>,
    ) -> MediaResult<MediaArtifact> {
        let backend = self.select_audio_backend(&req.engine, &req.mode)?;
        let desc = backend.descriptor();
        let media_req = MediaRequest::Audio(req.clone());
        self.guard
            .authorize(&media_req, &desc.data_exit, ctx, authority, token)?;

        let _permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| MediaError::Storage("sémaphore média fermé".into()))?;

        let artifact_id = Uuid::new_v4();
        let job_id = artifact_id.to_string();
        let (control, _guard_cancel) = self.register_job_control(&job_id, req.deadline_ms);
        let progress = self.make_progress_callback();
        let started = Instant::now();

        let output = backend.generate(&req, &control, Some(progress)).await?;
        let elapsed_ms = started.elapsed().as_millis() as u64;

        let (artifact, _) = self.store.store_bytes(
            output.wav_bytes,
            StoreArtifactParams {
                artifact_id,
                modality: MediaModality::Audio,
                extension: req.format.extension().into(),
                mime_type: req.format.mime_type().into(),
                width: None,
                height: None,
                duration_secs: Some(output.buffer.duration_secs()),
                fps: None,
                sample_rate: Some(output.buffer.sample_rate),
                channels: Some(output.buffer.channels),
                seed: output.seed,
                prompt: req.prompt,
                engine_id: output.engine_id,
                engine_kind: output.engine_kind,
                data_exit: output.data_exit,
                model_id: output.model_id,
                model_sha256: output.model_sha256,
                parent_artifacts,
                visemes_json: output.visemes_json,
                preview: Some(output.preview),
                elapsed_ms,
            },
            &self.signing_key,
        )?;
        Ok(artifact)
    }

    /// Génère une vidéo (APNG, Y4M, FrameSequence, MP4 H.264 ou WebM VP9) avec
    /// provenance signée Ed25519 et optionnellement une piste audio générée.
    pub async fn generate_video(
        &self,
        req: VideoGenRequest,
        external_audio_wav: Option<&Path>,
        ctx: &MediaAuthContext,
        authority: Option<&CapabilityAuthority>,
        token: Option<&CapabilityToken>,
        mut parent_artifacts: Vec<String>,
    ) -> MediaResult<MediaArtifact> {
        let backend = self.select_video_backend(&req.engine)?;
        let desc = backend.descriptor();
        let media_req = MediaRequest::Video(req.clone());
        self.guard
            .authorize(&media_req, &desc.data_exit, ctx, authority, token)?;

        // Si une piste audio embarquée est demandée dans la requête, on la génère d'abord.
        let mut owned_audio_path: Option<PathBuf> = external_audio_wav.map(PathBuf::from);
        if owned_audio_path.is_none()
            && let Some(audio_req) = req.audio_track.clone()
        {
            let audio_art = self
                .generate_audio(*audio_req, ctx, authority, token, Vec::new())
                .await?;
            parent_artifacts.push(audio_art.sha256.clone());
            owned_audio_path = Some(audio_art.path);
        }

        let _permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| MediaError::Storage("sémaphore média fermé".into()))?;

        let artifact_id = Uuid::new_v4();
        let job_id = artifact_id.to_string();
        let (control, _guard_cancel) = self.register_job_control(&job_id, req.deadline_ms);
        let progress = self.make_progress_callback();
        let started = Instant::now();

        let output = backend
            .generate(&req, owned_audio_path.as_deref(), &control, Some(progress))
            .await?;
        let elapsed_ms = started.elapsed().as_millis() as u64;

        let (artifact, _) = self.store.store_bytes(
            output.encoded_bytes,
            StoreArtifactParams {
                artifact_id,
                modality: MediaModality::Video,
                extension: output.container.extension().into(),
                mime_type: output.container.mime_type().into(),
                width: Some(req.width),
                height: Some(req.height),
                duration_secs: Some(req.duration_secs()),
                fps: Some(req.fps),
                sample_rate: None,
                channels: None,
                seed: output.seed,
                prompt: req.prompt,
                engine_id: output.engine_id,
                engine_kind: output.engine_kind,
                data_exit: output.data_exit,
                model_id: output.model_id,
                model_sha256: output.model_sha256,
                parent_artifacts,
                visemes_json: None,
                preview: Some(output.preview),
                elapsed_ms,
            },
            &self.signing_key,
        )?;
        Ok(artifact)
    }

    /// Exécute un graphe [`MediaPipelineDag`] dans son ordre topologique avec
    /// checkpoints et propagation complète de la lignée `parent_artifacts`.
    pub async fn execute_dag(
        &self,
        dag: MediaPipelineDag,
        ctx: &MediaAuthContext,
        authority: Option<&CapabilityAuthority>,
        token: Option<&CapabilityToken>,
    ) -> MediaResult<DagExecutionResult> {
        let started = Instant::now();
        let order = dag.topological_order()?;
        let mut artifacts: HashMap<String, MediaArtifact> = HashMap::with_capacity(order.len());

        for node_id in &order {
            let node = dag.node_by_id(node_id).expect("nœud validé");
            let mut parents: Vec<String> = Vec::new();
            for dep in node
                .depends_on
                .iter()
                .map(|s| s.as_str())
                .chain(implicit_dependencies(&node.kind))
            {
                if let Some(art) = artifacts.get(dep)
                    && !parents.contains(&art.sha256)
                {
                    parents.push(art.sha256.clone());
                }
            }

            let artifact = match &node.kind {
                DagNodeKind::GenerateImage { request } => {
                    self.generate_image(request.clone(), ctx, authority, token, parents)
                        .await?
                }
                DagNodeKind::GenerateAudio { request } => {
                    self.generate_audio(request.clone(), ctx, authority, token, parents)
                        .await?
                }
                DagNodeKind::GenerateVideo {
                    request,
                    source_image_node,
                    audio_reactive_node,
                } => {
                    let mut req = request.clone();
                    let mut ext_audio: Option<PathBuf> = None;
                    if let Some(img_node) = source_image_node
                        && let Some(img_art) = artifacts.get(img_node)
                    {
                        req.mode = VideoGenMode::ImageToVideo {
                            init_image_path: img_art.path.clone(),
                        };
                    }
                    if let Some(aud_node) = audio_reactive_node
                        && let Some(aud_art) = artifacts.get(aud_node)
                    {
                        if source_image_node.is_none() {
                            req.mode = VideoGenMode::AudioReactive {
                                audio_wav_path: aud_art.path.clone(),
                            };
                        }
                        ext_audio = Some(aud_art.path.clone());
                    }
                    self.generate_video(req, ext_audio.as_deref(), ctx, authority, token, parents)
                        .await?
                }
                DagNodeKind::MixAudio {
                    voice_node,
                    bed_node,
                    bed_gain,
                    duck_gain,
                } => {
                    let voice_art = artifacts.get(voice_node).ok_or_else(|| {
                        MediaError::InvalidRequest(format!(
                            "nœud audio voix '{voice_node}' introuvable"
                        ))
                    })?;
                    let bed_art = artifacts.get(bed_node).ok_or_else(|| {
                        MediaError::InvalidRequest(format!(
                            "nœud audio lit '{bed_node}' introuvable"
                        ))
                    })?;
                    let voice_path = voice_art.path.clone();
                    let bed_path = bed_art.path.clone();
                    let bg = *bed_gain;
                    let dg = *duck_gain;
                    let mix_started = Instant::now();
                    let mixed = tokio::task::spawn_blocking(move || -> MediaResult<_> {
                        let v = read_wav_file(&voice_path)?;
                        let b = read_wav_file(&bed_path)?;
                        mix_with_sidechain_ducking(&v, &b, bg, dg)
                    })
                    .await
                    .map_err(|e| MediaError::InferenceFailed {
                        engine_id: "audio-mixer-dsp".into(),
                        reason: e.to_string(),
                    })??;
                    let wav = mixed.encode_wav(AudioFormat::WavPcm16)?;
                    let preview = mixed.render_waveform_preview(200, 96);
                    let (art, _) = self.store.store_bytes(
                        wav,
                        StoreArtifactParams {
                            artifact_id: Uuid::new_v4(),
                            modality: MediaModality::Audio,
                            extension: "wav".into(),
                            mime_type: "audio/wav".into(),
                            width: None,
                            height: None,
                            duration_secs: Some(mixed.duration_secs()),
                            fps: None,
                            sample_rate: Some(mixed.sample_rate),
                            channels: Some(mixed.channels),
                            seed: 0,
                            prompt: format!("mix({voice_node} + {bed_node})"),
                            engine_id: "audio-mixer-dsp".into(),
                            engine_kind: EngineKind::ProceduralDsp,
                            data_exit: MediaDataExit::Local,
                            model_id: None,
                            model_sha256: None,
                            parent_artifacts: parents,
                            visemes_json: None,
                            preview: Some(preview),
                            elapsed_ms: mix_started.elapsed().as_millis() as u64,
                        },
                        &self.signing_key,
                    )?;
                    art
                }
                DagNodeKind::MuxAudioVideo {
                    video_node,
                    audio_node,
                    container,
                } => {
                    let vid_art = artifacts.get(video_node).ok_or_else(|| {
                        MediaError::InvalidRequest(format!("nœud vidéo '{video_node}' introuvable"))
                    })?;
                    let aud_art = artifacts.get(audio_node).ok_or_else(|| {
                        MediaError::InvalidRequest(format!("nœud audio '{audio_node}' introuvable"))
                    })?;
                    let muxer =
                        FfmpegMuxer::detect().ok_or_else(|| MediaError::MuxerUnavailable {
                            binary: "ffmpeg".into(),
                            reason: "ffmpeg est requis pour le nœud MuxAudioVideo".into(),
                        })?;
                    let vp = vid_art.path.clone();
                    let ap = aud_art.path.clone();
                    let cont = *container;
                    let mux_started = Instant::now();
                    let muxed_bytes =
                        tokio::task::spawn_blocking(move || muxer.mux_files(&vp, Some(&ap), cont))
                            .await
                            .map_err(|e| MediaError::InferenceFailed {
                                engine_id: "ffmpeg-muxer".into(),
                                reason: e.to_string(),
                            })??;
                    let (art, _) = self.store.store_bytes(
                        muxed_bytes,
                        StoreArtifactParams {
                            artifact_id: Uuid::new_v4(),
                            modality: MediaModality::Composite,
                            extension: cont.extension().into(),
                            mime_type: cont.mime_type().into(),
                            width: vid_art.width,
                            height: vid_art.height,
                            duration_secs: vid_art.duration_secs.or(aud_art.duration_secs),
                            fps: vid_art.fps,
                            sample_rate: aud_art.sample_rate,
                            channels: aud_art.channels,
                            seed: vid_art.seed,
                            prompt: format!("mux({video_node} + {audio_node})"),
                            engine_id: "ffmpeg-muxer".into(),
                            engine_kind: EngineKind::ProceduralDsp,
                            data_exit: MediaDataExit::Local,
                            model_id: None,
                            model_sha256: None,
                            parent_artifacts: parents,
                            visemes_json: None,
                            preview: vid_art.preview.clone(),
                            elapsed_ms: mux_started.elapsed().as_millis() as u64,
                        },
                        &self.signing_key,
                    )?;
                    art
                }
            };
            artifacts.insert(node_id.clone(), artifact);
        }

        let final_node_id = order.last().cloned().unwrap_or_default();
        Ok(DagExecutionResult {
            pipeline_name: dag.name,
            execution_order: order,
            artifacts,
            final_node_id,
            total_elapsed_ms: started.elapsed().as_millis() as u64,
        })
    }

    /// Sélectionne le moteur image selon la règle d'honnêteté :
    /// - un sélecteur explicite (`sd-1.5-candle`, `flux-schnell-candle`,
    ///   `onnx-diffusion`, `loopback`, `cloud`, `procedural-dsp`) renvoie ce
    ///   moteur exact (qui échouera honnêtement avec `ModelMissing` s'il manque
    ///   ses poids — jamais de repli silencieux) ;
    /// - `auto` choisit le premier moteur local **prêt** (neuronal local en
    ///   priorité, puis `procedural-dsp`, jamais cloud sans demande explicite).
    fn select_image_backend(&self, selector: &str) -> MediaResult<Arc<dyn ImageGeneratorBackend>> {
        let sel = normalize_image_selector(selector);
        if sel == "auto" {
            for b in &self.image_backends {
                let d = b.descriptor();
                if d.data_exit.is_cloud() {
                    continue;
                }
                if matches!(d.availability, EngineAvailability::Ready { .. }) {
                    return Ok(b.clone());
                }
            }
        } else {
            for b in &self.image_backends {
                if b.descriptor().id == sel {
                    return Ok(b.clone());
                }
            }
        }
        Err(MediaError::BackendUnavailable {
            engine_id: selector.to_string(),
            reason: "moteur image inconnu (disponibles : auto, sd-turbo-candle, sd-1.5-candle, flux-schnell-candle, onnx-diffusion, loopback, cloud, procedural-dsp)".into(),
        })
    }

    fn select_audio_backend(
        &self,
        selector: &str,
        mode: &AudioGenMode,
    ) -> MediaResult<Arc<dyn AudioGeneratorBackend>> {
        let sel = normalize_audio_selector(selector);
        if sel == "auto" {
            let prefer_speech = matches!(mode, AudioGenMode::Speech { .. });
            if prefer_speech {
                for b in &self.audio_backends {
                    let d = b.descriptor();
                    if d.data_exit.is_cloud() || d.id == "procedural-dsp" {
                        continue;
                    }
                    if matches!(d.availability, EngineAvailability::Ready { .. }) {
                        return Ok(b.clone());
                    }
                }
            } else {
                // Musique / Soundscape / Foley : utilise le synthétiseur polyphonique DSP.
                for b in &self.audio_backends {
                    if b.descriptor().id == "procedural-dsp" {
                        return Ok(b.clone());
                    }
                }
            }
        } else {
            for b in &self.audio_backends {
                if b.descriptor().id == sel {
                    return Ok(b.clone());
                }
            }
        }
        Err(MediaError::BackendUnavailable {
            engine_id: selector.to_string(),
            reason: "moteur audio inconnu (disponibles : auto, parler-tts-candle, onnx-tts, formant-dsp, procedural-dsp, loopback, cloud)".into(),
        })
    }

    fn select_video_backend(&self, selector: &str) -> MediaResult<Arc<dyn VideoGeneratorBackend>> {
        let sel = normalize_video_selector(selector);
        if sel == "auto" {
            for b in &self.video_backends {
                let d = b.descriptor();
                if d.data_exit.is_cloud() {
                    continue;
                }
                if matches!(d.availability, EngineAvailability::Ready { .. }) {
                    return Ok(b.clone());
                }
            }
        } else {
            for b in &self.video_backends {
                if b.descriptor().id == sel {
                    return Ok(b.clone());
                }
            }
        }
        Err(MediaError::BackendUnavailable {
            engine_id: selector.to_string(),
            reason: "moteur vidéo inconnu (disponibles : auto, onnx-video, candle-keyframe, loopback, procedural-dsp)".into(),
        })
    }

    fn register_job_control(
        &self,
        job_id: &str,
        deadline_ms: u64,
    ) -> (GenerationControl, JobCancelDropGuard) {
        let flag = Arc::new(AtomicBool::new(false));
        if let Ok(mut map) = self.active_cancels.lock() {
            map.insert(job_id.to_string(), flag.clone());
        }
        let control = GenerationControl::with_cancel_flag(job_id, flag, deadline_ms);
        let drop_guard = JobCancelDropGuard {
            job_id: job_id.to_string(),
            active_cancels: self.active_cancels.clone(),
        };
        (control, drop_guard)
    }

    fn make_progress_callback(&self) -> ProgressCallback {
        let tx = self.progress_tx.clone();
        Arc::new(move |ev| {
            let _ = tx.send(ev);
        })
    }
}

struct JobCancelDropGuard {
    job_id: String,
    active_cancels: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl Drop for JobCancelDropGuard {
    fn drop(&mut self) {
        if let Ok(mut map) = self.active_cancels.lock() {
            map.remove(&self.job_id);
        }
    }
}

fn normalize_image_selector(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" | "auto" => "auto".into(),
        "sd" | "sd-candle" | "sd15" | "sd-1.5" | "sd-1.5-candle" => "sd-1.5-candle".into(),
        "sd-turbo" | "turbo" | "sd-turbo-candle" => "sd-turbo-candle".into(),
        "flux" | "flux-candle" | "flux-schnell" | "flux-schnell-candle" => {
            "flux-schnell-candle".into()
        }
        "onnx" | "onnx-diffusion" => "onnx-diffusion".into(),
        "dsp" | "procedural" | "procedural-dsp" => "procedural-dsp".into(),
        "loopback" => "loopback".into(),
        "cloud" => "cloud".into(),
        other => other.into(),
    }
}

fn normalize_audio_selector(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" | "auto" => "auto".into(),
        "parler" | "parler-tts" | "parler-tts-candle" => "parler-tts-candle".into(),
        "onnx" | "onnx-tts" => "onnx-tts".into(),
        "formant" | "formant-dsp" | "klatt" => "formant-dsp".into(),
        "dsp" | "procedural" | "procedural-dsp" => "procedural-dsp".into(),
        "loopback" => "loopback".into(),
        "cloud" => "cloud".into(),
        other => other.into(),
    }
}

fn normalize_video_selector(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" | "auto" => "auto".into(),
        "onnx" | "onnx-video" => "onnx-video".into(),
        "candle" | "slerp" | "candle-keyframe" => "candle-keyframe".into(),
        "dsp" | "procedural" | "procedural-dsp" => "procedural-dsp".into(),
        "loopback" => "loopback".into(),
        other => other.into(),
    }
}
