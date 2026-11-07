//! Détecteur de fatigue, vigilance et somnolence physiologique (EAR, MAR, PERCLOS 60s).
//!
//! Ce module implémente les standards médicaux de surveillance de l'attention au travail et au volant :
//! 1. **EAR (Eye Aspect Ratio)** : Détection de fermeture palpébrale continue (seuil $\text{EAR} < 0.20$).
//! 2. **PERCLOS (Percentage of Eye Closure)** : Fraction du temps où les yeux sont fermés à au moins 80%
//!    sur une fenêtre glissante de 60 secondes.
//!    - $\text{PERCLOS} < 8\%$ : Vigilance nominale et attention active.
//!    - $8\% \le \text{PERCLOS} < 15\%$ : Fatigue modérée, baisse de concentration.
//!    - $\text{PERCLOS} \ge 15\%$ : Somnolence critique / micro-sommeils avérés.
//! 3. **MAR (Mouth Aspect Ratio)** : Détection des bâillements fréquents ($\text{MAR} > 0.45$ durant $\ge 1.5\,\text{s}$).

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Seuil standard EAR marquant un œil fermé (80% d'occlusion palpébrale).
pub const EYE_CLOSED_EAR_THRESHOLD: f32 = 0.20;
/// Seuil standard MAR marquant une ouverture buccale de bâillement.
pub const YAWN_MAR_THRESHOLD: f32 = 0.45;
/// Durée minimale d'une ouverture buccale pour être comptabilisée comme bâillement (secondes).
pub const MIN_YAWN_DURATION_SECS: f32 = 1.5;
/// Seuil critique de somnolence PERCLOS (15%).
pub const PERCLOS_DROWSY_THRESHOLD: f32 = 0.15;
/// Seuil de fatigue modérée PERCLOS (8%).
pub const PERCLOS_FATIGUE_THRESHOLD: f32 = 0.08;
/// Taille maximale de l'historique temporel (60 secondes à 30 FPS = 1800 trames).
pub const SLIDING_WINDOW_CAPACITY: usize = 1800;

/// Échantillon temporel individuel.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
struct FrameSample {
    timestamp: Instant,
    ear: f32,
    mar: f32,
    is_closed: bool,
    is_yawning: bool,
}

/// Profil d'attention, regard et vigilance physiologique.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttentionProfile {
    /// L'utilisateur regarde-t-il activement l'écran de travail ?
    pub is_screen_focused: bool,
    /// Angles de regard (Pitch, Yaw en radians).
    pub gaze_angles: (f32, f32),
    /// Ratio d'aspect oculaire moyen (EAR) actuel.
    pub eye_aspect_ratio: f32,
    /// Score PERCLOS glissant sur 60 secondes [0.0, 1.0].
    pub perclos_score: f32,
    /// Alerte de somnolence critique activée.
    pub is_drowsy: bool,
    /// Alerte de fatigue modérée activée.
    pub is_fatigued: bool,
    /// Nombre de clignements normaux détectés par minute.
    pub blink_rate_per_min: f32,
    /// Nombre de bâillements récents durant la fenêtre glissante.
    pub recent_yawn_count: usize,
    /// Confiance de la mesure [0.0, 1.0].
    pub confidence: f32,
}

/// Détecteur physiologique de fatigue à fenêtre glissante de 60 secondes.
pub struct FatigueDetector {
    window_duration: Duration,
    samples: VecDeque<FrameSample>,
    current_closed_start: Option<Instant>,
    current_yawn_start: Option<Instant>,
    completed_yawns: VecDeque<Instant>,
    total_blinks: u64,
}

impl FatigueDetector {
    /// Crée un détecteur avec une fenêtre glissante de 60 secondes par défaut.
    pub fn new() -> Self {
        Self {
            window_duration: Duration::from_secs(60),
            samples: VecDeque::with_capacity(SLIDING_WINDOW_CAPACITY),
            current_closed_start: None,
            current_yawn_start: None,
            completed_yawns: VecDeque::new(),
            total_blinks: 0,
        }
    }

    /// Enregistre une nouvelle observation palpébrale et buccale.
    pub fn update(
        &mut self,
        now: Instant,
        left_ear: f32,
        right_ear: f32,
        mar: f32,
        is_screen_focused: bool,
        gaze_angles: (f32, f32),
    ) -> AttentionProfile {
        let avg_ear = (left_ear + right_ear) * 0.5;
        let is_closed = avg_ear < EYE_CLOSED_EAR_THRESHOLD;
        let is_yawn_opening = mar >= YAWN_MAR_THRESHOLD;

        // Suivi des clignements
        if is_closed {
            if self.current_closed_start.is_none() {
                self.current_closed_start = Some(now);
            }
        } else if let Some(start) = self.current_closed_start {
            let closure_dur = now.duration_since(start).as_secs_f32();
            if closure_dur >= 0.08 && closure_dur <= 0.80 {
                self.total_blinks += 1;
            }
            self.current_closed_start = None;
        }

        // Suivi des bâillements
        if is_yawn_opening {
            if self.current_yawn_start.is_none() {
                self.current_yawn_start = Some(now);
            }
        } else if let Some(start) = self.current_yawn_start {
            let yawn_dur = now.duration_since(start).as_secs_f32();
            if yawn_dur >= MIN_YAWN_DURATION_SECS {
                self.completed_yawns.push_back(now);
            }
            self.current_yawn_start = None;
        }

        // Ajout du nouvel échantillon
        if self.samples.len() >= SLIDING_WINDOW_CAPACITY {
            self.samples.pop_front();
        }
        self.samples.push_back(FrameSample {
            timestamp: now,
            ear: avg_ear,
            mar,
            is_closed,
            is_yawning: is_yawn_opening,
        });

        // Élagage des échantillons hors fenêtre de 60s
        let cutoff = now.checked_sub(self.window_duration).unwrap_or(now);
        while let Some(front) = self.samples.front() {
            if front.timestamp < cutoff {
                self.samples.pop_front();
            } else {
                break;
            }
        }

        // Élagage des bâillements plus anciens que 5 minutes
        let yawn_cutoff = now.checked_sub(Duration::from_secs(300)).unwrap_or(now);
        while let Some(front) = self.completed_yawns.front() {
            if *front < yawn_cutoff {
                self.completed_yawns.pop_front();
            } else {
                break;
            }
        }

        // Calcul du score PERCLOS
        let total_samples = self.samples.len();
        let closed_samples = self.samples.iter().filter(|s| s.is_closed).count();
        let perclos_score = if total_samples > 0 {
            (closed_samples as f32) / (total_samples as f32)
        } else {
            0.0
        };

        let is_drowsy = perclos_score >= PERCLOS_DROWSY_THRESHOLD;
        let is_fatigued = perclos_score >= PERCLOS_FATIGUE_THRESHOLD;

        let elapsed_secs =
            if let (Some(first), Some(last)) = (self.samples.front(), self.samples.back()) {
                last.timestamp
                    .duration_since(first.timestamp)
                    .as_secs_f32()
                    .max(1.0)
            } else {
                1.0
            };

        let blink_rate_per_min = (self.total_blinks as f32) / (elapsed_secs / 60.0);

        AttentionProfile {
            is_screen_focused,
            gaze_angles,
            eye_aspect_ratio: avg_ear,
            perclos_score,
            is_drowsy,
            is_fatigued,
            blink_rate_per_min,
            recent_yawn_count: self.completed_yawns.len(),
            confidence: 0.92,
        }
    }
}

impl Default for FatigueDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fatigue_nominal_state() {
        let mut detector = FatigueDetector::new();
        let start = Instant::now();

        // 100 trames avec yeux bien ouverts (EAR = 0.32)
        let mut profile = detector.update(start, 0.32, 0.32, 0.12, true, (0.0, 0.0));
        for i in 1..100 {
            let t = start + Duration::from_millis(i * 33);
            profile = detector.update(t, 0.32, 0.32, 0.12, true, (0.0, 0.0));
        }

        assert_eq!(profile.perclos_score, 0.0);
        assert!(!profile.is_drowsy);
        assert!(!profile.is_fatigued);
        assert!(profile.is_screen_focused);
    }

    #[test]
    fn test_drowsiness_perclos_trigger() {
        let mut detector = FatigueDetector::new();
        let start = Instant::now();

        // 80 trames yeux ouverts, puis 25 trames yeux fermés (EAR = 0.12)
        // Ratio = 25 / 105 ~ 23.8% > 15% (drowsy)
        for i in 0..80 {
            let t = start + Duration::from_millis(i * 33);
            detector.update(t, 0.30, 0.30, 0.12, true, (0.0, 0.0));
        }

        let mut final_profile = detector.update(
            start + Duration::from_millis(80 * 33),
            0.10,
            0.10,
            0.12,
            true,
            (0.0, 0.0),
        );
        for i in 81..105 {
            let t = start + Duration::from_millis(i * 33);
            final_profile = detector.update(t, 0.12, 0.12, 0.12, true, (0.0, 0.0));
        }

        assert!(
            final_profile.perclos_score >= 0.15,
            "PERCLOS score : {}",
            final_profile.perclos_score
        );
        assert!(final_profile.is_drowsy, "Alerte somnolence active");
        assert!(final_profile.is_fatigued, "Alerte fatigue active");
    }

    #[test]
    fn test_yawn_detection() {
        let mut detector = FatigueDetector::new();
        let start = Instant::now();

        // Bâillement durant 2.0 secondes (MAR = 0.55)
        for i in 0..60 {
            let t = start + Duration::from_millis(i * 33);
            detector.update(t, 0.30, 0.30, 0.55, true, (0.0, 0.0));
        }
        // Fermeture de la bouche
        let end_t = start + Duration::from_millis(61 * 33);
        let profile = detector.update(end_t, 0.30, 0.30, 0.10, true, (0.0, 0.0));

        assert_eq!(
            profile.recent_yawn_count, 1,
            "Un bâillement complet détecté"
        );
    }
}
