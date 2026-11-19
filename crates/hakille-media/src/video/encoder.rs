//! Encodeurs vidéo 100 % Rust pur (zéro dépendance externe) :
//! - **APNG** (`png 0.18` avec `set_animated` + `set_frame_delay`) ;
//! - **Y4M (`YUV4MPEG2` BT.709 4:2:0)** — flux vidéo sans perte de conteneur,
//!   directement consommable par lecteurs ou par `FfmpegMuxer` ;
//! - **FrameSequence** — manifeste JSON + séquence de frames PNG.

use crate::error::{MediaError, MediaResult};
use crate::image::encode::RgbaImageBuf;
use std::io::Cursor;

/// Encode une séquence de frames RGBA8 en APNG (Animated PNG) pur Rust.
pub fn encode_apng(frames: &[RgbaImageBuf], fps: u16) -> MediaResult<Vec<u8>> {
    if frames.is_empty() {
        return Err(MediaError::Encoding(
            "impossible d'encoder un APNG sans aucune frame".into(),
        ));
    }
    let width = frames[0].width;
    let height = frames[0].height;
    let fps = fps.max(1);

    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(Cursor::new(&mut out), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .set_animated(frames.len() as u32, 0)
            .map_err(|e| MediaError::Encoding(format!("configuration APNG : {e}")))?;
        encoder
            .set_frame_delay(1, fps)
            .map_err(|e| MediaError::Encoding(format!("cadence APNG : {e}")))?;

        let mut writer = encoder
            .write_header()
            .map_err(|e| MediaError::Encoding(format!("en-tête APNG : {e}")))?;
        for (idx, frame) in frames.iter().enumerate() {
            if frame.width != width || frame.height != height {
                return Err(MediaError::Encoding(format!(
                    "dimensions incohérentes frame {idx} ({}x{} != {width}x{height})",
                    frame.width, frame.height
                )));
            }
            writer
                .write_image_data(&frame.pixels)
                .map_err(|e| MediaError::Encoding(format!("écriture frame APNG {idx} : {e}")))?;
        }
        writer
            .finish()
            .map_err(|e| MediaError::Encoding(format!("finalisation APNG : {e}")))?;
    }
    Ok(out)
}

/// Encode une séquence de frames RGBA8 en flux **YUV4MPEG2 (Y4M)** 4:2:0 BT.709.
///
/// Format standard non compressé lisible nativement par VLC, mpv, et transmis
/// directement en entrée à `FfmpegMuxer` pour produire MP4 H.264 / WebM VP9.
pub fn encode_y4m(frames: &[RgbaImageBuf], fps: u16) -> MediaResult<Vec<u8>> {
    if frames.is_empty() {
        return Err(MediaError::Encoding(
            "impossible d'encoder un Y4M sans aucune frame".into(),
        ));
    }
    let width = frames[0].width as usize;
    let height = frames[0].height as usize;
    if !width.is_multiple_of(2) || !height.is_multiple_of(2) {
        return Err(MediaError::Encoding(format!(
            "Y4M 4:2:0 exige des dimensions paires (reçu {width}x{height})"
        )));
    }
    let fps = fps.max(1);
    let y_size = width * height;
    let uv_size = (width / 2) * (height / 2);
    let frame_bytes = y_size + 2 * uv_size;

    let header = format!("YUV4MPEG2 W{width} H{height} F{fps}:1 Ip A1:1 C420mpeg2\n");
    let mut out = Vec::with_capacity(header.len() + frames.len() * (6 + frame_bytes));
    out.extend_from_slice(header.as_bytes());

    let mut y_plane = vec![0u8; y_size];
    let mut u_plane = vec![0u8; uv_size];
    let mut v_plane = vec![0u8; uv_size];

    for (idx, frame) in frames.iter().enumerate() {
        if frame.width as usize != width || frame.height as usize != height {
            return Err(MediaError::Encoding(format!(
                "dimensions incohérentes frame Y4M {idx}"
            )));
        }
        rgba_to_yuv420_bt709(
            &frame.pixels,
            width,
            height,
            &mut y_plane,
            &mut u_plane,
            &mut v_plane,
        );
        out.extend_from_slice(b"FRAME\n");
        out.extend_from_slice(&y_plane);
        out.extend_from_slice(&u_plane);
        out.extend_from_slice(&v_plane);
    }
    Ok(out)
}

/// Encode un manifeste `FrameSequence` JSON contenant les métadonnées de chaque
/// frame PNG encodée en base64 (auto-contenu pour le dépôt content-addressed).
pub fn encode_frame_sequence_bundle(frames: &[RgbaImageBuf], fps: u16) -> MediaResult<Vec<u8>> {
    use base64::Engine as _;
    if frames.is_empty() {
        return Err(MediaError::Encoding("séquence de frames vide".into()));
    }
    let mut encoded_frames = Vec::with_capacity(frames.len());
    for (idx, frame) in frames.iter().enumerate() {
        let png_bytes = frame.encode(crate::types::ImageFormat::Png)?;
        encoded_frames.push(serde_json::json!({
            "index": idx,
            "sha256": crate::catalog::sha256_bytes(&png_bytes),
            "png_base64": base64::engine::general_purpose::STANDARD.encode(&png_bytes),
        }));
    }
    let bundle = serde_json::json!({
        "format": "hakille.frame_sequence.v1",
        "width": frames[0].width,
        "height": frames[0].height,
        "fps": fps.max(1),
        "frame_count": frames.len(),
        "frames": encoded_frames,
    });
    serde_json::to_vec(&bundle).map_err(|e| MediaError::Encoding(e.to_string()))
}

/// Conversion précise RGBA8 → YUV420p (matrice Rec.709 plage limitée 16..235 / 16..240).
fn rgba_to_yuv420_bt709(
    rgba: &[u8],
    width: usize,
    height: usize,
    y_plane: &mut [u8],
    u_plane: &mut [u8],
    v_plane: &mut [u8],
) {
    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) * 4;
            let r = f32::from(rgba[i]);
            let g = f32::from(rgba[i + 1]);
            let b = f32::from(rgba[i + 2]);
            let y_val = (16.0 + 0.183 * r + 0.614 * g + 0.062 * b)
                .round()
                .clamp(16.0, 235.0) as u8;
            y_plane[y * width + x] = y_val;
        }
    }

    let uv_w = width / 2;
    for y in (0..height).step_by(2) {
        for x in (0..width).step_by(2) {
            let mut r_sum = 0.0f32;
            let mut g_sum = 0.0f32;
            let mut b_sum = 0.0f32;
            for dy in 0..2 {
                for dx in 0..2 {
                    let i = ((y + dy) * width + (x + dx)) * 4;
                    r_sum += f32::from(rgba[i]);
                    g_sum += f32::from(rgba[i + 1]);
                    b_sum += f32::from(rgba[i + 2]);
                }
            }
            let r = r_sum * 0.25;
            let g = g_sum * 0.25;
            let b = b_sum * 0.25;
            let u_val = (128.0 - 0.101 * r - 0.339 * g + 0.439 * b)
                .round()
                .clamp(16.0, 240.0) as u8;
            let v_val = (128.0 + 0.439 * r - 0.399 * g - 0.040 * b)
                .round()
                .clamp(16.0, 240.0) as u8;
            let uv_idx = (y / 2) * uv_w + (x / 2);
            u_plane[uv_idx] = u_val;
            v_plane[uv_idx] = v_val;
        }
    }
}
