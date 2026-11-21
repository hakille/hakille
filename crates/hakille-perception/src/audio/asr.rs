use crate::audio::capture::AudioChunk;
use async_trait::async_trait;
use hakille_core::{HakilleError, Result};
use hakille_inference::{SenseVoiceOnnx, SenseVoiceTokenizer};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Hypothèse ASR réelle (SenseVoice-Small, forward `ort`).
///
/// Contrat v1 honnête :
/// - `transcript`/`language`/`confidence` : décodage CTC greedy réel.
/// - `emotion`/`acoustic_events` : tokens auxiliaires réellement émis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrHypothesis {
    pub transcript: String,
    pub language: String,
    /// Émotion émise par le modèle, vide si absente.
    pub emotion: String,
    /// Événements émis par le modèle, vide si absents.
    pub acoustic_events: Vec<String>,
    pub confidence: f32,
    pub duration_seconds: f32,
}

#[async_trait]
pub trait AsrEngine: Send + Sync {
    async fn transcribe(&self, chunk: &AudioChunk) -> Result<AsrHypothesis>;
}

/// SenseVoice-Small RÉEL — forward `ort` via [`SenseVoiceOnnx`] (mel pur
/// Rust + décodage CTC greedy + tokenizer 25 055 tokens réel).
///
/// - Le moteur charge paresseusement (`OnceLock`) depuis `models_dir`
///   (`sensevoice_small.int8.onnx` + `tokens.txt`) ; modèle absent ->
///   erreur LOUD explicite, jamais de repli silencieux.
/// - Langue, confiance, émotion et événements viennent du décodage réel.
pub struct SenseVoiceSmall {
    model_name: String,
    tokenizer: Option<SenseVoiceTokenizer>,
    models_dir: Option<PathBuf>,
    engine: std::sync::Arc<std::sync::Mutex<Option<SenseVoiceOnnx>>>,
}

impl SenseVoiceSmall {
    pub fn new() -> Self {
        Self {
            model_name: "SenseVoice-Small-INT8".to_string(),
            tokenizer: None,
            models_dir: None,
            engine: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub fn with_tokenizer(tokenizer: SenseVoiceTokenizer) -> Self {
        Self {
            model_name: "SenseVoice-Small-INT8".to_string(),
            tokenizer: Some(tokenizer),
            models_dir: None,
            engine: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub fn with_tokens_file(path: impl AsRef<Path>) -> Result<Self> {
        let tokenizer = SenseVoiceTokenizer::from_file(path)?;
        Ok(Self::with_tokenizer(tokenizer))
    }

    /// Dossier contenant `sensevoice_small.int8.onnx` + `tokens.txt`.
    /// Sans appel, résolution automatique : `models/`, `../models/`,
    /// `../../models/` puis `$HAKILLE_MODELS_DIR`.
    pub fn with_models_dir(dir: impl AsRef<Path>) -> Self {
        Self {
            model_name: "SenseVoice-Small-INT8".to_string(),
            tokenizer: None,
            models_dir: Some(dir.as_ref().to_path_buf()),
            engine: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn tokenizer(&self) -> Option<&SenseVoiceTokenizer> {
        self.tokenizer.as_ref()
    }

    pub fn is_tokenizer_loaded(&self) -> bool {
        self.tokenizer.is_some()
    }

    fn resolve_model_files(&self) -> Result<(PathBuf, PathBuf)> {
        const MODEL: &str = "sensevoice_small.int8.onnx";
        const TOKENS: &str = "tokens.txt";
        // Dossier explicite -> faisant foi seul (erreur LOUD si incomplet).
        if let Some(dir) = &self.models_dir {
            let model = dir.join(MODEL);
            let tokens = dir.join(TOKENS);
            if model.exists() && tokens.exists() {
                return Ok((model, tokens));
            }
            return Err(HakilleError::InferenceError(format!(
                "SenseVoiceSmall::transcribe: modèle RÉEL absent — '{}' + '{}' introuvables dans '{}' (dossier explicite faisant foi) ; placez les fichiers dans models/ ou corrigez SenseVoiceSmall::with_models_dir",
                MODEL,
                TOKENS,
                dir.display()
            )));
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
        for dir in &candidates {
            let model = dir.join(MODEL);
            let tokens = dir.join(TOKENS);
            if model.exists() && tokens.exists() {
                return Ok((model, tokens));
            }
        }
        Err(HakilleError::InferenceError(format!(
            "SenseVoiceSmall::transcribe: modèle RÉEL absent — '{}' + '{}' introuvables (dossiers essayés : {:?}) ; placez les fichiers dans models/ ou utilisez SenseVoiceSmall::with_models_dir",
            MODEL, TOKENS, candidates
        )))
    }
}

impl Default for SenseVoiceSmall {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AsrEngine for SenseVoiceSmall {
    async fn transcribe(&self, chunk: &AudioChunk) -> Result<AsrHypothesis> {
        let pcm = hakille_inference::audio_resample::try_to_mono_16k(
            &chunk.samples,
            chunk.channels,
            chunk.sample_rate,
        )?;
        let duration = pcm.len() as f32 / 16_000.0;
        let (model_path, tokens_path) = self.resolve_model_files()?;
        let engine_arc = std::sync::Arc::clone(&self.engine);

        // Déport sur spawn_blocking pour ne jamais bloquer les workers Tokio
        // pendant le chargement ONNX ou le forward CPU SenseVoice.
        let out = tokio::task::spawn_blocking(move || -> Result<_> {
            let mut slot = engine_arc.lock().map_err(|e| {
                HakilleError::InferenceError(format!(
                    "SenseVoiceSmall::transcribe: moteur empoisonné : {e}"
                ))
            })?;
            if slot.is_none() {
                let engine = SenseVoiceOnnx::open(&model_path, &tokens_path)?;
                *slot = Some(engine);
            }
            let engine = slot
                .as_mut()
                .ok_or_else(|| HakilleError::InferenceError("moteur SenseVoice absent".into()))?;
            engine.transcribe(&pcm)
        })
        .await
        .map_err(|e| {
            HakilleError::InferenceError(format!("SenseVoiceSmall::transcribe interrompu : {e}"))
        })??;

        Ok(AsrHypothesis {
            transcript: out.text,
            language: out.language,
            emotion: out.emotion_event.emotion,
            acoustic_events: out.emotion_event.events,
            confidence: out.confidence,
            duration_seconds: duration,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_real_engine_missing_model_errors_loud() {
        let asr = SenseVoiceSmall::with_models_dir("/definitivement/absent/hakille-test-xyz");
        let chunk = AudioChunk::new(vec![0.1f32; 1600], 16_000, 1);
        let err = asr.transcribe(&chunk).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("absent"),
            "erreur LOUD attendue, obtenue : {msg}"
        );
    }

    #[tokio::test]
    async fn invalid_audio_fails_before_loading_model() {
        let asr = SenseVoiceSmall::with_models_dir("/absent");
        for chunk in [
            AudioChunk::new(vec![f32::NAN; 1600], 16000, 1),
            AudioChunk::new(vec![0.0; 1601], 16000, 2),
            AudioChunk::new(vec![0.0; 1600], 0, 1),
        ] {
            let error = asr.transcribe(&chunk).await.unwrap_err().to_string();
            assert!(error.contains("audio invalide"));
        }
    }

    fn workspace_models_dir() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("HAKILLE_MODELS_DIR") {
            let p = PathBuf::from(dir);
            if p.join("sensevoice_small.int8.onnx").exists() {
                return Some(p);
            }
        }
        for cand in ["models", "../models", "../../models", "../../../models"] {
            let p = PathBuf::from(cand);
            if p.join("sensevoice_small.int8.onnx").exists() && p.join("tokens.txt").exists() {
                return Some(p);
            }
        }
        None
    }

    /// Golden test — vrai moteur via `SenseVoiceSmall` : forward RÉEL sur
    /// silence 1 s (contrat réel : pas d'erreur, texte court, confiance
    /// MESURÉE — voir `sensevoice_onnx.rs` : le vide-sur-silence est une
    /// propriété VAD-système, pas encodeur ; aucun gate d'énergie ici).
    /// Latence loggée (< 60 s avec chargement paresseux).
    #[tokio::test]
    async fn golden_real_asr_silence_via_lazy_engine() {
        let models_dir = match workspace_models_dir() {
            Some(d) => d,
            None => {
                eprintln!("SKIP golden ASR perception : models/ introuvable");
                return;
            }
        };
        let asr = SenseVoiceSmall::with_models_dir(&models_dir);
        let chunk = AudioChunk::new(vec![0.0f32; 16_000], 16_000, 1);
        let t0 = std::time::Instant::now();
        let hyp = asr.transcribe(&chunk).await.expect("forward réel");
        let dt = t0.elapsed();
        eprintln!(
            "GOLDEN perception silence: texte={:?} langue={} emotion={:?} events={:?} conf={:.4} latence={:?}",
            hyp.transcript, hyp.language, hyp.emotion, hyp.acoustic_events, hyp.confidence, dt
        );
        assert!(
            hyp.transcript.chars().count() <= 20,
            "hallucination courte (pas d'emballement), obtenue {:?}",
            hyp.transcript
        );
        assert!(
            hyp.confidence.is_finite() && (0.0..=1.0).contains(&hyp.confidence),
            "confiance MESURÉE dans [0,1], obtenue {}",
            hyp.confidence
        );
        assert!(!hyp.language.is_empty());
        assert_eq!(hyp.emotion, "NEUTRAL");
        assert_eq!(hyp.acoustic_events, ["Speech"]);
        assert!(
            dt.as_secs() < 60,
            "latence (incl. chargement) {dt:?} < 60 s"
        );
    }
}
