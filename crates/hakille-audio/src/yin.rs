//! DSP Acoustique Pur Rust — Algorithme YIN pour l'extraction du pitch fondamental ($F_0$),
//! du Jitter, du Shimmer et de l'Harmonic-to-Noise Ratio (HNR).
//!
//! Ce module implémente l'algorithme YIN (de Cheveigné & Kawahara, JASA 2002) pour l'analyse
//! continue de la prosodie et des micro-variations vocales, 100% en pur Rust sans aucune
//! allocation dynamique sur le chemin chaud (espace de travail pré-alloué [`YinWorkspace`]),
//! et conforme à la Règle 8 de la Constitution Hakille.
//!
//! ### Performances et Métriques :
//! - Latence : < 100 µs par trame de 100 ms (1 600 échantillons à 16 kHz) sur CPU.
//! - Précision sub-échantillonique par interpolation parabolique fine.
//! - Zéro allocation tas (`no vec!`) dans [`YinPitchDetector::analyze`].

use serde::{Deserialize, Serialize};

/// Plage standard de détection de pitch humain.
pub const DEFAULT_MIN_F0_HZ: f32 = 60.0;
pub const DEFAULT_MAX_F0_HZ: f32 = 600.0;
pub const DEFAULT_YIN_THRESHOLD: f32 = 0.15;

/// Taille maximale du tampon de décalage temporel $\tau$ pour 16 kHz (couvre jusqu'à 31.25 Hz).
pub const YIN_WORKSPACE_SIZE: usize = 513;

/// Espace de travail pré-alloué pour éliminer toute allocation tas lors de l'analyse YIN.
#[derive(Debug, Clone, Copy)]
pub struct YinWorkspace {
    /// Fonction de différence carrée $d(\tau)$ pour $\tau \in [0, 512]$.
    pub diff: [f32; YIN_WORKSPACE_SIZE],
    /// Fonction de différence moyenne normalisée cumulée $d'(\tau)$ (CMNDF).
    pub cmndf: [f32; YIN_WORKSPACE_SIZE],
}

impl Default for YinWorkspace {
    fn default() -> Self {
        Self {
            diff: [0.0; YIN_WORKSPACE_SIZE],
            cmndf: [0.0; YIN_WORKSPACE_SIZE],
        }
    }
}

impl YinWorkspace {
    /// Crée un nouvel espace de travail YIN initialisé à zéro.
    pub const fn new() -> Self {
        Self {
            diff: [0.0; YIN_WORKSPACE_SIZE],
            cmndf: [0.0; YIN_WORKSPACE_SIZE],
        }
    }

    /// Réinitialise les tableaux internes.
    pub fn reset(&mut self) {
        self.diff.fill(0.0);
        self.cmndf.fill(0.0);
    }
}

/// Résultat de l'analyse acoustique d'une fenêtre audio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcousticFeatures {
    /// Fréquence fondamentale estimée en Hz (None si la trame est non voisée ou silencieuse).
    pub f0_hz: Option<f32>,
    /// Indice d'harmonicité / voisement (1.0 = harmonique pure, 0.0 = bruit pur).
    pub periodicity: f32,
    /// Harmonic-to-Noise Ratio estimé en dB.
    pub hnr_db: f32,
    /// Énergie RMS de la trame.
    pub rms_energy: f32,
    /// Jitter local relatif estimé (None si non voisé).
    pub jitter_local: Option<f32>,
    /// Shimmer local relatif estimé (None si non voisé).
    pub shimmer_local: Option<f32>,
    /// Booléen indiquant si la trame est classifiée voisée (présence vocale harmonique active).
    pub is_voiced: bool,
}

/// Détecteur de pitch et de prosodie vocale basé sur l'algorithme YIN (zéro allocation tas).
pub struct YinPitchDetector {
    sample_rate: u32,
    threshold: f32,
    min_period: usize,
    max_period: usize,
    prev_period: Option<f32>,
    prev_amplitude: Option<f32>,
    workspace: YinWorkspace,
}

impl YinPitchDetector {
    /// Initialise un nouveau détecteur YIN configuré pour une fréquence d'échantillonnage donnée.
    pub fn new(sample_rate: u32) -> Self {
        Self::with_bounds(
            sample_rate,
            DEFAULT_MIN_F0_HZ,
            DEFAULT_MAX_F0_HZ,
            DEFAULT_YIN_THRESHOLD,
        )
    }

    /// Initialise le détecteur avec des bornes de pitch et un seuil personnalisés.
    pub fn with_bounds(sample_rate: u32, min_f0: f32, max_f0: f32, threshold: f32) -> Self {
        let sr = sample_rate as f32;
        let min_period = (sr / max_f0.max(10.0)).floor() as usize;
        let max_period = (sr / min_f0.max(10.0)).ceil() as usize;

        Self {
            sample_rate,
            threshold: threshold.clamp(0.01, 0.5),
            min_period: min_period.max(2),
            max_period: max_period.max(min_period + 2).min(YIN_WORKSPACE_SIZE - 1),
            prev_period: None,
            prev_amplitude: None,
            workspace: YinWorkspace::new(),
        }
    }

    /// Réinitialise l'historique de pitch et d'amplitude pour le calcul du Jitter et Shimmer.
    pub fn reset_history(&mut self) {
        self.prev_period = None;
        self.prev_amplitude = None;
    }

    /// Accès à une référence sur l'espace de travail interne.
    pub fn workspace(&self) -> &YinWorkspace {
        &self.workspace
    }

    /// Analyse une fenêtre d'échantillons audio PCM mono 32-bit flottants normalisés [-1.0, 1.0].
    /// Cette méthode garantit **zéro allocation sur le tas**.
    pub fn analyze(&mut self, samples: &[f32]) -> AcousticFeatures {
        let sample_rate = self.sample_rate;
        let threshold = self.threshold;
        let min_period = self.min_period;
        let max_period = self.max_period;
        let prev_period = self.prev_period;
        let prev_amplitude = self.prev_amplitude;

        let (features, next_period, next_amplitude) = Self::compute_features(
            sample_rate,
            threshold,
            min_period,
            max_period,
            prev_period,
            prev_amplitude,
            samples,
            &mut self.workspace,
        );

        self.prev_period = next_period;
        self.prev_amplitude = next_amplitude;
        features
    }

    /// Analyse une fenêtre audio avec un espace de travail externe fourni par l'appelant.
    pub fn analyze_with_workspace(
        &mut self,
        samples: &[f32],
        workspace: &mut YinWorkspace,
    ) -> AcousticFeatures {
        let sample_rate = self.sample_rate;
        let threshold = self.threshold;
        let min_period = self.min_period;
        let max_period = self.max_period;
        let prev_period = self.prev_period;
        let prev_amplitude = self.prev_amplitude;

        let (features, next_period, next_amplitude) = Self::compute_features(
            sample_rate,
            threshold,
            min_period,
            max_period,
            prev_period,
            prev_amplitude,
            samples,
            workspace,
        );

        self.prev_period = next_period;
        self.prev_amplitude = next_amplitude;
        features
    }

    #[inline]
    fn compute_features(
        sample_rate: u32,
        threshold: f32,
        min_period: usize,
        max_period: usize,
        prev_period: Option<f32>,
        prev_amplitude: Option<f32>,
        samples: &[f32],
        workspace: &mut YinWorkspace,
    ) -> (AcousticFeatures, Option<f32>, Option<f32>) {
        let n = samples.len();
        if n == 0 {
            return (
                AcousticFeatures {
                    f0_hz: None,
                    periodicity: 0.0,
                    hnr_db: -100.0,
                    rms_energy: 0.0,
                    jitter_local: None,
                    shimmer_local: None,
                    is_voiced: false,
                },
                None,
                None,
            );
        }

        // 1. Calcul de l'énergie RMS (zéro allocation)
        let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
        let rms_energy = (sum_sq / n as f32).sqrt();

        // Si le signal est sous le plancher de bruit (-60 dB), trame silencieuse non voisée
        if rms_energy < 1e-4 {
            return (
                AcousticFeatures {
                    f0_hz: None,
                    periodicity: 0.0,
                    hnr_db: -100.0,
                    rms_energy,
                    jitter_local: None,
                    shimmer_local: None,
                    is_voiced: false,
                },
                None,
                None,
            );
        }

        let max_tau = max_period.min(n / 2).min(YIN_WORKSPACE_SIZE - 1);
        if max_tau <= min_period {
            return (
                AcousticFeatures {
                    f0_hz: None,
                    periodicity: 0.0,
                    hnr_db: -100.0,
                    rms_energy,
                    jitter_local: None,
                    shimmer_local: None,
                    is_voiced: false,
                },
                None,
                None,
            );
        }

        // 2. Étape 1 YIN : Fonction de différence carrée d(tau) — vectorisée par blocs de 8
        let window_size = n - max_tau;
        workspace.diff[0] = 0.0;
        let s_base = &samples[..window_size];

        for tau in 1..=max_tau {
            let s_shifted = &samples[tau..tau + window_size];
            let chunks_base = s_base.chunks_exact(8);
            let chunks_shifted = s_shifted.chunks_exact(8);
            let rem_base = chunks_base.remainder();
            let rem_shifted = chunks_shifted.remainder();

            let mut s0 = 0.0f32;
            let mut s1 = 0.0f32;
            let mut s2 = 0.0f32;
            let mut s3 = 0.0f32;
            let mut s4 = 0.0f32;
            let mut s5 = 0.0f32;
            let mut s6 = 0.0f32;
            let mut s7 = 0.0f32;

            for (cb, cs) in chunks_base.zip(chunks_shifted) {
                let d0 = cb[0] - cs[0];
                let d1 = cb[1] - cs[1];
                let d2 = cb[2] - cs[2];
                let d3 = cb[3] - cs[3];
                let d4 = cb[4] - cs[4];
                let d5 = cb[5] - cs[5];
                let d6 = cb[6] - cs[6];
                let d7 = cb[7] - cs[7];
                s0 += d0 * d0;
                s1 += d1 * d1;
                s2 += d2 * d2;
                s3 += d3 * d3;
                s4 += d4 * d4;
                s5 += d5 * d5;
                s6 += d6 * d6;
                s7 += d7 * d7;
            }
            let mut sum = (s0 + s1) + (s2 + s3) + (s4 + s5) + (s6 + s7);
            for (&b, &s) in rem_base.iter().zip(rem_shifted.iter()) {
                let d = b - s;
                sum += d * d;
            }
            workspace.diff[tau] = sum;
        }

        // 3. Étape 2 YIN : Fonction de différence moyenne normalisée cumulée d'(tau) (CMNDF)
        workspace.cmndf[0] = 1.0;
        let mut running_sum = 0.0f32;

        for tau in 1..=max_tau {
            running_sum += workspace.diff[tau];
            if running_sum > 0.0 {
                workspace.cmndf[tau] = workspace.diff[tau] * (tau as f32) / running_sum;
            } else {
                workspace.cmndf[tau] = 1.0;
            }
        }

        // 4. Étape 3 YIN : Recherche du minimum absolu sous le seuil
        let mut selected_tau = None;
        for tau in min_period..max_tau {
            if workspace.cmndf[tau] < threshold {
                let mut best_tau = tau;
                while best_tau + 1 < max_tau
                    && workspace.cmndf[best_tau + 1] < workspace.cmndf[best_tau]
                {
                    best_tau += 1;
                }
                selected_tau = Some(best_tau);
                break;
            }
        }

        // Si aucun minimum sous le seuil, recherche du minimum global dans la plage
        let best_tau = match selected_tau {
            Some(t) => t,
            None => {
                let mut min_val = f32::MAX;
                let mut min_idx = min_period;
                for tau in min_period..max_tau {
                    if workspace.cmndf[tau] < min_val {
                        min_val = workspace.cmndf[tau];
                        min_idx = tau;
                    }
                }
                min_idx
            }
        };

        // 5. Étape 4 YIN : Interpolation parabolique fine pour précision sub-pixel
        let refined_tau = if best_tau > 1 && best_tau + 1 <= max_tau {
            let s0 = workspace.cmndf[best_tau - 1];
            let s1 = workspace.cmndf[best_tau];
            let s2 = workspace.cmndf[best_tau + 1];
            let denom = 2.0 * (2.0 * s1 - s0 - s2);
            if denom.abs() > 1e-6 {
                let delta = (s2 - s0) / denom;
                (best_tau as f32 + delta).clamp(min_period as f32, max_tau as f32)
            } else {
                best_tau as f32
            }
        } else {
            best_tau as f32
        };

        let min_cmndf = workspace.cmndf[best_tau].clamp(0.0, 1.0);
        let periodicity = (1.0 - min_cmndf).max(0.0);
        let is_voiced = selected_tau.is_some() && rms_energy > 0.005;

        // Calcul du HNR en dB : HNR = 10 * log10(R / (1 - R))
        let hnr_db = if periodicity > 0.01 && periodicity < 0.9999 {
            10.0 * (periodicity / (1.0 - periodicity)).log10()
        } else if periodicity >= 0.9999 {
            40.0
        } else {
            -20.0
        };

        let f0_hz = if is_voiced && refined_tau > 0.0 {
            Some(sample_rate as f32 / refined_tau)
        } else {
            None
        };

        // 6. Calcul du Jitter et du Shimmer locaux relatifs
        let mut jitter_local = None;
        let mut shimmer_local = None;
        let next_period;
        let next_amplitude;

        if let Some(current_period) = f0_hz.map(|f| sample_rate as f32 / f) {
            // Amplitude crête locale sur la période
            let peak_amplitude = samples
                .iter()
                .take(best_tau.min(n))
                .map(|&s| s.abs())
                .fold(0.0f32, f32::max);

            if let Some(prev_p) = prev_period {
                let mean_p = (prev_p + current_period) * 0.5;
                if mean_p > 0.0 {
                    jitter_local = Some(((current_period - prev_p).abs() / mean_p).clamp(0.0, 1.0));
                }
            }

            if let Some(prev_a) = prev_amplitude {
                let mean_a = (prev_a + peak_amplitude) * 0.5;
                if mean_a > 1e-4 {
                    shimmer_local =
                        Some(((peak_amplitude - prev_a).abs() / mean_a).clamp(0.0, 1.0));
                }
            }

            next_period = Some(current_period);
            next_amplitude = Some(peak_amplitude);
        } else {
            next_period = None;
            next_amplitude = None;
        }

        (
            AcousticFeatures {
                f0_hz,
                periodicity,
                hnr_db,
                rms_energy,
                jitter_local,
                shimmer_local,
                is_voiced,
            },
            next_period,
            next_amplitude,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;
    use std::time::Instant;

    /// Générateur d'onde sinusoïdale pure.
    fn generate_sine_wave(
        freq_hz: f32,
        sample_rate: u32,
        duration_secs: f32,
        amplitude: f32,
    ) -> Vec<f32> {
        let total_samples = (sample_rate as f32 * duration_secs) as usize;
        (0..total_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (2.0 * PI * freq_hz * t).sin() * amplitude
            })
            .collect()
    }

    /// Générateur d'onde harmonique complexe (fondamentale + 2e et 3e harmoniques).
    fn generate_harmonic_wave(freq_hz: f32, sample_rate: u32, duration_secs: f32) -> Vec<f32> {
        let total_samples = (sample_rate as f32 * duration_secs) as usize;
        (0..total_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                let s1 = (2.0 * PI * freq_hz * t).sin() * 0.6;
                let s2 = (2.0 * PI * (freq_hz * 2.0) * t).sin() * 0.3;
                let s3 = (2.0 * PI * (freq_hz * 3.0) * t).sin() * 0.1;
                s1 + s2 + s3
            })
            .collect()
    }

    #[test]
    fn test_yin_pure_sine_220hz() {
        let sample_rate = 16_000;
        let sine_220 = generate_sine_wave(220.0, sample_rate, 0.1, 0.8); // 100 ms de La3
        let mut yin = YinPitchDetector::new(sample_rate);

        let features = yin.analyze(&sine_220);
        assert!(
            features.is_voiced,
            "L'onde sinusoïdale pure doit être détectée comme voisée"
        );
        assert!(features.f0_hz.is_some(), "F0 doit être présent");

        let f0 = features.f0_hz.unwrap();
        assert!(
            (f0 - 220.0).abs() < 2.0,
            "Erreur d'estimation de F0 trop élevée: {f0} Hz attendu ~220 Hz"
        );
        assert!(
            features.periodicity > 0.85,
            "Périodicité doit être très élevée (> 0.85)"
        );
        assert!(
            features.hnr_db > 10.0,
            "HNR doit être positif pour un son pur"
        );
    }

    #[test]
    fn test_yin_pure_sine_440hz() {
        let sample_rate = 16_000;
        let sine_440 = generate_sine_wave(440.0, sample_rate, 0.1, 0.7); // 100 ms de La4
        let mut yin = YinPitchDetector::new(sample_rate);

        let features = yin.analyze(&sine_440);
        assert!(features.is_voiced);
        assert!(features.f0_hz.is_some());
        let f0 = features.f0_hz.unwrap();
        assert!(
            (f0 - 440.0).abs() < 3.0,
            "Erreur d'estimation de F0 trop élevée: {f0} Hz attendu ~440 Hz"
        );
    }

    #[test]
    fn test_yin_harmonic_wave_detection() {
        let sample_rate = 16_000;
        let harmonic_200 = generate_harmonic_wave(200.0, sample_rate, 0.1);
        let mut yin = YinPitchDetector::new(sample_rate);

        let features = yin.analyze(&harmonic_200);
        assert!(features.is_voiced);
        assert!(features.f0_hz.is_some());
        let f0 = features.f0_hz.unwrap();
        assert!(
            (f0 - 200.0).abs() < 3.0,
            "F0 d'un signal poly-harmonique doit être la fondamentale 200 Hz (trouvé {f0})"
        );
        assert!(features.periodicity > 0.80);
    }

    #[test]
    fn test_yin_noisy_signal_robustness() {
        let sample_rate = 16_000;
        let mut noisy_signal = generate_sine_wave(250.0, sample_rate, 0.1, 0.7);
        // Ajout de bruit pseudo-aléatoire
        for (i, s) in noisy_signal.iter_mut().enumerate() {
            let noise = ((i * 1103515245 + 12345) % 1000) as f32 / 5000.0 - 0.1;
            *s += noise;
        }

        let mut yin = YinPitchDetector::new(sample_rate);
        let features = yin.analyze(&noisy_signal);
        assert!(features.is_voiced);
        let f0 = features
            .f0_hz
            .expect("F0 doit être extrait même en présence de bruit modéré");
        assert!(
            (f0 - 250.0).abs() < 5.0,
            "F0 sous bruit modéré doit rester proche de 250 Hz (trouvé {f0})"
        );
    }

    #[test]
    fn test_yin_white_noise_unvoiced() {
        let sample_rate = 16_000;
        let total_samples = (sample_rate as f32 * 0.1) as usize;
        // Générateur LCG simple et déterministe pour le bruit blanc
        let mut seed: u64 = 42;
        let noise: Vec<f32> = (0..total_samples)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                ((seed >> 33) as f32 / (u32::MAX as f32 * 0.5)) - 1.0
            })
            .collect();

        let mut yin = YinPitchDetector::new(sample_rate);
        let features = yin.analyze(&noise);

        assert!(
            !features.is_voiced,
            "Le bruit blanc pur ne doit pas être voisé"
        );
        assert!(
            features.periodicity < 0.5,
            "Périodicité du bruit doit être faible"
        );
    }

    #[test]
    fn test_yin_silence_returns_unvoiced() {
        let sample_rate = 16_000;
        let silence = vec![0.0f32; 1600];
        let mut yin = YinPitchDetector::new(sample_rate);

        let features = yin.analyze(&silence);
        assert!(!features.is_voiced);
        assert_eq!(features.f0_hz, None);
        assert_eq!(features.rms_energy, 0.0);
    }

    #[test]
    fn test_yin_jitter_and_shimmer_detection() {
        let sample_rate = 16_000;
        let mut yin = YinPitchDetector::new(sample_rate);

        // Trame 1 : 220 Hz amplitude 0.5
        let frame1 = generate_sine_wave(220.0, sample_rate, 0.1, 0.5);
        let f1 = yin.analyze(&frame1);
        assert_eq!(
            f1.jitter_local, None,
            "Première trame : pas encore d'historique"
        );
        assert_eq!(f1.shimmer_local, None);

        // Trame 2 : légère variation de fréquence (225 Hz) et d'amplitude (0.55)
        let frame2 = generate_sine_wave(225.0, sample_rate, 0.1, 0.55);
        let f2 = yin.analyze(&frame2);

        assert!(f2.jitter_local.is_some(), "Deuxième trame : Jitter calculé");
        assert!(
            f2.shimmer_local.is_some(),
            "Deuxième trame : Shimmer calculé"
        );

        let jitter = f2.jitter_local.unwrap();
        let shimmer = f2.shimmer_local.unwrap();
        assert!(
            jitter > 0.0 && jitter < 0.1,
            "Jitter doit être cohérent et faible: {jitter}"
        );
        assert!(
            shimmer > 0.0 && shimmer < 0.2,
            "Shimmer doit être cohérent et modéré: {shimmer}"
        );
    }

    #[test]
    fn test_yin_workspace_zero_alloc() {
        let sample_rate = 16_000;
        let mut yin = YinPitchDetector::new(sample_rate);
        let mut ws = YinWorkspace::new();
        let sine = generate_sine_wave(300.0, sample_rate, 0.1, 0.7);

        let f = yin.analyze_with_workspace(&sine, &mut ws);
        assert!(f.is_voiced);
        assert!((f.f0_hz.unwrap() - 300.0).abs() < 3.0);
    }

    #[test]
    fn test_yin_benchmark_1600_samples_latency() {
        let sample_rate = 16_000;
        let mut yin = YinPitchDetector::new(sample_rate);
        // 1600 échantillons = 100 ms à 16 kHz
        let samples = generate_sine_wave(220.0, sample_rate, 0.1, 0.8);
        assert_eq!(samples.len(), 1600);

        // Warmup (10 exécutions)
        for _ in 0..10 {
            let _ = yin.analyze(&samples);
        }

        // Mesure sur 50 exécutions
        let iters = 50;
        let start = Instant::now();
        for _ in 0..iters {
            let _ = yin.analyze(&samples);
        }
        let elapsed = start.elapsed();
        let avg_micros = elapsed.as_micros() as f64 / iters as f64;

        eprintln!(
            "YinPitchDetector::analyze (1600 samples) : {:.2} µs par trame (sur {} itérations)",
            avg_micros, iters
        );

        // En mode release (optimisé), la latence doit être inférieure à 100 µs
        #[cfg(not(debug_assertions))]
        assert!(
            avg_micros < 100.0,
            "La latence d'analyse YIN doit être < 100 µs en mode release (mesuré: {:.2} µs)",
            avg_micros
        );

        // En mode debug sans optimisations du compilateur
        #[cfg(debug_assertions)]
        assert!(
            avg_micros < 5000.0,
            "La latence d'analyse YIN en mode debug doit rester modérée (mesuré: {:.2} µs)",
            avg_micros
        );
    }
}
