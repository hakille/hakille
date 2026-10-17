use crate::audio::capture::AudioChunk;
use annora_core::{AnnoraError, Result};
use annora_inference::{SenseVoiceOnnx, SenseVoiceTokenizer};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Hypothèse ASR réelle (SenseVoice-Small, forward `ort`).
///
/// Contrat v1 honnête :
/// - `transcript`/`language`/`confidence` : décodage CTC greedy réel.
/// - `emotion` : toujours `""` en v1 (tête auxiliaire émotion non décodée).
/// - `acoustic_events` : toujours `[]` en v1 (tête événements non décodée).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrHypothesis {
    pub transcript: String,
    pub language: String,
    /// v1 : toujours `""` — tête émotion SenseVoice non décodée (documenté, pas simulé).
    pub emotion: String,
    /// v1 : toujours `[]` — tête événements acoustiques non décodée (documenté).
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
/// - `language`/`confidence` viennent du décodage réel ; `emotion` reste
///   `""` et `acoustic_events` `[]` en v1 (têtes auxiliaires non décodées).
pub struct SenseVoiceSmall {
    model_name: String,
    tokenizer: Option<SenseVoiceTokenizer>,
    models_dir: Option<PathBuf>,
    engine: std::sync::Mutex<Option<SenseVoiceOnnx>>,
}

impl SenseVoiceSmall {
    pub fn new() -> Self {
        Self {
            model_name: "SenseVoice-Small-INT8".to_string(),
            tokenizer: None,
            models_dir: None,
            engine: std::sync::Mutex::new(None),
        }
    }

    pub fn with_tokenizer(tokenizer: SenseVoiceTokenizer) -> Self {
        Self {
            model_name: "SenseVoice-Small-INT8".to_string(),
            tokenizer: Some(tokenizer),
            models_dir: None,
            engine: std::sync::Mutex::new(None),
        }
    }

    pub fn with_tokens_file(path: impl AsRef<Path>) -> Result<Self> {
        let tokenizer = SenseVoiceTokenizer::from_file(path)?;
        Ok(Self::with_tokenizer(tokenizer))
    }

    /// Dossier contenant `sensevoice_small.int8.onnx` + `tokens.txt`.
    /// Sans appel, résolution automatique : `models/`, `../models/`,
    /// `../../models/` puis `$ANNORA_MODELS_DIR`.
    pub fn with_models_dir(dir: impl AsRef<Path>) -> Self {
        Self {
            model_name: "SenseVoice-Small-INT8".to_string(),
            tokenizer: None,
            models_dir: Some(dir.as_ref().to_path_buf()),
            engine: std::sync::Mutex::new(None),
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
            return Err(AnnoraError::InferenceError(format!(
                "SenseVoiceSmall::transcribe: modèle RÉEL absent — '{}' + '{}' introuvables dans '{}' (dossier explicite faisant foi) ; placez les fichiers dans models/ ou corrigez SenseVoiceSmall::with_models_dir",
                MODEL,
                TOKENS,
                dir.display()
            )));
        }
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(dir) = std::env::var_os("ANNORA_MODELS_DIR") {
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
        Err(AnnoraError::InferenceError(format!(
            "SenseVoiceSmall::transcribe: modèle RÉEL absent — '{}' + '{}' introuvables (dossiers essayés : {:?}) ; placez les fichiers dans models/ ou utilisez SenseVoiceSmall::with_models_dir",
            MODEL, TOKENS, candidates
        )))
    }

    /// Verrouille le slot moteur, initialise paresseusement au premier appel.
    /// Le verrou est tenu pendant le forward (transcriptions concurrentes
    /// sérialisées — v1 documenté ; `Session::run` exige `&mut`).
    fn lock_engine(&self) -> Result<std::sync::MutexGuard<'_, Option<SenseVoiceOnnx>>> {
        let mut slot = self.engine.lock().map_err(|e| {
            AnnoraError::InferenceError(format!(
                "SenseVoiceSmall::transcribe: moteur empoisonné : {e}"
            ))
        })?;
        if slot.is_none() {
            let (model, tokens) = self.resolve_model_files()?;
            let engine = SenseVoiceOnnx::open(&model, &tokens)?;
            *slot = Some(engine);
        }
        Ok(slot)
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
        let duration = chunk.duration_seconds();
        let mut slot = self.lock_engine().map_err(|e| {
            AnnoraError::InferenceError(format!("SenseVoiceSmall::transcribe: {e}"))
        })?;
        let engine = slot.as_mut().expect("moteur initialisé par lock_engine");
        // Forward RÉEL : mel + ort + CTC greedy + tokenizer réel.
        let out = engine.transcribe(&chunk.samples)?;
        Ok(AsrHypothesis {
            transcript: out.text,
            language: out.language,
            // v1 honnête : têtes auxiliaires émotion/événements non décodées.
            emotion: String::new(),
            acoustic_events: vec![],
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
        let asr = SenseVoiceSmall::with_models_dir("/definitivement/absent/annora-test-xyz");
        let chunk = AudioChunk::new(vec![0.1f32; 1600], 16_000, 1);
        let err = asr.transcribe(&chunk).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("absent"),
            "erreur LOUD attendue, obtenue : {msg}"
        );
    }

    #[test]
    fn test_asr_v1_auxiliary_heads_vides_par_contrat() {
        // Contrat v1 documenté sur le type : émotion/événements non décodés.
        // `emotion == ""` et `acoustic_events == []` — jamais simulés.
        let hyp = AsrHypothesis {
            transcript: "bonjour".to_string(),
            language: "fr".to_string(),
            emotion: String::new(),
            acoustic_events: vec![],
            confidence: 0.9,
            duration_seconds: 1.0,
        };
        assert_eq!(hyp.emotion, "", "v1 : emotion toujours vide");
        assert!(
            hyp.acoustic_events.is_empty(),
            "v1 : acoustic_events toujours vide"
        );
        // Sérialisation stable du contrat.
        let json = serde_json::to_string(&hyp).expect("sérialisation");
        assert!(json.contains("\"emotion\":\"\""));
        assert!(json.contains("\"acoustic_events\":[]"));
    }

    fn workspace_models_dir() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("ANNORA_MODELS_DIR") {
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
        // v1 honnête : têtes auxiliaires non décodées.
        assert!(hyp.emotion.is_empty());
        assert!(hyp.acoustic_events.is_empty());
        assert!(
            dt.as_secs() < 60,
            "latence (incl. chargement) {dt:?} < 60 s"
        );
    }
}
