//! VAD 100 % RÉEL : délégation exclusive à [`hakille_inference::OrtVadSession`].
//!
//! - Aucune heuristique RMS/ZCR : `speech_probability` provient uniquement de
//!   forwards `ort` sur `models/silero_vad.onnx` (fenêtres 512 @16 kHz).
//! - Modèle absent / illisible -> erreur LOUD
//!   ([`hakille_core::HakilleError::InferenceError`]) sur `process_real`, et
//!   `tracing::error!` + résultat négatif sur `process` — **jamais de repli
//!   silencieux vers une heuristique, jamais de probabilité inventée**.
//! - `hangover_frames` reste configurable et est alimenté par les vraies
//!   probabilités (seuil `threshold`).
//! - `energy_rms` est une MESURE (RMS réel du chunk), pas un score de parole.

use crate::audio::capture::AudioChunk;
use hakille_inference::{OrtVadSession, resolve_vad_model_path, to_mono_16k};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VadResult {
    pub is_speech: bool,
    pub speech_probability: f32,
    pub energy_rms: f32,
}

/// Détecteur d'activité vocale RÉEL (Silero via `ort`, état récurrent).
pub struct VoiceActivityDetector {
    threshold: f32,
    hangover_frames: usize,
    active_hangover: usize,
    model_path: Option<PathBuf>,
    real_session: Option<OrtVadSession>,
    last_used_real: bool,
}

impl VoiceActivityDetector {
    /// Seuil + hangover, modèle résolu par défaut (`None` si absent : l'erreur
    /// LOUD sera émise à l'usage, jamais de fallback silencieux).
    pub fn new(threshold: f32, hangover_frames: usize) -> Self {
        let model_path = resolve_vad_model_path(None).ok();
        Self {
            threshold,
            hangover_frames,
            active_hangover: 0,
            model_path,
            real_session: None,
            last_used_real: false,
        }
    }

    /// Seuil + hangover + dossier modèles explicite — erreur LOUD si absent.
    pub fn with_models_dir(
        models_dir: impl AsRef<Path>,
        threshold: f32,
        hangover_frames: usize,
    ) -> hakille_core::Result<Self> {
        let path = resolve_vad_model_path(Some(models_dir.as_ref()))?;
        Ok(Self {
            threshold,
            hangover_frames,
            active_hangover: 0,
            model_path: Some(path),
            real_session: None,
            last_used_real: false,
        })
    }

    /// Chemin de modèle explicite (fichier `.onnx`).
    pub fn with_onnx_model(path: impl AsRef<Path>) -> Self {
        Self {
            threshold: 0.5,
            hangover_frames: 3,
            active_hangover: 0,
            model_path: Some(path.as_ref().to_path_buf()),
            real_session: None,
            last_used_real: false,
        }
    }

    pub fn model_path(&self) -> Option<&Path> {
        self.model_path.as_deref()
    }

    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    pub fn hangover_frames(&self) -> usize {
        self.hangover_frames
    }

    /// Honnêteté mesurable : `true` uniquement si le dernier appel a réellement
    /// exécuté des forwards Silero via `ort` (jamais d'heuristique).
    pub fn last_used_real(&self) -> bool {
        self.last_used_real
    }

    /// Probabilité réelle du dernier forward ? Non — compteur de forwards réels
    /// de la session sous-jacente (`None` tant que la session n'est pas ouverte).
    pub fn real_forward_calls(&self) -> Option<u64> {
        self.real_session.as_ref().map(|s| s.forward_calls())
    }

    pub fn default_silero_profile() -> Self {
        Self::new(0.5, 3)
    }

    /// Réinitialise hangover + état récurrent + carry de la session réelle.
    pub fn reset(&mut self) {
        self.active_hangover = 0;
        if let Some(sess) = self.real_session.as_mut() {
            sess.reset_state();
        }
    }

    /// Ouvre la session réelle UNE fois (cache) — erreur LOUD sinon.
    fn ensure_session(&mut self) -> hakille_core::Result<&mut OrtVadSession> {
        if self.real_session.is_none() {
            let p = self.model_path.clone().ok_or_else(|| {
                hakille_core::HakilleError::InferenceError(
                    "VoiceActivityDetector: aucun model_path configuré et résolution \
                     par défaut impossible (models/silero_vad.onnx introuvable, \
                     $HAKILLE_MODELS_DIR vide) — VAD réel indisponible, pas de fallback"
                        .to_string(),
                )
            })?;
            if !p.exists() {
                return Err(hakille_core::HakilleError::InferenceError(format!(
                    "VoiceActivityDetector: modèle VAD absent '{}' — VAD réel \
                     indisponible, pas de fallback",
                    p.display()
                )));
            }
            let sess = OrtVadSession::open(&p)?;
            self.real_session = Some(sess);
        }
        self.real_session.as_mut().ok_or_else(|| {
            hakille_core::HakilleError::InferenceError(
                "VoiceActivityDetector: session réelle non ouverte (incohérence interne)"
                    .to_string(),
            )
        })
    }

    /// RMS réel (mesure, sanitize `NaN`/`Inf` -> `0.0`).
    fn measure_rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let mut sum_sq = 0.0f32;
        for s in samples.iter() {
            let v = *s;
            let clean = if v.is_finite() { v } else { 0.0 };
            sum_sq += clean * clean;
        }
        let rms = (sum_sq / samples.len() as f32).sqrt();
        if rms.is_finite() { rms } else { 0.0 }
    }

    fn apply_hangover(&mut self, probability: f32) -> bool {
        let instantaneous = probability >= self.threshold;
        if instantaneous {
            self.active_hangover = self.hangover_frames;
        } else if self.active_hangover > 0 {
            self.active_hangover -= 1;
        }
        instantaneous || self.active_hangover > 0
    }

    /// Cœur 100 % réel : resample mono16k + forwards `ort` (moyenne des
    /// fenêtres 512). Retourne `(probabilité moyenne réelle, forward_exécuté)`.
    fn real_probability(&mut self, chunk: &AudioChunk) -> hakille_core::Result<(f32, bool)> {
        if chunk.samples.is_empty() {
            return Ok((0.0, false));
        }
        let mono = to_mono_16k(&chunk.samples, chunk.channels, chunk.sample_rate);
        if mono.is_empty() {
            // Chunk dégénéré (sr/canaux à 0) : rien à scorer -> 0.0 conservateur.
            return Ok((0.0, false));
        }
        let sess = self.ensure_session()?;
        let outs = sess.process_stream(&mono)?;
        if outs.is_empty() {
            // Moins de 512 échantillons cumulés (carry) : pas encore de fenêtre.
            return Ok((0.0, false));
        }
        let mut sum = 0.0f32;
        let mut count = 0usize;
        for (_, p) in outs.iter() {
            sum += *p;
            count += 1;
        }
        if count == 0 {
            return Ok((0.0, false));
        }
        let mean = sum / count as f32;
        Ok((
            if mean.is_finite() {
                mean.clamp(0.0, 1.0)
            } else {
                0.0
            },
            true,
        ))
    }

    /// Chemin infaillible (compat) : 100 % réel ; en cas d'échec (modèle
    /// absent, forward `ort` en erreur) émet `tracing::error!` LOUD et retourne
    /// un résultat NÉGATIF — jamais de probabilité heuristique.
    pub fn process(&mut self, chunk: &AudioChunk) -> VadResult {
        let rms = Self::measure_rms(&chunk.samples);
        if chunk.samples.is_empty() {
            self.last_used_real = false;
            return VadResult {
                is_speech: false,
                speech_probability: 0.0,
                energy_rms: 0.0,
            };
        }
        match self.real_probability(chunk) {
            Ok((probability, forward_ran)) => {
                let is_speech = self.apply_hangover(probability);
                self.last_used_real = forward_ran;
                VadResult {
                    is_speech,
                    speech_probability: probability,
                    energy_rms: rms,
                }
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    "VoiceActivityDetector::process: VAD réel en échec — résultat \
                     négatif (AUCUN fallback heuristique)"
                );
                self.last_used_real = false;
                self.active_hangover = 0;
                VadResult {
                    is_speech: false,
                    speech_probability: 0.0,
                    energy_rms: rms,
                }
            }
        }
    }

    /// Chemin réel faillible : forwards `ort` uniquement ; toute anomalie
    /// (modèle absent, chunk inscorable, `ort` en erreur) remonte en `Err`
    /// LOUD — aucun repli, aucune valeur simulée.
    pub fn process_real(&mut self, chunk: &AudioChunk) -> hakille_core::Result<VadResult> {
        let rms = Self::measure_rms(&chunk.samples);
        if chunk.samples.is_empty() {
            self.last_used_real = false;
            return Ok(VadResult {
                is_speech: false,
                speech_probability: 0.0,
                energy_rms: 0.0,
            });
        }
        let (probability, forward_ran) = self.real_probability(chunk)?;
        let is_speech = self.apply_hangover(probability);
        self.last_used_real = forward_ran;
        Ok(VadResult {
            is_speech,
            speech_probability: probability,
            energy_rms: rms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_core::Timestamp;

    fn chunk(samples: Vec<f32>, sample_rate: u32, channels: u16) -> AudioChunk {
        AudioChunk {
            samples,
            sample_rate,
            channels,
            timestamp: Timestamp::now(),
        }
    }

    #[test]
    fn real_silence_is_negative_and_honest() {
        let mut vad = VoiceActivityDetector::default_silero_profile();
        let res = vad.process(&chunk(vec![0.0f32; 1600], 16_000, 1));
        assert!(!res.is_speech);
        assert!(res.speech_probability < 0.3);
        assert_eq!(res.energy_rms, 0.0);
        assert!(
            vad.last_used_real(),
            "le chemin doit être le forward réel, jamais l'heuristique"
        );
        assert!(
            vad.real_forward_calls().unwrap_or(0) > 0,
            "au moins un forward ort réel doit avoir été exécuté"
        );
    }

    #[test]
    fn hangover_is_fed_by_real_probs() {
        // Seuil 0.0 : toute prob réelle (même ~0.0005 du silence) arme le hangover.
        let mut vad = VoiceActivityDetector::new(0.0, 2);
        let r1 = vad.process(&chunk(vec![0.0f32; 1600], 16_000, 1));
        assert!(r1.is_speech, "hangover armé par prob réelle >= 0.0");
        assert!(vad.last_used_real());
        // Seuil 1.0 : jamais de parole, même sur signal voisé fort.
        let mut strict = VoiceActivityDetector::new(1.0, 5);
        let loud = vec![0.5f32; 1600];
        let r2 = strict.process(&chunk(loud, 16_000, 1));
        assert!(!r2.is_speech);
        assert!(strict.last_used_real(), "même négatif, le chemin est réel");
    }

    #[test]
    fn missing_model_is_loud_never_silent() {
        let mut vad = VoiceActivityDetector::with_onnx_model("/inexistant/quasi-certain/nope.onnx");
        // `process` : erreur LOUD (tracée) + résultat négatif, pas d'heuristique.
        let res = vad.process(&chunk(vec![0.5f32; 1600], 16_000, 1));
        assert!(!res.is_speech);
        assert_eq!(res.speech_probability, 0.0);
        assert!(!vad.last_used_real());
        // `process_real` : erreur LOUD remontée.
        let err = vad.process_real(&chunk(vec![0.5f32; 1600], 16_000, 1));
        assert!(err.is_err());
        let msg = format!("{}", err.expect_err("doit être une erreur"));
        assert!(
            msg.contains("absent") || msg.contains("Inference"),
            "erreur LOUD attendue: {msg}"
        );
    }

    #[test]
    fn degenerate_chunks_never_panic() {
        let mut vad = VoiceActivityDetector::default_silero_profile();
        let bad = chunk(vec![f32::NAN, f32::INFINITY, 0.2], 0, 0);
        let res = vad.process(&bad);
        assert!(res.energy_rms.is_finite());
        assert!((0.0..=1.0).contains(&res.speech_probability));
        let empty = vad.process(&chunk(vec![], 16_000, 1));
        assert!(!empty.is_speech);
    }
}
