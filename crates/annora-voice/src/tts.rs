//! # TTS RÉEL v1 : moteur formantique (Klatt simplifié)
//!
//! `MOTEUR FORMANTIQUE RÉEL v1 (checkpoint neuronal CosyVoice2/NeuTTS non
//! provisionné — voir `provision_neural_checkpoint()` pour la bascule)`.
//!
//! - La synthèse est produite par [`formant`](crate::tts::formant) : vraie
//!   phonémisation FR/EN + source glottique à F0 + 5 formants + prosodie.
//!   Zéro sinusoïde.
//! - Voix paramétrable : `rate` (via `TtsRequest.speed_rate`), `pitch`
//!   (via `voice_id`, mapping documenté ci-dessous), `emotional_tone`
//!   (mapping F0/débit documenté).
//! - Streaming par chunks avec **continuité de phase inter-chunks** : un seul
//!   `FormantStream` est consommé séquentiellement, donc aucun clic de
//!   raccord (les états filtres + phase glottique sont conservés).
//! - Compat : `SimulatedSineTts` et `CosyVoice2Engine` restent des alias
//!   (dépréciés) vers [`FormantTts`] pour ne pas casser les appelants.

#[path = "formant.rs"]
pub mod formant;

use crate::codec::MIMI_SAMPLE_RATE;
use annora_core::{AnnoraError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub use formant::{FORMANT_SAMPLE_RATE, FormantStream, FormantVoice};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsRequest {
    pub text: String,
    pub voice_id: String,
    pub speed_rate: f32, // 1.0 = normal
    pub emotional_tone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsAudioChunk {
    pub sample_rate: u32,
    pub samples: Vec<f32>,
    pub is_final: bool,
}

#[async_trait]
pub trait TtsEngine: Send + Sync {
    async fn synthesize_stream(&self, req: &TtsRequest) -> Result<Vec<TtsAudioChunk>>;
}

/// Durée d'un chunk de streaming (250 ms @ 24 kHz = 6000 échantillons).
pub const TTS_CHUNK_SAMPLES: usize = 6000;

/// Moteur TTS formantique RÉEL v1 (pur Rust, 24 kHz).
pub struct FormantTts {
    default_voice: FormantVoice,
    chunk_samples: usize,
}

/// Alias de compatibilité (déprécié) : l'ancien moteur 440 Hz simulé n'existe
/// plus. Pointe vers le moteur formantique RÉEL pour ne pas casser les
/// appelants (`CosyVoice2Engine::new()` construit désormais un `FormantTts`).
#[deprecated(
    since = "0.1.0",
    note = "Moteur sinusoïdal simulé supprimé — utilisez FormantTts (synthèse formantique réelle). Même API."
)]
pub type SimulatedSineTts = FormantTts;

impl FormantTts {
    pub fn new() -> Self {
        Self {
            default_voice: FormantVoice::default(),
            chunk_samples: TTS_CHUNK_SAMPLES,
        }
    }

    pub fn with_chunk_samples(chunk_samples: usize) -> Self {
        Self {
            default_voice: FormantVoice::default(),
            chunk_samples: chunk_samples.max(480),
        }
    }

    pub fn engine_name(&self) -> String {
        "FormantTts REAL v1 (formants Klatt FR/EN, 24kHz — checkpoint neuronal non provisionné)"
            .to_string()
    }

    /// `false` : synthèse formantique réelle (plus de simulation).
    pub fn is_simulated(&self) -> bool {
        false
    }

    /// Taux attendu : 24 kHz (Mimi). Tout chunk hors taux est rejeté comme non conforme.
    pub fn expected_sample_rate(&self) -> u32 {
        MIMI_SAMPLE_RATE
    }

    /// Vérifie qu'un taux d'échantillonnage correspond au pipeline voix 24 kHz.
    pub fn check_sample_rate(&self, sample_rate: u32) -> bool {
        sample_rate == MIMI_SAMPLE_RATE
    }

    /// Bascule vers un checkpoint neuronal (CosyVoice2 / NeuTTS).
    ///
    /// NON PROVISIONNÉ : aucun checkpoint TTS neural n'est fourni dans
    /// `models/` à ce jour. Documente le contrat de bascule : le fichier
    /// attendu est un safetensors / ONNX de synthèse (à brancher sur
    /// `synthesize_stream` quand il sera provisionné).
    /// Retourne honnêtement une erreur tant que rien n'est provisionné.
    pub fn provision_neural_checkpoint(&mut self, path: &str) -> Result<()> {
        if std::path::Path::new(path).exists() {
            return Err(AnnoraError::InferenceError(format!(
                "Checkpoint TTS {} présent mais pilote CosyVoice2/NeuTTS non implémenté v1 — moteur formantique conservé",
                path
            )));
        }
        Err(AnnoraError::InferenceError(format!(
            "Checkpoint TTS neural non provisionné ({} introuvable) — moteur formantique REAL v1 actif",
            path
        )))
    }

    /// Construit la voix effective d'une requête (mappings documentés).
    pub fn voice_for_request(&self, req: &TtsRequest) -> FormantVoice {
        // Pitch : F0 de base 120 Hz ; voix contenant "fem"/"alice"/"femme"/
        // "soprano"/"aigu" → 200 Hz ; "grave"/"bob"/"low" → 95 Hz.
        let vid = req.voice_id.to_lowercase();
        let mut f0 = if vid.contains("fem")
            || vid.contains("alice")
            || vid.contains("soprano")
            || vid.contains("aigu")
            || vid.contains("elle")
        {
            200.0
        } else if vid.contains("grave") || vid.contains("bob") || vid.contains("low") {
            95.0
        } else {
            self.default_voice.f0_base_hz
        };
        let mut rate = req.speed_rate.clamp(0.5, 2.0);
        // Tonalité émotionnelle → F0/débit (mapping v1 documenté).
        let tone = req.emotional_tone.to_lowercase();
        if tone.contains("joy") || tone.contains("joi") || tone.contains("enthous") {
            f0 *= 1.10;
            rate = (rate * 1.05).clamp(0.5, 2.0);
        } else if tone.contains("sad") || tone.contains("trist") || tone.contains("calm") {
            f0 *= 0.90;
            rate = (rate * 0.92).clamp(0.5, 2.0);
        } else if tone.contains("ang") || tone.contains("col") || tone.contains("urg") {
            f0 *= 1.06;
            rate = (rate * 1.10).clamp(0.5, 2.0);
        }
        let mut voice = FormantVoice::new(f0, rate);
        voice.breathiness = self.default_voice.breathiness;
        voice.gain = self.default_voice.gain;
        voice
    }

    /// Synthèse synchrone one-shot (brique utilisée par `moshi.rs`).
    pub fn synthesize_blocking(&self, req: &TtsRequest) -> Result<Vec<f32>> {
        debug_assert_eq!(FORMANT_SAMPLE_RATE, MIMI_SAMPLE_RATE);
        if !self.check_sample_rate(FORMANT_SAMPLE_RATE) {
            return Err(AnnoraError::InferenceError(
                "taux formantique non 24kHz".into(),
            ));
        }
        let voice = self.voice_for_request(req);
        Ok(formant::synthesize(&req.text, &voice))
    }
}

impl Default for FormantTts {
    fn default() -> Self {
        Self::new()
    }
}

// --- Compatibilité : l'ancien nom reste constructible et redirige vers le
// --- moteur formantique RÉEL (même API : `new()`, `engine_name()`, …).
// --- Le nom historique est conservé uniquement pour les appelants existants.
pub type CosyVoice2Engine = FormantTts;

#[async_trait]
impl TtsEngine for FormantTts {
    async fn synthesize_stream(&self, req: &TtsRequest) -> Result<Vec<TtsAudioChunk>> {
        let voice = self.voice_for_request(req);
        // UN SEUL stream consommé séquentiellement → continuité de phase
        // et d'états de filtres entre chunks (zéro clic de raccord).
        let mut stream = FormantStream::new(&req.text, &voice);
        if stream.total_samples() == 0 {
            return Ok(vec![TtsAudioChunk {
                sample_rate: MIMI_SAMPLE_RATE,
                samples: Vec::new(),
                is_final: true,
            }]);
        }
        let mut chunks = Vec::new();
        loop {
            let samples = stream.next_chunk(self.chunk_samples);
            let done = stream.is_done();
            chunks.push(TtsAudioChunk {
                sample_rate: MIMI_SAMPLE_RATE,
                samples,
                is_final: done,
            });
            if done {
                break;
            }
        }
        Ok(chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::SAMPLES_PER_FRAME;

    fn req(text: &str) -> TtsRequest {
        TtsRequest {
            text: text.to_string(),
            voice_id: "default".to_string(),
            speed_rate: 1.0,
            emotional_tone: "neutral".to_string(),
        }
    }

    #[tokio::test]
    async fn test_chunks_concatenes_egaux_oneshot_sans_clic() {
        let engine = FormantTts::new();
        assert!(!engine.is_simulated());
        let text = "bonjour madame comment allez vous";
        let chunks = engine.synthesize_stream(&req(text)).await.expect("stream");
        assert!(
            chunks.len() >= 2,
            "attendu plusieurs chunks, got {}",
            chunks.len()
        );
        assert!(chunks.last().unwrap().is_final);
        assert!(chunks.iter().take(chunks.len() - 1).all(|c| !c.is_final));
        for c in &chunks {
            assert!(engine.check_sample_rate(c.sample_rate));
            assert!(c.samples.iter().all(|x| x.is_finite()));
        }
        let concat: Vec<f32> = chunks
            .iter()
            .flat_map(|c| c.samples.iter().copied())
            .collect();
        let one = engine.synthesize_blocking(&req(text)).expect("oneshot");
        assert_eq!(concat.len(), one.len());
        let maxd: f32 = concat
            .iter()
            .zip(one.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(maxd < 1e-6, "écart max={}", maxd);
        // Pas de clic aux raccords : discontinuité locale bornée.
        let mut boundaries = Vec::new();
        let mut off = 0usize;
        for c in chunks.iter().take(chunks.len().saturating_sub(1)) {
            off += c.samples.len();
            boundaries.push(off);
        }
        for b in boundaries {
            if b > 8 && b + 8 < concat.len() {
                let before: f32 = concat[b - 8..b].iter().map(|x| x.abs()).fold(0.0, f32::max);
                let after: f32 = concat[b..b + 8].iter().map(|x| x.abs()).fold(0.0, f32::max);
                let jump = (concat[b] - concat[b - 1]).abs();
                let local = (before + after + 1e-4) * 3.0;
                assert!(
                    jump <= local,
                    "clic en {}: jump={} local={}",
                    b,
                    jump,
                    local
                );
            }
        }
        // Durée proportionnelle au texte ; premier contenu non silencieux.
        let short = engine.synthesize_blocking(&req("a")).expect("court");
        assert!(one.len() > short.len() * 3);
        let e: f32 = one.iter().map(|x| x * x).sum::<f32>() / one.len() as f32;
        assert!(e > 1e-6);
        let _ = SAMPLES_PER_FRAME;
    }

    #[tokio::test]
    async fn test_voix_parametrable_rate_pitch() {
        let mut engine = FormantTts::new();
        let base = engine.synthesize_blocking(&req("bonjour")).expect("base");
        let fast_req = TtsRequest {
            speed_rate: 1.5,
            ..req("bonjour")
        };
        let fast = engine.synthesize_blocking(&fast_req).expect("fast");
        assert!(
            fast.len() < base.len(),
            "rate doit raccourcir : {} vs {}",
            fast.len(),
            base.len()
        );
        let fem_req = TtsRequest {
            voice_id: "alice-femme".to_string(),
            ..req("aaa")
        };
        let fem = engine.synthesize_blocking(&fem_req).expect("fem");
        let mes = formant::estimate_f0_autocorr(&fem, MIMI_SAMPLE_RATE).expect("F0");
        assert!((mes - 200.0).abs() / 200.0 < 0.20, "F0 mesurée={}", mes);
        // Checkpoint neural non provisionné : erreur honnête.
        assert!(
            engine
                .provision_neural_checkpoint("models/cosyvoice2.safetensors")
                .is_err()
        );
    }
}
