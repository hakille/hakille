//! Silent Distress Blink: Covert Asymmetric Ocular State Machine & Duress Detection.
//!
//! Analyzes sub-pixel eye aspect ratios (EAR_left vs EAR_right) to identify intentional,
//! covert asymmetric distress blink sequences (3 rapid left-eye blinks under coercion)
//! while rejecting normal bilateral blinks, ocular fatigue, and involuntary twitches.

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Discrete ocular state computed from independent left and right Eye Aspect Ratios (EAR).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OcularState {
    /// Both eyes open (normal vision, EAR >= open_threshold).
    BilateralOpen,
    /// Covert signal: Left eye intentionally closed while right eye remains open.
    LeftEyeCovertClosed,
    /// Right eye closed while left eye remains open (not the trigger pattern).
    RightEyeCovertClosed,
    /// Both eyes closed (normal blink or sleep, resets covert sequence).
    BilateralClosed,
}

/// Progress state of the 3-blink covert duress trigger sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DistressTriggerState {
    /// Idle, waiting for initial covert blink.
    Idle,
    /// First rapid covert blink detected and validated.
    Blink1Detected,
    /// Second rapid covert blink detected and validated.
    Blink2Detected,
    /// Third rapid covert blink validated: distress sequence triggered!
    Triggered,
}

/// Asymmetric ocular distress detector tracking rapid covert blink sequences.
#[derive(Debug, Clone)]
pub struct CovertBlinkDetector {
    /// EAR below which an eye is considered closed (default: 0.20).
    pub ear_closed_threshold: f32,
    /// EAR at or above which an eye is considered fully open (default: 0.28).
    pub ear_open_threshold: f32,
    /// Minimum duration of an intentional rapid blink (default: 80 ms).
    pub min_blink_duration: Duration,
    /// Maximum duration of an intentional rapid blink (default: 450 ms).
    pub max_blink_duration: Duration,
    /// Maximum allowable pause between consecutive blinks (default: 650 ms).
    pub max_inter_blink_interval: Duration,
    /// Maximum overall window for the entire 3-blink sequence (default: 2200 ms).
    pub max_sequence_duration: Duration,

    state: DistressTriggerState,
    current_closed_start: Option<Instant>,
    last_blink_completed_at: Option<Instant>,
    sequence_start_at: Option<Instant>,
}

impl Default for CovertBlinkDetector {
    fn default() -> Self {
        Self {
            ear_closed_threshold: 0.20,
            ear_open_threshold: 0.28,
            min_blink_duration: Duration::from_millis(80),
            max_blink_duration: Duration::from_millis(450),
            max_inter_blink_interval: Duration::from_millis(650),
            max_sequence_duration: Duration::from_millis(2200),
            state: DistressTriggerState::Idle,
            current_closed_start: None,
            last_blink_completed_at: None,
            sequence_start_at: None,
        }
    }
}

impl CovertBlinkDetector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_timing(
        min_blink_ms: u64,
        max_blink_ms: u64,
        max_interval_ms: u64,
        max_sequence_ms: u64,
    ) -> Self {
        Self {
            min_blink_duration: Duration::from_millis(min_blink_ms),
            max_blink_duration: Duration::from_millis(max_blink_ms),
            max_inter_blink_interval: Duration::from_millis(max_interval_ms),
            max_sequence_duration: Duration::from_millis(max_sequence_ms),
            ..Default::default()
        }
    }

    /// Current state of the distress trigger state machine.
    pub fn state(&self) -> DistressTriggerState {
        self.state
    }

    /// Reset state machine to Idle and clear all timing accumulators.
    pub fn reset(&mut self) {
        self.state = DistressTriggerState::Idle;
        self.current_closed_start = None;
        self.last_blink_completed_at = None;
        self.sequence_start_at = None;
    }

    /// Classify independent left and right EAR into discrete ocular state.
    pub fn classify_ocular_state(&self, left_ear: f32, right_ear: f32) -> OcularState {
        let left_closed = left_ear < self.ear_closed_threshold;
        let right_closed = right_ear < self.ear_closed_threshold;
        let left_open = left_ear >= self.ear_open_threshold;
        let right_open = right_ear >= self.ear_open_threshold;

        if left_closed && right_closed {
            OcularState::BilateralClosed
        } else if left_closed && right_open {
            OcularState::LeftEyeCovertClosed
        } else if right_closed && left_open {
            OcularState::RightEyeCovertClosed
        } else {
            OcularState::BilateralOpen
        }
    }

    /// Process a new video frame with timestamps and eye aspect ratios.
    ///
    /// Returns `true` exactly upon completion of the 3rd rapid covert left blink,
    /// signifying a verified silent distress trigger under coercion.
    pub fn process_frame(&mut self, now: Instant, left_ear: f32, right_ear: f32) -> bool {
        let ocular = self.classify_ocular_state(left_ear, right_ear);

        // Security rule: Bilateral blink immediately cancels covert sequence
        if ocular == OcularState::BilateralClosed {
            self.reset();
            return false;
        }

        // Global sequence timeout check
        if let Some(seq_start) = self.sequence_start_at {
            if now.duration_since(seq_start) > self.max_sequence_duration {
                self.reset();
            }
        }

        // Track start of left-eye covert closure
        if ocular == OcularState::LeftEyeCovertClosed {
            if self.current_closed_start.is_none() {
                self.current_closed_start = Some(now);
            }
            return false;
        }

        // Left eye reopened (back to BilateralOpen)
        if ocular == OcularState::BilateralOpen {
            if let Some(closed_start) = self.current_closed_start.take() {
                let duration = now.duration_since(closed_start);

                // Check rapid blink duration bounds (80ms <= dt <= 450ms)
                if duration >= self.min_blink_duration && duration <= self.max_blink_duration {
                    return self.advance_state(now);
                } else {
                    // Blink too sluggish or too brief (noise/fatigue) -> reset sequence
                    self.reset();
                }
            }
        } else if ocular == OcularState::RightEyeCovertClosed {
            // Contralateral asymmetric movement invalidates covert cadence
            self.reset();
        }

        false
    }

    fn advance_state(&mut self, now: Instant) -> bool {
        match self.state {
            DistressTriggerState::Idle => {
                self.state = DistressTriggerState::Blink1Detected;
                self.sequence_start_at = Some(now);
                self.last_blink_completed_at = Some(now);
                false
            }
            DistressTriggerState::Blink1Detected => {
                let last = self.last_blink_completed_at.unwrap_or(now);
                let elapsed = now.duration_since(last);
                if elapsed <= self.max_inter_blink_interval {
                    self.state = DistressTriggerState::Blink2Detected;
                    self.last_blink_completed_at = Some(now);
                    false
                } else {
                    // Pause exceeded max interval -> restart as blink 1
                    self.state = DistressTriggerState::Blink1Detected;
                    self.sequence_start_at = Some(now);
                    self.last_blink_completed_at = Some(now);
                    false
                }
            }
            DistressTriggerState::Blink2Detected => {
                let last = self.last_blink_completed_at.unwrap_or(now);
                let elapsed = now.duration_since(last);
                if elapsed <= self.max_inter_blink_interval {
                    self.state = DistressTriggerState::Triggered;
                    self.reset();
                    true // 3rd rapid covert blink verified!
                } else {
                    self.state = DistressTriggerState::Blink1Detected;
                    self.sequence_start_at = Some(now);
                    self.last_blink_completed_at = Some(now);
                    false
                }
            }
            DistressTriggerState::Triggered => {
                self.reset();
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_ocular_state_asymmetry() {
        let detector = CovertBlinkDetector::new();

        // Both open
        assert_eq!(
            detector.classify_ocular_state(0.32, 0.32),
            OcularState::BilateralOpen
        );
        // Left covert closed (left < 0.20, right >= 0.28)
        assert_eq!(
            detector.classify_ocular_state(0.14, 0.30),
            OcularState::LeftEyeCovertClosed
        );
        // Right covert closed
        assert_eq!(
            detector.classify_ocular_state(0.30, 0.12),
            OcularState::RightEyeCovertClosed
        );
        // Bilateral closed
        assert_eq!(
            detector.classify_ocular_state(0.15, 0.16),
            OcularState::BilateralClosed
        );
    }

    #[test]
    fn test_three_rapid_blinks_triggers_distress() {
        let mut detector = CovertBlinkDetector::new();
        let base = Instant::now();

        // Helper to simulate one covert left blink: closure and reopening
        let simulate_blink = |d: &mut CovertBlinkDetector, start_ms: u64, duration_ms: u64| {
            let t_start = base + Duration::from_millis(start_ms);
            let t_end = t_start + Duration::from_millis(duration_ms);

            // Left eye closes
            let triggered_on_close = d.process_frame(t_start, 0.12, 0.32);
            assert!(!triggered_on_close);

            // Left eye reopens
            d.process_frame(t_end, 0.32, 0.32)
        };

        // Blink 1: at 0ms, duration 150ms
        let trig1 = simulate_blink(&mut detector, 0, 150);
        assert!(!trig1);
        assert_eq!(detector.state(), DistressTriggerState::Blink1Detected);

        // Blink 2: at 400ms (interval 250ms), duration 150ms
        let trig2 = simulate_blink(&mut detector, 400, 150);
        assert!(!trig2);
        assert_eq!(detector.state(), DistressTriggerState::Blink2Detected);

        // Blink 3: at 800ms (interval 250ms), duration 150ms
        let trig3 = simulate_blink(&mut detector, 800, 150);
        assert!(trig3, "3rd rapid covert blink must trigger distress!");
        assert_eq!(detector.state(), DistressTriggerState::Idle);
    }

    #[test]
    fn test_bilateral_blink_cancels_sequence() {
        let mut detector = CovertBlinkDetector::new();
        let base = Instant::now();

        // Blink 1
        detector.process_frame(base, 0.12, 0.32);
        detector.process_frame(base + Duration::from_millis(150), 0.32, 0.32);
        assert_eq!(detector.state(), DistressTriggerState::Blink1Detected);

        // Blink 2
        let t2 = base + Duration::from_millis(400);
        detector.process_frame(t2, 0.12, 0.32);
        detector.process_frame(t2 + Duration::from_millis(150), 0.32, 0.32);
        assert_eq!(detector.state(), DistressTriggerState::Blink2Detected);

        // Natural bilateral blink (both eyes closed)
        let t3 = base + Duration::from_millis(700);
        let triggered = detector.process_frame(t3, 0.10, 0.10);
        assert!(!triggered);
        assert_eq!(detector.state(), DistressTriggerState::Idle);
    }

    #[test]
    fn test_sluggish_blink_over_450ms_rejected() {
        let mut detector = CovertBlinkDetector::new();
        let base = Instant::now();

        // Left eye closed for 600ms (fatigue or intentional squint)
        detector.process_frame(base, 0.12, 0.32);
        let triggered = detector.process_frame(base + Duration::from_millis(600), 0.32, 0.32);
        assert!(!triggered);
        assert_eq!(detector.state(), DistressTriggerState::Idle);
    }

    #[test]
    fn test_inter_blink_interval_exceeded_resets_counter() {
        let mut detector = CovertBlinkDetector::new();
        let base = Instant::now();

        // Blink 1
        detector.process_frame(base, 0.12, 0.32);
        detector.process_frame(base + Duration::from_millis(150), 0.32, 0.32);
        assert_eq!(detector.state(), DistressTriggerState::Blink1Detected);

        // Delay 1000ms (> 650ms max interval)
        let t2 = base + Duration::from_millis(1150);
        detector.process_frame(t2, 0.12, 0.32);
        detector.process_frame(t2 + Duration::from_millis(150), 0.32, 0.32);
        // Should have reset and restarted as Blink 1, NOT advanced to Blink 2
        assert_eq!(detector.state(), DistressTriggerState::Blink1Detected);
    }
}
