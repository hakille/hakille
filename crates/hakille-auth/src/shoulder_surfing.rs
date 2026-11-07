//! Shoulder-Surfing Shield: Visual Surveillance Protection & Access Degradation (F27, F28, F29, F30).
//!
//! Multi-face background extraction with SCRFD-0.5GF, third-party gaze tracking via solvePnP 3D pose
//! (|Yaw| <= 28.0°, Pitch in [-20.0°, 25.0°]), instantaneous confidence degradation (tau <= 0.40),
//! acrylic window blurring, and HUD alert dispatch.

use crate::face::{FaceBoundingBox, FaceDetection};
use crate::face_liveness::solve_pnp_head_pose;
use crate::multimodal::ContinuousMultimodalFusion;
use hakille_core::Timestamp;
use hakille_ui_protocol::{PrivacyGuardEvent, PrivacyGuardTrigger};
use serde::{Deserialize, Serialize};

/// Discrete gaze orientation relative to display screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GazeDirection {
    /// Third party is directly focusing on the display.
    ScreenFocused,
    /// Third party is oriented away from display.
    LookingAway,
    /// Indeterminate head pose or angle.
    Indeterminate,
}

/// Metadata for a third-party observer detected in the ambient scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThirdPartyObserver {
    pub bbox: (f32, f32, f32, f32),
    pub estimated_distance_m: f32,
    pub head_yaw_deg: f32,
    pub head_pitch_deg: f32,
    pub gaze: GazeDirection,
    pub confidence: f32,
}

/// Assessment output returned after visual frame inspection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShoulderSurfingAssessment {
    pub is_threat_detected: bool,
    pub active_observers_count: usize,
    pub observers: Vec<ThirdPartyObserver>,
    pub recommended_tau_cap: f32, // 0.40 if threat, 1.0 if safe
    pub timestamp: Timestamp,
}

/// ShoulderSurfingDetector implements F27 and F28 sovereign shoulder surfing detection.
#[derive(Debug, Clone)]
pub struct ShoulderSurfingDetector {
    pub min_face_area_ratio: f32,    // 0.015 (filters distant micro-faces)
    pub gaze_yaw_threshold_deg: f32, // 28.0°
    pub gaze_pitch_min_deg: f32,     // -20.0°
    pub gaze_pitch_max_deg: f32,     // 25.0°
    pub primary_iou_threshold: f32,  // 0.40
}

impl Default for ShoulderSurfingDetector {
    fn default() -> Self {
        Self {
            min_face_area_ratio: 0.015,
            gaze_yaw_threshold_deg: 28.0,
            gaze_pitch_min_deg: -20.0,
            gaze_pitch_max_deg: 25.0,
            primary_iou_threshold: 0.40,
        }
    }
}

impl ShoulderSurfingDetector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluates all detected faces in a visual frame, isolates the primary enrolled user,
    /// and performs solvePnP head pose estimation to detect third parties gazing at the screen.
    pub fn evaluate(
        &self,
        faces: &[FaceDetection],
        width: u32,
        height: u32,
        primary: Option<&FaceBoundingBox>,
    ) -> ShoulderSurfingAssessment {
        let total_area = (width.max(1) * height.max(1)) as f32;
        let mut observers = Vec::new();

        for face in faces {
            // Exclude the primary user's face if identified
            if let Some(primary_box) = primary {
                if face.bbox.intersection_over_union(primary_box) > self.primary_iou_threshold {
                    continue;
                }
            }

            let area_ratio = face.bbox.area() / total_area;
            if area_ratio < self.min_face_area_ratio {
                continue;
            }

            // Estimate 3D head pose from 5 canonical landmarks via solvePnP
            let pose = solve_pnp_head_pose(&face.landmarks, width as f32, height as f32);

            let gaze = if pose.yaw.abs() <= self.gaze_yaw_threshold_deg
                && pose.pitch >= self.gaze_pitch_min_deg
                && pose.pitch <= self.gaze_pitch_max_deg
            {
                GazeDirection::ScreenFocused
            } else if pose.yaw.abs() > 35.0
                || pose.pitch < self.gaze_pitch_min_deg - 5.0
                || pose.pitch > self.gaze_pitch_max_deg + 5.0
            {
                GazeDirection::LookingAway
            } else {
                GazeDirection::Indeterminate
            };

            let estimated_distance_m = (0.28 / area_ratio.sqrt()).clamp(0.4, 5.0);

            observers.push(ThirdPartyObserver {
                bbox: (face.bbox.x, face.bbox.y, face.bbox.width, face.bbox.height),
                estimated_distance_m,
                head_yaw_deg: pose.yaw,
                head_pitch_deg: pose.pitch,
                gaze,
                confidence: face.confidence,
            });
        }

        let threat_count = observers
            .iter()
            .filter(|o| o.gaze == GazeDirection::ScreenFocused)
            .count();
        let is_threat = threat_count > 0;

        ShoulderSurfingAssessment {
            is_threat_detected: is_threat,
            active_observers_count: threat_count,
            observers,
            recommended_tau_cap: if is_threat { 0.40 } else { 1.0 },
            timestamp: Timestamp::now(),
        }
    }

    /// Evaluates visual threats, applies instantaneous tau degradation (tau <= 0.40) to multimodal fusion,
    /// and formats the corresponding `PrivacyGuardEvent` for acrylic HUD blurring.
    pub fn evaluate_and_guard(
        &self,
        faces: &[FaceDetection],
        width: u32,
        height: u32,
        primary: Option<&FaceBoundingBox>,
        fusion: &mut ContinuousMultimodalFusion,
    ) -> (ShoulderSurfingAssessment, Option<PrivacyGuardEvent>) {
        let assessment = self.evaluate(faces, width, height, primary);
        let event = if assessment.is_threat_detected {
            fusion.enforce_shoulder_surfing_shield();
            self.build_privacy_guard_event(&assessment)
        } else {
            None
        };
        (assessment, event)
    }

    /// Construct UI protocol PrivacyGuardEvent for ambient shell dispatch.
    pub fn build_privacy_guard_event(
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
            timestamp_ms: assessment.timestamp.0.timestamp_millis() as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face::FaceLandmarks;

    fn create_test_landmarks(yaw_offset: f32, pitch_offset: f32) -> FaceLandmarks {
        // Canonical landmarks: [left_eye, right_eye, nose_tip, left_mouth, right_mouth]
        // Base geometry: left_eye=(100, 100), right_eye=(200, 100), nose=(150, 150), left_mouth=(110, 200), right_mouth=(190, 200)
        // In solve_pnp_head_pose:
        // iod = 100
        // eye_center_x = 150, nose_rel_x = (nx - 150) / 100
        // yaw = nose_rel_x * 50.0  => nx = 150 + (yaw / 50.0) * 100 = 150 + yaw * 2.0
        // eye_center_y = 100, mouth_center_y = 200
        // nose_eye_dist = ny - 100, nose_mouth_dist = 200 - ny
        // pitch_ratio = (ny - 100) / (200 - ny) - 1.0
        // pitch = pitch_ratio * 30.0
        let nx = 150.0 + (yaw_offset / 50.0) * 100.0;
        let ny = 150.0 + (pitch_offset / 30.0) * 20.0;
        FaceLandmarks {
            points: [
                (100.0, 100.0),
                (200.0, 100.0),
                (nx, ny),
                (110.0, 200.0),
                (190.0, 200.0),
            ],
        }
    }

    #[test]
    fn test_shoulder_surfing_threat_detection_and_tau_degradation() {
        let detector = ShoulderSurfingDetector::new();
        let mut fusion = ContinuousMultimodalFusion::new(0.95);

        let primary_box = FaceBoundingBox::new(300.0, 200.0, 200.0, 200.0);
        let primary_face = FaceDetection {
            bbox: primary_box.clone(),
            landmarks: create_test_landmarks(0.0, 0.0),
            confidence: 0.99,
        };

        // Third party behind looking at screen (Yaw = 10.0°, Pitch = 5.0°)
        let third_party_box = FaceBoundingBox::new(50.0, 50.0, 120.0, 120.0);
        let third_party_face = FaceDetection {
            bbox: third_party_box,
            landmarks: create_test_landmarks(10.0, 5.0),
            confidence: 0.92,
        };

        let faces = vec![primary_face, third_party_face];
        let (assessment, event) =
            detector.evaluate_and_guard(&faces, 1280, 720, Some(&primary_box), &mut fusion);

        assert!(assessment.is_threat_detected);
        assert_eq!(assessment.active_observers_count, 1);
        assert_eq!(assessment.recommended_tau_cap, 0.40);
        assert_eq!(fusion.current_confidence, 0.40);
        assert_eq!(fusion.kalman_tracker.estimate, 0.40);
        assert_eq!(fusion.kalman_tracker.error_covariance, 0.35);

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
    fn test_third_party_looking_away_safe() {
        let detector = ShoulderSurfingDetector::new();
        let mut fusion = ContinuousMultimodalFusion::new(0.95);

        let primary_box = FaceBoundingBox::new(300.0, 200.0, 200.0, 200.0);
        let primary_face = FaceDetection {
            bbox: primary_box.clone(),
            landmarks: create_test_landmarks(0.0, 0.0),
            confidence: 0.99,
        };

        // Third party looking away (Yaw = 45.0°)
        let passerby_face = FaceDetection {
            bbox: FaceBoundingBox::new(50.0, 50.0, 120.0, 120.0),
            landmarks: create_test_landmarks(45.0, 0.0),
            confidence: 0.90,
        };

        let faces = vec![primary_face, passerby_face];
        let (assessment, event) =
            detector.evaluate_and_guard(&faces, 1280, 720, Some(&primary_box), &mut fusion);

        assert!(!assessment.is_threat_detected);
        assert_eq!(assessment.active_observers_count, 0);
        assert_eq!(assessment.recommended_tau_cap, 1.0);
        // Confidence remains untouched at 0.95
        assert_eq!(fusion.current_confidence, 0.95);
        assert!(event.is_none());
    }
}
