//! Analyse de prosodie vocale et lissage temporel — DSP acoustique pur Rust sans allocation.
//!
//! Ce module implémente [`VocalProsodyAnalyzer`], qui raccorde l'algorithme YIN de `hakille-audio`
//! au tampon circulaire continu [`ContinuousAudioRingBuffer`] (fenêtres de 100 ms / 1 600 échantillons à 16 kHz).
//!
//! Il maintient un historique temporel glissant et applique un lissage exponentiel (EMA)
//! pour stabiliser l'estimation du pitch $F_0$, du Jitter, du Shimmer et du HNR face au bruit.

use crate::audio::capture::{ContinuousAudioRingBuffer, SAMPLES_PER_100MS};
use hakille_audio::{AcousticFeatures, YinPitchDetector, YinWorkspace};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Capacité par défaut de l'historique glissant (10 fenêtres de 100 ms = 1 seconde d'audio).
pub const DEFAULT_PROSODY_HISTORY_CAP: usize = 10;
/// Facteur de lissage exponentiel (EMA) par défaut pour le pitch $F_0$ (0.0 = figé, 1.0 = instantané).
pub const DEFAULT_EMA_ALPHA: f32 = 0.35;

/// Métriques prosodiques lissées sur l'historique glissant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmoothedProsody {
    /// Pitch $F_0$ lissé par EMA (None si silence ou parole non voisée).
    pub smoothed_f0_hz: Option<f32>,
    /// Plage dynamique $[F_{0,\min}, F_{0,\max}]$ sur la fenêtre temporelle glissante.
    pub f0_range_hz: Option<(f32, f32)>,
    /// Jitter local moyen lissé.
    pub smoothed_jitter: Option<f32>,
    /// Shimmer local moyen lissé.
    pub smoothed_shimmer: Option<f32>,
    /// Rapport harmonique sur bruit (HNR) moyen en dB.
    pub smoothed_hnr_db: f32,
    /// Énergie RMS moyenne sur l'historique glissant.
    pub mean_rms_energy: f32,
    /// Ratio de trames voisées $\in [0.0, 1.0]$ sur la fenêtre (présence d'intonation active).
    pub voiced_ratio: f32,
    /// Caractéristiques brutes instantanées de la dernière fenêtre de 100 ms.
    pub raw_features: AcousticFeatures,
}

/// Analyseur prosodique vocal connecté au tampon circulaire audio.
pub struct VocalProsodyAnalyzer {
    detector: YinPitchDetector,
    workspace: YinWorkspace,
    history_capacity: usize,
    ema_alpha: f32,
    f0_history: VecDeque<f32>,
    jitter_history: VecDeque<f32>,
    shimmer_history: VecDeque<f32>,
    hnr_history: VecDeque<f32>,
    rms_history: VecDeque<f32>,
    voiced_history: VecDeque<bool>,
    last_smoothed_f0: Option<f32>,
}

impl VocalProsodyAnalyzer {
    /// Crée un nouvel analyseur configuré pour une fréquence d'échantillonnage et une taille d'historique.
    pub fn new(sample_rate: u32, history_capacity: usize) -> Self {
        Self::with_ema(sample_rate, history_capacity, DEFAULT_EMA_ALPHA)
    }

    /// Crée un analyseur avec paramètres personnalisés de lissage exponentiel.
    pub fn with_ema(sample_rate: u32, history_capacity: usize, ema_alpha: f32) -> Self {
        let cap = history_capacity.max(2);
        Self {
            detector: YinPitchDetector::new(sample_rate),
            workspace: YinWorkspace::new(),
            history_capacity: cap,
            ema_alpha: ema_alpha.clamp(0.05, 1.0),
            f0_history: VecDeque::with_capacity(cap),
            jitter_history: VecDeque::with_capacity(cap),
            shimmer_history: VecDeque::with_capacity(cap),
            hnr_history: VecDeque::with_capacity(cap),
            rms_history: VecDeque::with_capacity(cap),
            voiced_history: VecDeque::with_capacity(cap),
            last_smoothed_f0: None,
        }
    }

    /// Constructeur standard pour le pipeline Hakille (16 kHz mono, historique 1.0s).
    pub fn default_16k() -> Self {
        Self::new(16_000, DEFAULT_PROSODY_HISTORY_CAP)
    }

    /// Réinitialise l'historique glissant et l'état interne.
    pub fn reset(&mut self) {
        self.detector.reset_history();
        self.workspace.reset();
        self.f0_history.clear();
        self.jitter_history.clear();
        self.shimmer_history.clear();
        self.hnr_history.clear();
        self.rms_history.clear();
        self.voiced_history.clear();
        self.last_smoothed_f0 = None;
    }

    /// Traite directement la dernière fenêtre de 100 ms depuis un [`ContinuousAudioRingBuffer`].
    /// Cette méthode garantit **zéro allocation dynamique** sur le chemin chaud.
    pub fn process_ring_buffer(
        &mut self,
        ring_buffer: &ContinuousAudioRingBuffer,
    ) -> Option<SmoothedProsody> {
        let mut window = [0.0f32; SAMPLES_PER_100MS];
        if ring_buffer.read_prosody_100ms(&mut window).is_ok() {
            Some(self.process_window(&window))
        } else {
            None
        }
    }

    /// Analyse une fenêtre de 100 ms (1 600 échantillons à 16 kHz) et met à jour l'historique.
    pub fn process_window(&mut self, window_1600: &[f32; SAMPLES_PER_100MS]) -> SmoothedProsody {
        let raw = self
            .detector
            .analyze_with_workspace(window_1600, &mut self.workspace);
        self.update_history(&raw)
    }

    /// Analyse une tranche quelconque d'échantillons.
    pub fn process_slice(&mut self, slice: &[f32]) -> SmoothedProsody {
        let raw = self
            .detector
            .analyze_with_workspace(slice, &mut self.workspace);
        self.update_history(&raw)
    }

    fn update_history(&mut self, raw: &AcousticFeatures) -> SmoothedProsody {
        // Mise à jour de l'historique RMS et Voisement
        if self.rms_history.len() >= self.history_capacity {
            self.rms_history.pop_front();
        }
        self.rms_history.push_back(raw.rms_energy);

        if self.voiced_history.len() >= self.history_capacity {
            self.voiced_history.pop_front();
        }
        self.voiced_history.push_back(raw.is_voiced);

        if self.hnr_history.len() >= self.history_capacity {
            self.hnr_history.pop_front();
        }
        self.hnr_history.push_back(raw.hnr_db);

        // Mise à jour conditionnelle pour les trames voisées
        if let Some(f0) = raw.f0_hz {
            if self.f0_history.len() >= self.history_capacity {
                self.f0_history.pop_front();
            }
            self.f0_history.push_back(f0);

            // Lissage EMA du pitch F0
            let smoothed = match self.last_smoothed_f0 {
                Some(prev) => prev * (1.0 - self.ema_alpha) + f0 * self.ema_alpha,
                None => f0,
            };
            self.last_smoothed_f0 = Some(smoothed);
        } else if !raw.is_voiced {
            // Décroissance douce si silence prolongé
            if self.voiced_history.iter().filter(|&&v| v).count() == 0 {
                self.last_smoothed_f0 = None;
                self.f0_history.clear();
            }
        }

        if let Some(j) = raw.jitter_local {
            if self.jitter_history.len() >= self.history_capacity {
                self.jitter_history.pop_front();
            }
            self.jitter_history.push_back(j);
        }

        if let Some(s) = raw.shimmer_local {
            if self.shimmer_history.len() >= self.history_capacity {
                self.shimmer_history.pop_front();
            }
            self.shimmer_history.push_back(s);
        }

        // Calcul des métriques agrégées
        let voiced_count = self.voiced_history.iter().filter(|&&v| v).count();
        let voiced_ratio = if !self.voiced_history.is_empty() {
            voiced_count as f32 / self.voiced_history.len() as f32
        } else {
            0.0
        };

        let mean_rms = if !self.rms_history.is_empty() {
            let sum: f32 = self.rms_history.iter().sum();
            sum / self.rms_history.len() as f32
        } else {
            0.0
        };

        let mean_hnr = if !self.hnr_history.is_empty() {
            let sum: f32 = self.hnr_history.iter().sum();
            sum / self.hnr_history.len() as f32
        } else {
            -20.0
        };

        let smoothed_jitter = if !self.jitter_history.is_empty() {
            let sum: f32 = self.jitter_history.iter().sum();
            Some(sum / self.jitter_history.len() as f32)
        } else {
            None
        };

        let smoothed_shimmer = if !self.shimmer_history.is_empty() {
            let sum: f32 = self.shimmer_history.iter().sum();
            Some(sum / self.shimmer_history.len() as f32)
        } else {
            None
        };

        let f0_range_hz = if self.f0_history.len() >= 2 {
            let min_f0 = self.f0_history.iter().fold(f32::INFINITY, |a, &b| a.min(b));
            let max_f0 = self.f0_history.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
            Some((min_f0, max_f0))
        } else {
            None
        };

        SmoothedProsody {
            smoothed_f0_hz: self.last_smoothed_f0,
            f0_range_hz,
            smoothed_jitter,
            smoothed_shimmer,
            smoothed_hnr_db: mean_hnr,
            mean_rms_energy: mean_rms,
            voiced_ratio,
            raw_features: raw.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn generate_sine_wave(freq_hz: f32, sample_rate: u32, duration_secs: f32, amplitude: f32) -> Vec<f32> {
        let total_samples = (sample_rate as f32 * duration_secs) as usize;
        (0..total_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (2.0 * PI * freq_hz * t).sin() * amplitude
            })
            .collect()
    }

    #[test]
    fn test_prosody_analyzer_smoothing() {
        let sample_rate = 16_000;
        let mut analyzer = VocalProsodyAnalyzer::new(sample_rate, 5);

        let wave_220 = generate_sine_wave(220.0, sample_rate, 0.1, 0.8);
        let mut buf = [0.0f32; SAMPLES_PER_100MS];
        buf.copy_from_slice(&wave_220[..SAMPLES_PER_100MS]);

        // Trame 1
        let p1 = analyzer.process_window(&buf);
        assert!(p1.smoothed_f0_hz.is_some());
        assert!((p1.smoothed_f0_hz.unwrap() - 220.0).abs() < 2.0);
        assert!(p1.voiced_ratio > 0.0);

        // Trame 2
        let p2 = analyzer.process_window(&buf);
        assert!(p2.voiced_ratio > 0.9);
        assert!(p2.mean_rms_energy > 0.1);
    }

    #[test]
    fn test_prosody_analyzer_with_ring_buffer() {
        let rb = ContinuousAudioRingBuffer::new_default();
        let mut analyzer = VocalProsodyAnalyzer::default_16k();

        let wave = generate_sine_wave(300.0, 16_000, 0.1, 0.7);
        rb.push_samples(&wave);

        let prosody = analyzer.process_ring_buffer(&rb).expect("prosody extraite");
        assert!(prosody.raw_features.is_voiced);
        assert!((prosody.raw_features.f0_hz.unwrap() - 300.0).abs() < 3.0);
    }
}
