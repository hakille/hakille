//! Prétraitement d'images et tenseurs de vision — Pur Rust sans dépendance Python ni OpenCV.
//!
//! Ce module implémente :
//! - Normalisation normalisée (ImageNet, Zero-One, Centered [-1, 1]).
//! - Redimensionnement bilinéaire sub-pixel pur Rust zéro allocation tas sur le chemin chaud.
//! - Letterboxing avec conservation du rapport d'aspect et inversion de repère géométrique.

use serde::{Deserialize, Serialize};

/// Mode de normalisation des valeurs de pixels RGB.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum NormalizationMode {
    /// Normalisation standard ImageNet : `(x / 255 - mean) / std`.
    /// `mean = [0.485, 0.456, 0.406]`, `std = [0.229, 0.224, 0.225]`.
    ImageNet,
    /// Échelle normalisée `[0.0, 1.0]` : `x / 255.0`.
    ZeroOne,
    /// Échelle centrée `[-1.0, 1.0]` : `(x / 127.5) - 1.0`.
    Centered,
    /// Valeurs brutes sans normalisation `[0.0, 255.0]`.
    Raw,
}

impl NormalizationMode {
    #[inline(always)]
    pub fn normalize(&self, r: u8, g: u8, b: u8) -> (f32, f32, f32) {
        match self {
            Self::ImageNet => {
                let rf = (r as f32 / 255.0 - 0.485) / 0.229;
                let gf = (g as f32 / 255.0 - 0.456) / 0.224;
                let bf = (b as f32 / 255.0 - 0.406) / 0.225;
                (rf, gf, bf)
            }
            Self::ZeroOne => (
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
            ),
            Self::Centered => (
                (r as f32 / 127.5) - 1.0,
                (g as f32 / 127.5) - 1.0,
                (b as f32 / 127.5) - 1.0,
            ),
            Self::Raw => (r as f32, g as f32, b as f32),
        }
    }
}

/// Métadonnées de transformation de letterbox pour restituer les coordonnées originales.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LetterboxTransform {
    pub scale: f32,
    pub pad_x: f32,
    pub pad_y: f32,
    pub original_w: usize,
    pub original_h: usize,
    pub target_w: usize,
    pub target_h: usize,
}

impl LetterboxTransform {
    /// Projette un point du repère normalisé du modèle `[0.0, 1.0]` vers l'image originale `[0, original_w]`.
    pub fn model_to_original(&self, x_norm: f32, y_norm: f32) -> (f32, f32) {
        let x_target = x_norm * self.target_w as f32;
        let y_target = y_norm * self.target_h as f32;
        let orig_x = (x_target - self.pad_x) / self.scale;
        let orig_y = (y_target - self.pad_y) / self.scale;
        (
            orig_x.clamp(0.0, self.original_w as f32),
            orig_y.clamp(0.0, self.original_h as f32),
        )
    }

    /// Projette un point de l'image originale vers le repère normalisé `[0.0, 1.0]` du modèle.
    pub fn original_to_model(&self, orig_x: f32, orig_y: f32) -> (f32, f32) {
        let x_target = orig_x * self.scale + self.pad_x;
        let y_target = orig_y * self.scale + self.pad_y;
        (
            (x_target / self.target_w as f32).clamp(0.0, 1.0),
            (y_target / self.target_h as f32).clamp(0.0, 1.0),
        )
    }
}

/// Redimensionne une image RGB brute (HWC interleaved) vers un tampon tenseur planar NCHW `[3, dst_h, dst_w]`.
/// Zéro allocation tas : le tampon de sortie `dst` doit faire `3 * dst_w * dst_h` floats.
pub fn resize_bilinear_rgb_nchw(
    src: &[u8],
    src_w: usize,
    src_h: usize,
    dst: &mut [f32],
    dst_w: usize,
    dst_h: usize,
    norm: NormalizationMode,
) {
    assert_eq!(src.len(), src_w * src_h * 3, "src buffer size mismatch");
    assert_eq!(dst.len(), 3 * dst_w * dst_h, "dst buffer size mismatch");

    let x_ratio = if dst_w > 1 {
        (src_w - 1) as f32 / (dst_w - 1) as f32
    } else {
        0.0
    };
    let y_ratio = if dst_h > 1 {
        (src_h - 1) as f32 / (dst_h - 1) as f32
    } else {
        0.0
    };

    let plane_size = dst_w * dst_h;
    let (dst_r, rest) = dst.split_at_mut(plane_size);
    let (dst_g, dst_b) = rest.split_at_mut(plane_size);

    for y in 0..dst_h {
        let src_y = y as f32 * y_ratio;
        let y0 = src_y.floor() as usize;
        let y1 = (y0 + 1).min(src_h - 1);
        let y_diff = src_y - y0 as f32;
        let inv_y_diff = 1.0 - y_diff;

        let row0_offset = y0 * src_w * 3;
        let row1_offset = y1 * src_w * 3;
        let dst_row_offset = y * dst_w;

        for x in 0..dst_w {
            let src_x = x as f32 * x_ratio;
            let x0 = src_x.floor() as usize;
            let x1 = (x0 + 1).min(src_w - 1);
            let x_diff = src_x - x0 as f32;
            let inv_x_diff = 1.0 - x_diff;

            let w00 = inv_x_diff * inv_y_diff;
            let w01 = x_diff * inv_y_diff;
            let w10 = inv_x_diff * y_diff;
            let w11 = x_diff * y_diff;

            let idx00 = row0_offset + x0 * 3;
            let idx01 = row0_offset + x1 * 3;
            let idx10 = row1_offset + x0 * 3;
            let idx11 = row1_offset + x1 * 3;

            let r = src[idx00] as f32 * w00
                + src[idx01] as f32 * w01
                + src[idx10] as f32 * w10
                + src[idx11] as f32 * w11;

            let g = src[idx00 + 1] as f32 * w00
                + src[idx01 + 1] as f32 * w01
                + src[idx10 + 1] as f32 * w10
                + src[idx11 + 1] as f32 * w11;

            let b = src[idx00 + 2] as f32 * w00
                + src[idx01 + 2] as f32 * w01
                + src[idx10 + 2] as f32 * w10
                + src[idx11 + 2] as f32 * w11;

            let ru = r.round().clamp(0.0, 255.0) as u8;
            let gu = g.round().clamp(0.0, 255.0) as u8;
            let bu = b.round().clamp(0.0, 255.0) as u8;

            let (rf, gf, bf) = norm.normalize(ru, gu, bu);
            let dst_idx = dst_row_offset + x;
            dst_r[dst_idx] = rf;
            dst_g[dst_idx] = gf;
            dst_b[dst_idx] = bf;
        }
    }
}

/// Redimensionne avec letterboxing (remplissage neutre) pour préserver le ratio d'aspect.
/// `dst` doit être de taille `3 * dst_w * dst_h`.
pub fn letterbox_rgb_nchw(
    src: &[u8],
    src_w: usize,
    src_h: usize,
    dst: &mut [f32],
    dst_w: usize,
    dst_h: usize,
    norm: NormalizationMode,
) -> LetterboxTransform {
    let scale_w = dst_w as f32 / src_w as f32;
    let scale_h = dst_h as f32 / src_h as f32;
    let scale = scale_w.min(scale_h);

    let new_w = (src_w as f32 * scale).round() as usize;
    let new_h = (src_h as f32 * scale).round() as usize;

    let pad_x = ((dst_w - new_w) as f32 * 0.5).floor();
    let pad_y = ((dst_h - new_h) as f32 * 0.5).floor();

    // Remplissage neutre par défaut (114 / 255 pour standard YOLO / ImageNet)
    let (pad_r, pad_g, pad_b) = norm.normalize(114, 114, 114);
    let plane_size = dst_w * dst_h;
    let (dst_r, rest) = dst.split_at_mut(plane_size);
    let (dst_g, dst_b) = rest.split_at_mut(plane_size);
    dst_r.fill(pad_r);
    dst_g.fill(pad_g);
    dst_b.fill(pad_b);

    // Redimensionnement vers la zone centrale
    let start_x = pad_x as usize;
    let start_y = pad_y as usize;

    let x_ratio = if new_w > 1 {
        (src_w - 1) as f32 / (new_w - 1) as f32
    } else {
        0.0
    };
    let y_ratio = if new_h > 1 {
        (src_h - 1) as f32 / (new_h - 1) as f32
    } else {
        0.0
    };

    for y in 0..new_h {
        let src_y = y as f32 * y_ratio;
        let y0 = src_y.floor() as usize;
        let y1 = (y0 + 1).min(src_h - 1);
        let y_diff = src_y - y0 as f32;
        let inv_y_diff = 1.0 - y_diff;

        let row0_offset = y0 * src_w * 3;
        let row1_offset = y1 * src_w * 3;
        let dst_y = start_y + y;
        if dst_y >= dst_h {
            break;
        }
        let dst_row_offset = dst_y * dst_w;

        for x in 0..new_w {
            let dst_x = start_x + x;
            if dst_x >= dst_w {
                break;
            }

            let src_x = x as f32 * x_ratio;
            let x0 = src_x.floor() as usize;
            let x1 = (x0 + 1).min(src_w - 1);
            let x_diff = src_x - x0 as f32;
            let inv_x_diff = 1.0 - x_diff;

            let w00 = inv_x_diff * inv_y_diff;
            let w01 = x_diff * inv_y_diff;
            let w10 = inv_x_diff * y_diff;
            let w11 = x_diff * y_diff;

            let idx00 = row0_offset + x0 * 3;
            let idx01 = row0_offset + x1 * 3;
            let idx10 = row1_offset + x0 * 3;
            let idx11 = row1_offset + x1 * 3;

            let r = src[idx00] as f32 * w00
                + src[idx01] as f32 * w01
                + src[idx10] as f32 * w10
                + src[idx11] as f32 * w11;

            let g = src[idx00 + 1] as f32 * w00
                + src[idx01 + 1] as f32 * w01
                + src[idx10 + 1] as f32 * w10
                + src[idx11 + 1] as f32 * w11;

            let b = src[idx00 + 2] as f32 * w00
                + src[idx01 + 2] as f32 * w01
                + src[idx10 + 2] as f32 * w10
                + src[idx11 + 2] as f32 * w11;

            let ru = r.round().clamp(0.0, 255.0) as u8;
            let gu = g.round().clamp(0.0, 255.0) as u8;
            let bu = b.round().clamp(0.0, 255.0) as u8;

            let (rf, gf, bf) = norm.normalize(ru, gu, bu);
            let dst_idx = dst_row_offset + dst_x;
            dst_r[dst_idx] = rf;
            dst_g[dst_idx] = gf;
            dst_b[dst_idx] = bf;
        }
    }

    LetterboxTransform {
        scale,
        pad_x,
        pad_y,
        original_w: src_w,
        original_h: src_h,
        target_w: dst_w,
        target_h: dst_h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalization_modes() {
        let (r, g, b) = NormalizationMode::ZeroOne.normalize(255, 0, 127);
        assert!((r - 1.0).abs() < 1e-4);
        assert!((g - 0.0).abs() < 1e-4);
        assert!((b - 0.498).abs() < 1e-2);

        let (rc, gc, bc) = NormalizationMode::Centered.normalize(255, 0, 127);
        assert!((rc - 1.0).abs() < 1e-4);
        assert!((gc - (-1.0)).abs() < 1e-4);
        assert!(bc.abs() < 0.05);

        let (ri, gi, bi) = NormalizationMode::ImageNet.normalize(255, 255, 255);
        assert!(ri > 1.0 && gi > 1.0 && bi > 1.0);
    }

    #[test]
    fn test_resize_bilinear_rgb_nchw() {
        let src_w = 2;
        let src_h = 2;
        // 2x2 image: Red, Green, Blue, White
        let src = vec![
            255, 0, 0,    0, 255, 0,
            0, 0, 255,    255, 255, 255,
        ];

        let dst_w = 4;
        let dst_h = 4;
        let mut dst = vec![0.0f32; 3 * dst_w * dst_h];

        resize_bilinear_rgb_nchw(&src, src_w, src_h, &mut dst, dst_w, dst_h, NormalizationMode::ZeroOne);
        assert_eq!(dst.len(), 48);

        // Top-left pixel should be pure red (r=1.0, g=0.0, b=0.0)
        assert!((dst[0] - 1.0).abs() < 1e-4);
        assert!((dst[16] - 0.0).abs() < 1e-4);
        assert!((dst[32] - 0.0).abs() < 1e-4);
    }

    #[test]
    fn test_letterbox_transform() {
        let src_w = 1920;
        let src_h = 1080;
        let dst_w = 256;
        let dst_h = 256;
        let src = vec![128u8; src_w * src_h * 3];
        let mut dst = vec![0.0f32; 3 * dst_w * dst_h];

        let transform = letterbox_rgb_nchw(&src, src_w, src_h, &mut dst, dst_w, dst_h, NormalizationMode::Centered);
        assert!(transform.pad_y > 0.0);
        assert_eq!(transform.pad_x, 0.0);

        let orig = (960.0, 540.0); // Center of original
        let (mx, my) = transform.original_to_model(orig.0, orig.1);
        assert!((mx - 0.5).abs() < 0.05);
        assert!((my - 0.5).abs() < 0.05);

        let (back_x, back_y) = transform.model_to_original(mx, my);
        assert!((back_x - orig.0).abs() < 2.0);
        assert!((back_y - orig.1).abs() < 2.0);
    }
}
