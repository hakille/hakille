//! Encodage, décodage et vignettes RGBA8 pour les images générées.

use crate::error::{MediaError, MediaResult};
use crate::types::{ImageFormat, PreviewImage};
use image::{ColorType, ImageEncoder, ImageReader};
use std::path::Path;

/// Tampon image RGBA8 (4 octets par pixel).
#[derive(Debug, Clone)]
pub struct RgbaImageBuf {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaImageBuf {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> MediaResult<Self> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| MediaError::Encoding("dépassement de taille image RGBA".into()))?;
        if pixels.len() != expected {
            return Err(MediaError::Encoding(format!(
                "taille tampon RGBA incohérente : reçu {}, attendu {expected}",
                pixels.len()
            )));
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub fn from_rgb(width: u32, height: u32, rgb: &[u8]) -> MediaResult<Self> {
        let expected = (width as usize) * (height as usize) * 3;
        if rgb.len() != expected {
            return Err(MediaError::Encoding(format!(
                "taille tampon RGB incohérente : reçu {}, attendu {expected}",
                rgb.len()
            )));
        }
        let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);
        for chunk in rgb.as_chunks::<3>().0 {
            rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
        }
        Ok(Self {
            width,
            height,
            pixels: rgba,
        })
    }

    /// Convertit en octets RGB (3 octets par pixel) pour encodage JPEG ou VAE.
    pub fn to_rgb(&self) -> Vec<u8> {
        let mut rgb = Vec::with_capacity((self.width as usize) * (self.height as usize) * 3);
        for px in self.pixels.as_chunks::<4>().0 {
            rgb.extend_from_slice(&px[..3]);
        }
        rgb
    }

    /// Encode en PNG ou JPEG en mémoire.
    pub fn encode(&self, format: ImageFormat) -> MediaResult<Vec<u8>> {
        let mut out = Vec::new();
        match format {
            ImageFormat::Png => {
                let encoder = image::codecs::png::PngEncoder::new(&mut out);
                encoder
                    .write_image(
                        &self.pixels,
                        self.width,
                        self.height,
                        ColorType::Rgba8.into(),
                    )
                    .map_err(|e| MediaError::Encoding(format!("encodage PNG : {e}")))?;
            }
            ImageFormat::Jpeg => {
                let rgb = self.to_rgb();
                let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92);
                encoder
                    .encode(&rgb, self.width, self.height, ColorType::Rgb8.into())
                    .map_err(|e| MediaError::Encoding(format!("encodage JPEG : {e}")))?;
            }
        }
        Ok(out)
    }

    /// Redimensionne par interpolation bilinéaire en conservant le ratio dans
    /// une boîte bornée (`max_w × max_h`), pour affichage direct dans le HUD F5.
    pub fn make_preview(&self, max_w: u32, max_h: u32) -> PreviewImage {
        if self.width == 0 || self.height == 0 {
            return PreviewImage {
                width: 0,
                height: 0,
                rgba: Vec::new(),
            };
        }
        let scale_x = max_w as f32 / self.width as f32;
        let scale_y = max_h as f32 / self.height as f32;
        let scale = scale_x.min(scale_y).min(1.0);
        let dst_w = ((self.width as f32 * scale).round() as u32).max(1);
        let dst_h = ((self.height as f32 * scale).round() as u32).max(1);
        let resized = resize_rgba_bilinear(&self.pixels, self.width, self.height, dst_w, dst_h);
        PreviewImage {
            width: dst_w,
            height: dst_h,
            rgba: resized,
        }
    }
}

/// Charge une image locale (PNG/JPEG) en tampon RGBA8.
pub fn load_rgba_image(path: &Path) -> MediaResult<RgbaImageBuf> {
    let reader = ImageReader::open(path)
        .map_err(|e| MediaError::Storage(format!("ouverture image {} : {e}", path.display())))?;
    let decoded = reader
        .decode()
        .map_err(|e| MediaError::Encoding(format!("décodage image {} : {e}", path.display())))?;
    let rgba = decoded.to_rgba8();
    let (w, h) = rgba.dimensions();
    RgbaImageBuf::new(w, h, rgba.into_raw())
}

/// Redimensionnement bilinéaire RGBA8 pur Rust.
pub fn resize_rgba_bilinear(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return Vec::new();
    }
    let mut dst = vec![0u8; (dst_w as usize) * (dst_h as usize) * 4];
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

    for y in 0..dst_h {
        let sy = y as f32 * y_ratio;
        let y0 = (sy.floor() as u32).min(src_h - 1);
        let y1 = (y0 + 1).min(src_h - 1);
        let fy = sy - y0 as f32;
        for x in 0..dst_w {
            let sx = x as f32 * x_ratio;
            let x0 = (sx.floor() as u32).min(src_w - 1);
            let x1 = (x0 + 1).min(src_w - 1);
            let fx = sx - x0 as f32;

            let i00 = ((y0 * src_w + x0) * 4) as usize;
            let i10 = ((y0 * src_w + x1) * 4) as usize;
            let i01 = ((y1 * src_w + x0) * 4) as usize;
            let i11 = ((y1 * src_w + x1) * 4) as usize;
            let out_i = ((y * dst_w + x) * 4) as usize;

            for c in 0..4 {
                let top = src[i00 + c] as f32 * (1.0 - fx) + src[i10 + c] as f32 * fx;
                let bot = src[i01 + c] as f32 * (1.0 - fx) + src[i11 + c] as f32 * fx;
                let val = top * (1.0 - fy) + bot * fy;
                dst[out_i + c] = val.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    dst
}
