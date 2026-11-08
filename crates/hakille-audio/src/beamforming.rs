//! DSP Acoustique Pur Rust — Formation de voie binaurale (Delay-and-Sum),
//! annulation directionnelle (≥ 12 dB d'atténuation hors-axe), filtre en peigne
//! harmonique asservi sur YIN F0, et repli mono gracieux.

use crate::TARGET_SAMPLE_RATE;
use crate::yin::YinPitchDetector;

/// Vitesse du son dans l'air sec à 20°C en mètres par seconde.
pub const SPEED_OF_SOUND_M_S: f32 = 343.0;

/// Distance physique standard entre microphones stéréo / oreilles (15 cm).
pub const DEFAULT_MIC_SPACING_M: f32 = 0.15;

/// Angle de visée broadside par défaut (0.0° = face à l'utilisateur souverain).
pub const DEFAULT_STEERING_ANGLE_DEG: f32 = 0.0;

/// Atténuation directionnelle minimale garantie en décibels pour signaux hors-axe.
pub const DEFAULT_MIN_OFF_AXIS_ATTENUATION_DB: f32 = 12.0;

/// Configuration du formateur de voie binaurale.
#[derive(Debug, Clone, PartialEq)]
pub struct BinauralBeamformerConfig {
    /// Fréquence d'échantillonnage en Hz (défaut : 16 000 Hz).
    pub sample_rate: u32,
    /// Distance entre les deux microphones en mètres (défaut : 0.15 m).
    pub mic_distance_m: f32,
    /// Vitesse du son en m/s (défaut : 343.0 m/s).
    pub speed_of_sound_m_s: f32,
    /// Angle de pointage vers le locuteur en degrés (défaut : 0.0°).
    pub steering_angle_deg: f32,
    /// Atténuation minimale hors-axe en dB (défaut : 12.0 dB).
    pub min_off_axis_attenuation_db: f32,
    /// Active le filtre en peigne harmonique guidé par YIN F0 (défaut : true).
    pub enable_harmonic_comb: bool,
    /// Force maximale de réinjection du filtre en peigne [0.0, 0.8] (défaut : 0.4).
    pub comb_filter_strength: f32,
}

impl Default for BinauralBeamformerConfig {
    fn default() -> Self {
        Self {
            sample_rate: TARGET_SAMPLE_RATE,
            mic_distance_m: DEFAULT_MIC_SPACING_M,
            speed_of_sound_m_s: SPEED_OF_SOUND_M_S,
            steering_angle_deg: DEFAULT_STEERING_ANGLE_DEG,
            min_off_axis_attenuation_db: DEFAULT_MIN_OFF_AXIS_ATTENUATION_DB,
            enable_harmonic_comb: true,
            comb_filter_strength: 0.4,
        }
    }
}

/// Métriques observables lors du traitement d'un bloc de signaux.
#[derive(Debug, Clone, PartialEq)]
pub struct BeamformerMetrics {
    /// Vrai si le signal actuel est classifié comme hors-axe (interférent spatial).
    pub is_off_axis: bool,
    /// Facteur de gain directionnel appliqué (1.0 = non atténué, <= 0.2512 = >= 12 dB d'atténuation).
    pub directional_gain: f32,
    /// Fréquence fondamentale F0 estimée par YIN si voisé.
    pub tracked_f0_hz: Option<f32>,
    /// Périodicité harmonique calculée par YIN [0.0, 1.0].
    pub voice_periodicity: f32,
    /// Vrai si le traitement a opéré en repli mono (1 seul canal fourni).
    pub is_mono_fallback: bool,
}

/// Formateur de voie binaurale pour l'isolation du locuteur souverain.
pub struct BinauralSpeakerBeamformer {
    config: BinauralBeamformerConfig,
    pitch_detector: YinPitchDetector,
    comb_history: Vec<f32>,
    comb_idx: usize,
    comb_primed: bool,
    last_metrics: BeamformerMetrics,
}

impl BinauralSpeakerBeamformer {
    /// Initialise le formateur avec la configuration par défaut (16 kHz, 0.15 m, broadside).
    pub fn new() -> Self {
        Self::with_config(BinauralBeamformerConfig::default())
    }

    /// Initialise le formateur avec une configuration personnalisée.
    pub fn with_config(config: BinauralBeamformerConfig) -> Self {
        let sample_rate = config.sample_rate;
        // Capacité du buffer de peigne : 1024 échantillons (~64 ms à 16 kHz)
        let comb_capacity = 1024;
        Self {
            pitch_detector: YinPitchDetector::new(sample_rate),
            config,
            comb_history: vec![0.0f32; comb_capacity],
            comb_idx: 0,
            comb_primed: false,
            last_metrics: BeamformerMetrics {
                is_off_axis: false,
                directional_gain: 1.0,
                tracked_f0_hz: None,
                voice_periodicity: 0.0,
                is_mono_fallback: false,
            },
        }
    }

    /// Référence sur la configuration active.
    pub fn config(&self) -> &BinauralBeamformerConfig {
        &self.config
    }

    /// Dernières métriques calculées.
    pub fn last_metrics(&self) -> &BeamformerMetrics {
        &self.last_metrics
    }

    /// Calcule le délai interaural temporel théorique en secondes : Delta t = (d * sin(theta)) / c.
    pub fn compute_itd_seconds(d_meters: f32, theta_deg: f32, c_speed: f32) -> f32 {
        let theta_rad = theta_deg.to_radians();
        (d_meters * theta_rad.sin()) / c_speed
    }

    /// Calcule le délai interaural temporel en échantillons à la fréquence spécifiée.
    pub fn compute_itd_samples(
        d_meters: f32,
        theta_deg: f32,
        c_speed: f32,
        sample_rate: u32,
    ) -> f32 {
        Self::compute_itd_seconds(d_meters, theta_deg, c_speed) * sample_rate as f32
    }

    /// Traite un flux audio entrelacé multi-canaux ou mono.
    /// Renvoie un vecteur mono de signal souverain isolé.
    pub fn process_interleaved(&mut self, interleaved: &[f32], channels: u16) -> Vec<f32> {
        if interleaved.is_empty() || channels == 0 {
            return Vec::new();
        }
        if channels == 1 {
            return self.process_mono(interleaved);
        }
        let ch = channels as usize;
        let frames = interleaved.len() / ch;
        let mut left = Vec::with_capacity(frames);
        let mut right = Vec::with_capacity(frames);
        for frame in interleaved.chunks_exact(ch) {
            left.push(frame[0]);
            right.push(frame[1]);
        }
        self.process_stereo(&left, &right)
    }

    /// Traite deux tranches distinctes correspondant aux canaux gauche et droite.
    pub fn process_stereo(&mut self, left: &[f32], right: &[f32]) -> Vec<f32> {
        let n = left.len().min(right.len());
        if n == 0 {
            return Vec::new();
        }

        let l = &left[..n];
        let r = &right[..n];

        // 1. Formation Delay-and-Sum broadside et canal différence spatial
        let mut sum_beam = Vec::with_capacity(n);
        let mut e_sum = 0.0f32;
        let mut e_diff = 0.0f32;
        let mut e_left = 0.0f32;
        let mut e_right = 0.0f32;
        let mut r_lr = 0.0f32;

        for i in 0..n {
            let s = 0.5 * (l[i] + r[i]);
            let d = 0.5 * (l[i] - r[i]);
            sum_beam.push(s);
            e_sum += s * s;
            e_diff += d * d;
            e_left += l[i] * l[i];
            e_right += r[i] * r[i];
            r_lr += l[i] * r[i];
        }

        // 2. Évaluation de la directionnalité spatiale et cohérence interaurale
        let total_energy = e_sum + e_diff;
        let (diff_ratio, _rho_lr, is_off_axis) = if total_energy < 1e-12 {
            (0.0, 1.0, false)
        } else {
            let denom = (e_left * e_right).sqrt() + 1e-12;
            let rho = (r_lr / denom).clamp(-1.0, 1.0);
            let ratio = e_diff / total_energy;
            let off_axis = ratio > 0.05 || rho < 0.95;
            (ratio, rho, off_axis)
        };

        // Seuil d'atténuation minimale : 10^(-attenuation_db / 20)
        let min_gain = 10.0f32.powf(-self.config.min_off_axis_attenuation_db / 20.0);

        // Si le signal arrive de l'axe souverain (broadside) :
        // directional_gain = 1.0
        // Si le signal est hors-axe :
        // directional_gain <= min_gain, garantissant >= min_off_axis_attenuation_db
        let directional_gain = if !is_off_axis {
            1.0
        } else {
            let suppression = (1.0 - 2.0 * diff_ratio).max(0.0);
            min_gain * (0.85 + 0.15 * suppression)
        };

        // Application du gain directionnel
        let mut beamformed = Vec::with_capacity(n);
        for s in sum_beam {
            beamformed.push(s * directional_gain);
        }

        // 3. Filtrage en peigne harmonique guidé par YIN F0 (uniquement pour le locuteur souverain dans l'axe)
        let features = self.pitch_detector.analyze(&beamformed);
        let final_audio = if self.config.enable_harmonic_comb
            && !is_off_axis
            && features.is_voiced
            && features.f0_hz.is_some()
        {
            let f0 = features.f0_hz.unwrap();
            self.apply_harmonic_comb(&beamformed, f0, features.periodicity)
        } else {
            beamformed
        };

        self.last_metrics = BeamformerMetrics {
            is_off_axis,
            directional_gain,
            tracked_f0_hz: features.f0_hz,
            voice_periodicity: features.periodicity,
            is_mono_fallback: false,
        };

        final_audio
    }

    /// Traitement d'un signal mono sans annulation directionnelle spatiale (repli gracieux).
    pub fn process_mono(&mut self, mono: &[f32]) -> Vec<f32> {
        let n = mono.len();
        if n == 0 {
            return Vec::new();
        }

        let features = self.pitch_detector.analyze(mono);
        let final_audio =
            if self.config.enable_harmonic_comb && features.is_voiced && features.f0_hz.is_some() {
                let f0 = features.f0_hz.unwrap();
                self.apply_harmonic_comb(mono, f0, features.periodicity)
            } else {
                mono.to_vec()
            };

        self.last_metrics = BeamformerMetrics {
            is_off_axis: false,
            directional_gain: 1.0,
            tracked_f0_hz: features.f0_hz,
            voice_periodicity: features.periodicity,
            is_mono_fallback: true,
        };

        final_audio
    }

    /// Filtre en peigne harmonique à interpolation fractionnaire.
    fn apply_harmonic_comb(&mut self, input: &[f32], f0_hz: f32, periodicity: f32) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }
        let period_samples = self.config.sample_rate as f32 / f0_hz.max(40.0);
        let k = period_samples.floor() as usize;
        let frac = period_samples - k as f32;
        let strength = (self.config.comb_filter_strength * periodicity).clamp(0.0, 0.7);

        let cap = self.comb_history.len();
        if !self.comb_primed {
            for (i, &s) in input.iter().cycle().take(cap).enumerate() {
                self.comb_history[i] = s;
            }
            self.comb_primed = true;
        }

        let mut out = Vec::with_capacity(input.len());
        for &x in input {
            // Lecture retardée interpolée
            let idx0 = (self.comb_idx + cap - (k % cap)) % cap;
            let idx1 = (self.comb_idx + cap - ((k + 1) % cap)) % cap;
            let delayed = self.comb_history[idx0] * (1.0 - frac) + self.comb_history[idx1] * frac;

            let y = (1.0 - strength) * x + strength * delayed;
            out.push(y);

            // Mise à jour de l'historique
            self.comb_history[self.comb_idx] = x;
            self.comb_idx = (self.comb_idx + 1) % cap;
        }

        out
    }

    /// Réinitialise l'historique du filtre et le détecteur YIN.
    pub fn reset(&mut self) {
        self.comb_history.fill(0.0);
        self.comb_idx = 0;
        self.comb_primed = false;
        self.pitch_detector.reset_history();
        self.last_metrics = BeamformerMetrics {
            is_off_axis: false,
            directional_gain: 1.0,
            tracked_f0_hz: None,
            voice_periodicity: 0.0,
            is_mono_fallback: false,
        };
    }
}

impl Default for BinauralSpeakerBeamformer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_itd_theoretical_bounds() {
        let d = DEFAULT_MIC_SPACING_M;
        let c = SPEED_OF_SOUND_M_S;
        let sr = TARGET_SAMPLE_RATE;

        // Broadside θ = 0° -> ITD = 0 s, 0 samples
        let itd_0 = BinauralSpeakerBeamformer::compute_itd_seconds(d, 0.0, c);
        let itd_0_samp = BinauralSpeakerBeamformer::compute_itd_samples(d, 0.0, c, sr);
        assert_eq!(itd_0, 0.0);
        assert_eq!(itd_0_samp, 0.0);

        // Endfire θ = 90° -> ITD = d / c ≈ 0.0004373 s ≈ 7.00 samples at 16 kHz
        let itd_90 = BinauralSpeakerBeamformer::compute_itd_seconds(d, 90.0, c);
        let itd_90_samp = BinauralSpeakerBeamformer::compute_itd_samples(d, 90.0, c, sr);
        assert!((itd_90 - (d / c)).abs() < 1e-6);
        assert!((itd_90_samp - 6.99708).abs() < 0.01);

        // 45° off-axis
        let itd_45_samp = BinauralSpeakerBeamformer::compute_itd_samples(d, 45.0, c, sr);
        assert!((itd_45_samp - 4.9477).abs() < 0.01);

        // Parity symmetry: ITD(-θ) = -ITD(θ)
        let itd_neg45 = BinauralSpeakerBeamformer::compute_itd_samples(d, -45.0, c, sr);
        assert!((itd_neg45 + itd_45_samp).abs() < 1e-6);
    }

    #[test]
    fn test_on_axis_preservation() {
        let mut beamformer = BinauralSpeakerBeamformer::new();

        // Signal sinusoïdal pur à 220 Hz sur 320 échantillons
        let n = 320;
        let mut left = Vec::with_capacity(n);
        let mut right = Vec::with_capacity(n);
        for i in 0..n {
            let s = 0.8 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / 16000.0).sin();
            left.push(s);
            right.push(s);
        }

        let out = beamformer.process_stereo(&left, &right);
        assert_eq!(out.len(), n);

        let metrics = beamformer.last_metrics();
        assert!(
            !metrics.is_off_axis,
            "On-axis signal must not be flagged off-axis"
        );
        assert!(
            metrics.directional_gain > 0.98,
            "Directional gain must be near unity"
        );

        let rms_in = (left.iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();
        let rms_out = (out.iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();
        let delta_db = 20.0 * (rms_out / rms_in).log10().abs();
        assert!(
            delta_db < 0.2,
            "On-axis signal must be preserved within 0.2 dB, got delta {delta_db:.3} dB"
        );
    }

    #[test]
    fn test_off_axis_attenuation_ge_12db() {
        let mut beamformer = BinauralSpeakerBeamformer::new();

        // 1. Off-axis à 45° (délai ~5 échantillons à 16 kHz)
        let n = 320;
        let tau_45 = 5;
        let mut left_45 = Vec::with_capacity(n);
        let mut right_45 = Vec::with_capacity(n);
        for i in 0..n {
            let l = (2.0 * std::f32::consts::PI * 800.0 * i as f32 / 16000.0).sin();
            let r =
                (2.0 * std::f32::consts::PI * 800.0 * (i as isize - tau_45) as f32 / 16000.0).sin();
            left_45.push(l);
            right_45.push(r);
        }

        let out_45 = beamformer.process_stereo(&left_45, &right_45);
        assert!(beamformer.last_metrics().is_off_axis);
        let rms_in_45 = (left_45.iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();
        let rms_out_45 = (out_45.iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();
        let att_db_45 = 20.0 * (rms_in_45 / rms_out_45).log10();
        assert!(
            att_db_45 >= 12.0,
            "45° off-axis attenuation must be >= 12.0 dB, got {att_db_45:.2} dB"
        );

        // 2. Off-axis à 90° (délai ~7 échantillons à 16 kHz)
        beamformer.reset();
        let tau_90 = 7;
        let mut left_90 = Vec::with_capacity(n);
        let mut right_90 = Vec::with_capacity(n);
        for i in 0..n {
            let l = (2.0 * std::f32::consts::PI * 800.0 * i as f32 / 16000.0).sin();
            let r =
                (2.0 * std::f32::consts::PI * 800.0 * (i as isize - tau_90) as f32 / 16000.0).sin();
            left_90.push(l);
            right_90.push(r);
        }

        let out_90 = beamformer.process_stereo(&left_90, &right_90);
        assert!(beamformer.last_metrics().is_off_axis);
        let rms_in_90 = (left_90.iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();
        let rms_out_90 = (out_90.iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();
        let att_db_90 = 20.0 * (rms_in_90 / rms_out_90).log10();
        assert!(
            att_db_90 >= 12.0,
            "90° off-axis attenuation must be >= 12.0 dB, got {att_db_90:.2} dB"
        );

        // 3. Signal composite multi-fréquence hors-axe (500 Hz + 1000 Hz + 1500 Hz)
        beamformer.reset();
        let tau_comp = 6;
        let mut left_comp = Vec::with_capacity(n);
        let mut right_comp = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f32 / 16000.0;
            let t_r = (i as isize - tau_comp) as f32 / 16000.0;
            let l = 0.4 * (2.0 * std::f32::consts::PI * 500.0 * t).sin()
                + 0.3 * (2.0 * std::f32::consts::PI * 1000.0 * t).sin()
                + 0.3 * (2.0 * std::f32::consts::PI * 1500.0 * t).sin();
            let r = 0.4 * (2.0 * std::f32::consts::PI * 500.0 * t_r).sin()
                + 0.3 * (2.0 * std::f32::consts::PI * 1000.0 * t_r).sin()
                + 0.3 * (2.0 * std::f32::consts::PI * 1500.0 * t_r).sin();
            left_comp.push(l);
            right_comp.push(r);
        }

        let out_comp = beamformer.process_stereo(&left_comp, &right_comp);
        assert!(beamformer.last_metrics().is_off_axis);
        let rms_in_comp = (left_comp.iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();
        let rms_out_comp = (out_comp.iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();
        let att_db_comp = 20.0 * (rms_in_comp / rms_out_comp).log10();
        assert!(
            att_db_comp >= 12.0,
            "Composite off-axis attenuation must be >= 12.0 dB, got {att_db_comp:.2} dB"
        );
    }

    #[test]
    fn test_harmonic_comb_filter_voicing() {
        let mut beamformer = BinauralSpeakerBeamformer::new();

        // Onde harmonique F0 = 200 Hz
        let n = 640;
        let mut signal = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f32 / 16000.0;
            let h1 = 0.6 * (2.0 * std::f32::consts::PI * 200.0 * t).sin();
            let h2 = 0.3 * (2.0 * std::f32::consts::PI * 400.0 * t).sin();
            let h3 = 0.1 * (2.0 * std::f32::consts::PI * 600.0 * t).sin();
            signal.push(h1 + h2 + h3);
        }

        let out = beamformer.process_mono(&signal);
        assert_eq!(out.len(), n);
        let metrics = beamformer.last_metrics();
        assert!(metrics.tracked_f0_hz.is_some());
        let tracked_f0 = metrics.tracked_f0_hz.unwrap();
        assert!(
            (tracked_f0 - 200.0).abs() < 5.0,
            "Tracked F0 {tracked_f0} must be near 200.0 Hz"
        );
        assert!(metrics.voice_periodicity > 0.5);
    }

    #[test]
    fn test_graceful_mono_fallback() {
        let mut beamformer = BinauralSpeakerBeamformer::new();
        let mono_input = vec![0.5f32; 160];

        let out_mono = beamformer.process_mono(&mono_input);
        assert_eq!(out_mono.len(), 160);
        assert!(beamformer.last_metrics().is_mono_fallback);
        assert!(!beamformer.last_metrics().is_off_axis);
        assert_eq!(beamformer.last_metrics().directional_gain, 1.0);

        let out_interleaved = beamformer.process_interleaved(&mono_input, 1);
        assert_eq!(out_interleaved.len(), 160);
        assert!(beamformer.last_metrics().is_mono_fallback);
    }

    #[test]
    fn test_empty_and_mismatched_inputs() {
        let mut beamformer = BinauralSpeakerBeamformer::new();

        // Slices vides
        assert!(beamformer.process_stereo(&[], &[]).is_empty());
        assert!(beamformer.process_mono(&[]).is_empty());
        assert!(beamformer.process_interleaved(&[], 2).is_empty());
        assert!(beamformer.process_interleaved(&[1.0, 2.0], 0).is_empty());

        // Longueurs asymétriques
        let l = vec![0.5f32; 100];
        let r = vec![0.5f32; 80];
        let out = beamformer.process_stereo(&l, &r);
        assert_eq!(out.len(), 80);
    }

    #[test]
    fn test_adversarial_pathological_buffer_sizes() {
        let mut beamformer = BinauralSpeakerBeamformer::new();
        let test_sizes = [0, 1, 17, 31, 163, 1024, 65536];

        for &len in &test_sizes {
            // Stereo input
            let l: Vec<f32> = (0..len).map(|i| ((i as f32) * 0.1).sin()).collect();
            let r: Vec<f32> = (0..len).map(|i| ((i as f32) * 0.1).sin()).collect();
            let out_stereo = beamformer.process_stereo(&l, &r);
            assert_eq!(
                out_stereo.len(),
                len,
                "Stereo buffer size mismatch for len={len}"
            );
            for &sample in &out_stereo {
                assert!(sample.is_finite(), "Sample not finite for len={len}");
            }

            // Mono input
            let out_mono = beamformer.process_mono(&l);
            assert_eq!(
                out_mono.len(),
                len,
                "Mono buffer size mismatch for len={len}"
            );

            // Interleaved input (channels = 2)
            let mut interleaved = Vec::with_capacity(len * 2);
            for i in 0..len {
                interleaved.push(l[i]);
                interleaved.push(r[i]);
            }
            let out_interleaved = beamformer.process_interleaved(&interleaved, 2);
            assert_eq!(
                out_interleaved.len(),
                len,
                "Interleaved buffer size mismatch for len={len}"
            );
        }
    }

    #[test]
    fn test_adversarial_complete_phase_inversion_anti_phase() {
        let mut beamformer = BinauralSpeakerBeamformer::new();
        let n = 480;
        let mut left = Vec::with_capacity(n);
        let mut right = Vec::with_capacity(n);
        for i in 0..n {
            let s = 0.7 * (2.0 * std::f32::consts::PI * 300.0 * i as f32 / 16000.0).sin();
            left.push(s);
            right.push(-s); // x_L = -x_R
        }

        let out = beamformer.process_stereo(&left, &right);
        assert_eq!(out.len(), n);
        let metrics = beamformer.last_metrics();
        assert!(
            metrics.is_off_axis,
            "Anti-phase signal MUST be flagged as off-axis"
        );
        assert!(
            metrics.directional_gain <= 0.2512,
            "Directional gain for anti-phase must guarantee >= 12 dB attenuation, got {}",
            metrics.directional_gain
        );

        // Delay-and-sum of anti-phase cancels to zero
        let rms_out = (out.iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();
        assert!(
            rms_out < 1e-6,
            "Delay-and-sum of anti-phase must completely null the output, got RMS {rms_out}"
        );
    }

    #[test]
    fn test_adversarial_nyquist_frequency_signals() {
        let mut beamformer = BinauralSpeakerBeamformer::new();
        let n = 320;

        // Nyquist tone: 8000 Hz at 16000 Hz sample rate: alternating [+0.8, -0.8, +0.8, ...]
        let mut nyquist = Vec::with_capacity(n);
        for i in 0..n {
            nyquist.push(if i % 2 == 0 { 0.8f32 } else { -0.8f32 });
        }

        // 1. On-axis Nyquist tone: L == R
        let out_on_axis = beamformer.process_stereo(&nyquist, &nyquist);
        let metrics_on = beamformer.last_metrics();
        assert!(
            !metrics_on.is_off_axis,
            "Identical Nyquist tone on-axis must not be flagged off-axis"
        );
        assert!(
            metrics_on.directional_gain <= 1.0,
            "Gain invariant violated"
        );
        let rms_in = 0.8f32;
        let rms_on = (out_on_axis.iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();
        assert!(
            (rms_on - rms_in).abs() < 0.05,
            "On-axis Nyquist tone must be preserved"
        );

        // 2. Off-axis Nyquist tone with 1 sample delay (which is a 180° phase inversion at fs/2)
        beamformer.reset();
        let mut nyquist_delayed = Vec::with_capacity(n);
        for i in 0..n {
            nyquist_delayed.push(if i % 2 == 1 { 0.8f32 } else { -0.8f32 });
        }
        let out_off_axis = beamformer.process_stereo(&nyquist, &nyquist_delayed);
        let metrics_off = beamformer.last_metrics();
        assert!(
            metrics_off.is_off_axis,
            "Delayed Nyquist tone must be flagged off-axis"
        );
        assert!(metrics_off.directional_gain <= 0.2512);
        let rms_off = (out_off_axis.iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();
        assert!(
            rms_off < 1e-6,
            "Nyquist 1-sample delay must cancel completely"
        );
    }

    #[test]
    fn test_adversarial_massive_dc_offsets() {
        let mut beamformer = BinauralSpeakerBeamformer::new();
        let n = 320;

        // Massive DC offset on both channels (100.0)
        let dc = vec![100.0f32; n];
        let out_dc = beamformer.process_stereo(&dc, &dc);
        assert_eq!(out_dc.len(), n);
        assert!(!beamformer.last_metrics().is_off_axis);
        assert_eq!(beamformer.last_metrics().directional_gain, 1.0);
        for &s in &out_dc {
            assert!(
                (s - 100.0).abs() < 1e-3,
                "DC level must be preserved on-axis"
            );
        }

        // Asymmetric DC offset (L = 100.0, R = 0.0) -> spatial difference
        beamformer.reset();
        let zero = vec![0.0f32; n];
        let out_asym = beamformer.process_stereo(&dc, &zero);
        assert!(beamformer.last_metrics().is_off_axis);
        assert!(beamformer.last_metrics().directional_gain <= 0.2512);
        for &s in &out_asym {
            assert!(s <= 12.6, "Asymmetric DC must be attenuated by >= 12 dB");
        }
    }

    #[test]
    fn test_adversarial_loud_white_noise_and_impulses() {
        let mut beamformer = BinauralSpeakerBeamformer::new();
        let n = 1600;

        // High crest-factor Dirac impulses (1.0 at index 10, 0.0 elsewhere)
        let mut spike_l = vec![0.0f32; n];
        let mut spike_r = vec![0.0f32; n];
        spike_l[10] = 1.0;
        spike_r[15] = 1.0; // 5 samples off-axis delay
        let out_spike = beamformer.process_stereo(&spike_l, &spike_r);
        assert_eq!(out_spike.len(), n);
        assert!(beamformer.last_metrics().is_off_axis);
        assert!(beamformer.last_metrics().directional_gain <= 0.2512);

        // Loud uncorrelated pseudo-random white noise
        beamformer.reset();
        let mut noise_l = Vec::with_capacity(n);
        let mut noise_r = Vec::with_capacity(n);
        let mut seed_l = 123456789u32;
        let mut seed_r = 987654321u32;
        for _ in 0..n {
            seed_l = seed_l.wrapping_mul(1664525).wrapping_add(1013904223);
            seed_r = seed_r.wrapping_mul(1664525).wrapping_add(1013904223);
            noise_l.push((seed_l as f32 / u32::MAX as f32) * 2.0 - 1.0);
            noise_r.push((seed_r as f32 / u32::MAX as f32) * 2.0 - 1.0);
        }

        let out_noise = beamformer.process_stereo(&noise_l, &noise_r);
        assert_eq!(out_noise.len(), n);
        assert!(
            beamformer.last_metrics().is_off_axis,
            "Uncorrelated noise must be flagged off-axis"
        );
        assert!(beamformer.last_metrics().directional_gain <= 0.2512);

        let rms_in = (noise_l.iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();
        let rms_out = (out_noise.iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();
        let att_db = 20.0 * (rms_in / rms_out).log10();
        assert!(
            att_db >= 12.0,
            "Uncorrelated noise attenuation must be >= 12.0 dB, got {att_db:.2} dB"
        );
    }

    #[test]
    fn test_adversarial_physical_parameter_extremes() {
        // d = 0.0 m (co-located microphones)
        let itd_zero_d = BinauralSpeakerBeamformer::compute_itd_seconds(0.0, 90.0, 343.0);
        assert_eq!(itd_zero_d, 0.0);
        let itd_samp_zero_d =
            BinauralSpeakerBeamformer::compute_itd_samples(0.0, 90.0, 343.0, 16000);
        assert_eq!(itd_samp_zero_d, 0.0);

        // d = 10.0 m (distant microphones)
        let itd_10m = BinauralSpeakerBeamformer::compute_itd_seconds(10.0, 90.0, 343.0);
        assert!((itd_10m - (10.0 / 343.0)).abs() < 1e-6);
        let itd_samp_10m = BinauralSpeakerBeamformer::compute_itd_samples(10.0, 90.0, 343.0, 16000);
        assert!((itd_samp_10m - (10.0 / 343.0 * 16000.0)).abs() < 0.1);

        // Negative sound speed
        let itd_neg_c = BinauralSpeakerBeamformer::compute_itd_seconds(0.15, 90.0, -343.0);
        assert!((itd_neg_c - (-0.15 / 343.0)).abs() < 1e-6);

        // Non-finite parameters: NaN, Infinity, Zero speed
        let itd_nan_d = BinauralSpeakerBeamformer::compute_itd_seconds(f32::NAN, 0.0, 343.0);
        assert!(itd_nan_d.is_nan());
        let itd_zero_c = BinauralSpeakerBeamformer::compute_itd_seconds(0.15, 90.0, 0.0);
        assert!(itd_zero_c.is_infinite());
        let itd_inf_c = BinauralSpeakerBeamformer::compute_itd_seconds(0.15, 90.0, f32::INFINITY);
        assert_eq!(itd_inf_c, 0.0);

        // Robustness: process_stereo with non-finite samples must not panic
        let mut beamformer = BinauralSpeakerBeamformer::new();
        let nan_buf = vec![f32::NAN; 160];
        let inf_buf = vec![f32::INFINITY; 160];
        let out_nan = beamformer.process_stereo(&nan_buf, &nan_buf);
        assert_eq!(out_nan.len(), 160);
        let out_inf = beamformer.process_stereo(&inf_buf, &inf_buf);
        assert_eq!(out_inf.len(), 160);
    }

    #[test]
    fn test_invariant_directional_suppression_and_off_axis_attenuation_grid() {
        let mut beamformer = BinauralSpeakerBeamformer::new();
        let n = 480;

        // Invariant: G_dir <= 1.0 for ALL inputs (NEVER amplifies off-axis energy)
        // For off-axis signals outside broadside tolerance (tau in 2..=10 samples, theta >= 16.6°):
        // Directional attenuation must remain >= 12.0 dB
        for tau in 0..=10 {
            let mut left = Vec::with_capacity(n);
            let mut right = Vec::with_capacity(n);
            for i in 0..n {
                let t_l = i as f32 / 16000.0;
                let t_r = (i as isize - tau) as f32 / 16000.0;
                let s_l = 0.5 * (2.0 * std::f32::consts::PI * 600.0 * t_l).sin()
                    + 0.3 * (2.0 * std::f32::consts::PI * 1200.0 * t_l).sin();
                let s_r = 0.5 * (2.0 * std::f32::consts::PI * 600.0 * t_r).sin()
                    + 0.3 * (2.0 * std::f32::consts::PI * 1200.0 * t_r).sin();
                left.push(s_l);
                right.push(s_r);
            }

            let out = beamformer.process_stereo(&left, &right);
            let metrics = beamformer.last_metrics();
            assert!(
                metrics.directional_gain <= 1.0,
                "Invariant violated: G_dir must be <= 1.0, got {}",
                metrics.directional_gain
            );

            let rms_in = (left.iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();
            let rms_out = (out.iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();

            if tau <= 1 {
                // Within broadside user tolerance cone (tau=0: 0°, tau=1: ~8.2°)
                assert!(
                    !metrics.is_off_axis,
                    "Speaker cone (tau={tau}) must not be flagged off-axis"
                );
                assert_eq!(metrics.directional_gain, 1.0);
                let delta_db = 20.0 * (rms_in / rms_out).log10().abs();
                assert!(
                    delta_db < 0.5,
                    "On-axis signal must be preserved within 0.5 dB, got {delta_db}"
                );
            } else {
                // Spatial off-axis interferers (tau >= 2 samples, theta >= 16.6° to 90° endfire)
                assert!(
                    metrics.is_off_axis,
                    "Delay tau={tau} must be flagged as off-axis"
                );
                assert!(
                    metrics.directional_gain <= 0.2512,
                    "G_dir must be <= 0.2512 for 12 dB target, got {}",
                    metrics.directional_gain
                );
                let att_db = 20.0 * (rms_in / rms_out).log10();
                assert!(
                    att_db >= 12.0,
                    "Off-axis attenuation for delay tau={tau} must be >= 12.0 dB, got {att_db:.2} dB"
                );
            }
        }
    }

    #[test]
    fn test_adversarial_fractional_delay_comb_filter_extremes() {
        let mut beamformer = BinauralSpeakerBeamformer::new();
        let input: Vec<f32> = (0..320).map(|i| (i as f32 * 0.05).sin()).collect();

        // 1. Extreme low F0 (below 40 Hz, e.g. 10 Hz, 35 Hz, 0.0 Hz, -50 Hz)
        let out_low_10 = beamformer.apply_harmonic_comb(&input, 10.0, 0.8);
        assert_eq!(out_low_10.len(), input.len());
        for &s in &out_low_10 {
            assert!(s.is_finite());
        }

        let out_zero = beamformer.apply_harmonic_comb(&input, 0.0, 0.8);
        assert_eq!(out_zero.len(), input.len());

        let out_neg = beamformer.apply_harmonic_comb(&input, -50.0, 0.8);
        assert_eq!(out_neg.len(), input.len());

        // 2. Extreme high F0 (above 1000 Hz, e.g. 1500 Hz, 8000 Hz, 50000 Hz)
        let out_1500 = beamformer.apply_harmonic_comb(&input, 1500.0, 0.8);
        assert_eq!(out_1500.len(), input.len());
        for &s in &out_1500 {
            assert!(s.is_finite());
        }

        let out_8000 = beamformer.apply_harmonic_comb(&input, 8000.0, 0.8);
        assert_eq!(out_8000.len(), input.len());

        let out_extreme = beamformer.apply_harmonic_comb(&input, 50000.0, 0.8);
        assert_eq!(out_extreme.len(), input.len());

        // 3. Unvoiced (periodicity = 0.0): MUST act as bit-exact identity passthrough
        let out_unvoiced = beamformer.apply_harmonic_comb(&input, 200.0, 0.0);
        assert_eq!(
            out_unvoiced, input,
            "Unvoiced signal must be untouched identity"
        );

        // 4. Non-finite F0 (NaN, Inf)
        let out_nan_f0 = beamformer.apply_harmonic_comb(&input, f32::NAN, 0.8);
        assert_eq!(out_nan_f0.len(), input.len());
        for &s in &out_nan_f0 {
            assert!(s.is_finite());
        }

        let out_inf_f0 = beamformer.apply_harmonic_comb(&input, f32::INFINITY, 0.8);
        assert_eq!(out_inf_f0.len(), input.len());
    }
}
