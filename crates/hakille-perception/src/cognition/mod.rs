//! Sous-système de perception cognitive, biométrique et affective souveraine.

pub mod covert_blink;
pub mod fatigue_detector;
pub mod gaze_intent;
pub mod gaze_tracker;
pub mod height_estimator;
pub mod multimodal_fusion;
pub mod neuro_ergonomics;

pub use covert_blink::{CovertBlinkDetector, DistressTriggerState, OcularState};
pub use fatigue_detector::{
    AttentionProfile, EYE_CLOSED_EAR_THRESHOLD, FatigueDetector, PERCLOS_DROWSY_THRESHOLD,
    PERCLOS_FATIGUE_THRESHOLD, YAWN_MAR_THRESHOLD,
};
pub use gaze_intent::{
    ANTI_DOUBLE_TRIGGER_COOLDOWN, ContactlessGestureAuthorizer, DWELL_DURATION_MIN,
    GESTURE_VALIDITY_WINDOW, GazeInteractableTarget, GestureAuthDecision, GestureAuthState,
    JITTER_TOLERANCE_WINDOW, MAX_GAZE_DISPERSION_RAD, MAX_LATERAL_YAW_STABILITY_DEG,
    MAX_NOD_DURATION, MAX_PITCH_EXCURSION_DEG, MAX_REBOUND_BASELINE_DEG,
    MAX_SHOULDER_DELTA_Y_METERS, MIN_NOD_DURATION, MIN_PEAK_ANGULAR_VELOCITY_DEG_PER_SEC,
    MIN_PITCH_EXCURSION_DEG,
};
pub use gaze_tracker::{GazeTracker, GazeTrackingResult};
pub use height_estimator::{
    AVERAGE_HUMAN_IPD_METERS, HeightEstimator, PhysicalProfile, SEATED_STATURE_RATIO,
};
pub use multimodal_fusion::{
    DemographicsEstimation, EmotionalState, MultimodalCognitiveEngine, UserMultimodalSnapshot,
};
pub use neuro_ergonomics::{
    BREAK_20_20_20_INTERVAL, CircadianPhase, CognitiveLoadProfile, CognitiveLoadState,
    HYPER_BLINK_THRESHOLD, HYPO_BLINK_THRESHOLD, NeuroErgonomicManager, RestfulUiThemeConfig,
    SLUGGISH_BLINK_DURATION_MS, TECH_NECK_ANGLE_DEG_THRESHOLD, TECH_NECK_SUSTAINED_THRESHOLD,
    TaskScheduleDecision, TaskStrainTier,
};
