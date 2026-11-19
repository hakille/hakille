//! Mastering, mixage multipiste avec sidechain ducking automatique,
//! rééchantillonnage, encodage/décodage WAV (`hound`) et rendu de forme d'onde
//! RGBA8 pour le HUD Desktop (`F5`).

use crate::error::{MediaError, MediaResult};
use crate::types::{AudioFormat, PreviewImage};
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use std::io::Cursor;
use std::path::Path;

/// Tampon audio PCM flottant entrelacé dans `[-1.0, 1.0]`.
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

impl AudioBuffer {
    pub fn new(sample_rate: u32, channels: u16, samples: Vec<f32>) -> MediaResult<Self> {
        if sample_rate == 0 || !matches!(channels, 1 | 2) {
            return Err(MediaError::InvalidRequest(format!(
                "paramètres audio invalides : sample_rate={sample_rate}, channels={channels}"
            )));
        }
        Ok(Self {
            sample_rate,
            channels,
            samples,
        })
    }

    pub fn num_frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }

    pub fn duration_secs(&self) -> f32 {
        self.num_frames() as f32 / self.sample_rate as f32
    }

    /// Convertit en mono ou stéréo.
    pub fn to_channels(&self, target_channels: u16) -> Self {
        if self.channels == target_channels {
            return self.clone();
        }
        match (self.channels, target_channels) {
            (1, 2) => {
                let mut out = Vec::with_capacity(self.samples.len() * 2);
                for &s in &self.samples {
                    out.push(s);
                    out.push(s);
                }
                Self {
                    sample_rate: self.sample_rate,
                    channels: 2,
                    samples: out,
                }
            }
            (2, 1) => {
                let mut out = Vec::with_capacity(self.samples.len() / 2);
                for lr in self.samples.as_chunks::<2>().0 {
                    out.push(0.5 * (lr[0] + lr[1]));
                }
                Self {
                    sample_rate: self.sample_rate,
                    channels: 1,
                    samples: out,
                }
            }
            _ => self.clone(),
        }
    }

    /// Rééchantillonnage linéaire vers `target_rate`.
    pub fn resample(&self, target_rate: u32) -> Self {
        if self.sample_rate == target_rate || self.samples.is_empty() {
            return self.clone();
        }
        let ch = usize::from(self.channels);
        let src_frames = self.num_frames();
        let dst_frames =
            ((src_frames as f64 * target_rate as f64) / self.sample_rate as f64).round() as usize;
        let mut out = vec![0.0f32; dst_frames * ch];
        let ratio = self.sample_rate as f64 / target_rate as f64;
        for f in 0..dst_frames {
            let pos = f as f64 * ratio;
            let i0 = (pos.floor() as usize).min(src_frames.saturating_sub(1));
            let i1 = (i0 + 1).min(src_frames.saturating_sub(1));
            let frac = (pos - i0 as f64) as f32;
            for c in 0..ch {
                let a = self.samples[i0 * ch + c];
                let b = self.samples[i1 * ch + c];
                out[f * ch + c] = a + (b - a) * frac;
            }
        }
        Self {
            sample_rate: target_rate,
            channels: self.channels,
            samples: out,
        }
    }

    /// Applique un fondu entrant et sortant doux pour éviter tout clic.
    pub fn apply_fades(&mut self, fade_in_ms: u32, fade_out_ms: u32) {
        let ch = usize::from(self.channels);
        let frames = self.num_frames();
        let in_frames = ((self.sample_rate as u64 * u64::from(fade_in_ms)) / 1000) as usize;
        let out_frames = ((self.sample_rate as u64 * u64::from(fade_out_ms)) / 1000) as usize;

        for f in 0..in_frames.min(frames) {
            let g = f as f32 / in_frames.max(1) as f32;
            for c in 0..ch {
                self.samples[f * ch + c] *= g;
            }
        }
        for k in 0..out_frames.min(frames) {
            let f = frames - 1 - k;
            let g = k as f32 / out_frames.max(1) as f32;
            for c in 0..ch {
                self.samples[f * ch + c] *= g;
            }
        }
    }

    /// Normalisation crête douce vers `target_peak` (ex. 0.92).
    pub fn normalize_peak(&mut self, target_peak: f32) {
        let peak = self.samples.iter().fold(0.0f32, |acc, &s| acc.max(s.abs()));
        if peak > 1e-5 {
            let gain = target_peak.clamp(0.1, 0.99) / peak;
            for s in &mut self.samples {
                *s = (*s * gain).clamp(-1.0, 1.0);
            }
        }
    }

    /// Encode en WAV PCM16 ou IEEE F32 en mémoire.
    pub fn encode_wav(&self, format: AudioFormat) -> MediaResult<Vec<u8>> {
        let mut cursor = Cursor::new(Vec::<u8>::new());
        let spec = match format {
            AudioFormat::WavPcm16 => WavSpec {
                channels: self.channels,
                sample_rate: self.sample_rate,
                bits_per_sample: 16,
                sample_format: SampleFormat::Int,
            },
            AudioFormat::WavF32 => WavSpec {
                channels: self.channels,
                sample_rate: self.sample_rate,
                bits_per_sample: 32,
                sample_format: SampleFormat::Float,
            },
        };
        let mut writer = WavWriter::new(&mut cursor, spec)
            .map_err(|e| MediaError::Encoding(format!("création WAV : {e}")))?;
        match format {
            AudioFormat::WavPcm16 => {
                for &s in &self.samples {
                    let q = (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
                    writer
                        .write_sample(q)
                        .map_err(|e| MediaError::Encoding(format!("écriture PCM16 : {e}")))?;
                }
            }
            AudioFormat::WavF32 => {
                for &s in &self.samples {
                    writer
                        .write_sample(s.clamp(-1.0, 1.0))
                        .map_err(|e| MediaError::Encoding(format!("écriture F32 : {e}")))?;
                }
            }
        }
        writer
            .finalize()
            .map_err(|e| MediaError::Encoding(format!("finalisation WAV : {e}")))?;
        Ok(cursor.into_inner())
    }

    /// Calcule l'enveloppe RMS par tranche vidéo (utilisée par la vidéo
    /// audio-réactive pour synchroniser le mouvement à la piste audio).
    pub fn rms_envelope_for_frames(&self, num_video_frames: usize) -> Vec<f32> {
        if num_video_frames == 0 || self.samples.is_empty() {
            return vec![0.0; num_video_frames];
        }
        let mono = self.to_channels(1);
        let chunk = (mono.samples.len() / num_video_frames).max(1);
        let mut env = Vec::with_capacity(num_video_frames);
        for i in 0..num_video_frames {
            let start = (i * chunk).min(mono.samples.len());
            let end = ((i + 1) * chunk).min(mono.samples.len());
            if start >= end {
                env.push(0.0);
                continue;
            }
            let sum_sq: f32 = mono.samples[start..end].iter().map(|&s| s * s).sum();
            let rms = (sum_sq / (end - start) as f32).sqrt();
            env.push((rms * 2.5).clamp(0.0, 1.0));
        }
        env
    }

    /// Rend une vignette RGBA8 de la forme d'onde audio pour le HUD `F5`.
    pub fn render_waveform_preview(&self, width: u32, height: u32) -> PreviewImage {
        let w = width.max(32);
        let h = height.max(16);
        let mut rgba = vec![0u8; (w as usize) * (h as usize) * 4];
        // Fond ardoise sombre Hakille.
        for px in rgba.as_chunks_mut::<4>().0 {
            px[0] = 14;
            px[1] = 18;
            px[2] = 28;
            px[3] = 255;
        }
        let mono = self.to_channels(1);
        let mid_y = (h / 2) as i32;
        if !mono.samples.is_empty() {
            let step = (mono.samples.len() as f32 / w as f32).max(1.0);
            for x in 0..w {
                let s0 = (x as f32 * step) as usize;
                let s1 = (((x + 1) as f32 * step) as usize)
                    .max(s0 + 1)
                    .min(mono.samples.len());
                let mut peak = 0.0f32;
                for &v in &mono.samples[s0..s1] {
                    peak = peak.max(v.abs());
                }
                let half_bar = ((peak.clamp(0.0, 1.0) * (h as f32 * 0.44)).round() as i32).max(1);
                let y0 = (mid_y - half_bar).max(1) as u32;
                let y1 = (mid_y + half_bar).min(h as i32 - 2) as u32;
                for y in y0..=y1 {
                    let dist = ((y as i32 - mid_y).abs() as f32) / (half_bar as f32 + 0.001);
                    let idx = ((y * w + x) * 4) as usize;
                    rgba[idx] = (56.0 + (1.0 - dist) * 140.0) as u8;
                    rgba[idx + 1] = (195.0 + (1.0 - dist) * 55.0) as u8;
                    rgba[idx + 2] = 248;
                    rgba[idx + 3] = 255;
                }
            }
        }
        // Ligne médiane subtile.
        for x in 0..w {
            let idx = ((mid_y as u32 * w + x) * 4) as usize;
            rgba[idx] = rgba[idx].saturating_add(24);
            rgba[idx + 1] = rgba[idx + 1].saturating_add(24);
            rgba[idx + 2] = rgba[idx + 2].saturating_add(30);
        }
        PreviewImage {
            width: w,
            height: h,
            rgba,
        }
    }
}

/// Lit un fichier WAV local en [`AudioBuffer`].
pub fn read_wav_file(path: &Path) -> MediaResult<AudioBuffer> {
    let mut reader = WavReader::open(path)
        .map_err(|e| MediaError::Storage(format!("lecture WAV {} : {e}", path.display())))?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| MediaError::Encoding(format!("échantillons WAV F32 : {e}")))?,
        SampleFormat::Int => {
            let max_val = ((1i64 << (spec.bits_per_sample.saturating_sub(1))) - 1).max(1) as f32;
            reader
                .samples::<i32>()
                .map(|r| r.map(|v| (v as f32 / max_val).clamp(-1.0, 1.0)))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| MediaError::Encoding(format!("échantillons WAV Int : {e}")))?
        }
    };
    AudioBuffer::new(spec.sample_rate, spec.channels, samples)
}

/// Mixe une piste de voix prioritaire sur un lit musical/ambiance avec
/// **sidechain ducking automatique** : lorsque la voix est active, le gain du
/// lit d'accompagnement est atténué de façon lissée (`duck_gain`, ex. 0.28).
pub fn mix_with_sidechain_ducking(
    voice: &AudioBuffer,
    bed: &AudioBuffer,
    bed_gain: f32,
    duck_gain: f32,
) -> MediaResult<AudioBuffer> {
    let target_rate = voice.sample_rate.max(bed.sample_rate);
    let target_ch = voice.channels.max(bed.channels);
    let v = voice.resample(target_rate).to_channels(target_ch);
    let b = bed.resample(target_rate).to_channels(target_ch);

    let frames = v.num_frames().max(b.num_frames());
    let ch = usize::from(target_ch);
    let mut out = vec![0.0f32; frames * ch];

    // Suiveur d'enveloppe attaque rapide / relâchement lent.
    let attack = (-1.0 / (target_rate as f32 * 0.015)).exp();
    let release = (-1.0 / (target_rate as f32 * 0.220)).exp();
    let mut env = 0.0f32;
    let duck_floor = duck_gain.clamp(0.1, 1.0);
    let base_bed = bed_gain.clamp(0.0, 2.0);

    for f in 0..frames {
        let mut v_energy = 0.0f32;
        if f < v.num_frames() {
            for c in 0..ch {
                v_energy = v_energy.max(v.samples[f * ch + c].abs());
            }
        }
        let coeff = if v_energy > env { attack } else { release };
        env = coeff * env + (1.0 - coeff) * v_energy;

        // Atténuation progressive dès que la voix dépasse le seuil -30 dBFS (~0.03).
        let duck_factor = ((env - 0.025) / 0.14).clamp(0.0, 1.0);
        let eff_bed_gain = base_bed * (1.0 - duck_factor * (1.0 - duck_floor));

        for c in 0..ch {
            let vs = if f < v.num_frames() {
                v.samples[f * ch + c]
            } else {
                0.0
            };
            let bs = if f < b.num_frames() {
                b.samples[f * ch + c]
            } else {
                0.0
            };
            // Saturation douce tanh pour éviter tout écrêtage dur.
            out[f * ch + c] = (vs + bs * eff_bed_gain).tanh();
        }
    }

    let mut mixed = AudioBuffer::new(target_rate, target_ch, out)?;
    mixed.apply_fades(15, 60);
    mixed.normalize_peak(0.94);
    Ok(mixed)
}
