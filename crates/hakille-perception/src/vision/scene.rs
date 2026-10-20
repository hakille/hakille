//! Compréhension de scène RÉELLE — mesures pixels, zéro gabarit.
//!
//! Vs l'ancien stub : plus de `DesktopGUI`/`12` éléments/`File-Edit-View` ni de
//! confiance 0.90. Tout est mesuré via [`crate::vision::cv`] :
//! luminance moyenne, contraste (écart-type), densité de contours (Sobel à
//! seuil adaptatif), nombre réel de régions, dominante couleur.
//!
//! Classification (`scene_type`) par seuils de décision appliqués à ces
//! mesures — calibrage documenté, pas des perceptions déguisées :
//! - `EmptyFrame` : données vides ou dimensions nulles (compte 0, OCR vide,
//!   confiance 0.0) ;
//! - `IndecodableFormat` : `Jpeg`, indécodable en `std` pur (honnête, pas de
//!   devinette) ;
//! - `Uniforme` : contraste sous le plancher de bruit (6 niveaux de gris,
//!   ordre de grandeur du bruit capteur/quantification) ;
//! - `Sombre` / `Clair` : luminance moyenne < 45 / > 205 (bornes d'exposition
//!   documentées) ;
//! - `InterfaceDense` : ≥ 6 régions ET densité de contours ≥ 0.015
//!   (structure fragmentée typique d'une UI) ;
//! - `Riche` : contenu structuré restant (photo, dégradé, fond complexe).
//!
//! `active_window_title` vaut `None` : le titre de fenêtre est inconnaissable
//! sans API OS ni modèle (l'ancien `Some("ActiveApplication")` mentait).
//! `ocr_text_sample` est vide : **aucun caractère n'est reconnu sans modèle
//! OCR** (le nombre de *candidats* géométriques est exposé dans
//! `text_candidate_regions`, cf. `cv::detect_text_regions`).
//! `confidence` = contraste normalisé (`std/128`, borné [0.05, 0.95]) :
//! une image uniforme donne une confiance faible — c'est une mesure,
//! pas une constante.
use crate::vision::capture::{FrameFormat, VideoFrame};
use crate::vision::cv;
use async_trait::async_trait;
use hakille_core::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneUnderstanding {
    pub scene_type: String,
    pub active_window_title: Option<String>,
    pub detected_elements_count: usize,
    pub ocr_text_sample: Vec<String>,
    /// Nombre de régions candidates « probablement textuelles » (géométrie,
    /// pas de lecture — voir `cv::detect_text_regions`).
    pub text_candidate_regions: usize,
    pub confidence: f32,
}

#[async_trait]
pub trait SceneAnalyzer: Send + Sync {
    async fn analyze(&self, frame: &VideoFrame) -> Result<SceneUnderstanding>;
}

pub struct DefaultSceneAnalyzer;

impl DefaultSceneAnalyzer {
    /// Plancher de bruit documenté (niveaux de gris) : en dessous, les
    /// variations sont du bruit, pas de la structure.
    const NOISE_FLOOR_CONTRAST: f32 = 6.0;
    /// Bornes d'exposition documentées (luminance moyenne mesurée).
    const DARK_BELOW: f32 = 45.0;
    const BRIGHT_ABOVE: f32 = 205.0;
    /// Seuils de densité UI documentés (fraction de pixels de contours).
    const DENSE_EDGE_DENSITY: f32 = 0.015;
    const DENSE_REGION_COUNT: usize = 6;

    fn empty(confidence: f32, scene_type: &str) -> SceneUnderstanding {
        SceneUnderstanding {
            scene_type: scene_type.to_string(),
            active_window_title: None,
            detected_elements_count: 0,
            ocr_text_sample: Vec::new(),
            text_candidate_regions: 0,
            confidence,
        }
    }
}

#[async_trait]
impl SceneAnalyzer for DefaultSceneAnalyzer {
    async fn analyze(&self, frame: &VideoFrame) -> Result<SceneUnderstanding> {
        if frame.data.is_empty() || frame.width == 0 || frame.height == 0 {
            return Ok(Self::empty(0.0, "EmptyFrame"));
        }
        let layout = match frame.format {
            FrameFormat::Rgb8 => cv::PixelLayout::RGB,
            FrameFormat::Rgba8 => cv::PixelLayout::RGBA,
            FrameFormat::Bgra8 => cv::PixelLayout::BGRA,
            FrameFormat::Jpeg => return Ok(Self::empty(0.0, "IndecodableFormat")),
        };
        let gray = match cv::to_grayscale(&frame.data, frame.width, frame.height, layout) {
            Some(g) => g,
            None => return Ok(Self::empty(0.0, "EmptyFrame")),
        };
        // Mesures réelles.
        let (mean_lum, std_lum) = cv::luminance_stats(&gray);
        let edges = cv::edge_stats(&gray, frame.width, frame.height);
        let regions = cv::detect_regions(&gray, frame.width, frame.height);
        let text_candidates = cv::detect_text_regions(&gray, frame.width, frame.height);
        let (r_mean, g_mean, b_mean) =
            cv::channel_means(&frame.data, frame.width, frame.height, layout)
                .unwrap_or((mean_lum, mean_lum, mean_lum));
        let _dominant = cv::dominant_color_name(r_mean, g_mean, b_mean);

        let scene_type = if std_lum < Self::NOISE_FLOOR_CONTRAST {
            "Uniforme"
        } else if mean_lum < Self::DARK_BELOW {
            "Sombre"
        } else if mean_lum > Self::BRIGHT_ABOVE {
            "Clair"
        } else if regions.len() >= Self::DENSE_REGION_COUNT
            && edges.density >= Self::DENSE_EDGE_DENSITY
        {
            "InterfaceDense"
        } else {
            "Riche"
        };

        // Confiance géométrique mesurée : contraste normalisé (conversion
        // d'unité /128, pas un seuil), bornée pour rester une probabilité.
        let confidence = (std_lum / 128.0).clamp(0.05, 0.95);

        Ok(SceneUnderstanding {
            scene_type: scene_type.to_string(),
            active_window_title: None,
            detected_elements_count: regions.len(),
            ocr_text_sample: Vec::new(),
            text_candidate_regions: text_candidates.len(),
            confidence,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_core::Timestamp;

    fn rgba_frame(w: u32, h: u32, fill: impl Fn(u32, u32) -> (u8, u8, u8)) -> VideoFrame {
        let mut data = Vec::with_capacity((w * h) as usize * 4);
        for y in 0..h {
            for x in 0..w {
                let (r, g, b) = fill(x, y);
                data.extend_from_slice(&[r, g, b, 255]);
            }
        }
        VideoFrame {
            width: w,
            height: h,
            format: FrameFormat::Rgba8,
            data,
            timestamp: Timestamp::now(),
        }
    }

    #[tokio::test]
    async fn test_scene_rectangle_is_measured() {
        let analyzer = DefaultSceneAnalyzer;
        let frame = rgba_frame(160, 120, |x, y| {
            if (40..120).contains(&x) && (30..90).contains(&y) {
                (255, 255, 255)
            } else {
                (0, 0, 0)
            }
        });
        let t0 = std::time::Instant::now();
        let scene = analyzer.analyze(&frame).await.unwrap();
        eprintln!("analyze 160x120: {:?} -> {:?}", t0.elapsed(), scene);
        assert!(
            scene.detected_elements_count >= 1,
            "régions réelles comptées"
        );
        assert_ne!(scene.scene_type, "DesktopGUI");
        assert!(scene.active_window_title.is_none());
        assert!(scene.ocr_text_sample.is_empty(), "pas d'OCR sans modèle");
        assert!((0.05..=0.95).contains(&scene.confidence));
    }

    #[tokio::test]
    async fn test_scene_empty_and_uniform_honest() {
        let analyzer = DefaultSceneAnalyzer;
        let empty = VideoFrame {
            width: 0,
            height: 0,
            format: FrameFormat::Rgba8,
            data: vec![],
            timestamp: Timestamp::now(),
        };
        let s = analyzer.analyze(&empty).await.unwrap();
        assert_eq!(s.scene_type, "EmptyFrame");
        assert_eq!(s.detected_elements_count, 0);
        assert_eq!(s.confidence, 0.0);

        let uniform = rgba_frame(64, 64, |_, _| (140, 140, 140));
        let s = analyzer.analyze(&uniform).await.unwrap();
        assert_eq!(s.scene_type, "Uniforme");
        assert_eq!(s.detected_elements_count, 0, "aucune région inventée");
    }

    #[tokio::test]
    async fn test_scene_dense_ui_classification() {
        let analyzer = DefaultSceneAnalyzer;
        // Damier fin = structure fragmentée à haute densité de contours.
        let frame = rgba_frame(160, 120, |x, y| {
            if ((x / 8) + (y / 8)) % 2 == 0 {
                (30, 30, 30)
            } else {
                (220, 220, 220)
            }
        });
        let s = analyzer.analyze(&frame).await.unwrap();
        eprintln!("dense: {:?}", s);
        assert_eq!(s.scene_type, "InterfaceDense");
        assert!(s.detected_elements_count >= 6);
    }
}
