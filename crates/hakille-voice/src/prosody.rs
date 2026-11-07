//! Living Voice Bayesian Adaptive Prosody Engine
//!
//! Transforms epistemic confidence (`ConfidenceLevel`) and situational urgency
//! into physically grounded formant voice parameters (Rate, Pitch F0, Breathiness,
//! and Terminal Interrogative Rise).

use crate::formant::{FORMANT_MAX_F0, FORMANT_MIN_F0, FormantVoice};
use hakille_core::trust::ConfidenceLevel;
use serde::{Deserialize, Serialize};

/// Constantes des bornes prosodiques.
pub const PROSODY_MIN_RATE: f32 = 0.70;
pub const PROSODY_MAX_RATE: f32 = 1.60;
pub const PROSODY_MIN_F0: f32 = FORMANT_MIN_F0; // 80.0 Hz
pub const PROSODY_MAX_F0: f32 = FORMANT_MAX_F0; // 300.0 Hz
pub const PROSODY_MIN_BREATHINESS: f32 = 0.08;
pub const PROSODY_MAX_BREATHINESS: f32 = 0.35;

pub const TERMINAL_RISE_DELTA_HZ: f32 = 40.0;
pub const TERMINAL_RISE_WINDOW_MS: f32 = 200.0;
pub const UNCERTAINTY_CONFIDENCE_THRESHOLD: f32 = 0.60;
pub const EPISTEMIC_DISCREPANCY_THRESHOLD: f32 = 0.40;

/// Contexte cognitif et situationnel gouvernant l'adaptation prosodique.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProsodyContext {
    /// Niveau de confiance bayésien (modèle LLM + corroboration graphe).
    pub confidence: ConfidenceLevel,
    /// Degré d'urgence situationnelle U ∈ [0.0, 1.0].
    pub urgency: f32,
}

impl ProsodyContext {
    /// Crée un contexte prosodique avec validation et bornage d'urgence.
    pub fn new(confidence: ConfidenceLevel, urgency: f32) -> Self {
        let urgency = if urgency.is_finite() {
            urgency.clamp(0.0, 1.0)
        } else {
            0.0
        };
        Self {
            confidence,
            urgency,
        }
    }

    /// Profil nominal : confiance maximale, urgence nulle.
    pub fn confident_neutral() -> Self {
        Self::new(ConfidenceLevel::compute(1.0, 1.0), 0.0)
    }

    /// Profil d'urgence maximale : confiance 1.0, urgence 1.0.
    pub fn high_urgency() -> Self {
        Self::new(ConfidenceLevel::compute(1.0, 1.0), 1.0)
    }

    /// Profil incertain : confiance 0.35, urgence 0.0.
    pub fn uncertain() -> Self {
        Self::new(ConfidenceLevel::compute(0.35, 0.35), 0.0)
    }

    /// Profil de contradiction épistémique (|model - graph| > 0.40).
    pub fn epistemic_contradiction() -> Self {
        Self::new(ConfidenceLevel::compute(0.85, 0.25), 0.0)
    }
}

/// Résultat de l'adaptation prosodique appliquée à un profil vocal.
#[derive(Debug, Clone, PartialEq)]
pub struct AdaptedProsody {
    /// Profil vocal adapté (débit, F0, souffle).
    pub voice: FormantVoice,
    /// Indique si la montée interrogative terminale (+40 Hz sur 200 ms) est activée.
    pub terminal_rise: bool,
    /// Élévation finale en Hz (+40 Hz si activée, 0 sinon).
    pub terminal_rise_delta_hz: f32,
    /// Fenêtre temporelle de la montée finale en millisecondes (200 ms).
    pub terminal_rise_window_ms: f32,
}

/// Moteur d'adaptation prosodique bayésienne.
#[derive(Debug, Default, Clone)]
pub struct AdaptiveProsodyEngine;

impl AdaptiveProsodyEngine {
    pub fn new() -> Self {
        Self
    }

    /// R = R_base * [1.0 + 0.25*U - 0.20*(1.0 - C)] ∈ [0.70, 1.60]
    pub fn compute_rate(base_rate: f32, urgency: f32, confidence_combined: f32) -> f32 {
        let base = if base_rate.is_finite() {
            base_rate
        } else {
            1.0
        };
        let u = if urgency.is_finite() {
            urgency.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let c = if confidence_combined.is_finite() {
            confidence_combined.clamp(0.0, 1.0)
        } else {
            1.0
        };
        let r = base * (1.0 + 0.25 * u - 0.20 * (1.0 - c));
        let clamped = r.clamp(PROSODY_MIN_RATE, PROSODY_MAX_RATE);
        if clamped.is_finite() { clamped } else { 1.0 }
    }

    /// F0 = F0_base * [1.0 + 0.18*U + 0.08*(1.0 - C)] ∈ [80.0, 300.0] Hz
    pub fn compute_pitch(base_f0: f32, urgency: f32, confidence_combined: f32) -> f32 {
        let base = if base_f0.is_finite() { base_f0 } else { 120.0 };
        let u = if urgency.is_finite() {
            urgency.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let c = if confidence_combined.is_finite() {
            confidence_combined.clamp(0.0, 1.0)
        } else {
            1.0
        };
        let f0 = base * (1.0 + 0.18 * u + 0.08 * (1.0 - c));
        let clamped = f0.clamp(PROSODY_MIN_F0, PROSODY_MAX_F0);
        if clamped.is_finite() { clamped } else { 120.0 }
    }

    /// B = B_base + 0.15 * (1.0 - C) * (1.0 - U) ∈ [0.08, 0.35]
    pub fn compute_breathiness(base_b: f32, urgency: f32, confidence_combined: f32) -> f32 {
        let base = if base_b.is_finite() { base_b } else { 0.15 };
        let u = if urgency.is_finite() {
            urgency.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let c = if confidence_combined.is_finite() {
            confidence_combined.clamp(0.0, 1.0)
        } else {
            1.0
        };
        let b = base + 0.15 * (1.0 - c) * (1.0 - u);
        let clamped = b.clamp(PROSODY_MIN_BREATHINESS, PROSODY_MAX_BREATHINESS);
        if clamped.is_finite() { clamped } else { 0.15 }
    }

    /// Condition de montée interrogative terminale : C < 0.60 OU |C_model - C_graph| > 0.40
    /// En cas de confiance non-finie (NaN/Inf), déclenchement fail-closed par précaution épistémique.
    pub fn should_terminal_rise(ctx: &ProsodyContext) -> bool {
        if !ctx.confidence.combined.is_finite()
            || !ctx.confidence.model_confidence.is_finite()
            || !ctx.confidence.graph_corroboration.is_finite()
        {
            return true;
        }
        let c = ctx.confidence.combined as f32;
        let c_model = ctx.confidence.model_confidence as f32;
        let c_graph = ctx.confidence.graph_corroboration as f32;
        let discrepancy = (c_model - c_graph).abs();
        c < UNCERTAINTY_CONFIDENCE_THRESHOLD || discrepancy > EPISTEMIC_DISCREPANCY_THRESHOLD
    }

    /// Évalue l'ensemble des transferts prosodiques à partir de la voix de base et du contexte.
    pub fn evaluate(&self, base_voice: &FormantVoice, ctx: &ProsodyContext) -> AdaptedProsody {
        let c = ctx.confidence.combined as f32;
        let rate = Self::compute_rate(base_voice.rate, ctx.urgency, c);
        let f0 = Self::compute_pitch(base_voice.f0_base_hz, ctx.urgency, c);
        let breathiness = Self::compute_breathiness(base_voice.breathiness, ctx.urgency, c);
        let terminal_rise = Self::should_terminal_rise(ctx);

        let mut voice = base_voice.clone();
        voice.rate = rate;
        voice.f0_base_hz = f0;
        voice.breathiness = breathiness;

        AdaptedProsody {
            voice,
            terminal_rise,
            terminal_rise_delta_hz: if terminal_rise {
                TERMINAL_RISE_DELTA_HZ
            } else {
                0.0
            },
            terminal_rise_window_ms: if terminal_rise {
                TERMINAL_RISE_WINDOW_MS
            } else {
                0.0
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prosody_formula_bounds_and_clamping() {
        // Base voice standard
        let base_rate = 1.0f32;
        let base_f0 = 120.0f32;
        let base_b = 0.15f32;

        // 1. Nominal : U = 0, C = 1 -> R = 1.0, F0 = 120.0, B = 0.15
        assert_eq!(
            AdaptiveProsodyEngine::compute_rate(base_rate, 0.0, 1.0),
            1.0
        );
        assert_eq!(
            AdaptiveProsodyEngine::compute_pitch(base_f0, 0.0, 1.0),
            120.0
        );
        assert_eq!(
            AdaptiveProsodyEngine::compute_breathiness(base_b, 0.0, 1.0),
            0.15
        );

        // 2. High urgency : U = 1, C = 1 -> R = 1.25, F0 = 120 * 1.18 = 141.6, B = 0.15
        let r_urg = AdaptiveProsodyEngine::compute_rate(base_rate, 1.0, 1.0);
        assert!((r_urg - 1.25).abs() < 1e-4);
        let f0_urg = AdaptiveProsodyEngine::compute_pitch(base_f0, 1.0, 1.0);
        assert!((f0_urg - 141.6).abs() < 1e-4);
        assert_eq!(
            AdaptiveProsodyEngine::compute_breathiness(base_b, 1.0, 1.0),
            0.15
        );

        // 3. Low confidence : U = 0, C = 0 -> R = 0.80, F0 = 120 * 1.08 = 129.6, B = 0.15 + 0.15 = 0.30
        let r_low = AdaptiveProsodyEngine::compute_rate(base_rate, 0.0, 0.0);
        assert!((r_low - 0.80).abs() < 1e-4);
        let f0_low = AdaptiveProsodyEngine::compute_pitch(base_f0, 0.0, 0.0);
        assert!((f0_low - 129.6).abs() < 1e-4);
        let b_low = AdaptiveProsodyEngine::compute_breathiness(base_b, 0.0, 0.0);
        assert!((b_low - 0.30).abs() < 1e-4);

        // 4. Emergency with uncertainty : U = 1, C = 0 -> R = 1.05, F0 = 120 * 1.26 = 151.2, B = 0.15
        let r_both = AdaptiveProsodyEngine::compute_rate(base_rate, 1.0, 0.0);
        assert!((r_both - 1.05).abs() < 1e-4);
        let f0_both = AdaptiveProsodyEngine::compute_pitch(base_f0, 1.0, 0.0);
        assert!((f0_both - 151.2).abs() < 1e-4);
        assert_eq!(
            AdaptiveProsodyEngine::compute_breathiness(base_b, 1.0, 0.0),
            0.15
        );

        // 5. Extreme clamping check
        assert_eq!(
            AdaptiveProsodyEngine::compute_rate(2.0, 1.0, 1.0),
            PROSODY_MAX_RATE
        );
        assert_eq!(
            AdaptiveProsodyEngine::compute_rate(0.4, 0.0, 0.0),
            PROSODY_MIN_RATE
        );
        assert_eq!(
            AdaptiveProsodyEngine::compute_pitch(290.0, 1.0, 0.0),
            PROSODY_MAX_F0
        );
        assert_eq!(
            AdaptiveProsodyEngine::compute_pitch(70.0, 0.0, 1.0),
            PROSODY_MIN_F0
        );
        assert_eq!(
            AdaptiveProsodyEngine::compute_breathiness(0.30, 0.0, 0.0),
            PROSODY_MAX_BREATHINESS
        );
        assert_eq!(
            AdaptiveProsodyEngine::compute_breathiness(0.05, 1.0, 1.0),
            PROSODY_MIN_BREATHINESS
        );

        // 6. Non-finite values handling
        assert_eq!(
            AdaptiveProsodyEngine::compute_rate(base_rate, f32::NAN, 1.0),
            1.0
        );
        assert_eq!(
            AdaptiveProsodyEngine::compute_pitch(base_f0, 0.0, f32::INFINITY),
            120.0
        );
    }

    #[test]
    fn test_terminal_rise_conditions() {
        // High confidence, corroborated -> no rise
        let ctx_conf = ProsodyContext::confident_neutral();
        assert!(!AdaptiveProsodyEngine::should_terminal_rise(&ctx_conf));

        // High urgency with full confidence -> no rise
        let ctx_urg = ProsodyContext::high_urgency();
        assert!(!AdaptiveProsodyEngine::should_terminal_rise(&ctx_urg));

        // Low confidence (0.35) -> triggers rise
        let ctx_uncert = ProsodyContext::uncertain();
        assert!(AdaptiveProsodyEngine::should_terminal_rise(&ctx_uncert));

        // Epistemic contradiction (|0.85 - 0.25| = 0.60 > 0.40) -> triggers rise
        let ctx_contra = ProsodyContext::epistemic_contradiction();
        assert!(AdaptiveProsodyEngine::should_terminal_rise(&ctx_contra));

        // Boundary testing
        // combined = 0.6 * 0.6 + 0.4 * 0.6 = 0.600 -> should not rise if discrepancy is 0
        let ctx_exact = ProsodyContext::new(ConfidenceLevel::compute(0.60, 0.60), 0.0);
        assert!(!AdaptiveProsodyEngine::should_terminal_rise(&ctx_exact));

        // combined = 0.6 * 0.59 + 0.4 * 0.59 = 0.59 < 0.60 -> triggers rise
        let ctx_below = ProsodyContext::new(ConfidenceLevel::compute(0.59, 0.59), 0.0);
        assert!(AdaptiveProsodyEngine::should_terminal_rise(&ctx_below));

        // discrepancy = |0.75 - 0.34| = 0.41 > 0.40 -> triggers rise
        let ctx_discr = ProsodyContext::new(ConfidenceLevel::compute(0.75, 0.34), 0.0);
        assert!(AdaptiveProsodyEngine::should_terminal_rise(&ctx_discr));

        // discrepancy = |0.70 - 0.31| = 0.39 <= 0.40, combined = 0.70*0.6 + 0.31*0.4 = 0.544 < 0.60 -> triggers because of C < 0.60
        // To test purely discrepancy <= 0.40 with C >= 0.60:
        // model = 0.85, graph = 0.50 -> discrepancy = 0.35 <= 0.40, combined = 0.85*0.6 + 0.50*0.4 = 0.71 >= 0.60 -> no rise
        let ctx_no_discr = ProsodyContext::new(ConfidenceLevel::compute(0.85, 0.50), 0.0);
        assert!(!AdaptiveProsodyEngine::should_terminal_rise(&ctx_no_discr));
    }

    #[test]
    fn test_engine_evaluate() {
        let engine = AdaptiveProsodyEngine::new();
        let base_voice = FormantVoice::default();

        // 1. Confident neutral
        let adapted_neutral = engine.evaluate(&base_voice, &ProsodyContext::confident_neutral());
        assert_eq!(adapted_neutral.voice.rate, 1.0);
        assert_eq!(adapted_neutral.voice.f0_base_hz, 120.0);
        assert_eq!(adapted_neutral.voice.breathiness, 0.15);
        assert!(!adapted_neutral.terminal_rise);
        assert_eq!(adapted_neutral.terminal_rise_delta_hz, 0.0);

        // 2. Uncertain
        let adapted_uncert = engine.evaluate(&base_voice, &ProsodyContext::uncertain());
        assert!(adapted_uncert.voice.rate < 1.0);
        assert!(adapted_uncert.voice.breathiness > 0.15);
        assert!(adapted_uncert.terminal_rise);
        assert_eq!(
            adapted_uncert.terminal_rise_delta_hz,
            TERMINAL_RISE_DELTA_HZ
        );
        assert_eq!(
            adapted_uncert.terminal_rise_window_ms,
            TERMINAL_RISE_WINDOW_MS
        );
    }
}
