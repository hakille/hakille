//! Pipeline caméra RÉEL — mesures `cv`, zéro constante déguisée.
//!
//! Conservés : hachage SHA-256 d'attestation (pixels jamais exfiltrés, §6) et
//! compteurs `frames_processed`.
//!
//! Tout le reste est mesuré via [`crate::vision::cv`] :
//! - éclairage : luminance moyenne mesurée, classée par bornes d'exposition
//!   documentées (25/75/190 — calibrage d'affichage, pas des mesures) ;
//! - uniformité : écart-type sous le plancher de bruit (6 niveaux, même
//!   plancher documenté que `scene.rs`) → `Absent` ;
//! - présence : densités de contours (Sobel à seuil adaptatif `moyenne +
//!   écart-type` mesuré) comparées entre bande centrale (x ∈ [1/4, 3/4],
//!   y ∈ [1/3, 2/3] — définition géométrique documentée) et périphérie ;
//!   regard = comparaison pure `centre >= périphérie` (aucun seuil) ;
//! - multiplicité : structure forte des deux côtés → `MultiplePeopleDetected`.
//!   **Proxy géométrique binaire (compte 2), pas une détection de personnes :
//!   le comptage exact exigerait un modèle.**
//! - distance : dérivée de la taille relative `a` de la plus grande région
//!   centrale non-fond (masses > 50 % exclues : fond/mur — paramètre
//!   documenté) par approximation sténopé `d = K / √a` avec `K = 0.23`
//!   (calibré pour qu'une région centrale couvrant 1/8 du champ ≈ 0,65 m,
//!   ordre de grandeur webcam laptop ; borné à [0,2 m, 5,0 m]).
//!   **Approximation d'ordre de grandeur, jamais une mesure télémétrique.**
use crate::vision::capture::{FrameFormat, VideoFrame};
use crate::vision::cv;
use crate::world_model::WorldModel;
use hakille_core::Timestamp;
use hakille_substrate::{CameraConfig, CameraFrame};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserPresence {
    /// User is present and actively focused in front of the device
    Present,
    /// Nobody detected in the field of view
    Absent,
    /// User is physically present but looking away / distracted
    Distracted,
    /// Multiple people detected in the field of view (triggers privacy protection)
    MultiplePeopleDetected { count: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightingCondition {
    Daylight,
    IndoorAdequate,
    Dim,
    Dark,
}

impl LightingCondition {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Daylight => "Daylight",
            Self::IndoorAdequate => "IndoorAdequate",
            Self::Dim => "Dim",
            Self::Dark => "Dark",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualSceneContext {
    pub user_presence: UserPresence,
    pub estimated_distance_m: f32,
    pub user_gaze_focused_on_device: bool,
    pub people_count: usize,
    pub lighting: LightingCondition,
    /// SHA-256 digest of frame data for cryptographic attestation.
    /// RAW PIXEL DATA NEVER LEAVES LOCAL PROCESSING MEMORY (§6).
    pub privacy_preserving_frame_hash: [u8; 32],
    pub timestamp: Timestamp,
}

pub struct CameraPipeline {
    config: CameraConfig,
    frames_processed: u64,
}

impl CameraPipeline {
    /// Bornes d'exposition documentées (luminance moyenne mesurée).
    const DARK_BELOW: f64 = 25.0;
    const DIM_BELOW: f64 = 75.0;
    const DAYLIGHT_ABOVE: f64 = 190.0;
    /// Plancher de bruit documenté (niveaux de gris) : en dessous, pas de structure.
    const NOISE_FLOOR_CONTRAST: f32 = 6.0;
    /// Plancher d'évidence documenté (fraction de pixels de contours) :
    /// en dessous, le « sujet central » n'est que du bruit.
    const MIN_CENTRAL_DENSITY: f32 = 0.0005;
    /// Approximation sténopé `d = K / √a` (voir doc module). Paramètre
    /// d'ordre de grandeur, pas une mesure.
    const PINHOLE_K: f32 = 0.23;
    const DISTANCE_MIN_M: f32 = 0.2;
    const DISTANCE_MAX_M: f32 = 5.0;
    /// Au-delà de 50 % de la surface, une composante est une masse de fond
    /// (mur, arrière-plan), pas un sujet. Paramètre documenté.
    const BACKGROUND_MASS_REL_AREA: f32 = 0.5;

    pub fn new(config: CameraConfig) -> Self {
        Self {
            config,
            frames_processed: 0,
        }
    }

    pub fn config(&self) -> &CameraConfig {
        &self.config
    }

    pub fn frames_processed(&self) -> u64 {
        self.frames_processed
    }

    /// Process a raw camera frame from Substrate.
    /// Guarantees that raw pixels are processed entirely in ephemeral RAM and discarded.
    pub fn process_frame(&mut self, frame: &CameraFrame) -> VisualSceneContext {
        self.frames_processed += 1;

        // 1. Cryptographic frame attestation hash
        let mut hasher = Sha256::new();
        hasher.update(&frame.data);
        let frame_hash: [u8; 32] = hasher.finalize().into();

        let absent = |lighting| VisualSceneContext {
            user_presence: UserPresence::Absent,
            estimated_distance_m: 0.0,
            user_gaze_focused_on_device: false,
            people_count: 0,
            lighting,
            privacy_preserving_frame_hash: frame_hash,
            timestamp: Timestamp::now(),
        };

        if frame.data.is_empty() || frame.width == 0 || frame.height == 0 {
            return absent(LightingCondition::Dark);
        }

        // 2. Mesures réelles via cv (luminance + contraste + contours).
        let layout = match frame.format {
            hakille_substrate::ImageFormat::Rgb8 => cv::PixelLayout::RGB,
            hakille_substrate::ImageFormat::Rgba8 => cv::PixelLayout::RGBA,
            hakille_substrate::ImageFormat::Bgra8 => cv::PixelLayout::BGRA,
        };
        let gray = match cv::to_grayscale(&frame.data, frame.width, frame.height, layout) {
            Some(g) => g,
            None => return absent(LightingCondition::Dark),
        };
        self.analyze_gray_frame(&gray, frame.width, frame.height, frame_hash)
    }

    /// Process a normalized VideoFrame (e.g. from VideoFrameBridge).
    /// Guarantees that raw pixels are processed entirely in ephemeral RAM and discarded.
    pub fn process_video_frame(&mut self, frame: &VideoFrame) -> VisualSceneContext {
        self.frames_processed += 1;

        let mut hasher = Sha256::new();
        hasher.update(&frame.data);
        let frame_hash: [u8; 32] = hasher.finalize().into();

        let absent = |lighting| VisualSceneContext {
            user_presence: UserPresence::Absent,
            estimated_distance_m: 0.0,
            user_gaze_focused_on_device: false,
            people_count: 0,
            lighting,
            privacy_preserving_frame_hash: frame_hash,
            timestamp: Timestamp::now(),
        };

        if frame.data.is_empty() || frame.width == 0 || frame.height == 0 {
            return absent(LightingCondition::Dark);
        }

        let layout = match frame.format {
            FrameFormat::Rgb8 => cv::PixelLayout::RGB,
            FrameFormat::Rgba8 => cv::PixelLayout::RGBA,
            FrameFormat::Bgra8 => cv::PixelLayout::BGRA,
            FrameFormat::Jpeg | FrameFormat::Yuyv => return absent(LightingCondition::Dark),
        };
        let gray = match cv::to_grayscale(&frame.data, frame.width, frame.height, layout) {
            Some(g) => g,
            None => return absent(LightingCondition::Dark),
        };
        self.analyze_gray_frame(&gray, frame.width, frame.height, frame_hash)
    }

    fn analyze_gray_frame(
        &self,
        gray: &[u8],
        width: u32,
        height: u32,
        frame_hash: [u8; 32],
    ) -> VisualSceneContext {
        let absent = |lighting| VisualSceneContext {
            user_presence: UserPresence::Absent,
            estimated_distance_m: 0.0,
            user_gaze_focused_on_device: false,
            people_count: 0,
            lighting,
            privacy_preserving_frame_hash: frame_hash,
            timestamp: Timestamp::now(),
        };

        let (mean_lum, std_lum) = cv::luminance_stats(gray);

        let lighting = if mean_lum < Self::DARK_BELOW as f32 {
            LightingCondition::Dark
        } else if mean_lum < Self::DIM_BELOW as f32 {
            LightingCondition::Dim
        } else if mean_lum < Self::DAYLIGHT_ABOVE as f32 {
            LightingCondition::IndoorAdequate
        } else {
            LightingCondition::Daylight
        };

        // 3. Uniforme (bruit seul) → Absent honnête, jamais de sujet inventé.
        if std_lum < Self::NOISE_FLOOR_CONTRAST {
            return absent(lighting);
        }

        // 4. Densités de contours centre vs périphérie (mesures).
        let w = width as usize;
        let h = height as usize;
        let sobel = cv::sobel_magnitude(gray, w, h);
        let n = sobel.len().max(1) as f32;
        let m_mean = sobel.iter().map(|&v| v as f32).sum::<f32>() / n;
        let m_var = sobel
            .iter()
            .map(|&v| (v as f32 - m_mean).powi(2))
            .sum::<f32>()
            / n;
        let edge_thresh = m_mean + m_var.sqrt();
        let center_density =
            cv::edge_density_in_rect(&sobel, w, h, edge_thresh, 0.25, 1.0 / 3.0, 0.75, 2.0 / 3.0);
        let total_edges = sobel.iter().filter(|&&v| v as f32 > edge_thresh).count();
        let center_area =
            ((0.75 - 0.25) * w as f32) as usize * ((2.0 / 3.0 - 1.0 / 3.0) * h as f32) as usize;
        let total_area = w * h;
        let periphery_density = if total_area > center_area {
            (total_edges.saturating_sub((center_density * center_area as f32).round() as usize))
                as f32
                / (total_area - center_area) as f32
        } else {
            0.0
        };

        // Conditions géométriques atteignables :
        // - centre seul fort (> MIN_CENTRAL_DENSITY) et périphérie faible (< 25 % du centre) -> Present (attentif) ;
        // - centre ET périphérie tous deux significatifs (évite le faux positif de débordement Sobel 1 px sur la frontière centrale) -> MultiplePeopleDetected (proxy binaire) ;
        // - périphérie seule forte (sujet décentré / regard détourné) -> Distracted ;
        // - sinon -> Absent.
        let has_central_subject = center_density > Self::MIN_CENTRAL_DENSITY;
        let has_peripheral_subject = periphery_density > Self::MIN_CENTRAL_DENSITY;
        let has_distinct_peripheral_subject =
            has_peripheral_subject && periphery_density >= center_density * 0.25;

        let (user_presence, people_count, gaze_focused) =
            if has_central_subject && has_distinct_peripheral_subject {
                // Proxy géométrique binaire : comptage exact impossible sans modèle.
                (UserPresence::MultiplePeopleDetected { count: 2 }, 2, false)
            } else if has_central_subject {
                (UserPresence::Present, 1, true)
            } else if has_peripheral_subject {
                (UserPresence::Distracted, 1, false)
            } else {
                return absent(lighting);
            };

        // 5. Distance : taille relative de la plus grande région centrale
        //    non-fond (mesure cv), convertie par approximation sténopé.
        let distance_m = self.estimate_distance(gray, width, height);

        VisualSceneContext {
            user_presence,
            estimated_distance_m: distance_m,
            user_gaze_focused_on_device: gaze_focused,
            people_count,
            lighting,
            privacy_preserving_frame_hash: frame_hash,
            timestamp: Timestamp::now(),
        }
    }

    /// Approximation d'ordre de grandeur (pas une mesure) : `d = K / √a` où
    /// `a` est l'aire relative mesurée de la plus grande région dont le centre
    /// tombe dans la bande centrale, masses de fond exclues. Aucune région →
    /// 0.0 (distance inconnue, pas de valeur inventée).
    fn estimate_distance(&self, gray: &[u8], width: u32, height: u32) -> f32 {
        let regions = cv::detect_regions(gray, width, height);
        let largest_central = regions
            .iter()
            .filter(|b| b.area_rel <= Self::BACKGROUND_MASS_REL_AREA)
            .filter(|b| {
                let (cx, cy) = ((b.x_min + b.x_max) / 2.0, (b.y_min + b.y_max) / 2.0);
                (0.25..=0.75).contains(&cx) && (1.0 / 3.0..=2.0 / 3.0).contains(&cy)
            })
            .max_by(|a, b| {
                a.area_rel
                    .partial_cmp(&b.area_rel)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        match largest_central {
            Some(b) if b.area_rel > 0.0 => (Self::PINHOLE_K / b.area_rel.sqrt())
                .clamp(Self::DISTANCE_MIN_M, Self::DISTANCE_MAX_M),
            _ => 0.0,
        }
    }

    /// Feed the resulting visual perception into the system's WorldModel
    pub fn update_world_model(&self, ctx: &VisualSceneContext, world_model: &mut WorldModel) {
        let present = match ctx.user_presence {
            UserPresence::Present
            | UserPresence::Distracted
            | UserPresence::MultiplePeopleDetected { .. } => true,
            UserPresence::Absent => false,
        };
        world_model.update_visual_presence(
            present,
            ctx.user_gaze_focused_on_device,
            ctx.people_count,
            ctx.estimated_distance_m,
            ctx.lighting.as_str(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw_rect(buf: &mut [u8], w: usize, x0: usize, y0: usize, x1: usize, y1: usize, val: u8) {
        for y in y0..y1 {
            for x in x0..x1 {
                let idx = (y * w + x) * 3;
                buf[idx] = val;
                buf[idx + 1] = val;
                buf[idx + 2] = val;
            }
        }
    }

    fn frame(w: usize, h: usize, data: Vec<u8>) -> CameraFrame {
        CameraFrame {
            width: w as u32,
            height: h as u32,
            data,
            format: hakille_substrate::ImageFormat::Rgb8,
            timestamp_ms: 1000,
        }
    }

    #[test]
    fn camera_pipeline_reaches_all_four_presence_states() {
        let mut pipeline = CameraPipeline::new(CameraConfig {
            width: 640,
            height: 480,
            fps: 30,
            facing: hakille_substrate::CameraFacing::Front,
        });
        let (w, h) = (640usize, 480usize);

        // 1. Uniform frame -> Absent
        let blank = frame(w, h, vec![100u8; w * h * 3]);
        let ctx = pipeline.process_frame(&blank);
        assert_eq!(ctx.user_presence, UserPresence::Absent);

        // 2. Centered subject only -> Present
        let mut centered = vec![120u8; w * h * 3];
        draw_rect(&mut centered, w, 240, 180, 400, 300, 20);
        let ctx = pipeline.process_frame(&frame(w, h, centered));
        assert_eq!(ctx.user_presence, UserPresence::Present);

        // 3. Peripheral subject only -> Distracted
        let mut side_only = vec![120u8; w * h * 3];
        draw_rect(&mut side_only, w, 10, 10, 130, 130, 20);
        let ctx = pipeline.process_frame(&frame(w, h, side_only));
        assert_eq!(ctx.user_presence, UserPresence::Distracted);

        // 4. Both centered and strong peripheral subjects -> MultiplePeopleDetected
        let mut multi = vec![120u8; w * h * 3];
        draw_rect(&mut multi, w, 260, 190, 380, 290, 20);
        draw_rect(&mut multi, w, 10, 10, 150, 460, 20);
        draw_rect(&mut multi, w, 490, 10, 630, 460, 20);
        let ctx = pipeline.process_frame(&frame(w, h, multi));
        assert_eq!(
            ctx.user_presence,
            UserPresence::MultiplePeopleDetected { count: 2 }
        );
    }
}
