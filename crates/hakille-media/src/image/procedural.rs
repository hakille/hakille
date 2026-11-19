//! Moteur visuel procédural DSP pur Rust (`procedural-dsp`).
//!
//! Contrat d'honnêteté : ce moteur se déclare explicitement
//! [`EngineKind::ProceduralDsp`] (jamais neuronal). Il permet :
//! - la synthèse déterministe immédiate (0 modèle externe requis) de fonds
//!   génératifs, textures organiques (bruit fBm / valeur multi-octaves),
//!   compositions géométriques, palettes chromatiques adaptées au prompt et
//!   fusion I2I / inpaint ;
//! - le test de bout en bout de toute la chaîne Hakille (Biscuit, Constitution,
//!   provenance Ed25519, chunk PNG `iTXt`, DAG multimodal, HUD F5).

use crate::error::MediaResult;
use crate::image::encode::{RgbaImageBuf, load_rgba_image, resize_rgba_bilinear};
use crate::image::{ImageGenerationOutput, ImageGeneratorBackend};
use crate::types::{
    EngineAvailability, EngineDescriptor, EngineKind, GenerationControl, ImageGenMode,
    ImageGenRequest, MediaDataExit, MediaModality, MediaProgressEvent, ProgressCallback,
    derive_seed,
};
use async_trait::async_trait;

pub struct ProceduralCanvasEngine;

impl ProceduralCanvasEngine {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ProceduralCanvasEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ImageGeneratorBackend for ProceduralCanvasEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            id: "procedural-dsp".into(),
            name: "Canvas Procédural DSP (Rust pur)".into(),
            modality: MediaModality::Image,
            kind: EngineKind::ProceduralDsp,
            data_exit: MediaDataExit::Local,
            availability: EngineAvailability::Ready {
                device: "CPU (DSP Rust pur)".into(),
                model_sha256: None,
            },
            estimated_ram_mb: 16,
            estimated_vram_mb: 0,
            capabilities: vec![
                "text_to_image".into(),
                "image_to_image".into(),
                "inpaint".into(),
                "deterministic_seed".into(),
            ],
            description: "Synthèse visuelle algorithmique déterministe (champs fBm multi-octaves, palettes chromatiques, géométrie spectrale). Explicite DSP — ne prétend jamais être un modèle neuronal.".into(),
        }
    }

    async fn generate(
        &self,
        req: &ImageGenRequest,
        control: &GenerationControl,
        progress: Option<ProgressCallback>,
    ) -> MediaResult<ImageGenerationOutput> {
        let req = req.clone();
        let control = control.clone();
        tokio::task::spawn_blocking(move || render_procedural_image(&req, &control, progress))
            .await
            .map_err(|e| crate::error::MediaError::InferenceFailed {
                engine_id: "procedural-dsp".into(),
                reason: format!("tâche de rendu interrompue : {e}"),
            })?
    }
}

/// Rendu synchrone déterministe sur CPU d'une scène procédurale riche.
pub fn render_procedural_image(
    req: &ImageGenRequest,
    control: &GenerationControl,
    progress: Option<ProgressCallback>,
) -> MediaResult<ImageGenerationOutput> {
    let seed = derive_seed(&req.prompt, req.seed);
    let palette = palette_from_prompt(&req.prompt, seed);
    let w = req.width;
    let h = req.height;
    let total_steps = req.steps.clamp(4, 24);

    // Charge éventuellement l'image de base (I2I / Inpaint).
    let (base_img, mask_img, strength) = match &req.mode {
        ImageGenMode::TextToImage => (None, None, 1.0f32),
        ImageGenMode::ImageToImage {
            init_image_path,
            strength,
        } => {
            let loaded = load_rgba_image(init_image_path)?;
            let resized = resize_rgba_bilinear(&loaded.pixels, loaded.width, loaded.height, w, h);
            (Some(resized), None, strength.clamp(0.05, 1.0))
        }
        ImageGenMode::Inpaint {
            init_image_path,
            mask_path,
            strength,
        } => {
            let loaded = load_rgba_image(init_image_path)?;
            let resized = resize_rgba_bilinear(&loaded.pixels, loaded.width, loaded.height, w, h);
            let mask = load_rgba_image(mask_path)?;
            let resized_mask = resize_rgba_bilinear(&mask.pixels, mask.width, mask.height, w, h);
            (Some(resized), Some(resized_mask), strength.clamp(0.05, 1.0))
        }
    };

    let pixels = render_scene_frame(
        w,
        h,
        seed,
        &palette,
        0.0,
        0.0,
        total_steps,
        control,
        progress.as_ref(),
    )?;

    let final_pixels = if let Some(base) = base_img {
        let mut blended = vec![0u8; pixels.len()];
        for idx in (0..pixels.len()).step_by(4) {
            let local_strength = if let Some(ref mask) = mask_img {
                (mask[idx] as f32 / 255.0) * strength
            } else {
                strength
            };
            for c in 0..3 {
                let a = base[idx + c] as f32;
                let b = pixels[idx + c] as f32;
                blended[idx + c] = (a * (1.0 - local_strength) + b * local_strength)
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
            blended[idx + 3] = 255;
        }
        blended
    } else {
        pixels
    };

    let rgba = RgbaImageBuf::new(w, h, final_pixels)?;
    let preview = rgba.make_preview(200, 120);
    let encoded = rgba.encode(req.format)?;

    Ok(ImageGenerationOutput {
        encoded_bytes: encoded,
        rgba,
        preview,
        seed,
        engine_id: "procedural-dsp".into(),
        engine_kind: EngineKind::ProceduralDsp,
        data_exit: MediaDataExit::Local,
        model_id: None,
        model_sha256: None,
    })
}

/// Palette chromatique de 4 couleurs (fond sombre, atmosphère, accent 1, éclat).
#[derive(Debug, Clone, Copy)]
pub struct ProceduralPalette {
    pub bg: [f32; 3],
    pub mid: [f32; 3],
    pub accent: [f32; 3],
    pub highlight: [f32; 3],
    pub celestial_rings: bool,
    pub grid_overlay: bool,
}

pub fn palette_from_prompt(prompt: &str, seed: u64) -> ProceduralPalette {
    let lower = prompt.to_ascii_lowercase();
    let celestial_rings = lower.contains("cosmos")
        || lower.contains("space")
        || lower.contains("espace")
        || lower.contains("nuit")
        || lower.contains("night")
        || lower.contains("nebula")
        || lower.contains("planet")
        || lower.contains("orbite")
        || lower.contains("soleil")
        || lower.contains("sun");
    let grid_overlay = lower.contains("cyber")
        || lower.contains("futur")
        || lower.contains("neon")
        || lower.contains("néon")
        || lower.contains("ville")
        || lower.contains("city")
        || lower.contains("matrix")
        || lower.contains("architecture");

    if lower.contains("océan")
        || lower.contains("ocean")
        || lower.contains("mer")
        || lower.contains("sea")
        || lower.contains("eau")
        || lower.contains("water")
    {
        return ProceduralPalette {
            bg: [6.0, 18.0, 38.0],
            mid: [14.0, 68.0, 112.0],
            accent: [36.0, 178.0, 198.0],
            highlight: [210.0, 246.0, 255.0],
            celestial_rings,
            grid_overlay,
        };
    }
    if lower.contains("forêt")
        || lower.contains("forest")
        || lower.contains("nature")
        || lower.contains("jungle")
        || lower.contains("arbre")
    {
        return ProceduralPalette {
            bg: [10.0, 24.0, 16.0],
            mid: [24.0, 74.0, 46.0],
            accent: [98.0, 182.0, 92.0],
            highlight: [232.0, 245.0, 180.0],
            celestial_rings,
            grid_overlay,
        };
    }
    if lower.contains("feu")
        || lower.contains("fire")
        || lower.contains("lave")
        || lower.contains("soleil")
        || lower.contains("sunset")
        || lower.contains("crépuscule")
        || lower.contains("désert")
        || lower.contains("desert")
    {
        return ProceduralPalette {
            bg: [28.0, 10.0, 18.0],
            mid: [132.0, 34.0, 42.0],
            accent: [242.0, 128.0, 38.0],
            highlight: [255.0, 232.0, 168.0],
            celestial_rings: true,
            grid_overlay,
        };
    }
    if grid_overlay {
        return ProceduralPalette {
            bg: [10.0, 12.0, 26.0],
            mid: [38.0, 20.0, 88.0],
            accent: [0.0, 214.0, 242.0],
            highlight: [255.0, 64.0, 188.0],
            celestial_rings,
            grid_overlay: true,
        };
    }

    // Palette déterministe dérivée de la graine.
    let hue = ((seed % 360) as f32) / 360.0;
    let c1 = hsv_to_rgb(hue, 0.72, 0.14);
    let c2 = hsv_to_rgb((hue + 0.08) % 1.0, 0.65, 0.42);
    let c3 = hsv_to_rgb((hue + 0.45) % 1.0, 0.78, 0.85);
    let c4 = hsv_to_rgb((hue + 0.52) % 1.0, 0.35, 0.98);
    ProceduralPalette {
        bg: c1,
        mid: c2,
        accent: c3,
        highlight: c4,
        celestial_rings: true,
        grid_overlay: false,
    }
}

/// Synthétise une trame RGBA8 procédurale à l'instant `phase_t` (utilisée aussi
/// par le moteur vidéo procédural pour des animations fluides).
#[allow(clippy::too_many_arguments)]
pub fn render_scene_frame(
    width: u32,
    height: u32,
    seed: u64,
    palette: &ProceduralPalette,
    phase_t: f32,
    audio_energy: f32,
    report_steps: u32,
    control: &GenerationControl,
    progress: Option<&ProgressCallback>,
) -> MediaResult<Vec<u8>> {
    let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
    let inv_w = 1.0 / (width.max(1) as f32);
    let inv_h = 1.0 / (height.max(1) as f32);
    let aspect = width as f32 / height.max(1) as f32;

    let sun_x = 0.5 + 0.18 * (phase_t * 0.9 + (seed as f32 * 0.001)).sin();
    let sun_y = 0.36 + 0.08 * (phase_t * 0.7).cos();
    let pulse = 1.0 + audio_energy.clamp(0.0, 1.5) * 0.45;
    let bands = report_steps.max(1);
    let rows_per_band = height.div_ceil(bands).max(1);

    for band in 0..bands {
        control.check()?;
        let y_start = band * rows_per_band;
        let y_end = ((band + 1) * rows_per_band).min(height);

        for y in y_start..y_end {
            let ny = y as f32 * inv_h;
            for x in 0..width {
                let nx = x as f32 * inv_w;

                // Domaine déformé (domain-warped fBm) pour des volutes organiques.
                let wx = fbm2d(nx * 2.8 + phase_t * 0.25, ny * 2.8 - phase_t * 0.15, seed);
                let wy = fbm2d(
                    nx * 2.8 - phase_t * 0.18 + 11.3,
                    ny * 2.8 + phase_t * 0.22 + 5.7,
                    seed.wrapping_add(7919),
                );
                let n = fbm2d(
                    nx * 3.5 + wx * 1.15 + phase_t * 0.12,
                    ny * 3.5 + wy * 1.15,
                    seed.wrapping_add(31337),
                );

                // Dégradé vertical atmosphérique + nuage fBm.
                let mut col = mix3(
                    palette.bg,
                    palette.mid,
                    (ny * 0.75 + n * 0.55).clamp(0.0, 1.0),
                );
                let ridge = (1.0 - (n * 2.0 - 1.0).abs()).powf(2.2);
                col = mix3(col, palette.accent, (ridge * 0.65 * pulse).clamp(0.0, 1.0));

                // Disque lumineux + anneaux orbitaux réactifs.
                let dx = (nx - sun_x) * aspect;
                let dy = ny - sun_y;
                let dist = (dx * dx + dy * dy).sqrt();
                let halo = (-dist * dist * (7.5 / pulse)).exp();
                col = mix3(col, palette.highlight, (halo * 0.85).clamp(0.0, 1.0));

                if palette.celestial_rings {
                    let ring_r = 0.22 * pulse;
                    let ring = (-(dist - ring_r).abs() * 55.0).exp()
                        + 0.55 * (-(dist - ring_r * 1.55).abs() * 70.0).exp();
                    col = mix3(col, palette.highlight, (ring * 0.55).clamp(0.0, 1.0));
                }

                // Horizon montagneux / ondes synthétiques en bas de scène.
                let horizon = 0.70
                    + 0.08 * (nx * 9.0 + phase_t * 1.6).sin()
                    + 0.05 * fbm2d(nx * 6.0 + phase_t * 0.4, 2.5, seed.wrapping_add(97));
                if ny > horizon {
                    let depth = ((ny - horizon) / (1.0 - horizon + 0.001)).clamp(0.0, 1.0);
                    let ground = mix3(palette.bg, palette.accent, (1.0 - depth) * 0.25);
                    col = mix3(col, ground, 0.82);

                    if palette.grid_overlay {
                        let persp_z = 1.0 / (depth + 0.08);
                        let gx = ((nx - 0.5) * persp_z * 2.4).fract().abs();
                        let gz = (persp_z * 0.45 - phase_t * 1.8).fract().abs();
                        let line = if gx < 0.06 || gx > 0.94 || gz < 0.07 || gz > 0.93 {
                            0.65
                        } else {
                            0.0
                        };
                        col = mix3(col, palette.highlight, line * (0.4 + 0.6 * depth));
                    }
                }

                // Vignette douce.
                let vx = nx - 0.5;
                let vy = ny - 0.5;
                let vig = (1.0 - (vx * vx + vy * vy) * 0.85).clamp(0.45, 1.0);
                let out_i = ((y * width + x) * 4) as usize;
                pixels[out_i] = (col[0] * vig).round().clamp(0.0, 255.0) as u8;
                pixels[out_i + 1] = (col[1] * vig).round().clamp(0.0, 255.0) as u8;
                pixels[out_i + 2] = (col[2] * vig).round().clamp(0.0, 255.0) as u8;
                pixels[out_i + 3] = 255;
            }
        }

        if let Some(cb) = progress {
            let step = band + 1;
            cb(MediaProgressEvent {
                job_id: control.job_id.clone(),
                modality: MediaModality::Image,
                stage: "synthèse_dsp".into(),
                step,
                total_steps: bands,
                fraction: step as f32 / bands as f32,
                elapsed_ms: control.elapsed_ms(),
                message: format!("Rendu procédural DSP étape {step}/{bands}"),
                preview: None,
            });
        }
    }

    Ok(pixels)
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match (i as i32).rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [r * 255.0, g * 255.0, b * 255.0]
}

fn hash2d(ix: i32, iy: i32, seed: u64) -> f32 {
    let mut h = (ix as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (iy as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)
        ^ seed.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    ((h >> 40) as f32) / ((1u32 << 24) as f32)
}

fn value_noise(x: f32, y: f32, seed: u64) -> f32 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = x - ix as f32;
    let fy = y - iy as f32;
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);

    let a = hash2d(ix, iy, seed);
    let b = hash2d(ix + 1, iy, seed);
    let c = hash2d(ix, iy + 1, seed);
    let d = hash2d(ix + 1, iy + 1, seed);
    a + (b - a) * ux + (c - a) * uy + (a - b - c + d) * ux * uy
}

fn fbm2d(mut x: f32, mut y: f32, seed: u64) -> f32 {
    let mut value = 0.0f32;
    let mut amp = 0.5f32;
    for oct in 0..4u64 {
        value += amp * value_noise(x, y, seed.wrapping_add(oct * 1013));
        x *= 2.03;
        y *= 2.03;
        amp *= 0.5;
    }
    value
}
