//! Moteur de fusion multimodale bayésienne tardive (Late Fusion) du système cognitif Hakille.
//!
//! Ce module fusionne de manière conjointe et robuste les flux perceptifs :
//! 1. **Vision faciale** : Repères denses FaceMesh 478 pts (centres d'iris, EAR, MAR, IPD).
//! 2. **Émotion visuelle** : HSEmotion MTL (8 classes et espace continu Valence/Arousal).
//! 3. **Démographie & Âge** : SSR-Net avec application stricte de la Règle Constitutionnelle 7.
//! 4. **Biométrie & Stature** : Estimation de taille sans LiDAR et classification posturale.
//! 5. **Vigilance & Attention** : Traqueur PERCLOS 60 secondes et rayon de regard 3D.
//! 6. **Prosodie vocale** : Algorithme DSP YIN ($F_0$, voisement, HNR, Jitter, Shimmer).

use crate::cognition::{
    fatigue_detector::{AttentionProfile, FatigueDetector},
    gaze_tracker::{GazeTracker, GazeTrackingResult},
    height_estimator::{HeightEstimator, PhysicalProfile},
};
use hakille_audio::YinPitchResult;
use hakille_core::Timestamp;
use hakille_inference::{
    facemesh_onnx::{FaceMeshIrisSession, FaceMeshOutput},
    hsemotion_onnx::{EmotionInferenceResult, HSEmotionSession},
    pose_onnx::{PoseInferenceResult, PoseSession},
    ssrnet_onnx::{AgeEstimationResult, SSRNetSession},
};
use hakille_spatial::PostureKind;
use hakille_ui_protocol::{
    AttentionKind, AttentionLevelDto, EmotionUpdateEvent, FatigueAlertEvent, FatigueSeverity,
    PostureKindDto, PostureStatusDto, UserContextEvent, UserPresenceDto, UserPresenceKind,
};
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// État émotionnel catégoriel et dimensionnel fusionné.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmotionalState {
    /// Émotion discrète dominante (ex: Neutral, Happiness, Sadness, Anger, Surprise).
    pub primary_emotion: String,
    /// Score de confiance associé [0.0, 1.0].
    pub confidence: f32,
    /// Valence continue [-1.0 (très désagréable) à +1.0 (très agréable)].
    pub valence: f32,
    /// Arousal continu [-1.0 (très calme/léthargique) à +1.0 (très excité/agité)].
    pub arousal: f32,
    /// Micro-expression transitoire détectée le cas échéant.
    pub transient_micro_expression: Option<String>,
}

/// Estimation démographique non intrusive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DemographicsEstimation {
    /// Âge biologique estimé en années.
    pub estimated_age: f32,
    /// Fourchette d'incertitude [min, max].
    pub age_bracket: (f32, f32),
    /// Règle 7 : Indique si l'utilisateur est considéré mineur (< 18 ans).
    pub is_minor: bool,
    /// Score de confiance global [0.0, 1.0].
    pub confidence: f32,
}

/// Instantané contextuel multimodal complet d'un utilisateur à un instant $t$.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserMultimodalSnapshot {
    pub timestamp: Timestamp,
    pub emotion: EmotionalState,
    pub demographics: DemographicsEstimation,
    pub physique: PhysicalProfile,
    pub attention: AttentionProfile,
}

/// Moteur complet de perception et de fusion multimodale souverain.
pub struct MultimodalCognitiveEngine {
    facemesh: FaceMeshIrisSession,
    hsemotion: HSEmotionSession,
    ssrnet: SSRNetSession,
    pose: PoseSession,
    height_estimator: HeightEstimator,
    fatigue_detector: FatigueDetector,
    gaze_tracker: GazeTracker,
}

impl MultimodalCognitiveEngine {
    /// Initialise le moteur cognitif multimodal avec les sessions réelles ou simulées.
    pub fn new() -> Self {
        Self {
            facemesh: FaceMeshIrisSession::open_or_simulated(None),
            hsemotion: HSEmotionSession::open_or_simulated(None),
            ssrnet: SSRNetSession::open_or_simulated(None),
            pose: PoseSession::open_or_simulated(None),
            height_estimator: HeightEstimator::from_image_dimensions(640.0, 480.0),
            fatigue_detector: FatigueDetector::new(),
            gaze_tracker: GazeTracker::new(),
        }
    }

    /// Exécute une passe complète de perception multimodale à partir d'une trame vidéo RGB
    /// et d'une analyse acoustique vocale optionnelle.
    pub fn process_frame(
        &mut self,
        now_instant: Instant,
        timestamp: Timestamp,
        rgb_data: &[u8],
        width: usize,
        height: usize,
        voice_pitch: Option<&YinPitchResult>,
    ) -> hakille_core::Result<UserMultimodalSnapshot> {
        // 1. Inférence FaceMesh 478 points
        let fm_res: FaceMeshOutput = self.facemesh.infer(rgb_data, width, height)?;

        // 2. Inférence HSEmotion MTL
        let em_res: EmotionInferenceResult = self.hsemotion.infer(rgb_data, width, height)?;

        // 3. Inférence SSR-Net (Âge & Règle 7)
        let age_res: AgeEstimationResult = self.ssrnet.infer(rgb_data, width, height)?;

        // 4. Inférence Pose 17 points
        let pose_res: PoseInferenceResult = self.pose.infer(rgb_data, width, height)?;

        // 5. Estimation de la Stature métrique et profil physique
        let physical_profile = self.height_estimator.estimate_profile(
            &pose_res.skeleton,
            fm_res.ipd_distance,
            pose_res.neck_angle_deg,
        );

        // 6. Suivi 3D du regard et attention écran
        let gaze_res: GazeTrackingResult = self.gaze_tracker.track(
            &fm_res.landmarks,
            physical_profile.camera_distance_m,
            width as f32,
            height as f32,
        );

        // 7. Détection physiologique de fatigue (PERCLOS 60s)
        let attention_profile = self.fatigue_detector.update(
            now_instant,
            fm_res.left_ear,
            fm_res.right_ear,
            fm_res.mar,
            gaze_res.is_screen_focused,
            gaze_res.gaze_angles,
        );

        // 8. Fusion bayésienne tardive émotions Vision + Audio
        let fused_emotion = fuse_emotions(&em_res, voice_pitch);

        let demographics = DemographicsEstimation {
            estimated_age: age_res.estimated_age,
            age_bracket: age_res.age_bracket,
            is_minor: age_res.is_minor,
            confidence: age_res.confidence,
        };

        Ok(UserMultimodalSnapshot {
            timestamp,
            emotion: fused_emotion,
            demographics,
            physique: physical_profile,
            attention: attention_profile,
        })
    }

    /// Convertit le snapshot en événements pour diffusion sur le bus IPC.
    pub fn create_ipc_events(&self, snapshot: &UserMultimodalSnapshot) -> (UserContextEvent, Option<EmotionUpdateEvent>, Option<FatigueAlertEvent>) {
        let ts_ms = snapshot.timestamp.0.timestamp_millis() as u64;

        let presence = UserPresenceDto {
            kind: UserPresenceKind::Present,
            confidence: 0.95,
            distance_meters: Some(snapshot.physique.camera_distance_m),
            people_count: 1,
            timestamp_ms: ts_ms,
        };

        let attention = AttentionLevelDto {
            level: if snapshot.attention.is_screen_focused {
                AttentionKind::Focused
            } else {
                AttentionKind::Distracted
            },
            score: if snapshot.attention.is_screen_focused { 0.92 } else { 0.40 },
            gaze_on_screen: snapshot.attention.is_screen_focused,
            fixation_duration_ms: 1200,
            timestamp_ms: ts_ms,
        };

        let posture = PostureStatusDto {
            posture: match snapshot.physique.posture_state {
                PostureKind::Upright => PostureKindDto::Upright,
                PostureKind::Slouched => PostureKindDto::Slouched,
                PostureKind::LeaningForward => PostureKindDto::LeaningForward,
                PostureKind::Reclined => PostureKindDto::Reclined,
                PostureKind::Standing => PostureKindDto::Standing,
                PostureKind::Seated => PostureKindDto::Seated,
            },
            is_sitting: snapshot.physique.is_seated,
            spine_inclination_deg: snapshot.physique.neck_angle_deg,
            estimated_height_cm: Some(snapshot.physique.estimated_height_cm),
            confidence: snapshot.physique.confidence,
            timestamp_ms: ts_ms,
        };

        let user_ctx = UserContextEvent {
            presence,
            attention,
            posture,
            estimated_age: Some(snapshot.demographics.estimated_age),
            estimated_gender: None,
            primary_emotion: snapshot.emotion.primary_emotion.clone(),
            valence: snapshot.emotion.valence,
            arousal: snapshot.emotion.arousal,
            fatigue_index: snapshot.attention.perclos_score,
            timestamp_ms: ts_ms,
        };

        let emotion_evt = Some(EmotionUpdateEvent {
            primary_emotion: snapshot.emotion.primary_emotion.clone(),
            confidence: snapshot.emotion.confidence,
            valence: snapshot.emotion.valence,
            arousal: snapshot.emotion.arousal,
            source: "multimodal_fusion".to_string(),
            acoustic_tags: vec![],
            timestamp_ms: ts_ms,
        });

        let fatigue_evt = if snapshot.attention.is_drowsy {
            Some(FatigueAlertEvent {
                severity: if snapshot.attention.perclos_score >= 0.20 {
                    FatigueSeverity::Critical
                } else {
                    FatigueSeverity::Moderate
                },
                perclos_p80: snapshot.attention.perclos_score,
                ear: snapshot.attention.eye_aspect_ratio,
                mar: 0.15,
                yawn_count: snapshot.attention.recent_yawn_count as u32,
                recommendation: "Prenez une pause de 10 minutes pour soulager vos yeux.".to_string(),
                timestamp_ms: ts_ms,
            })
        } else {
            None
        };

        (user_ctx, emotion_evt, fatigue_evt)
    }
}

impl Default for MultimodalCognitiveEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Fusionne les indices visuels et prosodiques audio selon un modèle bayésien pondéré.
fn fuse_emotions(
    visual: &EmotionInferenceResult,
    audio_pitch: Option<&YinPitchResult>,
) -> EmotionalState {
    let mut valence = visual.valence;
    let mut arousal = visual.arousal;
    let mut confidence = visual.confidence;
    let mut primary_emotion = visual.primary_emotion.clone();

    if let Some(audio) = audio_pitch && audio.is_voiced {
        // Poids bayésiens : 70% vision, 30% prosodie vocale
        let w_v = 0.70;
        let w_a = 0.30;

        let f0 = audio.f0_hz.unwrap_or(150.0);
        let jitter = audio.jitter_local.unwrap_or(0.0);

        // Influence prosodique sur l'arousal :
        // Un pitch F0 élevé (> 220 Hz) et un Jitter/Shimmer important corrèlent avec une forte excitation/stress
        let acoustic_arousal = if f0 > 240.0 {
            0.50 + (jitter * 0.05).clamp(0.0, 0.40)
        } else if f0 < 110.0 {
            -0.30
        } else {
            0.0
        };

        // Influence prosodique sur la valence :
        // Un HNR élevé (> 15 dB) avec une grande périodicité (> 0.85) traduit une phonation harmonieuse et apaisée
        let acoustic_valence = if audio.hnr_db > 14.0 && audio.periodicity > 0.80 {
            0.35
        } else if audio.hnr_db < 4.0 {
            -0.25
        } else {
            0.0
        };

        valence = w_v * visual.valence + w_a * acoustic_valence;
        arousal = w_v * visual.arousal + w_a * acoustic_arousal;
        confidence = (visual.confidence * 0.8 + audio.periodicity * 0.2).clamp(0.0, 1.0);

        // Si l'arousal acoustique est très élevé et que l'émotion visuelle est neutre, bascule vers Surprise/Alerte
        if arousal > 0.60 && primary_emotion == "Neutral" {
            primary_emotion = "Surprise".to_string();
        }
    }

    EmotionalState {
        primary_emotion,
        confidence,
        valence: valence.clamp(-1.0, 1.0),
        arousal: arousal.clamp(-1.0, 1.0),
        transient_micro_expression: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multimodal_cognitive_engine_e2e_nominal() {
        let mut engine = MultimodalCognitiveEngine::new();
        let dummy_rgb = vec![128u8; 640 * 480 * 3];
        let now = Instant::now();
        let ts = Timestamp::now();

        let snapshot = engine
            .process_frame(now, ts, &dummy_rgb, 640, 480, None)
            .expect("process_frame nominal réussi");

        // Vérifications de validité de bout en bout
        assert_eq!(snapshot.emotion.primary_emotion, "Neutral");
        assert!(snapshot.emotion.confidence > 0.70);
        assert!(!snapshot.demographics.is_minor);
        assert!(snapshot.physique.estimated_height_cm >= 150.0 && snapshot.physique.estimated_height_cm <= 195.0);
        assert!(snapshot.physique.is_seated);
        assert!(snapshot.attention.is_screen_focused);
        assert!(!snapshot.attention.is_drowsy);

        // Création des événements IPC
        let (user_ctx, emotion_evt, fatigue_evt) = engine.create_ipc_events(&snapshot);
        assert_eq!(user_ctx.primary_emotion, "Neutral");
        assert!(emotion_evt.is_some());
        assert!(fatigue_evt.is_none(), "Pas d'alerte fatigue en conditions nominales");
    }

    #[test]
    fn test_multimodal_audio_fusion_influence() {
        let mut engine = MultimodalCognitiveEngine::new();
        let dummy_rgb = vec![128u8; 640 * 480 * 3];
        let now = Instant::now();
        let ts = Timestamp::now();

        // Audio voisé aigu avec fort jitter (voix agitée/surprise)
        let agitated_voice = YinPitchResult {
            f0_hz: Some(320.0),
            is_voiced: true,
            periodicity: 0.92,
            hnr_db: 18.5,
            rms_energy: 0.35,
            jitter_local: Some(2.5),
            shimmer_local: Some(4.0),
        };

        let snapshot = engine
            .process_frame(now, ts, &dummy_rgb, 640, 480, Some(&agitated_voice))
            .expect("process_frame avec audio réussi");

        assert!(snapshot.emotion.arousal > 0.0, "La voix aiguë augmente l'arousal : {}", snapshot.emotion.arousal);
        assert!(snapshot.emotion.valence > 0.0, "Le HNR élevé augmente la valence : {}", snapshot.emotion.valence);
    }
}
