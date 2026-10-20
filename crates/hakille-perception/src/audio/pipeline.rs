//! Pipeline voix complet : chaînage VAD réel → état de parole → transcription ASR.
//!
//! Chaîne effective (aucune brique simulée) :
//! 1. `push_audio` fragmente le flux en blocs de 10 ms (@16 kHz mono) ;
//! 2. le VAD [`hakille_inference::OrtVadSession`] (Silero, `models/silero_vad.onnx`)
//!    score les fenêtres 512 réelles — modèle absent à la construction ->
//!    [`VoicePipelineError::ModelMissing`] LOUD, jamais de faux VAD ;
//! 3. machine d'état `Idle -> Listening -> Transcribing` avec hangover ;
//! 4. wake word : détecteur EXISTANT [`WakeWordDetector::process_gated`]
//!    (chemin énergétique VAD-gaté — **openWakeWord n'est pas provisionné**,
//!    voir `wake_word.rs` : aucun checkpoint `openwakeword_*.onnx` dans
//!    `models/`, l'auto-bascule `ort` s'active si l'utilisateur en provisionne un) ;
//! 5. ASR : délégation à [`SenseVoiceSmall`] (SenseVoice-Small ONNX réel)
//!    uniquement si `sensevoice_small.int8.onnx` + `tokens.txt` sont présents ;
//!    sinon `push_speech_transcript()` retourne SANS événement `Transcript`
//!    avec un warning clair — jamais de texte inventé.
//!
//! L'audio finalisé (`SpeechEnd`) peut être récupéré via
//! [`VoicePipeline::last_utterance`] ou dirigé vers CAM++ via
//! [`VoicePipeline::set_embedding_hook`] (biométrie locuteur, facultative).

use crate::audio::asr::{AsrEngine, SenseVoiceSmall};
use crate::audio::capture::AudioChunk;
use crate::audio::wake_word::{WakeBackendKind, WakeWordDetector};
use hakille_inference::{OrtVadSession, resolve_vad_model_path};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Nom de fichier du modèle VAD (Silero v5) exigé par le pipeline.
pub const PIPELINE_VAD_FILENAME: &str = "silero_vad.onnx";
/// Fréquence d'échantillonnage exigée en entrée du pipeline.
pub const PIPELINE_SAMPLE_RATE: u32 = 16_000;
/// Taille d'un bloc interne : 10 ms @16 kHz.
pub const PIPELINE_BLOCK_SAMPLES: usize = 160;

/// Erreur du pipeline voix — chaque échec est EXPLICITE, jamais compensé par
/// une heuristique silencieuse.
#[derive(Debug, thiserror::Error)]
pub enum VoicePipelineError {
    /// Modèle VAD introuvable au chemin attendu : pas de pipeline (aucun faux VAD).
    #[error(
        "VoicePipeline: modèle VAD absent '{path}' — provisionnez silero_vad.onnx \
         (aucun faux VAD : le pipeline exige le vrai modèle Silero)"
    )]
    ModelMissing { path: PathBuf },
    /// Configuration invalide (champ + raison).
    #[error("VoicePipeline: configuration invalide ({field}) : {message}")]
    Config {
        field: &'static str,
        message: String,
    },
    /// Échec d'inférence à la construction (session `ort` impossible).
    #[error("VoicePipeline: inférence impossible : {message}")]
    Inference { message: String },
}

/// Événement émis par le pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VoiceEvent {
    /// Mot-clé détecté (détecteur VAD-gaté existant ; openWakeWord non provisionné).
    Wake { confidence: f32 },
    /// Début de parole confirmé par le VAD réel (prob >= seuil).
    SpeechStart,
    /// Fin de parole (hangover épuisé) — l'énoncé est prêt pour transcription.
    SpeechEnd,
    /// Transcription réelle (SenseVoice-Small) de l'énoncé finalisé.
    Transcript { text: String },
    /// Erreur runtime (VAD/wake) — émise au plus une fois par sous-système.
    Error(String),
}

/// État observable de la machine du pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelineState {
    /// Aucune parole en cours.
    Idle,
    /// Parole en cours (VAD actif, énoncé accumulé).
    Listening,
    /// Énoncé finalisé, en attente de [`VoicePipeline::push_speech_transcript`].
    Transcribing,
}

impl PipelineState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Listening => "Listening",
            Self::Transcribing => "Transcribing",
        }
    }
}

/// Configuration du pipeline (valeurs par défaut documentées).
#[derive(Debug, Clone)]
pub struct VoicePipelineConfig {
    /// Dossier modèles pour le VAD. `Some` : fait foi (absent -> `ModelMissing`,
    /// pas de recherche ailleurs). `None` : résolution standard
    /// (`$HAKILLE_MODELS_DIR`, `models/`, `../models/`, `../../models/`).
    pub models_dir: Option<PathBuf>,
    /// Seuil de probabilité Silero pour déclarer la parole (défaut 0.5).
    pub vad_threshold: f32,
    /// Hangover en fenêtres VAD de 512 échantillons (~32 ms) après la dernière
    /// fenêtre chaude avant `SpeechEnd` (défaut 3 ≈ 96 ms).
    pub vad_hangover_windows: usize,
    /// Mot-clé de réveil (`None` = wake désactivé). Défaut `"Dis Hakille"`.
    pub wake_word: Option<String>,
    /// Seuil de confiance du détecteur de wake (défaut 0.75).
    pub wake_threshold: f32,
    /// Pré-roll conservé avant `SpeechStart` (ms) pour ne pas tronquer l'onset
    /// (défaut 200 ms).
    pub pre_roll_ms: u32,
    /// Durée maximale d'un énoncé en cours (s) avant `SpeechEnd` forcé
    /// (défaut 30 s).
    pub max_utterance_seconds: f32,
    /// Active l'ASR (SenseVoice-Small) si les fichiers modèles sont présents.
    pub enable_asr: bool,
    /// Dossier modèles contenant `sensevoice_small.int8.onnx` + `tokens.txt`
    /// (`None` = résolution standard).
    pub asr_models_dir: Option<PathBuf>,
}

impl Default for VoicePipelineConfig {
    fn default() -> Self {
        Self {
            models_dir: None,
            vad_threshold: 0.5,
            vad_hangover_windows: 3,
            wake_word: Some("Dis Hakille".to_string()),
            wake_threshold: 0.75,
            pre_roll_ms: 200,
            max_utterance_seconds: 30.0,
            enable_asr: true,
            asr_models_dir: None,
        }
    }
}

/// Hook de segmentation pour biométrie locuteur (CAM++ `SpeakerIdentifier`).
///
/// Appelé de façon SYNCHRONE à `SpeechEnd` avec l'énoncé 16 kHz mono finalisé.
/// Pour CAM++ (API async), l'implémentation déléguée peut déposer les samples
/// dans un canal ou lancer une tâche courte — l'appel ne doit pas bloquer
/// durablement le thread d'alimentation audio.
pub trait EmbeddingHook: Send + Sync {
    fn on_speech_segment(&self, samples_16k_mono: &[f32], duration_seconds: f32);
}

impl<F> EmbeddingHook for F
where
    F: Fn(&[f32], f32) + Send + Sync,
{
    fn on_speech_segment(&self, samples_16k_mono: &[f32], duration_seconds: f32) {
        self(samples_16k_mono, duration_seconds);
    }
}

/// Pipeline voix complet (VAD réel + wake VAD-gaté + ASR réel optionnel).
pub struct VoicePipeline {
    config: VoicePipelineConfig,
    vad: OrtVadSession,
    wake: Option<WakeWordDetector>,
    asr: Option<SenseVoiceSmall>,
    /// Échantillons en attente de complétion d'un bloc de 10 ms.
    pending: VecDeque<f32>,
    /// Tampon circulaire des derniers échantillons (pré-roll + énoncé en cours).
    rolling: VecDeque<f32>,
    /// Capacité du tampon circulaire (échantillons) = pré-roll + max énoncé.
    rolling_cap: usize,
    /// Total d'échantillons entrés dans `rolling` (index absolu).
    total_pushed: u64,
    /// Index absolu de début de l'énoncé en cours (pré-roll inclus).
    utterance_start_abs: Option<u64>,
    /// Dernier énoncé finalisé (disponible pour ASR/CAM++).
    last_utterance: Option<Vec<f32>>,
    state: PipelineState,
    hangover_left: usize,
    frames_processed: u64,
    events_emitted: u64,
    vad_error_reported: bool,
    wake_error_reported: bool,
    embedding_hook: Option<Arc<dyn EmbeddingHook>>,
}

impl VoicePipeline {
    /// Construit le pipeline : charge le VAD ONNX réel si le fichier existe.
    ///
    /// - `config.models_dir = Some(dir)` : `dir/silero_vad.onnx` fait foi ;
    ///   absent -> [`VoicePipelineError::ModelMissing`].
    /// - `config.models_dir = None` : résolution standard ; si aucun candidat
    ///   n'existe -> [`VoicePipelineError::ModelMissing`] avec le chemin
    ///   canonique `models/silero_vad.onnx`.
    /// - Wake et ASR sont optionnels : leur indisponibilité n'échoue PAS la
    ///   construction (wake : VAD partagé du même dossier ; ASR : warning +
    ///   transcripts absents, aucun texte inventé).
    pub fn new(config: VoicePipelineConfig) -> Result<Self, VoicePipelineError> {
        Self::validate_config(&config)?;

        let vad_path = Self::resolve_vad_path(config.models_dir.as_deref())?;
        let vad = OrtVadSession::open(&vad_path).map_err(|e| VoicePipelineError::Inference {
            message: format!("ouverture VAD '{}' : {e}", vad_path.display()),
        })?;

        let wake = match &config.wake_word {
            Some(keyword) => {
                let detector = match config.models_dir.as_deref() {
                    Some(dir) => WakeWordDetector::with_models_dir(
                        keyword.clone(),
                        config.wake_threshold,
                        dir,
                    )
                    .map_err(|e| VoicePipelineError::Inference {
                        message: format!("construction wake detector : {e}"),
                    })?,
                    None => WakeWordDetector::new(keyword.clone(), config.wake_threshold),
                };
                Some(detector)
            }
            None => None,
        };

        let asr = if config.enable_asr {
            match Self::resolve_asr_dir(config.asr_models_dir.as_deref()) {
                Some(dir) => {
                    tracing::info!(
                        dir = %dir.display(),
                        "VoicePipeline: ASR SenseVoice-Small réel disponible"
                    );
                    Some(SenseVoiceSmall::with_models_dir(&dir))
                }
                None => {
                    tracing::warn!(
                        "VoicePipeline: ASR indisponible — \
                         sensevoice_small.int8.onnx + tokens.txt introuvables ; \
                         push_speech_transcript() n'émettra PAS de Transcript \
                         (aucun texte inventé)"
                    );
                    None
                }
            }
        } else {
            None
        };

        tracing::info!(
            vad = %vad_path.display(),
            threshold = config.vad_threshold,
            hangover_windows = config.vad_hangover_windows,
            wake = config.wake_word.as_deref().unwrap_or("<désactivé>"),
            wake_backend = wake
                .as_ref()
                .map(|w| w.backend_kind().label())
                .unwrap_or("<aucun>"),
            asr = asr.is_some(),
            "VoicePipeline: prêt (VAD réel chargé)"
        );

        let pre_roll_samples = (config.pre_roll_ms as usize * PIPELINE_SAMPLE_RATE as usize) / 1000;
        let max_utterance_samples =
            (config.max_utterance_seconds * PIPELINE_SAMPLE_RATE as f32) as usize;
        let rolling_cap = pre_roll_samples
            .saturating_add(max_utterance_samples)
            .max(PIPELINE_BLOCK_SAMPLES);

        Ok(Self {
            config,
            vad,
            wake,
            asr,
            pending: VecDeque::new(),
            rolling: VecDeque::with_capacity(rolling_cap),
            rolling_cap,
            total_pushed: 0,
            utterance_start_abs: None,
            last_utterance: None,
            state: PipelineState::Idle,
            hangover_left: 0,
            frames_processed: 0,
            events_emitted: 0,
            vad_error_reported: false,
            wake_error_reported: false,
            embedding_hook: None,
        })
    }

    fn validate_config(config: &VoicePipelineConfig) -> Result<(), VoicePipelineError> {
        let bad = |field: &'static str, message: String| {
            Err(VoicePipelineError::Config { field, message })
        };
        if !config.vad_threshold.is_finite()
            || config.vad_threshold <= 0.0
            || config.vad_threshold >= 1.0
        {
            return bad(
                "vad_threshold",
                format!(
                    "seuil invalide {} (attendu dans (0, 1))",
                    config.vad_threshold
                ),
            );
        }
        if config.vad_hangover_windows == 0 {
            return bad(
                "vad_hangover_windows",
                "hangover nul : SpeechEnd immédiat à la première fenêtre sous le seuil \
                 (attendu >= 1)"
                    .to_string(),
            );
        }
        if let Some(keyword) = &config.wake_word
            && keyword.trim().is_empty()
        {
            return bad(
                "wake_word",
                "mot-clé vide (utiliser None pour désactiver)".to_string(),
            );
        }
        if !config.wake_threshold.is_finite() || !(0.0..=1.0).contains(&config.wake_threshold) {
            return bad(
                "wake_threshold",
                format!(
                    "seuil invalide {} (attendu dans [0, 1])",
                    config.wake_threshold
                ),
            );
        }
        if (config.pre_roll_ms as usize) > 5_000 {
            return bad(
                "pre_roll_ms",
                format!("pré-roll {} ms > 5000 ms", config.pre_roll_ms),
            );
        }
        if !config.max_utterance_seconds.is_finite()
            || config.max_utterance_seconds < 0.1
            || config.max_utterance_seconds > 120.0
        {
            return bad(
                "max_utterance_seconds",
                format!(
                    "durée invalide {} s (attendu dans [0.1, 120])",
                    config.max_utterance_seconds
                ),
            );
        }
        Ok(())
    }

    /// Résout `silero_vad.onnx` — erreur `ModelMissing` EXPLICITE si absent.
    fn resolve_vad_path(models_dir: Option<&Path>) -> Result<PathBuf, VoicePipelineError> {
        if let Some(dir) = models_dir {
            let path = dir.join(PIPELINE_VAD_FILENAME);
            if path.exists() {
                return Ok(path);
            }
            return Err(VoicePipelineError::ModelMissing { path });
        }
        match resolve_vad_model_path(None) {
            Ok(p) => Ok(p),
            Err(e) => {
                tracing::error!(error = %e, "VoicePipeline: résolution VAD par défaut échouée");
                Err(VoicePipelineError::ModelMissing {
                    path: PathBuf::from("models").join(PIPELINE_VAD_FILENAME),
                })
            }
        }
    }

    /// Résout le dossier ASR réel (`sensevoice_small.int8.onnx` + `tokens.txt`).
    /// `None` si absent — PAS une erreur (l'ASR est optionnel par contrat).
    fn resolve_asr_dir(models_dir: Option<&Path>) -> Option<PathBuf> {
        const MODEL: &str = "sensevoice_small.int8.onnx";
        const TOKENS: &str = "tokens.txt";
        if let Some(dir) = models_dir {
            let ok = dir.join(MODEL).exists() && dir.join(TOKENS).exists();
            return if ok { Some(dir.to_path_buf()) } else { None };
        }
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(dir) = std::env::var_os("HAKILLE_MODELS_DIR") {
            candidates.push(PathBuf::from(dir));
        }
        candidates.extend(
            ["models", "../models", "../../models"]
                .iter()
                .map(PathBuf::from),
        );
        candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models"));
        candidates
            .into_iter()
            .find(|d| d.join(MODEL).exists() && d.join(TOKENS).exists())
    }

    /// Installe le hook d'embeddings locuteur (CAM++), appelé au `SpeechEnd`.
    pub fn set_embedding_hook(&mut self, hook: Arc<dyn EmbeddingHook>) {
        self.embedding_hook = Some(hook);
    }

    /// Alimente le pipeline en échantillons mono 16 kHz (déjà conformes —
    /// aucun ré-échantillonnage n'est fait ici ; `NaN`/`Inf` -> 0.0, clamp
    /// `[-1, 1]`). Retourne les événements produits dans l'ordre.
    pub fn push_audio(&mut self, samples_16k_mono: &[f32]) -> Vec<VoiceEvent> {
        let mut events: Vec<VoiceEvent> = Vec::new();
        for &s in samples_16k_mono {
            let v = if s.is_finite() {
                s.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            self.pending.push_back(v);
        }
        while self.pending.len() >= PIPELINE_BLOCK_SAMPLES {
            let block: Vec<f32> = self.pending.drain(..PIPELINE_BLOCK_SAMPLES).collect();
            self.process_block(&block, &mut events);
        }
        self.events_emitted += events.len() as u64;
        events
    }

    /// Transcrit le dernier énoncé finalisé via SenseVoice-Small RÉEL.
    ///
    /// - Retourne `[Transcript { .. }]` si l'ASR est chargé (modèle réel présent) ;
    /// - Retourne `[]` + warning clair si aucun modèle ASR n'est chargé
    ///   (le pipeline ne fabrique JAMAIS de texte) ;
    /// - Erreur `ort` -> `[Error(..)]` (message réel remonté).
    ///
    /// Passe l'état de `Transcribing` à `Idle` dans tous les cas.
    pub async fn push_speech_transcript(&mut self) -> Vec<VoiceEvent> {
        if self.state != PipelineState::Transcribing {
            tracing::warn!(
                state = self.state.label(),
                "VoicePipeline::push_speech_transcript: aucun énoncé finalisé en attente"
            );
            return Vec::new();
        }
        let Some(utterance) = self.last_utterance.take() else {
            tracing::warn!(
                "VoicePipeline::push_speech_transcript: état Transcribing sans énoncé \
                 (incohérence interne) — aucun transcript"
            );
            self.state = PipelineState::Idle;
            return Vec::new();
        };
        self.state = PipelineState::Idle;
        let Some(asr) = &self.asr else {
            tracing::warn!(
                duration_s = utterance.len() as f32 / PIPELINE_SAMPLE_RATE as f32,
                "VoicePipeline::push_speech_transcript: AUCUN modèle ASR chargé \
                 (sensevoice_small.int8.onnx + tokens.txt absents) — transcript \
                 absent, aucun texte inventé"
            );
            return Vec::new();
        };
        let chunk = AudioChunk::new(utterance, PIPELINE_SAMPLE_RATE, 1);
        match asr.transcribe(&chunk).await {
            Ok(hyp) => {
                let mut events = vec![VoiceEvent::Transcript {
                    text: hyp.transcript,
                }];
                self.events_emitted += events.len() as u64;
                events.shrink_to_fit();
                events
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    "VoicePipeline::push_speech_transcript: forward ASR réel en échec"
                );
                let mut events = vec![VoiceEvent::Error(format!("ASR: {e}"))];
                self.events_emitted += events.len() as u64;
                events.shrink_to_fit();
                events
            }
        }
    }

    fn process_block(&mut self, block: &[f32], events: &mut Vec<VoiceEvent>) {
        self.frames_processed += 1;
        for &s in block {
            self.rolling.push_back(s);
        }
        self.total_pushed += block.len() as u64;
        while self.rolling.len() > self.rolling_cap {
            self.rolling.pop_front();
        }

        // Wake : détecteur existant VAD-gaté, actif hors écoute (Idle/Transcribing).
        if self.state != PipelineState::Listening
            && let Some(wake) = self.wake.as_mut()
        {
            let chunk = AudioChunk::new(block.to_vec(), PIPELINE_SAMPLE_RATE, 1);
            match wake.process_gated(&chunk) {
                Ok(d) if d.detected => events.push(VoiceEvent::Wake {
                    confidence: d.confidence,
                }),
                Ok(_) => {}
                Err(e) => {
                    if !self.wake_error_reported {
                        self.wake_error_reported = true;
                        events.push(VoiceEvent::Error(format!("wake: {e}")));
                    }
                }
            }
        }

        // VAD réel : fenêtres 512 complétées par ce bloc.
        let windows = match self.vad.process_stream(block) {
            Ok(w) => w,
            Err(e) => {
                if !self.vad_error_reported {
                    self.vad_error_reported = true;
                    tracing::error!(error = %e, "VoicePipeline: VAD réel en échec");
                    events.push(VoiceEvent::Error(format!("VAD: {e}")));
                }
                return;
            }
        };
        for (_idx, prob) in windows {
            let p = if prob.is_finite() {
                prob.clamp(0.0, 1.0)
            } else {
                0.0
            };
            self.advance_state(p, events);
        }

        // Garde-fou : énoncé anormalement long -> SpeechEnd forcé (borné).
        if self.state == PipelineState::Listening
            && let Some(start) = self.utterance_start_abs
        {
            let max_samples =
                (self.config.max_utterance_seconds * PIPELINE_SAMPLE_RATE as f32) as u64;
            if self.total_pushed.saturating_sub(start) > max_samples {
                self.finalize_utterance(events);
            }
        }
    }

    fn advance_state(&mut self, probability: f32, events: &mut Vec<VoiceEvent>) {
        let is_speech = probability >= self.config.vad_threshold;
        match self.state {
            PipelineState::Idle => {
                if is_speech {
                    self.begin_utterance();
                    events.push(VoiceEvent::SpeechStart);
                }
            }
            PipelineState::Listening => {
                if is_speech {
                    self.hangover_left = self.config.vad_hangover_windows;
                } else if self.hangover_left > 0 {
                    self.hangover_left -= 1;
                }
                if !is_speech && self.hangover_left == 0 {
                    self.finalize_utterance(events);
                }
            }
            PipelineState::Transcribing => {
                // Nouvelle prise de parole pendant qu'un transcript est en
                // attente : la précédente utterance reste disponible via
                // `last_utterance` jusqu'à `push_speech_transcript`.
                if is_speech {
                    self.begin_utterance();
                    events.push(VoiceEvent::SpeechStart);
                }
            }
        }
    }

    fn begin_utterance(&mut self) {
        self.state = PipelineState::Listening;
        self.hangover_left = self.config.vad_hangover_windows;
        let pre_roll_samples =
            (self.config.pre_roll_ms as u64 * PIPELINE_SAMPLE_RATE as u64) / 1000;
        let available = self.rolling.len() as u64;
        self.utterance_start_abs = Some(
            self.total_pushed
                .saturating_sub(pre_roll_samples.min(available)),
        );
    }

    fn finalize_utterance(&mut self, events: &mut Vec<VoiceEvent>) {
        if let Some(start) = self.utterance_start_abs.take() {
            let available_start = self.total_pushed.saturating_sub(self.rolling.len() as u64);
            let offset = start.saturating_sub(available_start) as usize;
            let utterance: Vec<f32> = self.rolling.iter().skip(offset).copied().collect();
            if !utterance.is_empty() {
                if let Some(hook) = &self.embedding_hook {
                    hook.on_speech_segment(
                        &utterance,
                        utterance.len() as f32 / PIPELINE_SAMPLE_RATE as f32,
                    );
                }
                self.last_utterance = Some(utterance);
            }
        }
        self.state = PipelineState::Transcribing;
        self.hangover_left = 0;
        events.push(VoiceEvent::SpeechEnd);
    }

    // --- Observabilité (tests / démon) ---

    /// Nombre de blocs de 10 ms traités.
    pub fn frames_processed(&self) -> u64 {
        self.frames_processed
    }

    /// Nombre total d'événements émis par `push_audio` + `push_speech_transcript`.
    pub fn events_emitted(&self) -> u64 {
        self.events_emitted
    }

    /// État courant de la machine.
    pub fn state(&self) -> PipelineState {
        self.state
    }

    /// Dernier énoncé finalisé (16 kHz mono), pour ASR/CAM++ externes.
    pub fn last_utterance(&self) -> Option<&[f32]> {
        self.last_utterance.as_deref()
    }

    /// Forwards VAD `ort` réellement exécutés (preuve de chaînage réel).
    pub fn vad_forward_calls(&self) -> u64 {
        self.vad.forward_calls()
    }

    /// Chemin du modèle VAD réellement chargé.
    pub fn vad_model_path(&self) -> &Path {
        self.vad.model_path()
    }

    /// Backend du détecteur de wake (`None` si wake désactivé).
    pub fn wake_backend(&self) -> Option<WakeBackendKind> {
        self.wake.as_ref().map(|w| w.backend_kind())
    }

    /// `true` si un moteur ASR réel est chargé (modèle SenseVoice présent).
    pub fn asr_available(&self) -> bool {
        self.asr.is_some()
    }

    /// Seuil VAD effectif (documenté pour les tests).
    pub fn vad_threshold(&self) -> f32 {
        self.config.vad_threshold
    }

    /// Configuration effective.
    pub fn config(&self) -> &VoicePipelineConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_inference::OrtVadSession;
    use std::sync::Mutex;

    /// Dossier modèles contenant silero_vad.onnx (skip gracieux sinon).
    fn workspace_vad_dir() -> Option<PathBuf> {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(d) = std::env::var_os("HAKILLE_MODELS_DIR") {
            candidates.push(PathBuf::from(d));
        }
        candidates.extend(
            ["models", "../models", "../../models"]
                .iter()
                .map(PathBuf::from),
        );
        candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models"));
        candidates
            .into_iter()
            .find(|d| d.join(PIPELINE_VAD_FILENAME).exists())
    }

    fn test_pipeline(wake: Option<&str>, asr: bool) -> Option<VoicePipeline> {
        let dir = workspace_vad_dir()?;
        let config = VoicePipelineConfig {
            models_dir: Some(dir),
            wake_word: wake.map(|k| k.to_string()),
            enable_asr: asr,
            ..Default::default()
        };
        Some(VoicePipeline::new(config).expect("pipeline réel"))
    }

    /// Bruit blanc déterministe (xorshift), amplitude crête `amp`.
    struct Noise(u64);
    impl Noise {
        fn next(&mut self, amp: f32) -> f32 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            let u = (self.0.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64;
            ((u as f32) * 2.0 - 1.0) * amp
        }
    }

    /// Résonateur 2 pôles (formant).
    fn reson(x: &[f32], fc: f32, r: f32) -> Vec<f32> {
        use std::f32::consts::PI;
        const SR: f32 = 16_000.0;
        let b = 2.0 * r * (2.0 * PI * fc / SR).cos();
        let c = -(r * r);
        let (mut y1, mut y2) = (0.0f32, 0.0f32);
        x.iter()
            .map(|xn| {
                let yn = xn + b * y1 + c * y2;
                y2 = y1;
                y1 = yn;
                yn
            })
            .collect()
    }

    /// Burst synthétique voisé ~430 ms : trame f0 = 150 Hz + 16 harmoniques
    /// (1/k) + bruit léger déterministe, excitée en 3 syllabes CV (burst plosif
    /// 12 ms + voyelle formantée 121 ms, /a/ /i/ /u/), transitoires 12 ms.
    ///
    /// Mesures réelles du Silero v5 épinglé (documentées, pas d'approximation) :
    /// - harmoniques stationnaires + bruit léger SANS structure syllabique :
    ///   prob ≤ 0.003 — rejeté (le vrai Silero exige une structure syllabique
    ///   réaliste) ;
    /// - ce burst après 512 échantillons de silence : fenêtre 512 n° 9
    ///   (échantillons 4608..5120) = 0.5367 > seuil 0.5 (fenêtre chaude) ;
    /// - un silence pur plus long (≥ 500 ms) avant le burst endort l'état
    ///   récurrent Silero (attracteur non-parole, max mesuré 0.04) : le test
    ///   utilise donc 32 ms de silence initial, durée où la détection est
    ///   prouvée — fait mesuré du modèle épinglé, pas une hypothèse.
    fn voiced_burst_400ms() -> Vec<f32> {
        use std::f32::consts::PI;
        const SR: f32 = 16_000.0;
        let vowels = [
            (730.0f32, 1090.0f32, 2440.0f32, 0.0f32),
            (270.0, 2290.0, 3010.0, 10.0),
            (300.0, 870.0, 2240.0, 18.0),
        ];
        let mut noise = Noise(0xC0FF_EE00_1234_5678);
        let mut out: Vec<f32> = Vec::new();
        for (f1, f2, f3, df0) in vowels {
            let f0 = 150.0 + df0;
            // Burst plosif 12 ms (bruit large bande).
            out.extend((0..192).map(|_| noise.next(0.15)));
            // Voyelle formantée 121 ms, excitation harmonique + bruit léger.
            let m = (0.121 * SR) as usize;
            let edge = (0.012 * SR).max(8.0);
            let g: Vec<f32> = (0..m)
                .map(|t| {
                    let tt = t as f32 / SR;
                    let env = ((t as f32 / edge).min(1.0)) * (((m - t) as f32 / edge).min(1.0));
                    let mut s = 0.0f32;
                    for k in 1..=16 {
                        s += (tt * f0 * k as f32 * 2.0 * PI).sin() * 0.5 / k as f32;
                    }
                    (s + noise.next(0.01)) * env
                })
                .collect();
            let (a, b, c) = (
                reson(&g, f1, 0.985),
                reson(&g, f2, 0.98),
                reson(&g, f3, 0.97),
            );
            out.extend(
                a.into_iter()
                    .zip(b)
                    .zip(c)
                    .map(|((x, y), z)| x * 0.5 + y * 0.35 + z * 0.25),
            );
            // 10 ms de silence inter-syllabe.
            out.extend(std::iter::repeat_n(0.0f32, 160));
        }
        let peak = out.iter().map(|s| s.abs()).fold(0.0f32, f32::max).max(1e-6);
        for s in out.iter_mut() {
            *s = (*s / peak * 0.5).clamp(-1.0, 1.0);
        }
        out
    }

    fn silence(seconds: f32) -> Vec<f32> {
        vec![0.0; (seconds * PIPELINE_SAMPLE_RATE as f32) as usize]
    }

    /// Silence pur (bruit nul) -> AUCUN `SpeechStart` (seuil VAD par défaut
    /// 0.5 ; probabilité Silero mesurée sur silence ≈ 0.0005).
    #[test]
    fn silence_yields_no_speech_events() {
        let Some(mut pipeline) = test_pipeline(Some("Dis Hakille"), false) else {
            eprintln!("SKIP silence: models/silero_vad.onnx absent");
            return;
        };
        assert_eq!(pipeline.vad_threshold(), 0.5);
        // Alimentation morcelée impaire : la fragmentation ne change rien.
        let sig = silence(2.0);
        let mut events = Vec::new();
        for chunk in sig.chunks(321) {
            events.extend(pipeline.push_audio(chunk));
        }
        assert!(
            events.is_empty(),
            "silence pur: aucun événement attendu, obtenus {events:?}"
        );
        assert_eq!(pipeline.state(), PipelineState::Idle);
        assert_eq!(pipeline.frames_processed(), (2.0 * 16_000.0 / 160.0) as u64);
        assert_eq!(pipeline.events_emitted(), 0);
        assert!(pipeline.vad_forward_calls() > 0, "le VAD réel doit tourner");
        assert!(
            !pipeline.asr_available(),
            "enable_asr=false doit désactiver l'ASR"
        );
    }

    /// Burst voisé synthétique après silence -> `SpeechStart` puis `SpeechEnd`,
    /// énoncé finalisé disponible.
    #[test]
    fn voiced_burst_triggers_start_then_end() {
        let Some(mut pipeline) = test_pipeline(None, false) else {
            eprintln!("SKIP burst: models/silero_vad.onnx absent");
            return;
        };
        let mut sig = silence(512.0 / 16_000.0); // 32 ms (alignement mesuré)
        sig.extend_from_slice(&voiced_burst_400ms());
        sig.extend_from_slice(&silence(0.5));
        let mut events = Vec::new();
        for chunk in sig.chunks(160) {
            events.extend(pipeline.push_audio(chunk));
        }
        let first_start = events
            .iter()
            .position(|e| matches!(e, VoiceEvent::SpeechStart));
        let first_end = events
            .iter()
            .position(|e| matches!(e, VoiceEvent::SpeechEnd));
        let (Some(start_idx), Some(end_idx)) = (first_start, first_end) else {
            panic!("SpeechStart puis SpeechEnd attendus, obtenus {events:?}");
        };
        assert!(
            start_idx < end_idx,
            "SpeechStart avant SpeechEnd attendus, obtenus {events:?}"
        );
        assert!(
            !events.iter().any(|e| matches!(e, VoiceEvent::Wake { .. })),
            "wake désactivé: aucun événement Wake attendu"
        );
        assert_eq!(pipeline.state(), PipelineState::Transcribing);
        let utterance = pipeline
            .last_utterance()
            .expect("énoncé finalisé disponible");
        let dur = utterance.len() as f32 / 16_000.0;
        assert!(
            (0.2..=1.5).contains(&dur),
            "durée d'énoncé inattendue: {dur:.3}s"
        );
        assert!(
            pipeline.vad_forward_calls() > 0,
            "forwards VAD réels exigés (jamais de faux VAD)"
        );
    }

    /// Golden VAD conditionnel : le modèle ONNX réel est chargé et forwardé.
    /// Rapide (< 1 s) ici ; peut être `#[ignore]` en CI contrainte de latence.
    /// Mesures : silence max ≈ 0.0005 (sous 0.05) ; burst mesuré 0.5367
    /// (fenêtre 9), au-dessus du seuil 0.5. Si la phase des fenêtres change
    /// (silence de tête non multiple de 512), la probabilité peut retomber —
    /// fait mesuré du modèle épinglé.
    #[test]
    fn golden_vad_real_model_conditional() {
        let Some(dir) = workspace_vad_dir() else {
            eprintln!("SKIP golden VAD: models/silero_vad.onnx absent");
            return;
        };
        let mut vad =
            OrtVadSession::open(dir.join(PIPELINE_VAD_FILENAME)).expect("session Silero réelle");
        let silent_probs = vad.process_stream(&silence(0.5)).expect("forward silence");
        let max_silence = silent_probs.iter().map(|(_, p)| *p).fold(0.0f32, f32::max);
        assert!(
            max_silence < 0.05,
            "silence doit rester sous 0.05: max={max_silence:.4}"
        );
        vad.reset_state();
        let mut sig = silence(512.0 / 16_000.0);
        sig.extend_from_slice(&voiced_burst_400ms());
        let probs = vad.process_stream(&sig).expect("forward burst");
        let max_burst = probs.iter().map(|(_, p)| *p).fold(0.0f32, f32::max);
        eprintln!(
            "golden pipeline VAD: silence_max={max_silence:.4} burst_max={max_burst:.4} \
             probs={:?}",
            probs.iter().map(|(_, p)| *p).collect::<Vec<f32>>()
        );
        assert!(
            max_burst > 0.5,
            "le burst voisé mesuré doit décoller (> 0.5): max={max_burst:.4}"
        );
        assert!(vad.forward_calls() >= probs.len() as u64);
    }

    /// Chemin forcé absent -> `ModelMissing { path }` EXPLICITE (jamais de faux VAD).
    #[test]
    fn model_missing_is_explicit_error() {
        let dir = std::env::temp_dir().join("hakille_pipeline_vad_absent_dir");
        let _ = std::fs::create_dir_all(&dir);
        let expected = dir.join(PIPELINE_VAD_FILENAME);
        let _ = std::fs::remove_file(&expected);
        let config = VoicePipelineConfig {
            models_dir: Some(dir.clone()),
            ..Default::default()
        };
        match VoicePipeline::new(config) {
            Err(VoicePipelineError::ModelMissing { path }) => {
                assert_eq!(path, expected, "le chemin manquant doit être exact");
            }
            Err(other) => panic!("ModelMissing attendu, obtenu {other:?}"),
            Ok(_) => panic!("ModelMissing attendu, mais un pipeline a été construit"),
        }
    }

    /// Validation stricte de configuration (aucun clamp silencieux).
    #[test]
    fn config_validation_rejects_invalid_values() {
        let dir = std::env::temp_dir().join("hakille_pipeline_cfg_probe");
        let bad = [
            VoicePipelineConfig {
                vad_threshold: 0.0,
                ..Default::default()
            },
            VoicePipelineConfig {
                vad_threshold: f32::NAN,
                ..Default::default()
            },
            VoicePipelineConfig {
                vad_hangover_windows: 0,
                ..Default::default()
            },
            VoicePipelineConfig {
                pre_roll_ms: 6_000,
                ..Default::default()
            },
            VoicePipelineConfig {
                max_utterance_seconds: 0.0,
                ..Default::default()
            },
            VoicePipelineConfig {
                wake_word: Some("   ".to_string()),
                ..Default::default()
            },
        ];
        for config in bad {
            let config = VoicePipelineConfig {
                models_dir: Some(dir.clone()),
                ..config
            };
            match VoicePipeline::new(config) {
                Err(VoicePipelineError::Config { field, .. }) => {
                    assert!(!field.is_empty(), "champ fautif nommé");
                }
                Err(other) => panic!("Config attendu, obtenu {other:?}"),
                Ok(_) => panic!("Config attendu, mais un pipeline a été construit"),
            }
        }
    }

    /// Sans modèle ASR : `push_speech_transcript` retourne SANS `Transcript`
    /// (warning clair, aucun texte inventé) et repasse l'état à `Idle`.
    #[tokio::test]
    async fn transcript_absent_without_asr_model() {
        let Some(mut pipeline) = test_pipeline(None, false) else {
            eprintln!("SKIP transcript: models/silero_vad.onnx absent");
            return;
        };
        let mut sig = silence(512.0 / 16_000.0);
        sig.extend_from_slice(&voiced_burst_400ms());
        sig.extend_from_slice(&silence(0.5));
        let _ = pipeline.push_audio(&sig);
        assert_eq!(pipeline.state(), PipelineState::Transcribing);
        let events = pipeline.push_speech_transcript().await;
        assert!(
            events.is_empty(),
            "aucun Transcript sans modèle ASR, obtenus {events:?}"
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, VoiceEvent::Transcript { .. })),
            "aucun texte ne doit être inventé"
        );
        assert_eq!(pipeline.state(), PipelineState::Idle);
        // Sans énoncé en attente : plus aucun événement.
        assert!(pipeline.push_speech_transcript().await.is_empty());
    }

    /// Hook d'embeddings (intégration CAM++) : appelé au `SpeechEnd` avec
    /// l'énoncé réel et sa durée.
    #[test]
    fn embedding_hook_receives_finalized_utterance() {
        let Some(mut pipeline) = test_pipeline(None, false) else {
            eprintln!("SKIP hook: models/silero_vad.onnx absent");
            return;
        };
        let calls: Arc<Mutex<Vec<(usize, f32)>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&calls);
        pipeline.set_embedding_hook(Arc::new(move |samples: &[f32], dur: f32| {
            sink.lock().expect("verrou hook").push((samples.len(), dur));
        }));
        let mut sig = silence(512.0 / 16_000.0);
        sig.extend_from_slice(&voiced_burst_400ms());
        sig.extend_from_slice(&silence(0.5));
        let events = pipeline.push_audio(&sig);
        assert!(
            events.iter().any(|e| matches!(e, VoiceEvent::SpeechEnd)),
            "SpeechEnd attendu: {events:?}"
        );
        let recorded = calls.lock().expect("verrou hook").clone();
        assert_eq!(recorded.len(), 1, "hook appelé une fois par énoncé");
        let (len, dur) = recorded[0];
        assert!(len > 0, "énoncé non vide");
        assert!(
            (dur - len as f32 / 16_000.0).abs() < 1e-4,
            "durée cohérente avec les échantillons"
        );
        assert!(
            (0.2..=1.5).contains(&dur),
            "durée d'énoncé plausible: {dur:.3}s"
        );
    }
}
