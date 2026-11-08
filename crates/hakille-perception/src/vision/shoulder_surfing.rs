//! Shoulder-Surfing Shield: Visual Surveillance Defense & Third-Party Gaze Tracking.
//!
//! Real-time multi-face background analysis, 3D head pose and gaze evaluation (|Yaw| <= 28.0°, Pitch in [-20.0°, 25.0°]),
//! and integration with sovereign privacy guard notifications.

use hakille_ui_protocol::{PrivacyGuardEvent, PrivacyGuardTrigger};
use serde::{Deserialize, Serialize};

/// Direction of attention and gaze relative to the user's sovereign screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GazeDirection {
    /// Observer is looking directly at the display / screen.
    ScreenFocused,
    /// Observer is looking elsewhere (passerby, distracted).
    LookingAway,
    /// Head pose or gaze orientation could not be determined conclusively.
    Indeterminate,
}

/// Metadata describing a third-party observer detected in the ambient visual background.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThirdPartyObserver {
    /// Bounding box normalized or absolute (x, y, width, height).
    pub bbox: (f32, f32, f32, f32),
    /// Estimated distance from camera in meters.
    pub estimated_distance_m: f32,
    /// Head yaw in degrees (-90.0° to +90.0°, where 0.0° is facing forward).
    pub head_yaw_deg: f32,
    /// Head pitch in degrees (-60.0° to +60.0°, where 0.0° is level).
    pub head_pitch_deg: f32,
    /// Screen-directed gaze classification.
    pub gaze: GazeDirection,
    /// Detection confidence score in [0.0, 1.0].
    pub confidence: f32,
}

/// Consolidated shoulder-surfing risk assessment for a visual frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShoulderSurfingAssessment {
    /// True if one or more third parties are actively focusing on the screen.
    pub is_threat_detected: bool,
    /// Number of third parties currently gazing towards the screen.
    pub active_observers_count: usize,
    /// All detected third-party observers.
    pub observers: Vec<ThirdPartyObserver>,
    /// Recommended session trust cap: 0.40 on threat, 1.0 if safe.
    pub recommended_tau_cap: f32,
    /// Unix timestamp in milliseconds.
    pub timestamp_ms: u64,
}

/// Ambient shoulder-surfing detector analyzing visual scenes for espionage threats.
#[derive(Debug, Clone)]
pub struct ShoulderSurfingDetector {
    /// Minimum relative face area to consider (filters distant background noise).
    pub min_face_area_ratio: f32,
    /// Maximum absolute head yaw angle considered screen-focused (default: 28.0°).
    pub gaze_yaw_threshold_deg: f32,
    /// Minimum head pitch angle considered screen-focused (default: -20.0°).
    pub gaze_pitch_min_deg: f32,
    /// Maximum head pitch angle considered screen-focused (default: 25.0°).
    pub gaze_pitch_max_deg: f32,
}

impl Default for ShoulderSurfingDetector {
    fn default() -> Self {
        Self {
            min_face_area_ratio: 0.015,
            gaze_yaw_threshold_deg: 28.0,
            gaze_pitch_min_deg: -20.0,
            gaze_pitch_max_deg: 25.0,
        }
    }
}

impl ShoulderSurfingDetector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_thresholds(
        min_face_area_ratio: f32,
        gaze_yaw_threshold_deg: f32,
        gaze_pitch_min_deg: f32,
        gaze_pitch_max_deg: f32,
    ) -> Self {
        Self {
            min_face_area_ratio: min_face_area_ratio.max(0.001),
            gaze_yaw_threshold_deg: gaze_yaw_threshold_deg.abs(),
            gaze_pitch_min_deg,
            gaze_pitch_max_deg,
        }
    }

    /// Classify gaze direction based on 3D head pose angles.
    pub fn classify_gaze(&self, yaw_deg: f32, pitch_deg: f32) -> GazeDirection {
        if yaw_deg.abs() <= self.gaze_yaw_threshold_deg
            && pitch_deg >= self.gaze_pitch_min_deg
            && pitch_deg <= self.gaze_pitch_max_deg
        {
            GazeDirection::ScreenFocused
        } else if yaw_deg.abs() > 35.0
            || pitch_deg < self.gaze_pitch_min_deg - 5.0
            || pitch_deg > self.gaze_pitch_max_deg + 5.0
        {
            GazeDirection::LookingAway
        } else {
            GazeDirection::Indeterminate
        }
    }

    /// Estimate observer distance in meters via relative face area.
    pub fn estimate_distance(&self, face_area: f32, total_frame_area: f32) -> f32 {
        if total_frame_area <= 0.0 || face_area <= 0.0 {
            return 2.0;
        }
        let area_ratio = (face_area / total_frame_area).clamp(1e-5, 1.0);
        // Calibrated empirical pinhole projection: sqrt(area_ratio) ~ 0.28 / distance
        (0.28 / area_ratio.sqrt()).clamp(0.4, 5.0)
    }

    /// Evaluate candidate faces: `(bbox(x, y, w, h), yaw_deg, pitch_deg, confidence)`
    pub fn evaluate_candidates(
        &self,
        faces: &[((f32, f32, f32, f32), f32, f32, f32)],
        frame_width: u32,
        frame_height: u32,
        primary_user_index: Option<usize>,
    ) -> ShoulderSurfingAssessment {
        let total_area = (frame_width.max(1) * frame_height.max(1)) as f32;
        let mut observers = Vec::new();

        for (idx, &(bbox, yaw, pitch, confidence)) in faces.iter().enumerate() {
            if Some(idx) == primary_user_index {
                continue;
            }

            let face_area = bbox.2 * bbox.3;
            let area_ratio = face_area / total_area;
            if area_ratio < self.min_face_area_ratio {
                continue;
            }

            let gaze = self.classify_gaze(yaw, pitch);
            let estimated_distance_m = self.estimate_distance(face_area, total_area);

            observers.push(ThirdPartyObserver {
                bbox,
                estimated_distance_m,
                head_yaw_deg: yaw,
                head_pitch_deg: pitch,
                gaze,
                confidence: confidence.clamp(0.0, 1.0),
            });
        }

        let threat_count = observers
            .iter()
            .filter(|o| o.gaze == GazeDirection::ScreenFocused)
            .count();
        let is_threat = threat_count > 0;

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        ShoulderSurfingAssessment {
            is_threat_detected: is_threat,
            active_observers_count: threat_count,
            observers,
            recommended_tau_cap: if is_threat { 0.40 } else { 1.0 },
            timestamp_ms: now_ms,
        }
    }

    /// Build a UI protocol `PrivacyGuardEvent` when a shoulder-surfing threat is detected.
    pub fn create_privacy_guard_event(
        &self,
        assessment: &ShoulderSurfingAssessment,
    ) -> Option<PrivacyGuardEvent> {
        if !assessment.is_threat_detected {
            return None;
        }

        Some(PrivacyGuardEvent {
            trigger: PrivacyGuardTrigger::ShoulderSurfingDetected {
                people_count: assessment.active_observers_count as u32,
            },
            description: format!(
                "Shoulder-surfing threat detected: {} third-party observer(s) looking at screen",
                assessment.active_observers_count
            ),
            recommended_action:
                "Sensitive windows acrylic blur applied, session confidence clamped to 0.40"
                    .to_string(),
            timestamp_ms: assessment.timestamp_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gaze_classification_screen_focused() {
        let detector = ShoulderSurfingDetector::new();
        assert_eq!(
            detector.classify_gaze(0.0, 0.0),
            GazeDirection::ScreenFocused
        );
        assert_eq!(
            detector.classify_gaze(25.0, 10.0),
            GazeDirection::ScreenFocused
        );
        assert_eq!(
            detector.classify_gaze(-20.0, -15.0),
            GazeDirection::ScreenFocused
        );
    }

    #[test]
    fn test_gaze_classification_looking_away() {
        let detector = ShoulderSurfingDetector::new();
        assert_eq!(
            detector.classify_gaze(45.0, 0.0),
            GazeDirection::LookingAway
        );
        assert_eq!(
            detector.classify_gaze(-50.0, 10.0),
            GazeDirection::LookingAway
        );
        assert_eq!(
            detector.classify_gaze(0.0, 35.0),
            GazeDirection::LookingAway
        );
    }

    #[test]
    fn test_evaluate_candidates_primary_excluded_and_threat_detected() {
        let detector = ShoulderSurfingDetector::new();
        let frame_w = 1280;
        let frame_h = 720;

        let faces = vec![
            // Index 0: Primary user in center (large area, looking straight)
            ((400.0, 200.0, 300.0, 300.0), 2.0, 0.0, 0.98),
            // Index 1: Third party over shoulder looking at screen
            ((100.0, 100.0, 150.0, 150.0), 12.0, 5.0, 0.90),
            // Index 2: Third party walking past looking away
            ((900.0, 150.0, 140.0, 140.0), 55.0, 0.0, 0.85),
        ];

        let assessment = detector.evaluate_candidates(&faces, frame_w, frame_h, Some(0));

        assert!(assessment.is_threat_detected);
        assert_eq!(assessment.active_observers_count, 1);
        assert_eq!(assessment.observers.len(), 2);
        assert_eq!(assessment.recommended_tau_cap, 0.40);

        let event = detector.create_privacy_guard_event(&assessment);
        assert!(event.is_some());
        let ev = event.unwrap();
        match ev.trigger {
            PrivacyGuardTrigger::ShoulderSurfingDetected { people_count } => {
                assert_eq!(people_count, 1);
            }
            _ => panic!("Expected ShoulderSurfingDetected trigger"),
        }
    }

    #[test]
    fn test_evaluate_candidates_safe_when_all_looking_away() {
        let detector = ShoulderSurfingDetector::new();
        let frame_w = 1280;
        let frame_h = 720;

        let faces = vec![
            // Index 0: Primary user
            ((400.0, 200.0, 300.0, 300.0), 0.0, 0.0, 0.98),
            // Index 1: Passerby looking away
            ((100.0, 100.0, 150.0, 150.0), 45.0, 0.0, 0.88),
        ];

        let assessment = detector.evaluate_candidates(&faces, frame_w, frame_h, Some(0));
        assert!(!assessment.is_threat_detected);
        assert_eq!(assessment.active_observers_count, 0);
        assert_eq!(assessment.recommended_tau_cap, 1.0);
        assert!(detector.create_privacy_guard_event(&assessment).is_none());
    }

    #[test]
    fn test_small_background_noise_face_filtered() {
        let detector = ShoulderSurfingDetector::new();
        let frame_w = 1920;
        let frame_h = 1080;

        // Tiny face area = 50 * 50 = 2500, ratio = 2500 / 2073600 = 0.0012 < 0.015
        let faces = vec![((100.0, 100.0, 50.0, 50.0), 0.0, 0.0, 0.70)];

        let assessment = detector.evaluate_candidates(&faces, frame_w, frame_h, None);
        assert_eq!(assessment.observers.len(), 0);
        assert!(!assessment.is_threat_detected);
    }
}
