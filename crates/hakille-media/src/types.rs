//! Contrats typés du Moteur Génératif Multimodal Souverain (`hakille-media`).
//!
//! Chaque moteur déclare explicitement son [`EngineKind`] et sa [`EngineAvailability`]
//! afin qu'aucune synthèse algorithmique ou distante ne soit confondue avec une
//! inférence neurale locale.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use uuid::Uuid;

/// Modalité média principale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaModality {
    Image,
    Video,
    Audio,
    Composite,
}

impl MediaModality {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Composite => "composite",
        }
    }
}

/// Nature réelle du moteur d'exécution — contrat d'honnêteté non négociable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    /// Inférence neurale locale 100 % Rust via Candle (`candle-transformers`).
    NeuralLocalCandle,
    /// Inférence neurale locale via ONNX Runtime (`ort`).
    NeuralLocalOnnx,
    /// Serveur d'inférence local explicite sur boucle locale (`127.0.0.1` / `[::1]`).
    NeuralLoopbackServer,
    /// API distante HTTPS opt-in (requiert Biscuit `Network` + `MediaGeneration` + `DataExit::Cloud`).
    NeuralCloudApi,
    /// Synthèse vocale formantique réelle (Klatt 24 kHz, `hakille-voice::FormantTts`).
    FormantDsp,
    /// Synthèse algorithmique/DSP déterministe pure Rust (jamais présentée comme neurale).
    ProceduralDsp,
}

impl EngineKind {
    pub fn is_neural(&self) -> bool {
        matches!(
            self,
            Self::NeuralLocalCandle
                | Self::NeuralLocalOnnx
                | Self::NeuralLoopbackServer
                | Self::NeuralCloudApi
        )
    }

    pub fn is_local(&self) -> bool {
        !matches!(self, Self::NeuralCloudApi)
    }

    pub fn badge_fr(&self) -> &'static str {
        match self {
            Self::NeuralLocalCandle => "NEURAL LOCAL (Candle)",
            Self::NeuralLocalOnnx => "NEURAL LOCAL (ONNX)",
            Self::NeuralLoopbackServer => "NEURAL LOOPBACK (127.0.0.1)",
            Self::NeuralCloudApi => "NEURAL CLOUD (HTTPS)",
            Self::FormantDsp => "FORMANT DSP (Klatt 24 kHz)",
            Self::ProceduralDsp => "PROCEDURAL DSP (Rust pur)",
        }
    }
}

/// Destination des données pour une génération donnée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MediaDataExit {
    Local,
    Loopback { endpoint: String },
    Cloud { provider: String, endpoint: String },
}

impl MediaDataExit {
    pub fn is_cloud(&self) -> bool {
        matches!(self, Self::Cloud { .. })
    }
}

/// Disponibilité réelle d'un moteur à l'instant T.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EngineAvailability {
    Ready {
        device: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model_sha256: Option<String>,
    },
    ModelMissing {
        model_id: String,
        missing_files: Vec<String>,
        hint: String,
    },
    Unconfigured {
        env_hint: String,
    },
    Unavailable {
        reason: String,
    },
}

impl EngineAvailability {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    pub fn status_label(&self) -> &'static str {
        match self {
            Self::Ready { .. } => "PRÊT",
            Self::ModelMissing { .. } => "MODÈLE ABSENT",
            Self::Unconfigured { .. } => "NON CONFIGURÉ",
            Self::Unavailable { .. } => "INDISPONIBLE",
        }
    }
}

/// Fiche honnête d'un moteur exposée à la CLI, au REPL et au HUD F5.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineDescriptor {
    pub id: String,
    pub name: String,
    pub modality: MediaModality,
    pub kind: EngineKind,
    pub data_exit: MediaDataExit,
    pub availability: EngineAvailability,
    pub estimated_ram_mb: u64,
    pub estimated_vram_mb: u64,
    pub capabilities: Vec<String>,
    pub description: String,
}

/// Format de sortie image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    #[default]
    Png,
    Jpeg,
}

impl ImageFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }
}

/// Mode de génération image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ImageGenMode {
    #[default]
    TextToImage,
    ImageToImage {
        init_image_path: PathBuf,
        strength: f32,
    },
    Inpaint {
        init_image_path: PathBuf,
        mask_path: PathBuf,
        strength: f32,
    },
}

/// Ordonnanceur de débruitage pour la diffusion latente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DiffusionScheduler {
    #[default]
    Ddim,
    EulerAncestral,
    UniPc,
    RectifiedFlow,
}

/// Requête complète de génération d'image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageGenRequest {
    pub prompt: String,
    #[serde(default)]
    pub negative_prompt: Option<String>,
    #[serde(default = "default_img_width")]
    pub width: u32,
    #[serde(default = "default_img_height")]
    pub height: u32,
    #[serde(default = "default_steps")]
    pub steps: u32,
    #[serde(default = "default_guidance")]
    pub guidance_scale: f32,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub format: ImageFormat,
    #[serde(default)]
    pub mode: ImageGenMode,
    #[serde(default)]
    pub scheduler: DiffusionScheduler,
    /// Sélecteur explicite (`auto`, `sd-candle`, `flux-candle`, `onnx-diffusion`,
    /// `procedural-dsp`, `loopback`, `cloud`).
    #[serde(default = "default_engine_selector")]
    pub engine: String,
    #[serde(default)]
    pub style_preset: Option<String>,
    #[serde(default = "default_deadline_ms")]
    pub deadline_ms: u64,
}

fn default_img_width() -> u32 {
    512
}
fn default_img_height() -> u32 {
    512
}
fn default_steps() -> u32 {
    20
}
fn default_guidance() -> f32 {
    7.5
}
fn default_engine_selector() -> String {
    "auto".to_string()
}
fn default_deadline_ms() -> u64 {
    180_000
}

impl ImageGenRequest {
    pub fn simple(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            negative_prompt: None,
            width: default_img_width(),
            height: default_img_height(),
            steps: default_steps(),
            guidance_scale: default_guidance(),
            seed: None,
            format: ImageFormat::Png,
            mode: ImageGenMode::TextToImage,
            scheduler: DiffusionScheduler::Ddim,
            engine: default_engine_selector(),
            style_preset: None,
            deadline_ms: default_deadline_ms(),
        }
    }
}

/// Format de sortie audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AudioFormat {
    #[default]
    WavPcm16,
    WavF32,
}

impl AudioFormat {
    pub fn extension(&self) -> &'static str {
        "wav"
    }

    pub fn mime_type(&self) -> &'static str {
        "audio/wav"
    }
}

/// Mode de synthèse audio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum AudioGenMode {
    Speech {
        #[serde(default = "default_voice")]
        voice: String,
        #[serde(default = "default_lang")]
        language: String,
        #[serde(default = "default_speed")]
        speed: f32,
        #[serde(default = "default_emotion")]
        emotion: String,
        /// Description vocale optionnelle pour Parler-TTS (ex. "warm calm French voice").
        #[serde(default)]
        style_description: Option<String>,
    },
    Music {
        #[serde(default = "default_bpm")]
        bpm: u16,
        #[serde(default = "default_mood")]
        mood: String,
    },
    Soundscape {
        #[serde(default = "default_env")]
        environment: String,
    },
    Foley {
        #[serde(default = "default_foley")]
        event: String,
    },
}

fn default_voice() -> String {
    "hakille-neutre".into()
}
fn default_lang() -> String {
    "fr".into()
}
fn default_speed() -> f32 {
    1.0
}
fn default_emotion() -> String {
    "calm".into()
}
fn default_bpm() -> u16 {
    92
}
fn default_mood() -> String {
    "ambient".into()
}
fn default_env() -> String {
    "night_rain".into()
}
fn default_foley() -> String {
    "chime".into()
}

impl Default for AudioGenMode {
    fn default() -> Self {
        Self::Speech {
            voice: default_voice(),
            language: default_lang(),
            speed: default_speed(),
            emotion: default_emotion(),
            style_description: None,
        }
    }
}

/// Requête complète de génération audio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioGenRequest {
    pub prompt: String,
    #[serde(default)]
    pub mode: AudioGenMode,
    #[serde(default = "default_audio_duration")]
    pub duration_secs: f32,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    #[serde(default = "default_channels")]
    pub channels: u16,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub format: AudioFormat,
    /// Sélecteur explicite (`auto`, `parler-tts-candle`, `onnx-tts`,
    /// `formant-dsp`, `procedural-dsp`, `loopback`, `cloud`).
    #[serde(default = "default_engine_selector")]
    pub engine: String,
    /// Exporte la timeline de visèmes phonémiques lorsque le moteur la produit.
    #[serde(default = "default_true")]
    pub emit_visemes: bool,
    #[serde(default = "default_deadline_ms")]
    pub deadline_ms: u64,
}

fn default_audio_duration() -> f32 {
    4.0
}
fn default_sample_rate() -> u32 {
    24_000
}
fn default_channels() -> u16 {
    1
}
fn default_true() -> bool {
    true
}

impl AudioGenRequest {
    pub fn speech(text: impl Into<String>) -> Self {
        Self {
            prompt: text.into(),
            mode: AudioGenMode::default(),
            duration_secs: default_audio_duration(),
            sample_rate: 24_000,
            channels: 1,
            seed: None,
            format: AudioFormat::WavPcm16,
            engine: default_engine_selector(),
            emit_visemes: true,
            deadline_ms: default_deadline_ms(),
        }
    }

    pub fn soundscape(prompt: impl Into<String>, duration_secs: f32) -> Self {
        let p = prompt.into();
        Self {
            mode: AudioGenMode::Soundscape {
                environment: p.clone(),
            },
            prompt: p,
            duration_secs,
            sample_rate: 24_000,
            channels: 2,
            seed: None,
            format: AudioFormat::WavPcm16,
            engine: default_engine_selector(),
            emit_visemes: false,
            deadline_ms: default_deadline_ms(),
        }
    }

    pub fn music(prompt: impl Into<String>, duration_secs: f32, bpm: u16) -> Self {
        let p = prompt.into();
        Self {
            mode: AudioGenMode::Music {
                bpm,
                mood: p.clone(),
            },
            prompt: p,
            duration_secs,
            sample_rate: 24_000,
            channels: 2,
            seed: None,
            format: AudioFormat::WavPcm16,
            engine: default_engine_selector(),
            emit_visemes: false,
            deadline_ms: default_deadline_ms(),
        }
    }
}

/// Profil de mouvement caméra virtuel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CameraMotion {
    Static,
    PanLeft,
    PanRight,
    ZoomIn,
    ZoomOut,
    Orbit,
    #[default]
    KenBurns,
}

impl CameraMotion {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "static" => Some(Self::Static),
            "pan-left" | "pan_left" => Some(Self::PanLeft),
            "pan-right" | "pan_right" => Some(Self::PanRight),
            "zoom-in" | "zoom_in" => Some(Self::ZoomIn),
            "zoom-out" | "zoom_out" => Some(Self::ZoomOut),
            "orbit" => Some(Self::Orbit),
            "kenburns" | "ken-burns" | "ken_burns" => Some(Self::KenBurns),
            _ => None,
        }
    }
}

/// Conteneur de sortie vidéo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VideoContainer {
    /// Animated PNG 100 % Rust pur (zéro dépendance externe).
    #[default]
    Apng,
    /// Flux YUV4MPEG2 (BT.709 4:2:0) 100 % Rust pur.
    Y4m,
    /// Séquence de frames PNG numérotées + `sequence.json`.
    FrameSequence,
    /// MP4 H.264 + AAC via `FfmpegMuxer` explicite.
    Mp4H264,
    /// WebM VP9 + Opus via `FfmpegMuxer` explicite.
    WebmVp9,
}

impl VideoContainer {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Apng => "apng",
            Self::Y4m => "y4m",
            Self::FrameSequence => "frames",
            Self::Mp4H264 => "mp4",
            Self::WebmVp9 => "webm",
        }
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Apng => "image/apng",
            Self::Y4m => "video/x-yuv4mpeg",
            Self::FrameSequence => "application/vnd.hakille.frame-sequence+json",
            Self::Mp4H264 => "video/mp4",
            Self::WebmVp9 => "video/webm",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "apng" => Some(Self::Apng),
            "y4m" => Some(Self::Y4m),
            "frames" | "frame-sequence" | "frame_sequence" => Some(Self::FrameSequence),
            "mp4" | "mp4-h264" | "h264" => Some(Self::Mp4H264),
            "webm" | "webm-vp9" | "vp9" => Some(Self::WebmVp9),
            _ => None,
        }
    }
}

/// Spécification d'un keyframe pour la génération vidéo guidée.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyframeSpec {
    pub prompt: String,
    pub frame_index: u32,
}

/// Mode de génération vidéo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum VideoGenMode {
    #[default]
    TextToVideo,
    ImageToVideo {
        init_image_path: PathBuf,
    },
    KeyframeInterpolation {
        keyframes: Vec<KeyframeSpec>,
    },
    AudioReactive {
        audio_wav_path: PathBuf,
    },
}

/// Requête complète de génération vidéo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoGenRequest {
    pub prompt: String,
    #[serde(default)]
    pub negative_prompt: Option<String>,
    #[serde(default = "default_vid_width")]
    pub width: u32,
    #[serde(default = "default_vid_height")]
    pub height: u32,
    #[serde(default = "default_fps")]
    pub fps: u16,
    #[serde(default = "default_num_frames")]
    pub num_frames: u32,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub mode: VideoGenMode,
    #[serde(default)]
    pub camera_motion: CameraMotion,
    #[serde(default)]
    pub container: VideoContainer,
    #[serde(default)]
    pub audio_track: Option<Box<AudioGenRequest>>,
    /// Sélecteur explicite (`auto`, `onnx-video`, `candle-keyframe`,
    /// `procedural-dsp`, `loopback`, `cloud`).
    #[serde(default = "default_engine_selector")]
    pub engine: String,
    #[serde(default = "default_deadline_ms")]
    pub deadline_ms: u64,
}

fn default_vid_width() -> u32 {
    320
}
fn default_vid_height() -> u32 {
    180
}
fn default_fps() -> u16 {
    12
}
fn default_num_frames() -> u32 {
    24
}

impl VideoGenRequest {
    pub fn simple(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            negative_prompt: None,
            width: default_vid_width(),
            height: default_vid_height(),
            fps: default_fps(),
            num_frames: default_num_frames(),
            seed: None,
            mode: VideoGenMode::TextToVideo,
            camera_motion: CameraMotion::KenBurns,
            container: VideoContainer::Apng,
            audio_track: None,
            engine: default_engine_selector(),
            deadline_ms: default_deadline_ms(),
        }
    }

    pub fn duration_secs(&self) -> f32 {
        if self.fps == 0 {
            0.0
        } else {
            self.num_frames as f32 / self.fps as f32
        }
    }
}

/// Union typée d'une requête média unitaire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MediaRequest {
    Image(ImageGenRequest),
    Audio(AudioGenRequest),
    Video(VideoGenRequest),
}

impl MediaRequest {
    pub fn modality(&self) -> MediaModality {
        match self {
            Self::Image(_) => MediaModality::Image,
            Self::Audio(_) => MediaModality::Audio,
            Self::Video(_) => MediaModality::Video,
        }
    }

    pub fn prompt(&self) -> &str {
        match self {
            Self::Image(r) => &r.prompt,
            Self::Audio(r) => &r.prompt,
            Self::Video(r) => &r.prompt,
        }
    }

    pub fn engine_selector(&self) -> &str {
        match self {
            Self::Image(r) => &r.engine,
            Self::Audio(r) => &r.engine,
            Self::Video(r) => &r.engine,
        }
    }

    pub fn deadline_ms(&self) -> u64 {
        match self {
            Self::Image(r) => r.deadline_ms,
            Self::Audio(r) => r.deadline_ms,
            Self::Video(r) => r.deadline_ms,
        }
    }
}

/// Vignette RGBA8 bornée pour affichage direct dans le HUD Desktop (`F5`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewImage {
    pub width: u32,
    pub height: u32,
    /// Pixels RGBA8 (`width * height * 4` octets).
    pub rgba: Vec<u8>,
}

/// Métadonnées techniques d'un artefact média généré et stocké.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaArtifact {
    pub id: Uuid,
    pub modality: MediaModality,
    pub path: PathBuf,
    pub mime_type: String,
    pub sha256: String,
    pub bytes_len: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_secs: Option<f32>,
    pub fps: Option<u16>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub seed: u64,
    pub prompt_summary: String,
    pub engine_id: String,
    pub engine_kind: EngineKind,
    pub data_exit: MediaDataExit,
    pub model_id: Option<String>,
    pub provenance_path: PathBuf,
    pub provenance_signature_hex: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visemes_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parent_artifacts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<PreviewImage>,
    pub created_at: DateTime<Utc>,
    pub elapsed_ms: u64,
}

/// Événement de progression diffusé en temps réel (REPL, CLI, IPC HUD).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaProgressEvent {
    pub job_id: String,
    pub modality: MediaModality,
    pub stage: String,
    pub step: u32,
    pub total_steps: u32,
    pub fraction: f32,
    pub elapsed_ms: u64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<PreviewImage>,
}

/// Contrôle coopératif d'annulation + échéance dure partagé par tous les moteurs.
#[derive(Clone, Debug)]
pub struct GenerationControl {
    pub job_id: String,
    cancel: Arc<AtomicBool>,
    started_at: Instant,
    deadline_ms: u64,
}

impl GenerationControl {
    pub fn new(job_id: impl Into<String>, deadline_ms: u64) -> Self {
        Self {
            job_id: job_id.into(),
            cancel: Arc::new(AtomicBool::new(false)),
            started_at: Instant::now(),
            deadline_ms,
        }
    }

    pub fn with_cancel_flag(
        job_id: impl Into<String>,
        cancel: Arc<AtomicBool>,
        deadline_ms: u64,
    ) -> Self {
        Self {
            job_id: job_id.into(),
            cancel,
            started_at: Instant::now(),
            deadline_ms,
        }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }

    pub fn check(&self) -> crate::error::MediaResult<()> {
        if self.is_cancelled() {
            return Err(crate::error::MediaError::Cancelled {
                job_id: self.job_id.clone(),
            });
        }
        let elapsed = self.elapsed_ms();
        if self.deadline_ms > 0 && elapsed > self.deadline_ms {
            return Err(crate::error::MediaError::DeadlineExceeded {
                elapsed_ms: elapsed,
                budget_ms: self.deadline_ms,
            });
        }
        Ok(())
    }
}

/// Puits de progression appelé entre chaque étape/frame.
pub type ProgressCallback = Arc<dyn Fn(MediaProgressEvent) + Send + Sync>;

/// Graine déterministe dérivée d'un prompt si l'utilisateur n'en fixe pas.
pub fn derive_seed(prompt: &str, explicit: Option<u64>) -> u64 {
    if let Some(seed) = explicit {
        return seed;
    }
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(prompt.as_bytes());
    u64::from_le_bytes(digest[..8].try_into().expect("sha256 >= 8 octets"))
}
