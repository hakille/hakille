//! Modèles de Vision SOTA, Pose & Biométrie — Inférence locale ONNX & pur Rust.
//!
//! Ce module intègre les modèles d'inférence légère de pointe pour le profilage situationnel :
//! - [`facemesh`] : MediaPipe FaceMesh Iris (478 repères 3D, EAR/MAR, IPD, SolvePnP).
//! - [`hsemotion`] : HSEmotion MTL (8 émotions AffectNet + Valence/Arousal continu).
//! - [`ssrnet`] : SSR-Net (estimation d'âge continue 0-100, genre, protection mineurs Règle 7).
//! - [`rtmpose`] : RTMPose-t (17 points COCO, décodage SimCC, posture 3D).
//! - [`preprocess`] : Redimensionnement bilinéaire pur Rust, normalisation et letterboxing.
//! - [`mock`] : Fixtures et générateurs synthétiques pour validation CI 100% déterministe.

pub mod facemesh;
pub mod hsemotion;
pub mod mock;
pub mod preprocess;
pub mod rtmpose;
pub mod ssrnet;

pub use facemesh::{
    CHIN_INDEX, CanonicalPnPPoints, FACEMESH_INPUT_HEIGHT, FACEMESH_INPUT_WIDTH,
    FACEMESH_MODEL_FILENAME, FACEMESH_POINTS_COUNT, FaceMeshEngine, FaceMeshOutput,
    LEFT_EYE_INDICES, LEFT_IRIS_CENTER_INDEX, MOUTH_BOTTOM, MOUTH_LEFT, MOUTH_RIGHT, MOUTH_TOP,
    NOSE_TIP_INDEX, RIGHT_EYE_INDICES, RIGHT_IRIS_CENTER_INDEX,
};
pub use hsemotion::{
    EMOTION_CLASSES, EmotionInferenceResult, HSEMOTION_INPUT_SIZE, HSEMOTION_MODEL_FILENAME,
    HsEmotionEngine,
};
pub use mock::{
    synthetic_blank_rgb, synthetic_checkerboard_rgb, synthetic_face_crop, synthetic_gradient_rgb,
};
pub use preprocess::{
    LetterboxTransform, NormalizationMode, letterbox_rgb_nchw, resize_bilinear_rgb_nchw,
};
pub use rtmpose::{
    DEFAULT_SIMCC_SPLIT_RATIO, POSE_INPUT_SIZE, POSE_MODEL_FILENAME, PoseInferenceResult,
    RtmPoseEngine, decode_simcc_joints,
};
pub use ssrnet::{
    AgeEstimationResult, AgeGroup, GenderKind, MINOR_AGE_THRESHOLD, MINOR_CONFIDENCE_THRESHOLD,
    SSRNET_INPUT_SIZE, SSRNET_MODEL_FILENAME, SsrNetEngine,
};

/// Trait commun pour les moteurs d'inférence de vision de Hakille.
pub trait VisionEngine {
    /// Nom lisible du modèle.
    fn engine_name(&self) -> &'static str;
    /// Indique si le moteur s'exécute en simulation synthétique ou avec de vrais poids ONNX.
    fn is_simulated(&self) -> bool;
}

impl VisionEngine for FaceMeshEngine {
    fn engine_name(&self) -> &'static str {
        "FaceMeshIris"
    }
    fn is_simulated(&self) -> bool {
        self.is_mock()
    }
}

impl VisionEngine for HsEmotionEngine {
    fn engine_name(&self) -> &'static str {
        "HSEmotionMTL"
    }
    fn is_simulated(&self) -> bool {
        self.is_mock()
    }
}

impl VisionEngine for SsrNetEngine {
    fn engine_name(&self) -> &'static str {
        "SSRNetAge"
    }
    fn is_simulated(&self) -> bool {
        self.is_mock()
    }
}

impl VisionEngine for RtmPoseEngine {
    fn engine_name(&self) -> &'static str {
        "RTMPose"
    }
    fn is_simulated(&self) -> bool {
        self.is_mock()
    }
}
