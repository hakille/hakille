//! Grounding GUI RÉEL — vision classique mesurée, zéro coordonnée en dur.
//!
//! Vs l'ancien stub : plus de boîtes fixes (`Submit`/`SearchInput`) ni de
//! confiance 0.95/0.92. Chaque `GroundedElement` provient de
//! [`crate::vision::cv::detect_regions`] appliqué aux pixels de la frame :
//! binarisation d'Otsu + composantes connexes, scores géométriques mesurés
//! (contraste local × taille × centralité).
//!
//! Limites honnêtes :
//! - les `label` sont ordinaux (`region-0`, …), PAS sémantiques : sans modèle
//!   vision-langage on ne peut nommer un bouton ;
//! - `locate_element(frame, query)` ne peut pas vérifier visuellement une
//!   requête textuelle sans modèle OCR/VLM : il retourne le meilleur candidat
//!   géométrique avec un label préfixé `ungrounded-candidate:` (requête non
//!   vérifiée) et une confiance mesurée. Pour une localisation exacte, fournir
//!   un gabarit pixels via [`UiTarsGroundingEngine::locate_template`] (vrai NCC
//!   multi-échelle) ;
//! - frame vide, dimensions nulles ou `Jpeg` (indécodable en `std` pur) → `[]`
//!   / `None`. Aucun élément n'est jamais inventé.
use crate::vision::capture::{FrameFormat, VideoFrame};
use crate::vision::cv;
use annora_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x_min: f32, // 0.0 to 1.0
    pub y_min: f32,
    pub x_max: f32,
    pub y_max: f32,
}

impl BoundingBox {
    pub fn center(&self) -> (f32, f32) {
        (
            (self.x_min + self.x_max) / 2.0,
            (self.y_min + self.y_max) / 2.0,
        )
    }

    pub fn to_pixel_coordinates(&self, screen_width: u32, screen_height: u32) -> (i32, i32) {
        let (cx, cy) = self.center();
        (
            (cx * screen_width as f32) as i32,
            (cy * screen_height as f32) as i32,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundedElement {
    pub label: String,
    /// Type d'élément. Sans modèle de classification, vaut `candidate`
    /// (région mesurée) ou `template-match` (NCC) — jamais `button`/`input`
    /// affirmé sans preuve visuelle.
    pub element_type: String, // "candidate", "template-match", etc.
    pub bbox: BoundingBox,
    /// Score géométrique mesuré dans [0,1] (cf. `cv::detect_regions`),
    /// ou score NCC pour un template-match. Jamais une constante.
    pub confidence: f32,
}

#[async_trait]
pub trait GuiGroundingEngine: Send + Sync {
    async fn ground_elements(&self, frame: &VideoFrame) -> Result<Vec<GroundedElement>>;
    async fn locate_element(
        &self,
        frame: &VideoFrame,
        query: &str,
    ) -> Result<Option<GroundedElement>>;
}

pub struct UiTarsGroundingEngine;

impl UiTarsGroundingEngine {
    /// Limite de travail documentée : au-delà de 24 régions (triées par aire),
    /// on tronque pour borner le coût aval. Paramètre, pas une mesure.
    const MAX_ELEMENTS: usize = 24;

    fn frame_layout(format: &FrameFormat) -> Option<cv::PixelLayout> {
        match format {
            FrameFormat::Rgb8 => Some(cv::PixelLayout::RGB),
            FrameFormat::Rgba8 => Some(cv::PixelLayout::RGBA),
            FrameFormat::Bgra8 => Some(cv::PixelLayout::BGRA),
            // Jpeg indécodable en std pur (aucun décodeur) → honnêtement ignoré.
            FrameFormat::Jpeg => None,
        }
    }

    fn measured_boxes(frame: &VideoFrame) -> Vec<cv::DetectedBox> {
        let layout = match Self::frame_layout(&frame.format) {
            Some(l) => l,
            None => return Vec::new(),
        };
        let gray = match cv::to_grayscale(&frame.data, frame.width, frame.height, layout) {
            Some(g) => g,
            None => return Vec::new(),
        };
        cv::detect_regions(&gray, frame.width, frame.height)
    }

    fn to_element(box_: &cv::DetectedBox, label: String, kind: &str) -> GroundedElement {
        GroundedElement {
            label,
            element_type: kind.to_string(),
            bbox: BoundingBox {
                x_min: box_.x_min,
                y_min: box_.y_min,
                x_max: box_.x_max,
                y_max: box_.y_max,
            },
            confidence: box_.score,
        }
    }

    /// Localisation RÉELLE par gabarit : NCC normalisé multi-échelle
    /// ([`cv::locate_template`]) entre la frame et un gabarit en niveaux de
    /// gris fourni. Score = corrélation mesurée dans [-1, 1].
    ///
    /// Retourne `None` si la frame est vide/indécodable, si le gabarit est
    /// vide ou plus grand que l'image.
    pub fn locate_template(
        &self,
        frame: &VideoFrame,
        template_gray: &[u8],
        template_w: u32,
        template_h: u32,
    ) -> Option<GroundedElement> {
        let layout = Self::frame_layout(&frame.format)?;
        let gray = cv::to_grayscale(&frame.data, frame.width, frame.height, layout)?;
        let hit = cv::locate_template(
            &gray,
            frame.width as usize,
            frame.height as usize,
            template_gray,
            template_w as usize,
            template_h as usize,
        )?;
        let (w, h) = (frame.width as f32, frame.height as f32);
        Some(GroundedElement {
            label: "template-match".to_string(),
            element_type: "template-match".to_string(),
            bbox: BoundingBox {
                x_min: hit.x as f32 / w,
                y_min: hit.y as f32 / h,
                x_max: (hit.x + hit.w) as f32 / w,
                y_max: (hit.y + hit.h) as f32 / h,
            },
            confidence: hit.score.clamp(0.0, 1.0),
        })
    }
}

#[async_trait]
impl GuiGroundingEngine for UiTarsGroundingEngine {
    async fn ground_elements(&self, frame: &VideoFrame) -> Result<Vec<GroundedElement>> {
        let boxes = Self::measured_boxes(frame);
        Ok(boxes
            .into_iter()
            .take(Self::MAX_ELEMENTS)
            .enumerate()
            .map(|(i, b)| Self::to_element(&b, format!("region-{i}"), "candidate"))
            .collect())
    }

    async fn locate_element(
        &self,
        frame: &VideoFrame,
        query: &str,
    ) -> Result<Option<GroundedElement>> {
        let boxes = Self::measured_boxes(frame);
        if boxes.is_empty() {
            return Ok(None);
        }
        // Sans modèle OCR/VLM, une requête textuelle ne peut être vérifiée
        // visuellement : on retourne le meilleur candidat géométrique
        // (score maximal mesuré) en marquant la requête comme NON vérifiée.
        let best = boxes
            .into_iter()
            .max_by(|a, b| {
                a.score
                    .partial_cmp(&b.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("non vide (vérifié ci-dessus)");
        let label = if query.trim().is_empty() {
            "top-candidate".to_string()
        } else {
            format!("ungrounded-candidate(query=\"{}\")", query.trim())
        };
        Ok(Some(Self::to_element(&best, label, "candidate")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vision::capture::FrameFormat;
    use annora_core::Timestamp;

    fn rgba_frame(w: u32, h: u32, rect: Option<(u32, u32, u32, u32)>) -> VideoFrame {
        let mut data = vec![0u8; (w * h) as usize * 4];
        if let Some((x0, y0, x1, y1)) = rect {
            for y in y0..y1.min(h) {
                for x in x0..x1.min(w) {
                    let i = ((y * w + x) * 4) as usize;
                    data[i] = 255;
                    data[i + 1] = 255;
                    data[i + 2] = 255;
                    data[i + 3] = 255;
                }
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
    async fn test_grounding_finds_synthetic_rectangle() {
        let engine = UiTarsGroundingEngine;
        // Rectangle blanc centré 80x60 sur 160x120.
        let frame = rgba_frame(160, 120, Some((40, 30, 120, 90)));
        let t0 = std::time::Instant::now();
        let elements = engine.ground_elements(&frame).await.unwrap();
        eprintln!(
            "ground_elements 160x120: {:?}, {} éléments",
            t0.elapsed(),
            elements.len()
        );
        assert!(!elements.is_empty(), "le rectangle doit être ancré");
        let (ex0, ey0, ex1, ey1) = (0.25, 0.25, 0.75, 0.75);
        let eps = 0.03;
        let found = elements.iter().any(|e| {
            (e.bbox.x_min - ex0).abs() <= eps
                && (e.bbox.y_min - ey0).abs() <= eps
                && (e.bbox.x_max - ex1).abs() <= eps
                && (e.bbox.y_max - ey1).abs() <= eps
        });
        assert!(
            found,
            "bbox mesurées: {:?}",
            elements.iter().map(|e| &e.bbox).collect::<Vec<_>>()
        );
        // Scores mesurés : jamais de constante — ils doivent varier et être dans [0,1].
        for e in &elements {
            assert!(
                (0.0..=1.0).contains(&e.confidence),
                "confiance={}",
                e.confidence
            );
        }
        // Labels ordinaux honnêtes, pas de "Submit"/"SearchInput".
        assert!(elements.iter().all(|e| e.label.starts_with("region-")));
        assert!(elements.iter().all(|e| e.element_type == "candidate"));
    }

    #[tokio::test]
    async fn test_grounding_empty_frame_yields_nothing() {
        let engine = UiTarsGroundingEngine;
        let empty = rgba_frame(160, 120, None);
        assert!(engine.ground_elements(&empty).await.unwrap().is_empty());
        assert!(
            engine
                .locate_element(&empty, "Save")
                .await
                .unwrap()
                .is_none()
        );
        let zero = VideoFrame {
            width: 0,
            height: 0,
            format: FrameFormat::Rgba8,
            data: vec![],
            timestamp: Timestamp::now(),
        };
        assert!(engine.ground_elements(&zero).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_locate_returns_measured_top_candidate() {
        let engine = UiTarsGroundingEngine;
        let frame = rgba_frame(160, 120, Some((40, 30, 120, 90)));
        let found = engine
            .locate_element(&frame, "Save")
            .await
            .unwrap()
            .expect("candidat attendu");
        eprintln!(
            "locate_element: label={} conf={:.3} bbox={:?}",
            found.label, found.confidence, found.bbox
        );
        // Requête marquée non vérifiée (honnêteté), bbox = région mesurée.
        assert!(found.label.contains("ungrounded-candidate"));
        assert!((found.bbox.x_min - 0.25).abs() <= 0.05);
    }

    #[test]
    fn test_locate_template_ncc_real() {
        let engine = UiTarsGroundingEngine;
        let (w, h) = (64u32, 48u32);
        let mut data = vec![20u8; (w * h) as usize * 4];
        for y in 10..30 {
            for x in 8..40 {
                let i = ((y * w + x) * 4) as usize;
                data[i] = 200;
                data[i + 1] = 200;
                data[i + 2] = 200;
            }
        }
        for y in 14..26 {
            for x in 20..28 {
                let i = ((y * w + x) * 4) as usize;
                data[i] = 40;
                data[i + 1] = 40;
                data[i + 2] = 40;
            }
        }
        let frame = VideoFrame {
            width: w,
            height: h,
            format: FrameFormat::Rgba8,
            data,
            timestamp: Timestamp::now(),
        };
        // Gabarit = crop exact (variance non nulle) en niveaux de gris.
        let gray = cv::to_grayscale(&frame.data, w, h, cv::PixelLayout::RGBA).unwrap();
        let (tx, ty, tw, th) = (4usize, 6usize, 40usize, 28usize);
        let mut tmpl = vec![0u8; tw * th];
        for dy in 0..th {
            for dx in 0..tw {
                tmpl[dy * tw + dx] = gray[(ty + dy) * w as usize + (tx + dx)];
            }
        }
        let hit = engine
            .locate_template(&frame, &tmpl, tw as u32, th as u32)
            .expect("NCC doit localiser");
        eprintln!(
            "template hit: conf={:.5} bbox={:?}",
            hit.confidence, hit.bbox
        );
        assert!(
            (hit.confidence - 1.0).abs() < 1e-4,
            "NCC identique → ~1.0, obtenu {}",
            hit.confidence
        );
        assert!((hit.bbox.x_min - 4.0 / 64.0).abs() < 1e-6);
        assert!((hit.bbox.y_min - 6.0 / 48.0).abs() < 1e-6);
    }
}
