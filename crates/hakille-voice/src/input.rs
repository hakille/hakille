//! Écoute locale continue : CPAL → Silero → Whisper → activation explicite.
//! Aucun audio n'est envoyé au réseau. La capture est fermée pendant l'ASR
//! et les actions ; les résultats d'une génération annulée sont abandonnés.

use hakille_audio::Capture;
pub use hakille_inference::whisper_engine::WHISPER_LANGUAGES;
/// Indice lexical natif ; aucune substitution dans la transcription obtenue.
pub const VOICE_LEXICAL_HINT: &str = "Hakille";
use hakille_inference::{
    ModelProvisioner,
    whisper_engine::{WhisperEngine, WhisperTranscription},
};
use hakille_perception::audio::{
    ActivationOutcome, ActivationState, VoiceEvent, VoicePipeline, VoicePipelineConfig,
    WakeActivation, WakeActivationConfig,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct VoiceInputConfig {
    pub models_dir: PathBuf,
    pub language: Option<String>,
    pub device_name: Option<String>,
    pub activation: WakeActivationConfig,
}

impl Default for VoiceInputConfig {
    fn default() -> Self {
        Self {
            models_dir: std::env::var_os("HAKILLE_MODELS_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("models")),
            language: None,
            device_name: None,
            activation: WakeActivationConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceInputEvent {
    Loading,
    Listening,
    Speech,
    Transcribing,
    Armed,
    Command {
        text: String,
        language: String,
        epoch: u64,
    },
    Ignored,
    Error(String),
    Stopped,
}

// Une seule valeur atomique lie génération et états. Cela ferme la course entre
// une annulation et la publication d'une transcription déjà calculée.
#[derive(Default)]
struct Control {
    state: AtomicU64,
}
const STOPPED: u64 = 1;
const SUSPENDED: u64 = 2;
impl Control {
    fn epoch(&self) -> u64 {
        self.state.load(Ordering::SeqCst)
    }
    fn stopped(&self) -> bool {
        self.epoch() & STOPPED != 0
    }
    fn suspended(&self) -> bool {
        self.epoch() & SUSPENDED != 0
    }
    fn suspend(&self, value: bool) {
        let _ = self
            .state
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |state| {
                Some((state.wrapping_add(4) & !SUSPENDED) | if value { SUSPENDED } else { 0 })
            });
    }
    fn stop(&self) {
        let _ = self
            .state
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |state| {
                Some(state.wrapping_add(4) | STOPPED)
            });
    }
    fn current(&self, epoch: u64) -> bool {
        epoch & (STOPPED | SUSPENDED) == 0 && self.epoch() == epoch
    }
    fn reserve_command(&self, epoch: u64) -> Option<u64> {
        if epoch & (STOPPED | SUSPENDED) != 0 {
            return None;
        }
        let next = epoch.wrapping_add(4) | SUSPENDED;
        self.state
            .compare_exchange(epoch, next, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| next)
    }
    fn accept_command(&self, epoch: u64) -> bool {
        if epoch & (STOPPED | SUSPENDED) != SUSPENDED {
            return false;
        }
        // Un jeton ne peut être consommé qu'une fois, avant toute reprise/annulation.
        self.state
            .compare_exchange(
                epoch,
                epoch.wrapping_add(4),
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
    }
}

/// Le thread possède le microphone (CPAL n'est pas Send sur toutes les plateformes).
/// Stop/Drop ne bloquent pas le thread graphique. Un forward Candle commencé
/// termine avant la destruction du moteur ; son résultat annulé n'est pas publié.
pub struct VoiceInputHandle {
    control: Arc<Control>,
}

impl VoiceInputHandle {
    pub fn start(
        config: VoiceInputConfig,
        callback: impl Fn(VoiceInputEvent) + Send + 'static,
    ) -> Self {
        let control = Arc::new(Control::default());
        let worker_control = control.clone();
        std::thread::spawn(move || {
            callback(VoiceInputEvent::Loading);
            if let Err(message) = run(config, &worker_control, &callback)
                && !worker_control.stopped()
            {
                callback(VoiceInputEvent::Error(message));
            }
            callback(VoiceInputEvent::Stopped);
        });
        Self { control }
    }
    pub fn set_suspended(&self, suspended: bool) {
        self.control.suspend(suspended);
    }
    /// Valide le jeton au moment de soumettre la commande, sur le thread consommateur.
    pub fn accept_command(&self, epoch: u64) -> bool {
        self.control.accept_command(epoch)
    }
    pub fn stop(&self) {
        self.control.stop();
    }
}

impl Drop for VoiceInputHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Même chemin ASR/activation pour les diagnostics WAV et la capture réelle.
pub struct VoiceRecognizer {
    engine: WhisperEngine,
    activation: WakeActivation,
    language: Option<String>,
    clock: Instant,
}

#[derive(Debug)]
pub struct VoiceRecognition {
    pub transcription: WhisperTranscription,
    pub activation: ActivationOutcome,
}

impl VoiceRecognizer {
    pub fn new(config: &VoiceInputConfig) -> Result<Self, String> {
        let activation =
            WakeActivation::new(config.activation.clone()).map_err(|e| e.to_string())?;
        if let Some(language) = &config.language
            && !hakille_inference::whisper_engine::WHISPER_LANGUAGES.contains(&language.as_str())
        {
            return Err(format!(
                "langue Whisper inconnue : {language} (utiliser fr, en, es… ou la détection automatique)"
            ));
        }
        verify(&config.models_dir, "whisper-tiny-multilingual")?;
        Ok(Self {
            engine: WhisperEngine::load_from_dir(&config.models_dir).map_err(|e| e.to_string())?,
            activation,
            language: config.language.clone(),
            clock: Instant::now(),
        })
    }
    fn now_ms(&self) -> u64 {
        self.clock.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
    pub fn recognize(&mut self, samples: &[f32]) -> Result<VoiceRecognition, String> {
        let captured_at = self.now_ms();
        let transcription = self.transcribe(samples)?;
        let activation = self.activate(&transcription, captured_at);
        Ok(VoiceRecognition {
            transcription,
            activation,
        })
    }
    pub fn finish_command(&mut self) {
        self.activation.finish_command(self.now_ms());
    }
    fn transcribe(&mut self, samples: &[f32]) -> Result<WhisperTranscription, String> {
        self.engine
            .transcribe_with_prompt(
                samples,
                self.language.as_deref(),
                Some(VOICE_LEXICAL_HINT),
                Some(Instant::now() + Duration::from_secs(120)),
            )
            .map_err(|e| e.to_string())
    }
    fn activate(
        &mut self,
        transcript: &WhisperTranscription,
        captured_at: u64,
    ) -> ActivationOutcome {
        self.activation.on_transcript_captured(
            &transcript.text,
            transcript.language.as_deref().unwrap_or("und"),
            captured_at,
            self.now_ms(),
        )
    }
}

fn verify(models_dir: &std::path::Path, id: &str) -> Result<(), String> {
    let reports = ModelProvisioner::new(models_dir.to_owned())
        .verify_model_by_id(id)
        .map_err(|e| {
            format!(
                "{e}. Installer : hakille-daemon models download --model {id} --models-dir {}",
                models_dir.display()
            )
        })?;
    if reports.is_empty() || reports.iter().any(|r| !r.verified) {
        return Err(format!(
            "intégrité du modèle {id} refusée ; relancer models download --model {id}"
        ));
    }
    Ok(())
}

// 200 blocs de 10 ms maximum. Une surcharge invalide tout le segment :
// une commande tronquée ou ancienne ne doit jamais être exécutée.
struct AudioBlock {
    samples: Vec<f32>,
    captured: Instant,
}

fn enqueue(tx: &SyncSender<AudioBlock>, samples: &[f32], overflow: &AtomicBool) {
    if let Err(TrySendError::Full(_)) = tx.try_send(AudioBlock {
        samples: samples.to_vec(),
        captured: Instant::now(),
    }) {
        overflow.store(true, Ordering::SeqCst);
    }
}

/// Vérifier après la fermeture du flux avant toute transcription : une erreur
/// ou une saturation peut arriver pendant le forward VAD de fin de segment.
fn capture_health_error(
    error: Option<hakille_audio::AudioError>,
    failed: bool,
    overflow: bool,
) -> Result<(), String> {
    if let Some(error) = error {
        return Err(format!("capture interrompue : {error}"));
    }
    if overflow {
        return Err("capture surchargée : segment abandonné ; relancer l'écoute".into());
    }
    if failed {
        return Err(
            "capture interrompue : le pilote a signalé une erreur ; relancer l'écoute".into(),
        );
    }
    Ok(())
}

fn check_capture(capture: &Capture, overflow: &AtomicBool) -> Result<(), String> {
    capture_health_error(
        capture.take_error(),
        capture.status().failed,
        overflow.load(Ordering::SeqCst),
    )
}

/// Un callback vide n'est pas une preuve de transport audio. Seule une
/// progression du nombre de blocs livrés renouvelle le délai de surveillance.
struct CaptureWatchdog {
    started_at: Instant,
    last_block_at: Option<Instant>,
    blocks_delivered: u64,
}

impl CaptureWatchdog {
    fn new(now: Instant) -> Self {
        Self {
            started_at: now,
            last_block_at: None,
            blocks_delivered: 0,
        }
    }

    fn observe(&mut self, blocks_delivered: u64, now: Instant) -> Result<(), String> {
        if blocks_delivered > self.blocks_delivered {
            self.blocks_delivered = blocks_delivered;
            self.last_block_at = Some(now);
        }
        let stalled = match self.last_block_at {
            Some(last) => now.saturating_duration_since(last) > Duration::from_secs(3),
            None => now.saturating_duration_since(self.started_at) > Duration::from_secs(5),
        };
        if stalled {
            Err("le microphone ne fournit plus de blocs audio ; vérifier le périphérique et ses permissions".into())
        } else {
            Ok(())
        }
    }
}

fn start_capture(
    capture: &mut Capture,
    control: &Arc<Control>,
    overflow: &Arc<AtomicBool>,
) -> Result<Receiver<AudioBlock>, String> {
    let (tx, rx) = mpsc::sync_channel(200);
    let control = control.clone();
    let overflow = overflow.clone();
    capture
        .start(move |samples| {
            if !control.stopped() && !control.suspended() {
                enqueue(&tx, samples, &overflow);
            }
        })
        .map_err(|e| format!("microphone : {e}"))?;
    Ok(rx)
}

fn run(
    config: VoiceInputConfig,
    control: &Arc<Control>,
    emit: &impl Fn(VoiceInputEvent),
) -> Result<(), String> {
    let mut capture = match &config.device_name {
        Some(name) => Capture::with_device_name(name),
        None => Capture::new(),
    }
    .map_err(|e| {
        format!("microphone indisponible : {e}. Vérifier les permissions et voice devices.")
    })?;
    verify(&config.models_dir, "silero-vad-v5")?;
    let mut recognizer = VoiceRecognizer::new(&config)?;
    if control.stopped() {
        return Ok(());
    }
    let mut pipeline = VoicePipeline::new(VoicePipelineConfig {
        models_dir: Some(config.models_dir),
        wake_word: None,
        enable_asr: false,
        vad_hangover_windows: 25,
        pre_roll_ms: 300,
        max_utterance_seconds: 12.0,
        ..VoicePipelineConfig::default()
    })
    .map_err(|e| e.to_string())?;
    let overflow = Arc::new(AtomicBool::new(false));
    let mut receiver = None;
    let mut watchdog = CaptureWatchdog::new(Instant::now());
    let mut captured_at = 0;
    let mut epoch = control.epoch();
    let mut busy_command = false;
    while !control.stopped() {
        let next_epoch = control.epoch();
        if next_epoch != epoch || control.suspended() {
            capture.stop().map_err(|e| e.to_string())?;
            receiver = None; // Détruire toute file de la génération précédente.
            pipeline.reset_audio();
            if !busy_command {
                recognizer.activation.reset(recognizer.now_ms());
            }
            epoch = next_epoch;
        }
        if control.suspended() {
            std::thread::sleep(Duration::from_millis(25));
            continue;
        }
        if receiver.is_none() {
            if busy_command {
                recognizer.finish_command();
                busy_command = false;
            }
            overflow.store(false, Ordering::SeqCst);
            receiver = Some(start_capture(&mut capture, control, &overflow)?);
            watchdog = CaptureWatchdog::new(Instant::now());
            if matches!(recognizer.activation.state(), ActivationState::Armed { .. }) {
                emit(VoiceInputEvent::Armed);
            } else {
                emit(VoiceInputEvent::Listening);
            }
        }
        check_capture(&capture, &overflow)?;
        watchdog.observe(capture.status().blocks_delivered, Instant::now())?;
        match receiver
            .as_ref()
            .expect("capture démarrée")
            .recv_timeout(Duration::from_millis(25))
        {
            Ok(samples) => {
                if !control.current(epoch) {
                    continue;
                }
                let block_at = samples
                    .captured
                    .saturating_duration_since(recognizer.clock)
                    .as_millis()
                    .min(u64::MAX as u128) as u64;
                for event in pipeline.push_audio(&samples.samples) {
                    match event {
                        VoiceEvent::SpeechStart => {
                            captured_at = block_at.saturating_sub(300);
                            emit(VoiceInputEvent::Speech);
                        }
                        VoiceEvent::Error(error) => return Err(error),
                        VoiceEvent::SpeechEnd => {
                            capture.stop().map_err(|e| e.to_string())?;
                            // Le flux est désormais fermé : aucune erreur ou perte
                            // de ce segment ne doit être effacée par le prochain start.
                            check_capture(&capture, &overflow)?;
                            receiver = None;
                            let samples = pipeline
                                .take_utterance()
                                .ok_or("segment audio absent après fin de parole")?;
                            if pipeline.last_utterance_truncated() {
                                return Err("phrase trop longue (12 secondes) : segment abandonné ; formuler une commande plus courte".into());
                            }
                            pipeline.reset_audio();
                            emit(VoiceInputEvent::Transcribing);
                            let transcript = recognizer.transcribe(&samples);
                            if !control.current(epoch) {
                                continue;
                            }
                            let transcript = transcript?;
                            match recognizer.activate(&transcript, captured_at) {
                                ActivationOutcome::Command { text, language } => {
                                    if let Some(command_epoch) = control.reserve_command(epoch) {
                                        busy_command = true;
                                        emit(VoiceInputEvent::Command {
                                            text,
                                            language,
                                            epoch: command_epoch,
                                        });
                                    }
                                }
                                ActivationOutcome::Armed { .. } => {}
                                ActivationOutcome::Ignored { .. } => emit(VoiceInputEvent::Ignored),
                            }
                        }
                        _ => {}
                    }
                }
                if pipeline.state() == hakille_perception::audio::PipelineState::Idle
                    && recognizer.activation.tick(block_at)
                {
                    emit(VoiceInputEvent::Listening);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if pipeline.state() == hakille_perception::audio::PipelineState::Idle
                    && recognizer.activation.tick(recognizer.now_ms())
                {
                    emit(VoiceInputEvent::Listening);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("flux du microphone fermé".into());
            }
        }
    }
    capture.stop().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_or_consumed_command_token_cannot_be_submitted() {
        let control = Control::default();
        let token = control.reserve_command(0).unwrap();
        control.suspend(true); // Même valeur : annule néanmoins la commande en file.
        assert!(!control.accept_command(token));
        control.suspend(false);
        let epoch = control.epoch();
        assert!(control.reserve_command(0).is_none());
        let token = control.reserve_command(epoch).unwrap();
        assert!(control.accept_command(token));
        assert!(!control.accept_command(token));
        assert!(!control.current(epoch));
    }

    #[test]
    fn segment_gate_rejects_overflow_or_errors_arriving_during_vad() {
        assert!(capture_health_error(None, false, false).is_ok());
        // Le contrôle initial peut réussir ; la vérification après fermeture
        // doit refuser une saturation ou une erreur apparue pendant le VAD.
        assert!(
            capture_health_error(None, false, true)
                .unwrap_err()
                .contains("surchargée")
        );
        assert!(
            capture_health_error(
                Some(hakille_audio::AudioError::StreamError("débranché".into())),
                true,
                false
            )
            .unwrap_err()
            .contains("débranché")
        );
        // Consommer le détail d'une erreur n'autorise pas le segment défaillant.
        assert!(capture_health_error(None, true, false).is_err());
    }

    #[test]
    fn microphone_watchdog_requires_blocks_not_just_callbacks() {
        let now = Instant::now();
        let mut watchdog = CaptureWatchdog::new(now);
        assert!(watchdog.observe(0, now + Duration::from_secs(5)).is_ok());
        assert!(
            watchdog
                .observe(0, now + Duration::from_millis(5001))
                .is_err()
        );
        let mut watchdog = CaptureWatchdog::new(now);
        assert!(watchdog.observe(1, now + Duration::from_secs(1)).is_ok());
        assert!(watchdog.observe(1, now + Duration::from_secs(4)).is_ok());
        assert!(
            watchdog
                .observe(1, now + Duration::from_millis(4001))
                .is_err()
        );
    }

    #[test]
    fn microphone_watchdog_progress_and_restart_renew_the_deadline() {
        let now = Instant::now();
        let mut watchdog = CaptureWatchdog::new(now);
        assert!(watchdog.observe(10, now + Duration::from_secs(1)).is_ok());
        assert!(watchdog.observe(20, now + Duration::from_secs(3)).is_ok());
        assert!(watchdog.observe(20, now + Duration::from_secs(6)).is_ok());
        assert!(
            watchdog
                .observe(20, now + Duration::from_millis(6001))
                .is_err()
        );
        let mut restarted = CaptureWatchdog::new(now + Duration::from_secs(10));
        assert!(restarted.observe(0, now + Duration::from_secs(14)).is_ok());
        assert!(restarted.observe(1, now + Duration::from_secs(15)).is_ok());
    }

    #[test]
    fn suspension_invalidates_results_even_after_fast_resume() {
        let control = Control::default();
        assert!(control.current(0));
        control.suspend(true);
        control.suspend(false);
        assert!(!control.current(0));
        assert!(control.current(8));
        control.stop();
        assert!(!control.current(8));
    }
    #[test]
    fn audio_queue_overflow_is_explicit_and_bounded() {
        let (tx, rx) = mpsc::sync_channel(1);
        let overflow = AtomicBool::new(false);
        enqueue(&tx, &[0.1; 160], &overflow);
        assert!(!overflow.load(Ordering::SeqCst));
        enqueue(&tx, &[0.2; 160], &overflow);
        assert!(overflow.load(Ordering::SeqCst));
        assert_eq!(rx.try_recv().unwrap().samples, vec![0.1; 160]);
        assert!(rx.try_recv().is_err());
    }
}
