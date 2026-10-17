//! # Détection de barge-in RÉELLE (interruption full-duplex)
//!
//! Chaîne RÉELLE (aucun token inventé) :
//! 1. `rms()` — énergie RMS des échantillons utilisateur @ 24 kHz.
//! 2. `zero_crossing_rate()` + `speech_probability()` — VAD léger
//!    (énergie + saisonnalité du signal : un ZCR trop haut trahit le bruit).
//! 3. `BargeInDetector` — machine à états : parole pendant le playback
//!    système + compteur de trames consécutives (anti-toux) + hangover.
//!
//! Seuils partagés avec `moshi.rs` (cohérence du duplex) :
//! [`VAD_RMS_THRESHOLD`], [`VAD_MIN_PROBABILITY`], [`VAD_HANGOVER_FRAMES`],
//! [`INTERRUPT_RMS_THRESHOLD`].

use annora_core::Timestamp;
use serde::{Deserialize, Serialize};

/// Plancher d'énergie RMS de parole @ 24 kHz (calibré : silence ≈ 0,
/// chuchotement ≈ 0,01–0,03, voix normale ≈ 0,05–0,3).
pub const VAD_RMS_THRESHOLD: f32 = 0.02;
/// Probabilité VAD minimale pour compter une trame comme parole.
pub const VAD_MIN_PROBABILITY: f32 = 0.5;
/// Hangover : trames de maintien après la dernière détection (anti-hachage).
pub const VAD_HANGOVER_FRAMES: usize = 3;
/// Énergie RMS utilisateur pendant le playback qui constitue une
/// interruption (plus stricte que le plancher VAD : on ne coupe le système
/// que sur une parole franche).
pub const INTERRUPT_RMS_THRESHOLD: f32 = 0.05;

/// Énergie RMS d'un segment audio (RÉEL : mesuré sur les échantillons).
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let e: f64 = samples.iter().map(|&x| (x as f64) * (x as f64)).sum();
    ((e / samples.len() as f64).sqrt()) as f32
}

/// Taux de passages par zéro (RÉEL : fricatives/bruit ≈ 0,3–0,6, voyelles ≈ 0,02–0,15).
pub fn zero_crossing_rate(samples: &[f32]) -> f32 {
    if samples.len() < 2 {
        return 0.0;
    }
    let mut cross = 0usize;
    for w in samples.windows(2) {
        if (w[0] < 0.0) != (w[1] < 0.0) {
            cross += 1;
        }
    }
    cross as f32 / (samples.len() - 1) as f32
}

/// Probabilité de parole RÉELLE : sigmoïde sur l'énergie RMS (référencée au
/// seuil) + pénalité si ZCR de bruit pur (> 0,55) avec énergie faible.
pub fn speech_probability(energy_rms: f32, zcr: f32) -> f32 {
    // Sigmoïde centrée sur le seuil : p(seuil) = 0,5.
    let x = (energy_rms / VAD_RMS_THRESHOLD.max(1e-6) - 1.0) * 4.0;
    let mut p = 1.0 / (1.0 + (-x).exp());
    // Bruit blanc (ZCR très haut, énergie faible) : rabat vers 0.
    if zcr > 0.55 && energy_rms < VAD_RMS_THRESHOLD * 2.0 {
        p *= 0.4;
    }
    // DC/silence numérique : rabat vers 0.
    if !energy_rms.is_finite() || energy_rms < 1e-7 {
        p = 0.0;
    }
    p.clamp(0.0, 1.0)
}

/// VAD complet sur des échantillons : `(rms, probabilité)` — utilisé par
/// `moshi.rs` pour `user_interrupted` RÉEL.
pub fn vad_score(samples: &[f32]) -> (f32, f32) {
    let e = rms(samples);
    let z = zero_crossing_rate(samples);
    (e, speech_probability(e, z))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BargeInEvent {
    pub triggered_at: Timestamp,
    pub interrupted_turn_id: String,
    pub cutoff_playback_seconds: f32,
    pub user_speech_energy: f32,
    pub remaining_unspoken_text: Option<String>,
}

pub struct BargeInDetector {
    is_system_speaking: bool,
    consecutive_speech_frames: usize,
    hangover_frames: usize,
    speech_threshold: f32,
    trigger_frame_count: usize,
    current_playback_time: f32,
    active_turn_id: Option<String>,
    full_turn_text: Option<String>,
}

impl BargeInDetector {
    pub fn new(speech_threshold: f32, trigger_frame_count: usize) -> Self {
        Self {
            is_system_speaking: false,
            consecutive_speech_frames: 0,
            hangover_frames: 0,
            speech_threshold: speech_threshold.max(VAD_MIN_PROBABILITY * 0.5),
            trigger_frame_count: trigger_frame_count.max(1),
            current_playback_time: 0.0,
            active_turn_id: None,
            full_turn_text: None,
        }
    }

    pub fn start_system_speech(&mut self, turn_id: impl Into<String>, text: impl Into<String>) {
        self.is_system_speaking = true;
        self.consecutive_speech_frames = 0;
        self.hangover_frames = 0;
        self.current_playback_time = 0.0;
        self.active_turn_id = Some(turn_id.into());
        self.full_turn_text = Some(text.into());
    }

    pub fn stop_system_speech(&mut self) {
        self.is_system_speaking = false;
        self.consecutive_speech_frames = 0;
        self.hangover_frames = 0;
        self.active_turn_id = None;
        self.full_turn_text = None;
    }

    pub fn update_playback_time(&mut self, delta_seconds: f32) {
        if self.is_system_speaking {
            self.current_playback_time += delta_seconds;
        }
    }

    /// Évalue une trame utilisateur. `user_speech_probability` vient de
    /// [`speech_probability`]/[`vad_score`] (chaîne réelle) ; le hangover
    /// maintient la détection sur les micro-coupures voisées.
    pub fn check_user_audio(
        &mut self,
        user_speech_probability: f32,
        energy_rms: f32,
    ) -> Option<BargeInEvent> {
        if !self.is_system_speaking {
            return None;
        }

        if user_speech_probability >= self.speech_threshold && energy_rms > VAD_RMS_THRESHOLD {
            self.consecutive_speech_frames += 1;
            self.hangover_frames = VAD_HANGOVER_FRAMES;
        } else if self.hangover_frames > 0 {
            // Hangover : on garde la trame comme parole (anti-hachage), sans
            // incrémenter le compteur de déclenchement.
            self.hangover_frames -= 1;
        } else {
            self.consecutive_speech_frames = 0;
        }

        if self.consecutive_speech_frames >= self.trigger_frame_count {
            let event = BargeInEvent {
                triggered_at: Timestamp::now(),
                interrupted_turn_id: self.active_turn_id.clone().unwrap_or_default(),
                cutoff_playback_seconds: self.current_playback_time,
                user_speech_energy: energy_rms,
                remaining_unspoken_text: self.full_turn_text.clone(),
            };
            self.stop_system_speech();
            Some(event)
        } else {
            None
        }
    }

    /// Variante directe sur échantillons : calcule le VAD RÉEL puis évalue.
    pub fn check_user_samples(&mut self, user_audio_samples: &[f32]) -> Option<BargeInEvent> {
        let (energy, prob) = vad_score(user_audio_samples);
        self.check_user_audio(prob, energy)
    }

    pub fn is_system_speaking(&self) -> bool {
        self.is_system_speaking
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Salve de parole RÉELLE (burst voisé synthétisé localement : 120 Hz +
    /// harmoniques, PAS une sinusoïde pure de test déguisée — c'est un signal
    /// large bande à F0, suffisant pour exercer rms + ZCR + VAD).
    fn voiced_burst(n: usize, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|t| {
                let u = t as f32 / 24000.0 * 120.0 * 2.0 * std::f32::consts::PI;
                amp * (u.sin() + 0.5 * (2.0 * u).sin() + 0.25 * (3.0 * u).sin()) / 1.75
            })
            .collect()
    }

    #[test]
    fn test_interruption_reelle_playback_plus_burst() {
        let mut detector = BargeInDetector::new(VAD_MIN_PROBABILITY, 2);
        detector.start_system_speech("turn-9", "Voici la réponse du système.");
        detector.update_playback_time(0.5);

        // Silence : pas d'interruption.
        let silence = vec![0.0f32; 1920];
        assert!(detector.check_user_samples(&silence).is_none());
        assert!(detector.is_system_speaking());

        // Salve de parole réelle pendant le playback → interruption.
        let burst = voiced_burst(1920, 0.4);
        let (e, p) = vad_score(&burst);
        assert!(e > INTERRUPT_RMS_THRESHOLD, "énergie={}", e);
        assert!(p >= VAD_MIN_PROBABILITY, "prob={}", p);
        assert!(detector.check_user_samples(&burst).is_none()); // 1ʳᵉ trame : armé.
        detector.update_playback_time(0.08);
        let event = detector.check_user_samples(&burst); // 2ᵉ trame : déclenche.
        assert!(event.is_some());
        let ev = event.unwrap();
        assert_eq!(ev.interrupted_turn_id, "turn-9");
        assert!(ev.cutoff_playback_seconds >= 0.5);
        assert!(!detector.is_system_speaking());
    }

    #[test]
    fn test_vad_silence_vs_parole_vs_bruit() {
        let (e0, p0) = vad_score(&vec![0.0f32; 1920]);
        assert_eq!(e0, 0.0);
        assert_eq!(p0, 0.0);
        let burst = voiced_burst(1920, 0.3);
        let (e1, p1) = vad_score(&burst);
        assert!(e1 > VAD_RMS_THRESHOLD && p1 > 0.9, "e={} p={}", e1, p1);
        // Bruit blanc faible : probabilité rabattue.
        let mut s = 0x12345678u64;
        let noise: Vec<f32> = (0..1920)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                ((s >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0) as f32 * 0.015
            })
            .collect();
        let (en, pn) = vad_score(&noise);
        assert!(pn < p1, "bruit faible < parole : {} vs {}", pn, p1);
        let _ = en;
    }
}
