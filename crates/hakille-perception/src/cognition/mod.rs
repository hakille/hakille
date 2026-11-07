//! Sous-système de perception cognitive, biométrique et affective souveraine.

pub mod fatigue_detector;
pub mod gaze_tracker;
pub mod height_estimator;
pub mod multimodal_fusion;

pub use fatigue_detector::{
    AttentionProfile, EYE_CLOSED_EAR_THRESHOLD, FatigueDetector, PERCLOS_DROWSY_THRESHOLD,
    PERCLOS_FATIGUE_THRESHOLD, YAWN_MAR_THRESHOLD,
};
pub use gaze_tracker::{GazeTracker, GazeTrackingResult};
pub use height_estimator::{
    AVERAGE_HUMAN_IPD_METERS, HeightEstimator, PhysicalProfile, SEATED_STATURE_RATIO,
};
pub use multimodal_fusion::{
    DemographicsEstimation, EmotionalState, MultimodalCognitiveEngine, UserMultimodalSnapshot,
};
