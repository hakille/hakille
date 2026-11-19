//! CLI souveraine, adaptateur hexagonal [`DaemonMediaPort`] et mappers DTO IPC
//! pour le moteur génératif multimodal [`hakille_media`].
//!
//! Sous-commandes CLI :
//! - `hakille-daemon media engines [--json]`
//! - `hakille-daemon media image <prompt> [--size WxH] [--steps N] [--seed N] [--engine ID] [--format png|jpg] [--init-image PATH] [--strength F] [--json]`
//! - `hakille-daemon media audio <prompt> [--mode speech|music|soundscape|foley] [--duration S] [--voice V] [--bpm N] [--engine ID] [--json]`
//! - `hakille-daemon media video <prompt> [--size WxH] [--fps N] [--frames N] [--format apng|y4m|frames|mp4|webm] [--motion M] [--init-image PATH] [--audio PATH] [--engine ID] [--json]`
//! - `hakille-daemon media compose <prompt> [--size WxH] [--fps N] [--duration S] [--seed N] [--no-mux] [--json]`
//! - `hakille-daemon media list [--limit N] [--json]`
//! - `hakille-daemon media verify <path> [--json]`

use async_trait::async_trait;
use base64::Engine as _;
use ed25519_dalek::SigningKey;
use hakille_core::{HakilleError, TrustLevel};
use hakille_kernel::media::{
    MediaCommand, MediaCommandKind, MediaGenerationPort, MediaGenerationReceipt,
};
use hakille_media::{
    AudioGenMode, AudioGenRequest, CameraMotion, EngineAvailability, EngineDescriptor, ImageFormat,
    ImageGenMode, ImageGenRequest, MediaArtifact, MediaAuthContext, MediaDataExit, MediaJobManager,
    MediaManagerConfig, MediaModality, MediaProgressEvent, StoryboardPreset, VideoContainer,
    VideoGenMode, VideoGenRequest, verify_artifact_provenance,
};
use hakille_ui_protocol::{
    MediaArtifactDto, MediaEngineDto, MediaGenerateRequestDto, MediaJobProgressDto,
    MediaModalityDto, MediaPreviewRgbaDto,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Construit le [`MediaJobManager`] souverain lié au répertoire de données et à
/// la clé Ed25519 persistante du démon.
pub fn build_media_manager(
    data_dir: &Path,
    models_dir: &Path,
    signing_key: SigningKey,
) -> Result<Arc<MediaJobManager>, String> {
    let media_dir = data_dir.join("media");
    let cfg = MediaManagerConfig::new(media_dir, models_dir);
    MediaJobManager::new(cfg, signing_key)
        .map(Arc::new)
        .map_err(|e| format!("initialisation hakille-media impossible : {e}"))
}

/// Adaptateur hexagonal branché sur `CognitiveOrchestrator.media_port`.
pub struct DaemonMediaPort {
    manager: Arc<MediaJobManager>,
}

impl DaemonMediaPort {
    pub fn new(manager: Arc<MediaJobManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl MediaGenerationPort for DaemonMediaPort {
    async fn execute(&self, cmd: &MediaCommand) -> hakille_core::Result<MediaGenerationReceipt> {
        let auth_ctx = MediaAuthContext {
            trust_level: TrustLevel::User,
            ..Default::default()
        };
        let (artifact, summary) = match cmd.kind {
            MediaCommandKind::Image => {
                let mut req = ImageGenRequest::simple(&cmd.prompt);
                if let Some(w) = cmd.width {
                    req.width = w;
                }
                if let Some(h) = cmd.height {
                    req.height = h;
                }
                if let Some(steps) = cmd.steps {
                    req.steps = steps;
                }
                req.seed = cmd.seed;
                req.engine = cmd.engine.clone();
                if let Some(fmt) = cmd.format.as_deref()
                    && (fmt.eq_ignore_ascii_case("jpg") || fmt.eq_ignore_ascii_case("jpeg"))
                {
                    req.format = ImageFormat::Jpeg;
                }
                let art = self
                    .manager
                    .generate_image(req, &auth_ctx, None, None, Vec::new())
                    .await
                    .map_err(|e| HakilleError::Internal(e.to_string()))?;
                let summary = format!(
                    "Image générée ({}×{}, {} octets)",
                    art.width.unwrap_or(0),
                    art.height.unwrap_or(0),
                    art.bytes_len
                );
                (art, summary)
            }
            MediaCommandKind::Audio => {
                let duration = cmd.duration_secs.unwrap_or(4.0);
                let mut req = match cmd.audio_mode.as_deref().unwrap_or("speech") {
                    "music" => AudioGenRequest::music(&cmd.prompt, duration, 110),
                    "soundscape" | "ambient" => AudioGenRequest::soundscape(&cmd.prompt, duration),
                    "foley" => {
                        let mut r = AudioGenRequest::soundscape(&cmd.prompt, duration);
                        r.mode = AudioGenMode::Foley {
                            event: cmd.prompt.clone(),
                        };
                        r
                    }
                    _ => {
                        let mut r = AudioGenRequest::speech(&cmd.prompt);
                        r.duration_secs = duration;
                        if let Some(voice) = &cmd.voice {
                            r.mode = AudioGenMode::Speech {
                                voice: voice.clone(),
                                language: "fr".into(),
                                speed: 1.0,
                                emotion: "neutral".into(),
                                style_description: None,
                            };
                        }
                        r
                    }
                };
                req.seed = cmd.seed;
                req.engine = cmd.engine.clone();
                let art = self
                    .manager
                    .generate_audio(req, &auth_ctx, None, None, Vec::new())
                    .await
                    .map_err(|e| HakilleError::Internal(e.to_string()))?;
                let summary = format!(
                    "Audio généré ({:.1} s, {} Hz, {} octets)",
                    art.duration_secs.unwrap_or(0.0),
                    art.sample_rate.unwrap_or(0),
                    art.bytes_len
                );
                (art, summary)
            }
            MediaCommandKind::Video => {
                let mut req = VideoGenRequest::simple(&cmd.prompt);
                if let Some(w) = cmd.width {
                    req.width = w;
                }
                if let Some(h) = cmd.height {
                    req.height = h;
                }
                if let Some(fps) = cmd.fps {
                    req.fps = fps;
                }
                if let Some(frames) = cmd.num_frames {
                    req.num_frames = frames;
                }
                if let Some(motion) = cmd.motion.as_deref().and_then(CameraMotion::parse) {
                    req.camera_motion = motion;
                }
                if let Some(fmt) = cmd.format.as_deref() {
                    req.container = parse_video_container(fmt).map_err(HakilleError::Internal)?;
                }
                req.seed = cmd.seed;
                req.engine = cmd.engine.clone();
                let ext_audio = cmd.audio_path.as_deref().map(Path::new);
                let art = self
                    .manager
                    .generate_video(req, ext_audio, &auth_ctx, None, None, Vec::new())
                    .await
                    .map_err(|e| HakilleError::Internal(e.to_string()))?;
                let summary = format!(
                    "Vidéo générée ({}×{} @ {} fps, {:.1} s, {} octets)",
                    art.width.unwrap_or(0),
                    art.height.unwrap_or(0),
                    art.fps.unwrap_or(0),
                    art.duration_secs.unwrap_or(0.0),
                    art.bytes_len
                );
                (art, summary)
            }
            MediaCommandKind::Story => {
                let w = cmd.width.unwrap_or(256);
                let h = cmd.height.unwrap_or(144);
                let fps = cmd.fps.unwrap_or(12);
                let dur = cmd.duration_secs.unwrap_or(3.0);
                let include_mux = self.manager.ffmpeg_available();
                let dag = StoryboardPreset::from_prompt(
                    &cmd.prompt,
                    w,
                    h,
                    fps,
                    dur,
                    cmd.seed,
                    include_mux,
                );
                let res = self
                    .manager
                    .execute_dag(dag, &auth_ctx, None, None)
                    .await
                    .map_err(|e| HakilleError::Internal(e.to_string()))?;
                let final_art =
                    res.artifacts
                        .get(&res.final_node_id)
                        .cloned()
                        .ok_or_else(|| {
                            HakilleError::Internal("artefact final DAG introuvable".into())
                        })?;
                let summary = format!(
                    "Storyboard multimodal terminé ({} nœuds, final={}, {} octets)",
                    res.execution_order.len(),
                    res.final_node_id,
                    final_art.bytes_len
                );
                (final_art, summary)
            }
        };

        Ok(MediaGenerationReceipt {
            artifact_id: artifact.id.to_string(),
            modality: format!("{:?}", artifact.modality).to_lowercase(),
            path: artifact.path.display().to_string(),
            provenance_path: artifact.provenance_path.display().to_string(),
            sha256: artifact.sha256,
            engine_id: artifact.engine_id,
            engine_badge: artifact.engine_kind.badge_fr().to_string(),
            elapsed_ms: artifact.elapsed_ms,
            summary,
        })
    }
}

// ---------------------------------------------------------------------------
// Mappers DTO IPC (`hakille-media` <-> `hakille-ui-protocol`)
// ---------------------------------------------------------------------------

pub fn modality_to_dto(m: MediaModality) -> MediaModalityDto {
    match m {
        MediaModality::Image => MediaModalityDto::Image,
        MediaModality::Audio => MediaModalityDto::Audio,
        MediaModality::Video => MediaModalityDto::Video,
        MediaModality::Composite => MediaModalityDto::Composite,
    }
}

pub fn engine_descriptor_to_dto(desc: &EngineDescriptor) -> MediaEngineDto {
    let status_detail = match &desc.availability {
        EngineAvailability::Ready {
            device,
            model_sha256,
        } => match model_sha256 {
            Some(sha) => format!("{device} (sha256:{}…)", &sha[..sha.len().min(12)]),
            None => device.clone(),
        },
        EngineAvailability::ModelMissing {
            model_id,
            missing_files,
            hint,
        } => format!(
            "modèle '{model_id}' absent ({}) — {hint}",
            missing_files.join(", ")
        ),
        EngineAvailability::Unconfigured { env_hint } => env_hint.clone(),
        EngineAvailability::Unavailable { reason } => reason.clone(),
    };
    MediaEngineDto {
        id: desc.id.clone(),
        name: desc.name.clone(),
        modality: modality_to_dto(desc.modality),
        kind_badge: desc.kind.badge_fr().to_string(),
        ready: desc.availability.is_ready(),
        status_detail,
        is_cloud: desc.data_exit.is_cloud(),
    }
}

pub fn progress_to_dto(ev: &MediaProgressEvent) -> MediaJobProgressDto {
    MediaJobProgressDto {
        job_id: ev.job_id.clone(),
        modality: modality_to_dto(ev.modality),
        stage: ev.stage.clone(),
        step: ev.step,
        total_steps: ev.total_steps,
        fraction: ev.fraction,
        elapsed_ms: ev.elapsed_ms,
        message: ev.message.clone(),
    }
}

pub fn artifact_to_dto(art: &MediaArtifact) -> MediaArtifactDto {
    let preview = art.preview.as_ref().map(|p| MediaPreviewRgbaDto {
        width: p.width,
        height: p.height,
        rgba_base64: base64::engine::general_purpose::STANDARD.encode(&p.rgba),
    });
    MediaArtifactDto {
        id: art.id.to_string(),
        modality: modality_to_dto(art.modality),
        path: art.path.display().to_string(),
        provenance_path: art.provenance_path.display().to_string(),
        sha256: art.sha256.clone(),
        mime_type: art.mime_type.clone(),
        bytes_len: art.bytes_len,
        engine_id: art.engine_id.clone(),
        engine_badge: art.engine_kind.badge_fr().to_string(),
        prompt_summary: art.prompt_summary.clone(),
        elapsed_ms: art.elapsed_ms,
        provenance_signed: !art.provenance_signature_hex.is_empty(),
        created_at: art.created_at.to_rfc3339(),
        preview,
    }
}

/// Exécute une requête IPC [`MediaGenerateRequestDto`] via le [`MediaJobManager`].
pub async fn execute_ipc_generate_request(
    manager: &MediaJobManager,
    req: &MediaGenerateRequestDto,
) -> Result<MediaArtifact, String> {
    let auth_ctx = MediaAuthContext {
        trust_level: TrustLevel::User,
        ..Default::default()
    };
    let engine = if req.engine.trim().is_empty() {
        "auto".to_string()
    } else {
        req.engine.clone()
    };
    match req.modality {
        MediaModalityDto::Image => {
            let mut r = ImageGenRequest::simple(&req.prompt);
            if let Some(w) = req.width {
                r.width = w;
            }
            if let Some(h) = req.height {
                r.height = h;
            }
            if let Some(steps) = req.steps {
                r.steps = steps;
            }
            r.seed = req.seed;
            r.engine = engine;
            if let Some(fmt) = req.format.as_deref()
                && (fmt.eq_ignore_ascii_case("jpg") || fmt.eq_ignore_ascii_case("jpeg"))
            {
                r.format = ImageFormat::Jpeg;
            }
            manager
                .generate_image(r, &auth_ctx, None, None, Vec::new())
                .await
                .map_err(|e| e.to_string())
        }
        MediaModalityDto::Audio => {
            let duration = req.duration_secs.unwrap_or(4.0);
            let mut r = match req.mode.as_deref().unwrap_or("speech") {
                "music" => AudioGenRequest::music(&req.prompt, duration, 110),
                "soundscape" | "ambient" => AudioGenRequest::soundscape(&req.prompt, duration),
                "foley" => {
                    let mut x = AudioGenRequest::soundscape(&req.prompt, duration);
                    x.mode = AudioGenMode::Foley {
                        event: req.prompt.clone(),
                    };
                    x
                }
                _ => {
                    let mut x = AudioGenRequest::speech(&req.prompt);
                    x.duration_secs = duration;
                    x
                }
            };
            r.seed = req.seed;
            r.engine = engine;
            manager
                .generate_audio(r, &auth_ctx, None, None, Vec::new())
                .await
                .map_err(|e| e.to_string())
        }
        MediaModalityDto::Video => {
            let mut r = VideoGenRequest::simple(&req.prompt);
            if let Some(w) = req.width {
                r.width = w;
            }
            if let Some(h) = req.height {
                r.height = h;
            }
            if let Some(fps) = req.fps {
                r.fps = fps;
            }
            if let Some(frames) = req.num_frames {
                r.num_frames = frames;
            }
            if let Some(motion) = req.mode.as_deref().and_then(CameraMotion::parse) {
                r.camera_motion = motion;
            }
            if let Some(fmt) = req.format.as_deref() {
                r.container = parse_video_container(fmt)?;
            }
            r.seed = req.seed;
            r.engine = engine;
            manager
                .generate_video(r, None, &auth_ctx, None, None, Vec::new())
                .await
                .map_err(|e| e.to_string())
        }
        MediaModalityDto::Composite => {
            let w = req.width.unwrap_or(256);
            let h = req.height.unwrap_or(144);
            let fps = req.fps.unwrap_or(10);
            let dur = req.duration_secs.unwrap_or(2.5);
            let include_mux = manager.ffmpeg_available();
            let dag =
                StoryboardPreset::from_prompt(&req.prompt, w, h, fps, dur, req.seed, include_mux);
            let res = manager
                .execute_dag(dag, &auth_ctx, None, None)
                .await
                .map_err(|e| e.to_string())?;
            res.artifacts
                .get(&res.final_node_id)
                .cloned()
                .ok_or_else(|| "artefact final DAG introuvable".to_string())
        }
    }
}

fn format_data_exit(exit: &MediaDataExit) -> String {
    match exit {
        MediaDataExit::Local => "Local uniquement".into(),
        MediaDataExit::Loopback { endpoint } => format!("Loopback ({endpoint})"),
        MediaDataExit::Cloud { provider, .. } => format!("Cloud opt-in ({provider})"),
    }
}

fn parse_size(raw: &str) -> Result<(u32, u32), String> {
    let lower = raw.to_ascii_lowercase();
    let (w, h) = lower
        .split_once('x')
        .ok_or_else(|| format!("format --size invalide '{raw}' (attendu LxH, ex. 512x512)"))?;
    let width: u32 = w
        .parse()
        .map_err(|_| format!("largeur invalide dans --size '{raw}'"))?;
    let height: u32 = h
        .parse()
        .map_err(|_| format!("hauteur invalide dans --size '{raw}'"))?;
    Ok((width, height))
}

fn parse_video_container(raw: &str) -> Result<VideoContainer, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "apng" => Ok(VideoContainer::Apng),
        "y4m" => Ok(VideoContainer::Y4m),
        "frames" | "png-sequence" => Ok(VideoContainer::FrameSequence),
        "mp4" | "mp4-h264" => Ok(VideoContainer::Mp4H264),
        "webm" | "webm-vp9" => Ok(VideoContainer::WebmVp9),
        other => Err(format!(
            "format vidéo inconnu '{other}' (attendus : apng|y4m|frames|mp4|webm)"
        )),
    }
}

// ---------------------------------------------------------------------------
// Point d'entrée CLI standalone (`hakille-daemon media <subcommand> ...`)
// ---------------------------------------------------------------------------

pub async fn run_cli(
    args: &[String],
    data_dir: PathBuf,
    models_dir: PathBuf,
    signing_key: SigningKey,
) -> Result<(), Box<dyn std::error::Error>> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    if matches!(sub, "help" | "--help" | "-h") {
        print_media_cli_help();
        return Ok(());
    }

    let manager =
        build_media_manager(&data_dir, &models_dir, signing_key).map_err(std::io::Error::other)?;
    let rest = &args[1..];
    let json_mode = rest.iter().any(|a| a == "--json");

    match sub {
        "engines" => {
            let engines = manager.list_engines();
            if json_mode {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "ffmpeg_muxer_available": manager.ffmpeg_available(),
                        "engines": engines,
                    }))?
                );
            } else {
                print_engines_table(&engines, manager.ffmpeg_available());
            }
            Ok(())
        }
        "image" => {
            let (prompt, flags) = split_prompt_and_flags(rest);
            if prompt.is_empty() {
                return Err("Usage : hakille-daemon media image <prompt> [--size WxH] [--steps N] [--seed N] [--engine ID] [--format png|jpg] [--init-image PATH] [--strength F] [--json]".into());
            }
            let mut req = ImageGenRequest::simple(prompt);
            if let Some(sz) = flags.get("--size") {
                let (w, h) = parse_size(sz)?;
                req.width = w;
                req.height = h;
            }
            if let Some(steps) = flags.get("--steps") {
                req.steps = steps.parse()?;
            }
            if let Some(seed) = flags.get("--seed") {
                req.seed = Some(seed.parse()?);
            }
            if let Some(engine) = flags.get("--engine") {
                req.engine = engine.clone();
            }
            if let Some(fmt) = flags.get("--format")
                && (fmt.eq_ignore_ascii_case("jpg") || fmt.eq_ignore_ascii_case("jpeg"))
            {
                req.format = ImageFormat::Jpeg;
            }
            if let Some(init_path) = flags.get("--init-image") {
                let strength: f32 = flags
                    .get("--strength")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0.65);
                req.mode = ImageGenMode::ImageToImage {
                    init_image_path: PathBuf::from(init_path),
                    strength,
                };
            }
            let auth_ctx = MediaAuthContext {
                trust_level: TrustLevel::User,
                ..Default::default()
            };
            let art = manager
                .generate_image(req, &auth_ctx, None, None, Vec::new())
                .await?;
            print_artifact_result(&art, json_mode)?;
            Ok(())
        }
        "audio" => {
            let (prompt, flags) = split_prompt_and_flags(rest);
            if prompt.is_empty() {
                return Err("Usage : hakille-daemon media audio <prompt> [--mode speech|music|soundscape|foley] [--duration S] [--voice V] [--bpm N] [--engine ID] [--json]".into());
            }
            let duration: f32 = flags
                .get("--duration")
                .and_then(|s| s.parse().ok())
                .unwrap_or(4.0);
            let bpm: u16 = flags
                .get("--bpm")
                .and_then(|s| s.parse().ok())
                .unwrap_or(110);
            let mode_str = flags.get("--mode").map(String::as_str).unwrap_or("speech");
            let mut req = match mode_str {
                "music" => AudioGenRequest::music(prompt, duration, bpm),
                "soundscape" | "ambient" => AudioGenRequest::soundscape(prompt, duration),
                "foley" => {
                    let mut r = AudioGenRequest::soundscape(prompt.clone(), duration);
                    r.mode = AudioGenMode::Foley { event: prompt };
                    r
                }
                _ => {
                    let mut r = AudioGenRequest::speech(prompt);
                    r.duration_secs = duration;
                    if let Some(voice) = flags.get("--voice") {
                        r.mode = AudioGenMode::Speech {
                            voice: voice.clone(),
                            language: "fr".into(),
                            speed: 1.0,
                            emotion: "neutral".into(),
                            style_description: None,
                        };
                    }
                    r
                }
            };
            if let Some(seed) = flags.get("--seed") {
                req.seed = Some(seed.parse()?);
            }
            if let Some(engine) = flags.get("--engine") {
                req.engine = engine.clone();
            }
            let auth_ctx = MediaAuthContext {
                trust_level: TrustLevel::User,
                ..Default::default()
            };
            let art = manager
                .generate_audio(req, &auth_ctx, None, None, Vec::new())
                .await?;
            print_artifact_result(&art, json_mode)?;
            Ok(())
        }
        "video" => {
            let (prompt, flags) = split_prompt_and_flags(rest);
            if prompt.is_empty() {
                return Err("Usage : hakille-daemon media video <prompt> [--size WxH] [--fps N] [--frames N] [--format apng|y4m|frames|mp4|webm] [--motion M] [--init-image PATH] [--audio PATH] [--engine ID] [--json]".into());
            }
            let mut req = VideoGenRequest::simple(prompt);
            if let Some(sz) = flags.get("--size") {
                let (w, h) = parse_size(sz)?;
                req.width = w;
                req.height = h;
            }
            if let Some(fps) = flags.get("--fps") {
                req.fps = fps.parse()?;
            }
            if let Some(frames) = flags.get("--frames") {
                req.num_frames = frames.parse()?;
            }
            if let Some(seed) = flags.get("--seed") {
                req.seed = Some(seed.parse()?);
            }
            if let Some(engine) = flags.get("--engine") {
                req.engine = engine.clone();
            }
            if let Some(fmt) = flags.get("--format") {
                req.container = parse_video_container(fmt)?;
            }
            if let Some(motion) = flags.get("--motion") {
                req.camera_motion = CameraMotion::parse(motion).ok_or_else(|| {
                    format!("mouvement caméra inconnu '{motion}' (static|pan-left|pan-right|zoom-in|zoom-out|orbit|kenburns)")
                })?;
            }
            if let Some(init_path) = flags.get("--init-image") {
                req.mode = VideoGenMode::ImageToVideo {
                    init_image_path: PathBuf::from(init_path),
                };
            }
            let ext_audio = flags.get("--audio").map(PathBuf::from);
            let auth_ctx = MediaAuthContext {
                trust_level: TrustLevel::User,
                ..Default::default()
            };
            let art = manager
                .generate_video(req, ext_audio.as_deref(), &auth_ctx, None, None, Vec::new())
                .await?;
            print_artifact_result(&art, json_mode)?;
            Ok(())
        }
        "compose" | "story" => {
            let (prompt, flags) = split_prompt_and_flags(rest);
            if prompt.is_empty() {
                return Err("Usage : hakille-daemon media compose <prompt> [--size WxH] [--fps N] [--duration S] [--seed N] [--no-mux] [--json]".into());
            }
            let (w, h) = match flags.get("--size") {
                Some(sz) => parse_size(sz)?,
                None => (256, 144),
            };
            let fps: u16 = flags
                .get("--fps")
                .and_then(|s| s.parse().ok())
                .unwrap_or(10);
            let duration: f32 = flags
                .get("--duration")
                .and_then(|s| s.parse().ok())
                .unwrap_or(2.5);
            let seed: Option<u64> = flags.get("--seed").and_then(|s| s.parse().ok());
            let no_mux = rest.iter().any(|a| a == "--no-mux");
            let include_mux = !no_mux && manager.ffmpeg_available();

            let dag =
                StoryboardPreset::from_prompt(&prompt, w, h, fps, duration, seed, include_mux);
            let auth_ctx = MediaAuthContext {
                trust_level: TrustLevel::User,
                ..Default::default()
            };
            let res = manager.execute_dag(dag, &auth_ctx, None, None).await?;
            if json_mode {
                println!("{}", serde_json::to_string_pretty(&res)?);
            } else {
                println!(
                    "Pipeline multimodal '{}' exécuté en {} ms ({} nœud(s)) :",
                    res.pipeline_name,
                    res.total_elapsed_ms,
                    res.execution_order.len()
                );
                for node_id in &res.execution_order {
                    if let Some(art) = res.artifacts.get(node_id) {
                        println!(
                            "  • {:<15} [{:<26}] {} ({} octets, parents={})",
                            node_id,
                            art.engine_kind.badge_fr(),
                            art.path.display(),
                            art.bytes_len,
                            art.parent_artifacts.len()
                        );
                    }
                }
                if let Some(final_art) = res.artifacts.get(&res.final_node_id) {
                    println!("  → Final : {}", final_art.path.display());
                    println!(
                        "  → Provenance Ed25519 : {}",
                        final_art.provenance_path.display()
                    );
                }
            }
            Ok(())
        }
        "list" => {
            let (_, flags) = split_prompt_and_flags(rest);
            let limit: usize = flags
                .get("--limit")
                .and_then(|s| s.parse().ok())
                .unwrap_or(30);
            let items = manager.store().list(limit)?;
            if json_mode {
                println!("{}", serde_json::to_string_pretty(&items)?);
            } else {
                print_gallery_table(&items);
            }
            Ok(())
        }
        "verify" => {
            let path_arg = rest
                .iter()
                .find(|a| !a.starts_with("--"))
                .ok_or("Usage : hakille-daemon media verify <chemin_artefact> [--json]")?;
            let verifying_key = manager.verifying_key();
            let report = verify_artifact_provenance(Path::new(path_arg), Some(&verifying_key))?;
            if json_mode {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("Vérification cryptographique Ed25519 : VALIDE");
                println!("  Artefact      : {}", report.artifact_path.display());
                println!("  Sidecar       : {}", report.provenance_path.display());
                println!("  SHA-256       : {}", report.content_sha256);
                println!(
                    "  Moteur        : {} ({})",
                    report.engine_id,
                    report.engine_kind.badge_fr()
                );
                println!("  Divulgation IA: {}", report.synthetic_ai_disclosure);
                println!(
                    "  Chunk PNG iTXt: {}",
                    if report.embedded_png_chunk_checked {
                        "vérifié"
                    } else {
                        "n/a"
                    }
                );
                println!("  Clé signataire: {}", report.signer_public_key_hex);
            }
            Ok(())
        }
        other => Err(format!(
            "sous-commande media inconnue '{other}'. Lancez `hakille-daemon media help`."
        )
        .into()),
    }
}

pub fn print_engines_table(engines: &[EngineDescriptor], ffmpeg_available: bool) {
    println!(
        "Moteurs génératifs multimodaux (muxer ffmpeg : {}) :",
        if ffmpeg_available {
            "détecté (MP4/WebM disponibles)"
        } else {
            "absent (APNG/Y4M/PNG-seq Rust pur disponibles)"
        }
    );
    for e in engines {
        let detail = match &e.availability {
            EngineAvailability::Ready { device, .. } => device.as_str(),
            EngineAvailability::ModelMissing { hint, .. } => hint.as_str(),
            EngineAvailability::Unconfigured { env_hint } => env_hint.as_str(),
            EngineAvailability::Unavailable { reason } => reason.as_str(),
        };
        println!(
            "  - {:<24} | {:<6} | {:<26} | {:<13} | {}",
            e.id,
            format!("{:?}", e.modality),
            e.kind.badge_fr(),
            e.availability.status_label(),
            detail
        );
    }
}

pub fn print_gallery_table(items: &[MediaArtifact]) {
    if items.is_empty() {
        println!("Galerie média vide (aucun artefact généré).");
        return;
    }
    println!(
        "Galerie média souveraine ({} artefact(s) récent(s)) :",
        items.len()
    );
    for a in items {
        println!(
            "  • [{}] {:?} | {} ({}) | {} octets | sha256:{}… | {}",
            &a.id.to_string()[..8],
            a.modality,
            a.engine_id,
            a.engine_kind.badge_fr(),
            a.bytes_len,
            &a.sha256[..a.sha256.len().min(12)],
            a.path.display()
        );
    }
}

fn print_artifact_result(
    art: &MediaArtifact,
    json_mode: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if json_mode {
        println!("{}", serde_json::to_string_pretty(art)?);
    } else {
        println!("Artefact généré avec succès (signé Ed25519) :");
        println!("  ID         : {}", art.id);
        println!("  Modalité   : {:?}", art.modality);
        println!(
            "  Moteur     : {} [{}]",
            art.engine_id,
            art.engine_kind.badge_fr()
        );
        println!("  Sortie     : {}", format_data_exit(&art.data_exit));
        println!("  Fichier    : {}", art.path.display());
        println!("  Provenance : {}", art.provenance_path.display());
        if let Some(vp) = &art.visemes_path {
            println!("  Visèmes    : {}", vp.display());
        }
        println!("  SHA-256    : {}", art.sha256);
        println!(
            "  Taille     : {} octets | Durée : {} ms",
            art.bytes_len, art.elapsed_ms
        );
    }
    Ok(())
}

fn print_media_cli_help() {
    println!("Hakille — Moteur Génératif Multimodal Souverain (ADR-024)");
    println!("Usage: hakille-daemon media <SUBCOMMAND> [OPTIONS]");
    println!();
    println!("Subcommands:");
    println!("  engines [--json]                 Inventaire honnête des moteurs Image/Audio/Vidéo");
    println!("  image <prompt> [OPTIONS]         Génère une image signée Ed25519 (PNG/JPG + iTXt)");
    println!(
        "  audio <prompt> [OPTIONS]         Génère audio WAV 16-bit (speech|music|soundscape|foley)"
    );
    println!("  video <prompt> [OPTIONS]         Génère vidéo (apng|y4m|frames|mp4|webm)");
    println!(
        "  compose <prompt> [OPTIONS]       Exécute un DAG Storyboard complet (image+voix+ambiance+vidéo+mux)"
    );
    println!("  list [--limit N] [--json]        Liste la galerie locale content-addressed");
    println!(
        "  verify <path> [--json]           Vérifie SHA-256, signature Ed25519 et chunk PNG iTXt"
    );
}

fn split_prompt_and_flags(args: &[String]) -> (String, std::collections::HashMap<String, String>) {
    let mut prompt_parts = Vec::new();
    let mut flags = std::collections::HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let tok = &args[i];
        if tok == "--json" || tok == "--no-mux" {
            i += 1;
            continue;
        }
        if tok.starts_with("--") && i + 1 < args.len() {
            flags.insert(tok.clone(), args[i + 1].clone());
            i += 2;
            continue;
        }
        prompt_parts.push(tok.as_str());
        i += 1;
    }
    (prompt_parts.join(" "), flags)
}
