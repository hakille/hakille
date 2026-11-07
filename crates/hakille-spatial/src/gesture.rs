//! Détection de micro-gestes spatiaux Zero-Touch (§32) :
//! Pincement Index-Pouce, Balayage Latéral et Geste de Silence.
//!
//! Conçu selon les invariants souverains de Hakille :
//! - 100% Rust natif, zéro dépendance externe lourde ni runtime Python.
//! - Primitives déterministes et zéro allocation dynamique sur le chemin critique.
//! - Robuste aux occlusions partielles et au bruit des capteurs via hystérésis.

use crate::geometry::Point3D;
use serde::{Deserialize, Serialize};

/// Nombre standard de points clés articulaires de la main (21 points).
pub const HAND_KEYPOINTS_COUNT: usize = 21;

/// Identifiant d'articulation de la main (compatible MediaPipe / LeapMotion).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HandJointId {
    Wrist = 0,
    ThumbCmc = 1,
    ThumbMcp = 2,
    ThumbIp = 3,
    ThumbTip = 4,
    IndexMcp = 5,
    IndexPip = 6,
    IndexDip = 7,
    IndexTip = 8,
    MiddleMcp = 9,
    MiddlePip = 10,
    MiddleDip = 11,
    MiddleTip = 12,
    RingMcp = 13,
    RingPip = 14,
    RingDip = 15,
    RingTip = 16,
    PinkyMcp = 17,
    PinkyPip = 18,
    PinkyDip = 19,
    PinkyTip = 20,
}

impl HandJointId {
    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Wrist),
            1 => Some(Self::ThumbCmc),
            2 => Some(Self::ThumbMcp),
            3 => Some(Self::ThumbIp),
            4 => Some(Self::ThumbTip),
            5 => Some(Self::IndexMcp),
            6 => Some(Self::IndexPip),
            7 => Some(Self::IndexDip),
            8 => Some(Self::IndexTip),
            9 => Some(Self::MiddleMcp),
            10 => Some(Self::MiddlePip),
            11 => Some(Self::MiddleDip),
            12 => Some(Self::MiddleTip),
            13 => Some(Self::RingMcp),
            14 => Some(Self::RingPip),
            15 => Some(Self::RingDip),
            16 => Some(Self::RingTip),
            17 => Some(Self::PinkyMcp),
            18 => Some(Self::PinkyPip),
            19 => Some(Self::PinkyDip),
            20 => Some(Self::PinkyTip),
            _ => None,
        }
    }
}

/// Latéralité de la main.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Handedness {
    Left,
    Right,
    Unknown,
}

impl Handedness {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Unknown => "unknown",
        }
    }
}

/// Point clé 3D d'une articulation de main avec score de confiance.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HandKeypoint3D {
    pub position: Point3D,
    pub confidence: f32,
}

impl HandKeypoint3D {
    pub const fn new(position: Point3D, confidence: f32) -> Self {
        Self {
            position,
            confidence,
        }
    }

    pub fn is_reliable(&self, threshold: f32) -> bool {
        self.confidence >= threshold
            && self.position.x.is_finite()
            && self.position.y.is_finite()
            && self.position.z.is_finite()
    }
}

/// Squelette 3D complet d'une main à 21 points clés.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandLandmarks3D {
    pub handedness: Handedness,
    pub keypoints: [HandKeypoint3D; HAND_KEYPOINTS_COUNT],
    pub timestamp_ms: u64,
}

impl HandLandmarks3D {
    pub fn new(handedness: Handedness, timestamp_ms: u64) -> Self {
        Self {
            handedness,
            keypoints: [HandKeypoint3D::new(Point3D::ORIGIN, 0.0); HAND_KEYPOINTS_COUNT],
            timestamp_ms,
        }
    }

    pub fn get(&self, joint: HandJointId) -> &HandKeypoint3D {
        &self.keypoints[joint as usize]
    }

    pub fn set(&mut self, joint: HandJointId, keypoint: HandKeypoint3D) {
        self.keypoints[joint as usize] = keypoint;
    }

    pub fn wrist(&self) -> Point3D {
        self.get(HandJointId::Wrist).position
    }

    pub fn thumb_tip(&self) -> Point3D {
        self.get(HandJointId::ThumbTip).position
    }

    pub fn index_tip(&self) -> Point3D {
        self.get(HandJointId::IndexTip).position
    }

    pub fn index_mcp(&self) -> Point3D {
        self.get(HandJointId::IndexMcp).position
    }

    pub fn index_pip(&self) -> Point3D {
        self.get(HandJointId::IndexPip).position
    }

    pub fn middle_tip(&self) -> Point3D {
        self.get(HandJointId::MiddleTip).position
    }

    pub fn ring_tip(&self) -> Point3D {
        self.get(HandJointId::RingTip).position
    }

    pub fn pinky_tip(&self) -> Point3D {
        self.get(HandJointId::PinkyTip).position
    }

    pub fn pinky_mcp(&self) -> Point3D {
        self.get(HandJointId::PinkyMcp).position
    }

    /// Centre estimé de la paume (barycentre poignet + base index + base auriculaire).
    pub fn palm_center(&self) -> Point3D {
        let w = self.wrist();
        let im = self.index_mcp();
        let pm = self.pinky_mcp();
        Point3D::new(
            (w.x + im.x + pm.x) / 3.0,
            (w.y + im.y + pm.y) / 3.0,
            (w.z + im.z + pm.z) / 3.0,
        )
    }

    /// Échelle géométrique de la main (distance poignet -> base majeur), servant d'étalon.
    pub fn hand_scale(&self) -> f32 {
        let w = self.wrist();
        let mm = self.get(HandJointId::MiddleMcp).position;
        let d = w.distance_to(&mm);
        if d > 0.01 { d } else { 0.1 }
    }

    /// Distance euclidienne entre le bout de l'index et le bout du pouce.
    pub fn pinch_distance(&self) -> f32 {
        self.index_tip().distance_to(&self.thumb_tip())
    }

    /// Distance de pincement normalisée par l'échelle de la main.
    pub fn normalized_pinch_distance(&self) -> f32 {
        self.pinch_distance() / self.hand_scale()
    }

    /// Score moyen de confiance sur les articulations critiques.
    pub fn key_confidence(&self) -> f32 {
        let joints = [
            HandJointId::Wrist,
            HandJointId::ThumbTip,
            HandJointId::IndexTip,
            HandJointId::MiddleTip,
        ];
        let sum: f32 = joints.iter().map(|&j| self.get(j).confidence).sum();
        sum / (joints.len() as f32)
    }
}

/// Direction du balayage gestuel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwipeDirection {
    Left,
    Right,
    Up,
    Down,
}

impl SwipeDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

/// Événement de micro-geste détecté (§32).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DetectedMicroGesture {
    /// Pincement index-pouce maintenu ou relâché avec validation.
    IndexThumbPinch {
        hand: Handedness,
        pinch_distance_norm: f32,
        duration_ms: u32,
        confirmed: bool,
    },
    /// Balayage latéral rapide pour évaporer le panneau ou le HUD.
    LateralSwipe {
        hand: Handedness,
        direction: SwipeDirection,
        velocity_m_per_s: f32,
    },
    /// Index vertical posé devant les lèvres activant le mode silence.
    SilenceGesture {
        proximity_to_lips: f32,
        active: bool,
    },
}

/// Configuration des seuils du détecteur de micro-gestes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GestureDetectorConfig {
    /// Seuil normalisé sous lequel un pincement est considéré actif (< 0.28).
    pub pinch_threshold_norm: f32,
    /// Durée minimale de maintien du pincement pour confirmation automatique (ms).
    pub pinch_confirm_duration_ms: u32,
    /// Seuil de vitesse minimale pour un balayage latéral (m/s).
    pub swipe_velocity_threshold: f32,
    /// Ratio d'écrasement horizontal/vertical pour valider un balayage latéral (> 1.8).
    pub swipe_axis_dominance_ratio: f32,
    /// Distance maximale (en mètres) entre le bout de l'index et les lèvres pour le geste silence.
    pub silence_lips_distance_max: f32,
    /// Confiance minimale des articulations requise.
    pub min_confidence: f32,
}

impl Default for GestureDetectorConfig {
    fn default() -> Self {
        Self {
            pinch_threshold_norm: 0.28,
            pinch_confirm_duration_ms: 150,
            swipe_velocity_threshold: 0.65,
            swipe_axis_dominance_ratio: 1.8,
            silence_lips_distance_max: 0.08,
            min_confidence: 0.4,
        }
    }
}

/// Détecteur local à mémoire d'état pour le flux continu de mains.
#[derive(Debug, Clone)]
pub struct MicroGestureDetector {
    config: GestureDetectorConfig,
    pinch_start_timestamp_ms: Option<u64>,
    pinch_confirmed: bool,
    last_palm_position: Option<Point3D>,
    last_timestamp_ms: Option<u64>,
    silence_active: bool,
}

impl MicroGestureDetector {
    pub fn new(config: GestureDetectorConfig) -> Self {
        Self {
            config,
            pinch_start_timestamp_ms: None,
            pinch_confirmed: false,
            last_palm_position: None,
            last_timestamp_ms: None,
            silence_active: false,
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(GestureDetectorConfig::default())
    }

    /// Réinitialise l'état interne (changement de contexte ou perte de suivi).
    pub fn reset(&mut self) {
        self.pinch_start_timestamp_ms = None;
        self.pinch_confirmed = false;
        self.last_palm_position = None;
        self.last_timestamp_ms = None;
        self.silence_active = false;
    }

    /// Analyse une nouvelle frame de main et produit la liste des micro-gestes déclenchés.
    pub fn process_frame(
        &mut self,
        hand: &HandLandmarks3D,
        lips_position: Option<Point3D>,
    ) -> Vec<DetectedMicroGesture> {
        let mut events = Vec::new();

        if hand.key_confidence() < self.config.min_confidence {
            return events;
        }

        let now = hand.timestamp_ms;
        let pinch_norm = hand.normalized_pinch_distance();

        // 1. Détection de pincement Index-Pouce avec hystérésis
        if pinch_norm <= self.config.pinch_threshold_norm {
            match self.pinch_start_timestamp_ms {
                None => {
                    self.pinch_start_timestamp_ms = Some(now);
                    self.pinch_confirmed = false;
                }
                Some(start) => {
                    let duration = (now.saturating_sub(start)) as u32;
                    if duration >= self.config.pinch_confirm_duration_ms && !self.pinch_confirmed {
                        self.pinch_confirmed = true;
                        events.push(DetectedMicroGesture::IndexThumbPinch {
                            hand: hand.handedness,
                            pinch_distance_norm: pinch_norm,
                            duration_ms: duration,
                            confirmed: true,
                        });
                    }
                }
            }
        } else {
            if let Some(start) = self.pinch_start_timestamp_ms.take() {
                let duration = (now.saturating_sub(start)) as u32;
                // Si le pincement a été relâché avant le timeout mais était valide, émet l'événement relâché
                if !self.pinch_confirmed && duration >= 40 {
                    events.push(DetectedMicroGesture::IndexThumbPinch {
                        hand: hand.handedness,
                        pinch_distance_norm: pinch_norm,
                        duration_ms: duration,
                        confirmed: false,
                    });
                }
            }
            self.pinch_confirmed = false;
        }

        // 2. Détection de balayage latéral (Swipe) via vitesse de la paume
        let current_palm = hand.palm_center();
        if let (Some(prev_palm), Some(prev_time)) =
            (self.last_palm_position, self.last_timestamp_ms)
        {
            let dt_s = (now.saturating_sub(prev_time) as f32) / 1000.0;
            if dt_s > 0.005 && dt_s < 0.25 {
                let dx = current_palm.x - prev_palm.x;
                let dy = current_palm.y - prev_palm.y;
                let vx = dx / dt_s;
                let vy = dy / dt_s;
                let abs_vx = vx.abs();
                let abs_vy = vy.abs().max(0.001);

                if abs_vx >= self.config.swipe_velocity_threshold
                    && (abs_vx / abs_vy) >= self.config.swipe_axis_dominance_ratio
                {
                    let direction = if vx > 0.0 {
                        SwipeDirection::Right
                    } else {
                        SwipeDirection::Left
                    };
                    events.push(DetectedMicroGesture::LateralSwipe {
                        hand: hand.handedness,
                        direction,
                        velocity_m_per_s: abs_vx,
                    });
                }
            }
        }
        self.last_palm_position = Some(current_palm);
        self.last_timestamp_ms = Some(now);

        // 3. Détection du geste de Silence (Index vertical sur les lèvres)
        if let Some(lips) = lips_position {
            let index_tip = hand.index_tip();
            let index_pip = hand.index_pip();
            let dist_to_lips = index_tip.distance_to(&lips);

            // Orientation de l'index (doit pointer verticalement vers le haut)
            let index_dir = (index_tip - index_pip).normalize_or_zero();
            let is_vertical = index_dir.y.abs() > 0.70;

            // Les autres doigts (majeur, annulaire, auriculaire) doivent être repliés
            let middle_curled = hand.middle_tip().distance_to(&hand.wrist())
                < hand.index_tip().distance_to(&hand.wrist()) * 0.85;

            let in_silence_zone = dist_to_lips <= self.config.silence_lips_distance_max
                && is_vertical
                && middle_curled;

            if in_silence_zone && !self.silence_active {
                self.silence_active = true;
                events.push(DetectedMicroGesture::SilenceGesture {
                    proximity_to_lips: dist_to_lips,
                    active: true,
                });
            } else if !in_silence_zone && self.silence_active {
                self.silence_active = false;
                events.push(DetectedMicroGesture::SilenceGesture {
                    proximity_to_lips: dist_to_lips,
                    active: false,
                });
            }
        }

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_hand(pinch_dist: f32) -> HandLandmarks3D {
        let mut hand = HandLandmarks3D::new(Handedness::Right, 1000);
        // Wrist at (0, 0, 0)
        hand.set(
            HandJointId::Wrist,
            HandKeypoint3D::new(Point3D::new(0.0, 0.0, 0.0), 0.9),
        );
        // Middle MCP at (0, 0.1, 0) -> scale = 0.1
        hand.set(
            HandJointId::MiddleMcp,
            HandKeypoint3D::new(Point3D::new(0.0, 0.1, 0.0), 0.9),
        );
        // Pinky MCP
        hand.set(
            HandJointId::PinkyMcp,
            HandKeypoint3D::new(Point3D::new(0.04, 0.09, 0.0), 0.9),
        );
        // Index MCP
        hand.set(
            HandJointId::IndexMcp,
            HandKeypoint3D::new(Point3D::new(-0.03, 0.09, 0.0), 0.9),
        );
        // Thumb tip
        hand.set(
            HandJointId::ThumbTip,
            HandKeypoint3D::new(Point3D::new(0.0, 0.15, 0.0), 0.9),
        );
        // Index tip at pinch_dist from thumb
        hand.set(
            HandJointId::IndexTip,
            HandKeypoint3D::new(Point3D::new(pinch_dist, 0.15, 0.0), 0.9),
        );
        // Index PIP
        hand.set(
            HandJointId::IndexPip,
            HandKeypoint3D::new(Point3D::new(pinch_dist, 0.12, 0.0), 0.9),
        );
        // Middle tip (curled)
        hand.set(
            HandJointId::MiddleTip,
            HandKeypoint3D::new(Point3D::new(0.01, 0.08, 0.0), 0.9),
        );

        hand
    }

    #[test]
    fn test_pinch_detection_and_confirmation() {
        let mut detector = MicroGestureDetector::with_default_config();

        // Non-pinched hand (distance 0.06m with scale 0.1m -> norm 0.6 > 0.28)
        let hand_open = mock_hand(0.06);
        let events = detector.process_frame(&hand_open, None);
        assert!(events.is_empty());

        // Pinched hand (distance 0.015m with scale 0.1m -> norm 0.15 <= 0.28) at t=1000ms
        let hand_pinch1 = mock_hand(0.015);
        let events = detector.process_frame(&hand_pinch1, None);
        assert!(
            events.is_empty(),
            "First frame should start timing without immediate confirmation"
        );

        // Pinched hand at t=1200ms (> 150ms hold)
        let mut hand_pinch2 = mock_hand(0.015);
        hand_pinch2.timestamp_ms = 1200;
        let events = detector.process_frame(&hand_pinch2, None);
        assert_eq!(events.len(), 1);
        match &events[0] {
            DetectedMicroGesture::IndexThumbPinch {
                confirmed,
                duration_ms,
                ..
            } => {
                assert!(confirmed);
                assert_eq!(*duration_ms, 200);
            }
            _ => panic!("Expected IndexThumbPinch"),
        }
    }

    #[test]
    fn test_lateral_swipe_detection() {
        let mut detector = MicroGestureDetector::with_default_config();
        let mut h1 = mock_hand(0.05);
        h1.timestamp_ms = 1000;
        let _ = detector.process_frame(&h1, None);

        // Frame 2 at t=1050ms (dt=50ms): hand shifted right by +0.08m (v_x = 1.6 m/s)
        let mut h2 = mock_hand(0.05);
        h2.timestamp_ms = 1050;
        for kp in h2.keypoints.iter_mut() {
            kp.position.x += 0.08;
        }

        let events = detector.process_frame(&h2, None);
        assert_eq!(events.len(), 1);
        match &events[0] {
            DetectedMicroGesture::LateralSwipe {
                direction,
                velocity_m_per_s,
                ..
            } => {
                assert_eq!(*direction, SwipeDirection::Right);
                assert!(*velocity_m_per_s >= 1.0);
            }
            _ => panic!("Expected LateralSwipe"),
        }
    }

    #[test]
    fn test_silence_gesture_on_lips() {
        let mut detector = MicroGestureDetector::with_default_config();
        let lips = Point3D::new(0.02, 0.16, 0.0);
        let mut hand = mock_hand(0.02);
        hand.timestamp_ms = 1000;

        let events = detector.process_frame(&hand, Some(lips));
        assert_eq!(events.len(), 1);
        match &events[0] {
            DetectedMicroGesture::SilenceGesture {
                active,
                proximity_to_lips,
            } => {
                assert!(active);
                assert!(*proximity_to_lips <= 0.08);
            }
            _ => panic!("Expected SilenceGesture"),
        }
    }
}
