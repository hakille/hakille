use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpoofingType {
    ReplayAttack,
    SyntheticVoiceClone,
    VocoderPhaseAnomaly,
    ZeroJitterUnnaturalRegularity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivenessCheckResult {
    pub is_live_human: bool,
    pub liveness_confidence: f64,
    pub synthetic_voice_probability: f64,
    pub detected_attack: Option<SpoofingType>,
    pub explanation: String,
}

/// Voice Liveness & Anti-Spoofing Classifier (§30c, §29f).
/// Protects against replay recordings and modern neural voice clones (NeuTTS, CosyVoice, etc.)
/// by inspecting acoustic micro-variations and physiological markers.
///
/// # Limites documentées (pas de faux spectral)
/// - Analyse **temporelle uniquement** : énergie RMS, taux de passages par
///   zéro (ZCR) et micro-jitter d'énergie par trames de 10 ms. **Aucune**
///   analyse spectrale (FFT, MFCC, phase vocoder) n'est effectuée : les
///   libellés n'évoquent donc jamais de « caractéristiques spectrales ».
/// - Échantillonnage attendu : **16 000 Hz mono**. Toute autre fréquence
///   est refusée (`analyze_pcm_frame_with_rate` → `Err` + `warn`) car les
///   seuils ZCR/jitter sont calibrés pour 16 kHz.
/// - Fenêtre minimale : 100 ms (1 600 échantillons à 16 kHz) ; en-deçà,
///   rejet fail-closed.
/// - Ne distingue pas finement rejeu vs clone neuronal : les deux sont
///   regroupés sous « régularité non naturelle ». Un audit forensique
///   (spectral + modèle dédié) reste nécessaire pour qualifier l'attaque.
/// - Ton pur constant ou silence → rejet systématique (non-biologique).
pub struct VoiceLivenessDetector {
    pub min_liveness_threshold: f64,
    pub max_synthetic_threshold: f64,
}

impl Default for VoiceLivenessDetector {
    fn default() -> Self {
        Self {
            min_liveness_threshold: 0.80,
            max_synthetic_threshold: 0.40,
        }
    }
}

impl VoiceLivenessDetector {
    pub fn new(min_liveness_threshold: f64, max_synthetic_threshold: f64) -> Self {
        Self {
            min_liveness_threshold,
            max_synthetic_threshold,
        }
    }

    /// Analyze a 16kHz audio sample buffer for biometric liveness indicators.
    /// Compat : suppose 16 000 Hz (délègue à `analyze_pcm_frame_with_rate`).
    pub fn analyze_pcm_frame(&self, samples: &[f32]) -> LivenessCheckResult {
        match self.analyze_pcm_frame_with_rate(samples, 16000) {
            Ok(r) => r,
            Err(e) => LivenessCheckResult {
                is_live_human: false,
                liveness_confidence: 0.0,
                synthetic_voice_probability: 0.5,
                detected_attack: None,
                explanation: e,
            },
        }
    }

    /// Variante stricte avec `sample_rate` explicite.
    /// Refuse (`Err` + `warn`) toute fréquence != 16 000 Hz — les seuils
    /// temporels (ZCR, jitter) ne sont valides qu'à 16 kHz.
    pub fn analyze_pcm_frame_with_rate(
        &self,
        samples: &[f32],
        sample_rate: u32,
    ) -> Result<LivenessCheckResult, String> {
        if sample_rate != 16000 {
            tracing::warn!(
                "Fréquence d'échantillonnage inattendue pour la vivacité vocale : {} Hz (attendu 16000 Hz) — seuils temporels non calibrés, rejet.",
                sample_rate
            );
            return Err(format!(
                "sample_rate {} Hz refusé (attendu 16000 Hz mono)",
                sample_rate
            ));
        }
        Ok(self.analyze_pcm_frame_16k(samples))
    }

    /// Cœur d'analyse temporelle à 16 kHz (RMS / ZCR / jitter d'énergie).
    fn analyze_pcm_frame_16k(&self, samples: &[f32]) -> LivenessCheckResult {
        if samples.len() < 1600 {
            // less than 100ms
            return LivenessCheckResult {
                is_live_human: false,
                liveness_confidence: 0.0,
                synthetic_voice_probability: 0.5,
                detected_attack: None,
                explanation: "Échantillon audio trop court pour l'analyse de vivacité (< 100ms)"
                    .to_string(),
            };
        }

        // 1. Calculate energy and zero-crossing rate (ZCR)
        let mut energy = 0.0f32;
        let mut zero_crossings = 0u32;
        let mut prev_sign = samples[0] >= 0.0;

        for &s in samples {
            energy += s * s;
            let current_sign = s >= 0.0;
            if current_sign != prev_sign {
                zero_crossings += 1;
                prev_sign = current_sign;
            }
        }
        let rms = (energy / samples.len() as f32).sqrt();
        let zcr = zero_crossings as f32 / samples.len() as f32;

        // 2. Micro-jitter variance estimation (physiological vocal cord vibration)
        let mut diffs = Vec::with_capacity(samples.len() / 160);
        for chunk in samples.chunks(160) {
            let chunk_energy: f32 = chunk.iter().map(|&x| x * x).sum();
            diffs.push(chunk_energy);
        }

        let mean_diff = diffs.iter().sum::<f32>() / diffs.len().max(1) as f32;
        let variance: f32 =
            diffs.iter().map(|&x| (x - mean_diff).powi(2)).sum::<f32>() / diffs.len().max(1) as f32;
        let jitter_metric = (variance.sqrt() / (mean_diff + 1e-6)).clamp(0.0, 1.0);

        // 3. Spoofing detection criteria
        // Constant flat tone or zero-crossing absence indicates non-biological audio
        let is_completely_flat = jitter_metric < 0.005 || zcr < 0.001;
        let is_suspiciously_synthetic = is_completely_flat || (jitter_metric < 0.02 && rms > 0.02);

        let synthetic_prob = if is_completely_flat {
            0.95
        } else if is_suspiciously_synthetic {
            0.75
        } else if zcr > 0.02 && jitter_metric >= 0.05 {
            0.05
        } else {
            0.20
        };

        let is_live = synthetic_prob < self.max_synthetic_threshold && rms > 0.01;
        let liveness_confidence = if is_live {
            1.0 - synthetic_prob
        } else {
            synthetic_prob
        };

        let detected_attack = if is_completely_flat {
            Some(SpoofingType::SyntheticVoiceClone)
        } else if is_suspiciously_synthetic {
            Some(SpoofingType::ZeroJitterUnnaturalRegularity)
        } else {
            None
        };

        let explanation = match detected_attack {
            Some(SpoofingType::SyntheticVoiceClone) => {
                "Rejet : signal temporel plat sans micro-variations physiologiques (énergie/ZCR/jitter), compatible avec une synthèse vocale ou un clonage neuronal — analyse temporelle seule, sans examen spectral.".to_string()
            }
            Some(SpoofingType::ZeroJitterUnnaturalRegularity) => {
                "Rejet : micro-variations physiologiques absentes (régularité temporelle non naturelle), possible rejeu d'enregistrement audio.".to_string()
            }
            _ => "Vivacité vocale humaine authentifiée avec micro-variations physiologiques naturelles (temporel : énergie/ZCR/jitter).".to_string(),
        };

        LivenessCheckResult {
            is_live_human: is_live,
            liveness_confidence,
            synthetic_voice_probability: synthetic_prob,
            detected_attack,
            explanation,
        }
    }
}
