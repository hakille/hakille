//! Mode Cognitive Load & Circadian Rhythm: Neuro-Ergonomic Governance (F37, F38, F39, F40, F41).
//!
//! Physiological eye fatigue detection (EAR, hypo-blinking <8/min, hyper-blinking >28/min,
//! sluggish blinks >350ms, PERCLOS 60s, MAR yawns) fused with Tech-Neck posture tracking
//! (>28° sustained for >60s) and solar circadian phase modeling (Day, EveningTwilight,
//! BiologicalLowNight, MorningDawn). Drives the 4 automated adaptations:
//! 1. Vocal rate adaptation (0.80x–0.88x speed rate with calmed prosody).
//! 2. Restful UI theme switching (#181512 warm palette).
//! 3. Cognitive strain governor & heavy task deferral to 09:00 daytime.
//! 4. 20-20-20 break guidance and posture warning.

use crate::cognition::fatigue_detector::AttentionProfile;
use chrono::{DateTime, NaiveTime, Timelike, Utc};
use hakille_ui_protocol::PostureStatusDto;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Threshold for hypo-blinking (staring strain) in blinks per minute.
pub const HYPO_BLINK_THRESHOLD: f32 = 8.0;

/// Threshold for hyper-blinking (compensatory dry eyes) in blinks per minute.
pub const HYPER_BLINK_THRESHOLD: f32 = 28.0;

/// Minimum duration for a sluggish blink (350 ms).
pub const SLUGGISH_BLINK_DURATION_MS: f32 = 350.0;

/// Tech-Neck neck inclination angle threshold in degrees.
pub const TECH_NECK_ANGLE_DEG_THRESHOLD: f32 = 28.0;

/// Duration poor posture must be sustained before emitting an alert (60 seconds).
pub const TECH_NECK_SUSTAINED_THRESHOLD: Duration = Duration::from_secs(60);

/// Continuous focused screen time before 20-20-20 break recommendation (20 minutes).
pub const BREAK_20_20_20_INTERVAL: Duration = Duration::from_secs(20 * 60);

/// Solar circadian phase of the day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CircadianPhase {
    /// 08:00 - 18:00: Nominal daytime focus.
    Day,
    /// 18:00 - 22:00: Evening twilight, melatonin induction, warm shift.
    EveningTwilight,
    /// 22:00 - 06:00: Biological low / night, high fatigue sensitivity.
    BiologicalLowNight,
    /// 06:00 - 08:00: Morning dawn, gradual contrast ramp.
    MorningDawn,
}

impl CircadianPhase {
    /// Classifies circadian phase based on 24-hour clock hour [0, 23].
    pub fn from_hour(hour: u32) -> Self {
        match hour {
            6..8 => Self::MorningDawn,
            8..18 => Self::Day,
            18..22 => Self::EveningTwilight,
            _ => Self::BiologicalLowNight,
        }
    }

    /// Classifies circadian phase from UTC timestamp.
    pub fn from_datetime(dt: &DateTime<Utc>) -> Self {
        Self::from_hour(dt.hour())
    }

    /// Returns `true` if phase corresponds to daytime active hours.
    pub fn is_daytime(&self) -> bool {
        matches!(self, Self::Day | Self::MorningDawn)
    }
}

/// Physiological and cognitive load classification state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CognitiveLoadState {
    /// Optimal vigilance and nominal cognitive capacity.
    Nominal,
    /// Mild sensory strain or twilight fatigue.
    MildFatigue,
    /// Elevated ocular/mental exhaustion requiring calming adjustments.
    ModerateFatigue,
    /// Critical drowsiness or prolonged high strain; heavy tasks restricted.
    CriticalDrowsiness,
}

impl CognitiveLoadState {
    pub fn is_exhausted(&self) -> bool {
        matches!(self, Self::ModerateFatigue | Self::CriticalDrowsiness)
    }
}

/// Tiered classification of task strain for cognitive scheduling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskStrainTier {
    /// Low impact: reading notifications, lightweight queries.
    Light,
    /// Medium impact: code editing, note taking.
    Medium,
    /// High impact: major system refactoring, financial wallet transfer, legal commitments.
    Heavy,
    /// Life-safety / security critical: emergency wipe, distress signal (never deferred).
    Emergency,
}

/// Decision returned by the cognitive strain governor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskScheduleDecision {
    /// Proceed with execution immediately.
    ExecuteImmediately,
    /// Defer task to next daytime window (09:00 UTC/local).
    DeferToNextDaytime {
        scheduled_at: DateTime<Utc>,
        reason: String,
    },
    /// Require explicit user confirmation prompt.
    PromptUserConfirmation { warning_message: String },
}

/// Restful UI theme parameters for twilight/night and elevated fatigue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestfulUiThemeConfig {
    /// Warm, low-contrast dark surface palette.
    pub surface_hex: String, // "#181512"
    /// Warm off-white text color preserving WCAG 2.1 AA.
    pub text_hex: String, // "#C2B5A3"
    /// Animation duration multiplier (1.5x) to calm visual excitation.
    pub animation_duration_scale: f32,
    /// Blue light wavelength attenuation factor (40%).
    pub blue_light_reduction_ratio: f32,
    /// Preserved WCAG contrast ratio.
    pub wcag_contrast_ratio: f32,
}

impl Default for RestfulUiThemeConfig {
    fn default() -> Self {
        Self {
            surface_hex: "#181512".to_string(),
            text_hex: "#C2B5A3".to_string(),
            animation_duration_scale: 1.5,
            blue_light_reduction_ratio: 0.40,
            wcag_contrast_ratio: 7.2,
        }
    }
}

/// Comprehensive real-time neuro-ergonomic assessment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CognitiveLoadProfile {
    pub circadian_phase: CircadianPhase,
    pub load_state: CognitiveLoadState,
    pub perclos: f32,
    pub blink_rate_per_min: f32,
    pub sluggish_blink_detected: bool,
    pub neck_angle_deg: f32,
    pub recommended_vocal_rate: f32,
    pub should_switch_restful_theme: bool,
    pub defer_heavy_tasks: bool,
    pub restful_theme_config: Option<RestfulUiThemeConfig>,
    pub posture_alert: Option<String>,
    pub break_guidance_alert: Option<String>,
}

/// Autonomous manager for physiological cognitive load and circadian ergonomics.
#[derive(Debug, Clone)]
pub struct NeuroErgonomicManager {
    tech_neck_start: Option<Instant>,
    screen_focus_start: Option<Instant>,
    last_break_alert: Option<Instant>,
    last_sluggish_blink: Option<Instant>,
    recent_blink_durations: Vec<(Instant, f32)>, // (timestamp, duration_ms)
}

impl Default for NeuroErgonomicManager {
    fn default() -> Self {
        Self::new()
    }
}

impl NeuroErgonomicManager {
    pub fn new() -> Self {
        Self {
            tech_neck_start: None,
            screen_focus_start: None,
            last_break_alert: None,
            last_sluggish_blink: None,
            recent_blink_durations: Vec::with_capacity(32),
        }
    }

    /// Record an observed blink event with its duration in milliseconds.
    pub fn record_blink(&mut self, now: Instant, duration_ms: f32) {
        if duration_ms >= SLUGGISH_BLINK_DURATION_MS {
            self.last_sluggish_blink = Some(now);
        }
        self.recent_blink_durations.push((now, duration_ms));

        // Retain only last 60 seconds of blink measurements
        self.recent_blink_durations
            .retain(|(ts, _)| now.duration_since(*ts) <= Duration::from_secs(60));
    }

    /// Evaluates multimodal attention, fatigue, posture, and circadian time to produce
    /// a unified cognitive load profile and adaptation triggers.
    pub fn evaluate_cognitive_load(
        &mut self,
        now: Instant,
        current_time: DateTime<Utc>,
        attention: &AttentionProfile,
        posture: &PostureStatusDto,
    ) -> CognitiveLoadProfile {
        let circadian_phase = CircadianPhase::from_datetime(&current_time);

        // 1. Posture tracking (Tech-Neck: neck/spine inclination > 28° sustained for > 60s)
        let neck_angle = posture.spine_inclination_deg;
        let mut posture_alert = None;

        if neck_angle > TECH_NECK_ANGLE_DEG_THRESHOLD {
            let start = *self.tech_neck_start.get_or_insert(now);
            if now.duration_since(start) >= TECH_NECK_SUSTAINED_THRESHOLD {
                posture_alert = Some(format!(
                    "Posture Tech-Neck détectée (inclinaison {:.1}° > 28° depuis plus de 60s). Redressez votre colonne vertébrale.",
                    neck_angle
                ));
            }
        } else {
            self.tech_neck_start = None;
        }

        // 2. Continuous screen focus tracking (20-20-20 rule)
        let mut break_guidance_alert = None;
        if attention.is_screen_focused {
            let start = *self.screen_focus_start.get_or_insert(now);
            let continuous_duration = now.duration_since(start);

            let should_alert = match self.last_break_alert {
                Some(last) => now.duration_since(last) >= BREAK_20_20_20_INTERVAL,
                None => continuous_duration >= BREAK_20_20_20_INTERVAL,
            };

            if should_alert {
                break_guidance_alert = Some(
                    "Règle 20-20-20 : Vous fixez l'écran depuis 20 minutes. Regardez un point à 6 mètres pendant 20 secondes pour détendre vos yeux."
                        .to_string(),
                );
                self.last_break_alert = Some(now);
            }
        } else {
            // User looked away: reset focus session if away for at least 20 seconds
            self.screen_focus_start = None;
        }

        // 3. Physiological eye fatigue detection
        let perclos = attention.perclos_score;
        let blink_rate = attention.blink_rate_per_min;
        let sluggish_blink_detected = match self.last_sluggish_blink {
            Some(ts) => now.duration_since(ts) <= Duration::from_secs(60),
            None => false,
        };

        let is_hypo = blink_rate < HYPO_BLINK_THRESHOLD && blink_rate > 0.0;
        let is_hyper = blink_rate > HYPER_BLINK_THRESHOLD;
        let yawns_elevated = attention.recent_yawn_count >= 2;

        // 4. Synthesize CognitiveLoadState
        let load_state = if perclos >= 0.15 || attention.is_drowsy {
            CognitiveLoadState::CriticalDrowsiness
        } else if perclos >= 0.08
            || sluggish_blink_detected
            || yawns_elevated
            || is_hypo
            || is_hyper
            || attention.is_fatigued
        {
            CognitiveLoadState::ModerateFatigue
        } else if posture_alert.is_some()
            || circadian_phase == CircadianPhase::EveningTwilight
            || (blink_rate < 10.0 || blink_rate > 24.0)
        {
            CognitiveLoadState::MildFatigue
        } else {
            CognitiveLoadState::Nominal
        };

        // 5. Determine the 4 Automated Adaptations
        // Adaptation 1: Vocal rate modulation
        let recommended_vocal_rate = match load_state {
            CognitiveLoadState::CriticalDrowsiness => 0.80,
            CognitiveLoadState::ModerateFatigue => {
                if circadian_phase == CircadianPhase::EveningTwilight {
                    0.88
                } else {
                    0.85
                }
            }
            CognitiveLoadState::MildFatigue => 0.92,
            CognitiveLoadState::Nominal => 1.0,
        };

        // Adaptation 2: Restful UI theme switching
        let should_switch_restful_theme = matches!(
            circadian_phase,
            CircadianPhase::EveningTwilight | CircadianPhase::BiologicalLowNight
        ) || load_state.is_exhausted();

        let restful_theme_config = if should_switch_restful_theme {
            Some(RestfulUiThemeConfig::default())
        } else {
            None
        };

        // Adaptation 3: Heavy task deferral
        let defer_heavy_tasks = load_state == CognitiveLoadState::CriticalDrowsiness
            || (load_state >= CognitiveLoadState::ModerateFatigue
                && circadian_phase == CircadianPhase::BiologicalLowNight);

        CognitiveLoadProfile {
            circadian_phase,
            load_state,
            perclos,
            blink_rate_per_min: blink_rate,
            sluggish_blink_detected,
            neck_angle_deg: neck_angle,
            recommended_vocal_rate,
            should_switch_restful_theme,
            defer_heavy_tasks,
            restful_theme_config,
            posture_alert,
            break_guidance_alert,
        }
    }

    /// Evaluates task execution against the cognitive strain governor.
    pub fn schedule_task_governance(
        &self,
        task_tier: TaskStrainTier,
        load_state: CognitiveLoadState,
        current_time: DateTime<Utc>,
    ) -> TaskScheduleDecision {
        // Emergency tier (security updates, distress wipe) is strictly non-negotiable
        if task_tier == TaskStrainTier::Emergency {
            return TaskScheduleDecision::ExecuteImmediately;
        }

        let circadian_phase = CircadianPhase::from_datetime(&current_time);

        match task_tier {
            TaskStrainTier::Heavy => {
                let should_defer = load_state == CognitiveLoadState::CriticalDrowsiness
                    || (load_state >= CognitiveLoadState::ModerateFatigue
                        && circadian_phase == CircadianPhase::BiologicalLowNight);

                if should_defer {
                    let scheduled_at = calculate_next_daytime_0900(current_time);
                    TaskScheduleDecision::DeferToNextDaytime {
                        scheduled_at,
                        reason: "Charge cognitive trop élevée ou période nocturne biologique. Tâche lourde reportée à 09:00.".to_string(),
                    }
                } else {
                    TaskScheduleDecision::ExecuteImmediately
                }
            }

            TaskStrainTier::Medium => {
                if load_state == CognitiveLoadState::CriticalDrowsiness {
                    TaskScheduleDecision::PromptUserConfirmation {
                        warning_message: "Somnolence critique détectée. Confirmez-vous l'exécution de cette tâche intermédiaire ?".to_string(),
                    }
                } else {
                    TaskScheduleDecision::ExecuteImmediately
                }
            }

            TaskStrainTier::Light | TaskStrainTier::Emergency => {
                TaskScheduleDecision::ExecuteImmediately
            }
        }
    }
}

/// Computes the next 09:00 UTC daytime timestamp from a reference datetime.
fn calculate_next_daytime_0900(now: DateTime<Utc>) -> DateTime<Utc> {
    let target_time = NaiveTime::from_hms_opt(9, 0, 0).unwrap();
    let today_date = now.date_naive();

    if now.time() < target_time {
        // Earlier than 09:00 today: schedule for 09:00 today
        let target_naive = today_date.and_time(target_time);
        DateTime::<Utc>::from_naive_utc_and_offset(target_naive, Utc)
    } else {
        // Later than 09:00 today: schedule for 09:00 tomorrow
        let tomorrow_date = today_date
            .checked_add_days(chrono::Days::new(1))
            .unwrap_or(today_date);
        let target_naive = tomorrow_date.and_time(target_time);
        DateTime::<Utc>::from_naive_utc_and_offset(target_naive, Utc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;
    use hakille_ui_protocol::PostureKindDto;

    fn make_attention(
        is_focused: bool,
        perclos: f32,
        blink_rate: f32,
        yawns: usize,
        is_fatigued: bool,
        is_drowsy: bool,
    ) -> AttentionProfile {
        AttentionProfile {
            is_screen_focused: is_focused,
            gaze_angles: (0.0, 0.0),
            eye_aspect_ratio: 0.28,
            perclos_score: perclos,
            is_drowsy,
            is_fatigued,
            blink_rate_per_min: blink_rate,
            recent_yawn_count: yawns,
            confidence: 0.95,
        }
    }

    fn make_posture(spine_angle_deg: f32) -> PostureStatusDto {
        PostureStatusDto {
            posture: if spine_angle_deg > 28.0 {
                PostureKindDto::Slouched
            } else {
                PostureKindDto::Upright
            },
            is_sitting: true,
            spine_inclination_deg: spine_angle_deg,
            estimated_height_cm: Some(175.0),
            confidence: 0.95,
            timestamp_ms: 1000,
        }
    }

    #[test]
    fn test_circadian_phase_solar_clock_mapping() {
        assert_eq!(CircadianPhase::from_hour(6), CircadianPhase::MorningDawn);
        assert_eq!(CircadianPhase::from_hour(7), CircadianPhase::MorningDawn);
        assert_eq!(CircadianPhase::from_hour(8), CircadianPhase::Day);
        assert_eq!(CircadianPhase::from_hour(14), CircadianPhase::Day);
        assert_eq!(CircadianPhase::from_hour(17), CircadianPhase::Day);
        assert_eq!(
            CircadianPhase::from_hour(18),
            CircadianPhase::EveningTwilight
        );
        assert_eq!(
            CircadianPhase::from_hour(21),
            CircadianPhase::EveningTwilight
        );
        assert_eq!(
            CircadianPhase::from_hour(22),
            CircadianPhase::BiologicalLowNight
        );
        assert_eq!(
            CircadianPhase::from_hour(2),
            CircadianPhase::BiologicalLowNight
        );
        assert_eq!(
            CircadianPhase::from_hour(5),
            CircadianPhase::BiologicalLowNight
        );

        assert!(CircadianPhase::Day.is_daytime());
        assert!(CircadianPhase::MorningDawn.is_daytime());
        assert!(!CircadianPhase::EveningTwilight.is_daytime());
        assert!(!CircadianPhase::BiologicalLowNight.is_daytime());
    }

    #[test]
    fn test_physiological_eye_fatigue_and_blinking() {
        let mut manager = NeuroErgonomicManager::new();
        let now = Instant::now();
        let daytime = DateTime::parse_from_rfc3339("2026-11-07T14:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        // Case 1: Nominal attention and healthy blinking (16 blinks/min, PERCLOS 0.04)
        let nominal_attention = make_attention(true, 0.04, 16.0, 0, false, false);
        let normal_posture = make_posture(12.0);
        let profile =
            manager.evaluate_cognitive_load(now, daytime, &nominal_attention, &normal_posture);
        assert_eq!(profile.load_state, CognitiveLoadState::Nominal);
        assert_eq!(profile.recommended_vocal_rate, 1.0);
        assert!(!profile.should_switch_restful_theme);
        assert!(!profile.defer_heavy_tasks);

        // Case 2: Sluggish blink detected (> 350ms duration)
        manager.record_blink(now, 420.0);
        let profile_sluggish =
            manager.evaluate_cognitive_load(now, daytime, &nominal_attention, &normal_posture);
        assert_eq!(
            profile_sluggish.load_state,
            CognitiveLoadState::ModerateFatigue
        );
        assert!(profile_sluggish.sluggish_blink_detected);
        assert_eq!(profile_sluggish.recommended_vocal_rate, 0.85);

        // Case 3: Hypo-blinking (< 8/min: stare strain)
        let mut manager2 = NeuroErgonomicManager::new();
        let hypo_attention = make_attention(true, 0.04, 5.0, 0, false, false);
        let profile_hypo =
            manager2.evaluate_cognitive_load(now, daytime, &hypo_attention, &normal_posture);
        assert_eq!(profile_hypo.load_state, CognitiveLoadState::ModerateFatigue);

        // Case 4: Hyper-blinking (> 28/min: dry eye compensatory fluttering)
        let mut manager3 = NeuroErgonomicManager::new();
        let hyper_attention = make_attention(true, 0.04, 32.0, 0, false, false);
        let profile_hyper =
            manager3.evaluate_cognitive_load(now, daytime, &hyper_attention, &normal_posture);
        assert_eq!(
            profile_hyper.load_state,
            CognitiveLoadState::ModerateFatigue
        );

        // Case 5: Critical drowsiness (PERCLOS >= 15%)
        let mut manager4 = NeuroErgonomicManager::new();
        let drowsy_attention = make_attention(true, 0.18, 14.0, 3, true, true);
        let profile_drowsy =
            manager4.evaluate_cognitive_load(now, daytime, &drowsy_attention, &normal_posture);
        assert_eq!(
            profile_drowsy.load_state,
            CognitiveLoadState::CriticalDrowsiness
        );
        assert_eq!(profile_drowsy.recommended_vocal_rate, 0.80);
        assert!(profile_drowsy.defer_heavy_tasks);
        assert!(profile_drowsy.should_switch_restful_theme);
    }

    #[test]
    fn test_tech_neck_posture_tracking_alert() {
        let mut manager = NeuroErgonomicManager::new();
        let t0 = Instant::now();
        let daytime = DateTime::parse_from_rfc3339("2026-11-07T14:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let attention = make_attention(true, 0.04, 16.0, 0, false, false);

        // Slouched at 32° (> 28°) at t=0
        let slouched_posture = make_posture(32.0);
        let p0 = manager.evaluate_cognitive_load(t0, daytime, &attention, &slouched_posture);
        assert!(
            p0.posture_alert.is_none(),
            "Alert should not fire before 60s"
        );

        // At t=40s: still slouched, but under 60s
        let t1 = t0 + Duration::from_secs(40);
        let p1 = manager.evaluate_cognitive_load(t1, daytime, &attention, &slouched_posture);
        assert!(p1.posture_alert.is_none());

        // At t=65s: sustained for > 60s -> alert MUST fire!
        let t2 = t0 + Duration::from_secs(65);
        let p2 = manager.evaluate_cognitive_load(t2, daytime, &attention, &slouched_posture);
        assert!(
            p2.posture_alert.is_some(),
            "Tech-Neck alert must fire after 60s"
        );
        assert!(p2.posture_alert.unwrap().contains("Tech-Neck"));

        // User corrects posture (spine angle drops to 15° <= 28°)
        let t3 = t0 + Duration::from_secs(70);
        let upright = make_posture(15.0);
        let p3 = manager.evaluate_cognitive_load(t3, daytime, &attention, &upright);
        assert!(
            p3.posture_alert.is_none(),
            "Alert must clear once posture is corrected"
        );
    }

    #[test]
    fn test_break_guidance_20_20_20() {
        let mut manager = NeuroErgonomicManager::new();
        let t0 = Instant::now();
        let daytime = DateTime::parse_from_rfc3339("2026-11-07T14:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let attention = make_attention(true, 0.04, 16.0, 0, false, false);
        let posture = make_posture(15.0);

        // Continuous focus at t=0
        let p0 = manager.evaluate_cognitive_load(t0, daytime, &attention, &posture);
        assert!(p0.break_guidance_alert.is_none());

        // At t = 20 minutes (1200 seconds) of continuous fixation
        let t_20min = t0 + Duration::from_secs(1200);
        let p_20min = manager.evaluate_cognitive_load(t_20min, daytime, &attention, &posture);
        assert!(p_20min.break_guidance_alert.is_some());
        assert!(p_20min.break_guidance_alert.unwrap().contains("20-20-20"));
    }

    #[test]
    fn test_restful_ui_theme_warm_palette() {
        let mut manager = NeuroErgonomicManager::new();
        let now = Instant::now();
        let evening_twilight = DateTime::parse_from_rfc3339("2026-11-07T20:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let attention = make_attention(true, 0.04, 16.0, 0, false, false);
        let posture = make_posture(15.0);

        let profile = manager.evaluate_cognitive_load(now, evening_twilight, &attention, &posture);
        assert!(profile.should_switch_restful_theme);
        assert!(profile.restful_theme_config.is_some());
        let theme = profile.restful_theme_config.unwrap();
        assert_eq!(theme.surface_hex, "#181512");
        assert_eq!(theme.text_hex, "#C2B5A3");
        assert_eq!(theme.animation_duration_scale, 1.5);
        assert_eq!(theme.blue_light_reduction_ratio, 0.40);
        assert!(theme.wcag_contrast_ratio >= 7.0);
    }

    #[test]
    fn test_cognitive_strain_governor_and_task_deferral() {
        let manager = NeuroErgonomicManager::new();

        // 1. Emergency task is ALWAYS executed immediately
        let night_time = DateTime::parse_from_rfc3339("2026-11-07T02:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let decision_emerg = manager.schedule_task_governance(
            TaskStrainTier::Emergency,
            CognitiveLoadState::CriticalDrowsiness,
            night_time,
        );
        assert_eq!(decision_emerg, TaskScheduleDecision::ExecuteImmediately);

        // 2. Heavy task during BiologicalLowNight (02:30 AM) with ModerateFatigue:
        // Must be deferred to 09:00 today!
        let decision_heavy_night = manager.schedule_task_governance(
            TaskStrainTier::Heavy,
            CognitiveLoadState::ModerateFatigue,
            night_time,
        );
        match decision_heavy_night {
            TaskScheduleDecision::DeferToNextDaytime {
                scheduled_at,
                reason,
            } => {
                assert_eq!(scheduled_at.hour(), 9);
                assert_eq!(scheduled_at.minute(), 0);
                assert_eq!(scheduled_at.day(), 7); // same day because 02:30 < 09:00
                assert!(reason.contains("09:00"));
            }
            _ => panic!("Expected DeferToNextDaytime"),
        }

        // 3. Heavy task during Night at 23:30 (after 09:00) with CriticalDrowsiness:
        // Must be deferred to 09:00 tomorrow (Nov 8)!
        let late_night_time = DateTime::parse_from_rfc3339("2026-11-07T23:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let decision_heavy_late = manager.schedule_task_governance(
            TaskStrainTier::Heavy,
            CognitiveLoadState::CriticalDrowsiness,
            late_night_time,
        );
        match decision_heavy_late {
            TaskScheduleDecision::DeferToNextDaytime { scheduled_at, .. } => {
                assert_eq!(scheduled_at.hour(), 9);
                assert_eq!(scheduled_at.minute(), 0);
                assert_eq!(scheduled_at.day(), 8); // tomorrow!
            }
            _ => panic!("Expected DeferToNextDaytime for tomorrow"),
        }

        // 4. Heavy task during daytime (14:00) with Nominal state -> Execute immediately!
        let day_time = DateTime::parse_from_rfc3339("2026-11-07T14:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let decision_heavy_day = manager.schedule_task_governance(
            TaskStrainTier::Heavy,
            CognitiveLoadState::Nominal,
            day_time,
        );
        assert_eq!(decision_heavy_day, TaskScheduleDecision::ExecuteImmediately);

        // 5. Light task during CriticalDrowsiness -> Execute immediately
        let decision_light = manager.schedule_task_governance(
            TaskStrainTier::Light,
            CognitiveLoadState::CriticalDrowsiness,
            night_time,
        );
        assert_eq!(decision_light, TaskScheduleDecision::ExecuteImmediately);

        // 6. Medium task during CriticalDrowsiness -> Prompt confirmation
        let decision_med = manager.schedule_task_governance(
            TaskStrainTier::Medium,
            CognitiveLoadState::CriticalDrowsiness,
            night_time,
        );
        assert!(matches!(
            decision_med,
            TaskScheduleDecision::PromptUserConfirmation { .. }
        ));
    }
}
