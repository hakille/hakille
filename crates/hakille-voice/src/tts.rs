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
use async_trait::async_trait;
use hakille_core::{HakilleError, Result};
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

/// Vitesse minimale acceptée par [`FormantTts::render_pcm`] / `render_pcm_stream`.
pub const RENDER_MIN_SPEED: f32 = 0.7;
/// Vitesse maximale acceptée par [`FormantTts::render_pcm`] / `render_pcm_stream`.
pub const RENDER_MAX_SPEED: f32 = 1.6;
/// Taille de chunk par défaut du rendu PCM streamé (80 ms = une trame Mimi).
pub const PCM_CHUNK_MS: u32 = 80;

/// Bloc PCM mono `f32` prêt à être envoyé à la carte son.
///
/// Convention du pipeline voix Hakille : 24 kHz mono (`FORMANT_SAMPLE_RATE`),
/// échantillons normalisés dans `[-1, 1]` après saturation douce `tanh`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcmChunk {
    /// Échantillons mono `f32` (–1.0 … 1.0).
    pub samples: Vec<f32>,
    /// Taux d'échantillonnage (24 000 Hz).
    pub sample_rate: u32,
}

impl PcmChunk {
    /// Durée du bloc en secondes.
    pub fn duration_seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.samples.len() as f64 / self.sample_rate as f64
        }
    }
}

/// Erreurs honnêtes du rendu PCM (API directe carte son).
#[derive(Debug, thiserror::Error)]
pub enum TtsError {
    /// Vitesse hors bornes supportées [`RENDER_MIN_SPEED`, `RENDER_MAX_SPEED`].
    #[error("vitesse {0} invalide : bornes supportées [{RENDER_MIN_SPEED}, {RENDER_MAX_SPEED}]")]
    InvalidSpeed(f32),
    /// Texte sans aucun échantillon synthétisable (vide ou blanc).
    #[error("aucun phonème à synthétiser : texte vide")]
    EmptyText,
    /// Taux d'échantillonnage incohérent avec le pipeline 24 kHz.
    #[error("taux d'échantillonnage {0} Hz non supporté (attendu 24000)")]
    UnsupportedSampleRate(u32),
    /// Erreur du moteur TTS sous-jacent.
    #[error(transparent)]
    Engine(#[from] HakilleError),
}

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
            return Err(HakilleError::InferenceError(format!(
                "Checkpoint TTS {} présent mais pilote CosyVoice2/NeuTTS non implémenté v1 — moteur formantique conservé",
                path
            )));
        }
        Err(HakilleError::InferenceError(format!(
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
            return Err(HakilleError::InferenceError(
                "taux formantique non 24kHz".into(),
            ));
        }
        let voice = self.voice_for_request(req);
        Ok(formant::synthesize(&req.text, &voice))
    }

    /// Construit la voix de rendu PCM : voix par défaut, `rate = speed`.
    ///
    /// `speed` doit être fini et dans [`RENDER_MIN_SPEED`, `RENDER_MAX_SPEED`],
    /// sinon `Err(TtsError::InvalidSpeed)` (pas de clamp silencieux).
    fn render_voice(&self, speed: f32) -> std::result::Result<FormantVoice, TtsError> {
        if !speed.is_finite() || !(RENDER_MIN_SPEED..=RENDER_MAX_SPEED).contains(&speed) {
            return Err(TtsError::InvalidSpeed(speed));
        }
        let mut voice = self.default_voice.clone();
        voice.rate = speed;
        Ok(voice)
    }

    /// Rend le texte complet en un bloc PCM 24 kHz mono `f32`, prêt pour la
    /// carte son (échantillons dans `[-1, 1]`, silence de fin inclus).
    ///
    /// `speed` (0.7–1.6) : > 1.0 = plus rapide. `Err(InvalidSpeed)` sinon,
    /// `Err(EmptyText)` si le texte ne produit aucun échantillon.
    pub fn render_pcm(&self, text: &str, speed: f32) -> std::result::Result<PcmChunk, TtsError> {
        let voice = self.render_voice(speed)?;
        let samples = formant::synthesize(text, &voice);
        if samples.is_empty() {
            return Err(TtsError::EmptyText);
        }
        Ok(PcmChunk {
            samples,
            sample_rate: FORMANT_SAMPLE_RATE,
        })
    }

    /// Rend le texte en plusieurs blocs PCM de `chunk_ms` millisecondes
    /// (80 ms par défaut si `chunk_ms == 0`), à consommer au fil de l'eau.
    ///
    /// **Continuité de phase prouvée** : tous les blocs sont produits par un
    /// seul [`FormantStream`] consommé séquentiellement — la concaténation
    /// est bit-à-bit égale à [`Self::render_pcm`] (mêmes états de filtres et
    /// phase glottique, aucun clic de raccord).
    pub fn render_pcm_stream(
        &self,
        text: &str,
        speed: f32,
        chunk_ms: u32,
    ) -> std::result::Result<Vec<PcmChunk>, TtsError> {
        let voice = self.render_voice(speed)?;
        let chunk_ms = if chunk_ms == 0 {
            PCM_CHUNK_MS
        } else {
            chunk_ms
        };
        let chunk_samples = ((chunk_ms as u64 * FORMANT_SAMPLE_RATE as u64) / 1000).max(1) as usize;
        let mut stream = FormantStream::new(text, &voice);
        if stream.total_samples() == 0 {
            return Err(TtsError::EmptyText);
        }
        let mut chunks = Vec::new();
        while !stream.is_done() {
            let samples = stream.next_chunk(chunk_samples);
            if samples.is_empty() {
                break;
            }
            chunks.push(PcmChunk {
                samples,
                sample_rate: FORMANT_SAMPLE_RATE,
            });
        }
        if chunks.is_empty() {
            return Err(TtsError::EmptyText);
        }
        Ok(chunks)
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

    #[test]
    fn test_render_pcm_non_vide_duree_plausible_et_deterministe() {
        let engine = FormantTts::new();
        let text = "bonjour madame comment allez vous";
        let pcm = engine.render_pcm(text, 1.0).expect("render_pcm");
        assert_eq!(pcm.sample_rate, MIMI_SAMPLE_RATE);
        assert!(!pcm.samples.is_empty());
        assert!(pcm.samples.iter().all(|x| x.is_finite()));
        assert!(
            pcm.samples.iter().any(|x| x.abs() > 1e-4),
            "audio non silencieux"
        );
        // Durée plausible : entre 20 ms et 150 ms par phonème.
        let n_ph = formant::phonemize(text).len();
        let ms = pcm.duration_seconds() * 1000.0;
        assert!(
            ms >= n_ph as f64 * 20.0 && ms <= n_ph as f64 * 150.0,
            "durée={} ms pour {} phonèmes",
            ms,
            n_ph
        );
        // Déterminisme : deux appels identiques.
        let pcm2 = engine.render_pcm(text, 1.0).expect("render_pcm 2");
        assert_eq!(pcm, pcm2);
    }

    #[test]
    fn test_render_pcm_vitesse_bornee_et_ratio_2x() {
        let engine = FormantTts::new();
        for bad in [0.0f32, 0.5, 0.69, 1.61, 2.0, f32::NAN, f32::INFINITY] {
            assert!(
                matches!(
                    engine.render_pcm("bonjour", bad),
                    Err(TtsError::InvalidSpeed(_))
                ),
                "speed={} doit être refusée",
                bad
            );
        }
        assert!(matches!(
            engine.render_pcm("", 1.0),
            Err(TtsError::EmptyText)
        ));
        assert!(matches!(
            engine.render_pcm("   ", 1.0),
            Err(TtsError::EmptyText)
        ));
        // 0.8 vs 1.6 : facteur 2 exact dans le pipeline (durées / rate).
        let text = "bonjour madame comment allez vous";
        let slow = engine.render_pcm(text, 0.8).expect("lent");
        let fast = engine.render_pcm(text, 1.6).expect("rapide");
        let ratio = slow.samples.len() as f32 / fast.samples.len() as f32;
        assert!((ratio - 2.0).abs() < 0.1, "ratio={}", ratio);
        // Bornes incluses acceptées.
        assert!(engine.render_pcm("bonjour", 0.7).is_ok());
        assert!(engine.render_pcm("bonjour", 1.6).is_ok());
    }

    #[test]
    fn test_render_pcm_stream_concat_egale_oneshot_et_phase() {
        let engine = FormantTts::new();
        let text = "Bonjour madame, comment allez-vous ? j'espère que tout va bien.";
        let one = engine.render_pcm(text, 1.0).expect("oneshot");
        let chunks = engine.render_pcm_stream(text, 1.0, 80).expect("stream");
        assert!(chunks.len() >= 2, "attendu plusieurs chunks");
        let per_chunk = (PCM_CHUNK_MS as usize * MIMI_SAMPLE_RATE as usize) / 1000;
        for c in &chunks[..chunks.len() - 1] {
            assert_eq!(c.sample_rate, MIMI_SAMPLE_RATE);
            assert_eq!(c.samples.len(), per_chunk);
        }
        assert_eq!(chunks.last().unwrap().sample_rate, MIMI_SAMPLE_RATE);
        // Concaténation bit-à-bit égale au rendu one-shot.
        let concat: Vec<f32> = chunks
            .iter()
            .flat_map(|c| c.samples.iter().copied())
            .collect();
        assert_eq!(concat.len(), one.samples.len());
        let maxd: f32 = concat
            .iter()
            .zip(one.samples.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(maxd < 1e-6, "écart max={}", maxd);
        // Continuité de phase : pas de clic aux raccords (même critère que
        // le test de streaming existant).
        let mut off = 0usize;
        for c in &chunks[..chunks.len() - 1] {
            off += c.samples.len();
            if off > 8 && off + 8 < concat.len() {
                let before = concat[off - 8..off]
                    .iter()
                    .map(|x| x.abs())
                    .fold(0.0, f32::max);
                let after = concat[off..off + 8]
                    .iter()
                    .map(|x| x.abs())
                    .fold(0.0, f32::max);
                let jump = (concat[off] - concat[off - 1]).abs();
                assert!(
                    jump <= (before + after + 1e-4) * 3.0,
                    "clic en {}: jump={}",
                    off,
                    jump
                );
            }
        }
        // chunk_ms == 0 → défaut 80 ms.
        let default_chunks = engine.render_pcm_stream(text, 1.0, 0).expect("défaut");
        assert_eq!(default_chunks[0].samples.len(), per_chunk);
        // Erreurs honnêtes : vitesse invalide et texte vide.
        assert!(engine.render_pcm_stream(text, 0.5, 80).is_err());
        assert!(engine.render_pcm_stream("", 1.0, 80).is_err());
    }
}
