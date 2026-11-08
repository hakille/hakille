//! Automated Vocal Rate Adaptation & Calmed Prosody for FormantTts (F39).
//!
//! Proactively modulates TTS speech rate (0.80x–0.88x) and shifts emotional tone
//! to "calm" or "restful" based on physiological eye fatigue, PERCLOS, and solar
//! circadian phases, preventing auditory sensory overload during exhaustion.

use crate::tts::{RENDER_MAX_SPEED, RENDER_MIN_SPEED, TtsRequest};
use hakille_perception::cognition::{CircadianPhase, CognitiveLoadProfile, CognitiveLoadState};
use serde::{Deserialize, Serialize};

/// Standard nominal vocal rate (1.0x).
pub const VOCAL_RATE_NOMINAL: f32 = 1.0;

/// Mild fatigue vocal rate deceleration (0.92x).
pub const VOCAL_RATE_MILD_FATIGUE: f32 = 0.92;

/// Moderate fatigue / Evening Twilight vocal rate deceleration (0.88x).
pub const VOCAL_RATE_MODERATE_TWILIGHT: f32 = 0.88;

/// Moderate fatigue daytime vocal rate deceleration (0.85x).
pub const VOCAL_RATE_MODERATE_DEFAULT: f32 = 0.85;

/// Critical drowsiness / Night vocal rate deceleration (0.80x).
pub const VOCAL_RATE_CRITICAL_DROWSINESS: f32 = 0.80;

/// Adaptive vocal governor applying neuro-ergonomic speech rate and prosody modulation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdaptiveVoiceGovernor {
    /// Minimum allowed speed rate clamp.
    pub min_speed_clamp: f32,
    /// Maximum allowed speed rate clamp.
    pub max_speed_clamp: f32,
}

impl AdaptiveVoiceGovernor {
    pub fn new() -> Self {
        Self {
            min_speed_clamp: RENDER_MIN_SPEED,
            max_speed_clamp: RENDER_MAX_SPEED,
        }
    }

    /// Compute the target vocal deceleration factor based on cognitive load and circadian phase.
    pub fn target_vocal_rate(load_state: CognitiveLoadState, phase: CircadianPhase) -> f32 {
        match load_state {
            CognitiveLoadState::CriticalDrowsiness => VOCAL_RATE_CRITICAL_DROWSINESS,
            CognitiveLoadState::ModerateFatigue => {
                if phase == CircadianPhase::EveningTwilight
                    || phase == CircadianPhase::BiologicalLowNight
                {
                    VOCAL_RATE_MODERATE_TWILIGHT
                } else {
                    VOCAL_RATE_MODERATE_DEFAULT
                }
            }
            CognitiveLoadState::MildFatigue => VOCAL_RATE_MILD_FATIGUE,
            CognitiveLoadState::Nominal => {
                if phase == CircadianPhase::EveningTwilight {
                    VOCAL_RATE_MODERATE_TWILIGHT
                } else if phase == CircadianPhase::BiologicalLowNight {
                    VOCAL_RATE_MODERATE_DEFAULT
                } else {
                    VOCAL_RATE_NOMINAL
                }
            }
        }
    }

    /// Modulate a base speed rate with a target factor, clamping to valid TTS boundaries.
    pub fn modulate_speed_rate(&self, base_rate: f32, target_factor: f32) -> f32 {
        let modulated = base_rate * target_factor;
        modulated.clamp(self.min_speed_clamp, self.max_speed_clamp)
    }

    /// Adapt an outgoing TTS request according to the user's real-time neuro-ergonomic profile.
    pub fn adapt_request(&self, req: &TtsRequest, profile: &CognitiveLoadProfile) -> TtsRequest {
        let mut adapted = req.clone();

        // 1. Modulate speed rate
        let target_factor = profile.recommended_vocal_rate;
        adapted.speed_rate = self.modulate_speed_rate(req.speed_rate, target_factor);

        // 2. Calmed prosody and emotional tone modulation
        match profile.load_state {
            CognitiveLoadState::CriticalDrowsiness => {
                adapted.emotional_tone = "restful".to_string();
            }
            CognitiveLoadState::ModerateFatigue => {
                adapted.emotional_tone = "calm".to_string();
            }
            CognitiveLoadState::MildFatigue => {
                if adapted.emotional_tone.is_empty() || adapted.emotional_tone == "neutral" {
                    adapted.emotional_tone = "soft".to_string();
                }
            }
            CognitiveLoadState::Nominal => {
                let is_evening_or_night = matches!(
                    profile.circadian_phase,
                    CircadianPhase::EveningTwilight | CircadianPhase::BiologicalLowNight
                );
                if is_evening_or_night
                    && (adapted.emotional_tone.is_empty() || adapted.emotional_tone == "neutral")
                {
                    adapted.emotional_tone = "calm".to_string();
                }
            }
        }

        adapted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::FormantTts;

    fn make_test_profile(
        phase: CircadianPhase,
        load: CognitiveLoadState,
        vocal_rate: f32,
    ) -> CognitiveLoadProfile {
        CognitiveLoadProfile {
            circadian_phase: phase,
            load_state: load,
            perclos: 0.05,
            blink_rate_per_min: 15.0,
            sluggish_blink_detected: false,
            neck_angle_deg: 12.0,
            recommended_vocal_rate: vocal_rate,
            should_switch_restful_theme: false,
            defer_heavy_tasks: false,
            restful_theme_config: None,
            posture_alert: None,
            break_guidance_alert: None,
        }
    }

    #[test]
    fn test_adaptive_vocal_rate_modulation_critical_drowsiness() {
        let governor = AdaptiveVoiceGovernor::new();
        let profile = make_test_profile(
            CircadianPhase::BiologicalLowNight,
            CognitiveLoadState::CriticalDrowsiness,
            0.80,
        );

        let req = TtsRequest {
            text: "Bonjour, voici votre synthèse du jour.".to_string(),
            voice_id: "default".to_string(),
            speed_rate: 1.0,
            emotional_tone: "neutral".to_string(),
        };

        let adapted = governor.adapt_request(&req, &profile);
        assert!(
            (adapted.speed_rate - 0.80).abs() < 1e-3,
            "Critical drowsiness must slow speed to 0.80x"
        );
        assert_eq!(
            adapted.emotional_tone, "restful",
            "Critical drowsiness tone must be restful"
        );
    }

    #[test]
    fn test_adaptive_vocal_rate_modulation_moderate_twilight() {
        let governor = AdaptiveVoiceGovernor::new();
        let profile = make_test_profile(
            CircadianPhase::EveningTwilight,
            CognitiveLoadState::ModerateFatigue,
            0.88,
        );

        let req = TtsRequest {
            text: "Votre réunion commence dans 5 minutes.".to_string(),
            voice_id: "default".to_string(),
            speed_rate: 1.0,
            emotional_tone: "neutral".to_string(),
        };

        let adapted = governor.adapt_request(&req, &profile);
        assert!(
            (adapted.speed_rate - 0.88).abs() < 1e-3,
            "Twilight moderate fatigue must slow speed to 0.88x"
        );
        assert_eq!(
            adapted.emotional_tone, "calm",
            "Moderate fatigue tone must be calm"
        );
    }

    #[test]
    fn test_adaptive_vocal_rate_nominal() {
        let governor = AdaptiveVoiceGovernor::new();
        let profile = make_test_profile(CircadianPhase::Day, CognitiveLoadState::Nominal, 1.0);

        let req = TtsRequest {
            text: "Exécution de la commande.".to_string(),
            voice_id: "default".to_string(),
            speed_rate: 1.0,
            emotional_tone: "neutral".to_string(),
        };

        let adapted = governor.adapt_request(&req, &profile);
        assert_eq!(adapted.speed_rate, 1.0);
        assert_eq!(adapted.emotional_tone, "neutral");
    }

    #[test]
    fn test_adaptive_vocal_rate_clamping_invariants() {
        let governor = AdaptiveVoiceGovernor::new();

        // User set high speed (1.5x) modulated by 0.80x = 1.20x -> within [0.7, 1.6]
        let high_rate = governor.modulate_speed_rate(1.5, 0.80);
        assert!((high_rate - 1.20).abs() < 1e-3);

        // Extreme speed 2.5x * 0.80x = 2.0x -> clamped to 1.6x (RENDER_MAX_SPEED)
        let extreme_high = governor.modulate_speed_rate(2.5, 0.80);
        assert_eq!(extreme_high, 1.6);

        // Low speed 0.75x * 0.80x = 0.60x -> clamped to 0.7x (RENDER_MIN_SPEED)
        let low_rate = governor.modulate_speed_rate(0.75, 0.80);
        assert_eq!(low_rate, 0.7);
    }

    #[test]
    fn test_adaptive_formant_tts_synthesis_integration() {
        let governor = AdaptiveVoiceGovernor::new();
        let profile = make_test_profile(
            CircadianPhase::BiologicalLowNight,
            CognitiveLoadState::CriticalDrowsiness,
            0.80,
        );

        let req = TtsRequest {
            text: "Bonne nuit, vos tâches sont sauvegardées.".to_string(),
            voice_id: "default".to_string(),
            speed_rate: 1.0,
            emotional_tone: "".to_string(),
        };

        let adapted = governor.adapt_request(&req, &profile);
        assert_eq!(adapted.speed_rate, 0.80);
        assert_eq!(adapted.emotional_tone, "restful");

        // Synthesize with real FormantTts engine
        let tts = FormantTts::new();
        let pcm = tts.render_pcm(&adapted.text, adapted.speed_rate);
        assert!(
            pcm.is_ok(),
            "Adapted request must synthesize PCM successfully"
        );
        let chunk = pcm.unwrap();
        assert!(!chunk.samples.is_empty());
        assert_eq!(chunk.sample_rate, 24_000);
        assert!(chunk.duration_seconds() > 0.5);
    }
}
