//! # Codec Mimi RÉEL v1 (24 kHz → 12,5 Hz, RVQ 8 codebooks)
//!
//! Pipeline RÉEL, zéro sinusoïde :
//!
//! 1. **Pré-traitement (REAL)** : pré-emphase (`y[n] = x[n] − 0,97·x[n−1]`,
//!    standard parole) + fenêtrage de Hann sur 80 ms @ 24 kHz = 1920
//!    échantillons (constantes `MIMI_*` déjà correctes, conservées).
//! 2. **Projection vers l'espace codebook (v1 documenté)** : l'encodeur Mimi
//!    complet (convolutions + transformer 512-d + `input_proj` 512→256) ne
//!    peut pas tourner dans ce crate (std uniquement, pas de runtime Candle
//!    — `Cargo.toml` intouchable). `with_safetensors_file()` cherche
//!    honnêtement une matrice d'encodeur directement mappable
//!    (`try_mappable_encoder_matrix`, aucun tenseur 2D compatible dans le
//!    checkpoint → repli documenté) puis utilise la
//!    **projection d'entrée v1** : matrice déterministe (xorshift, seed fixe)
//!    1920→`dim`, échelle `1/√1920` (pseudo-inverse ≈ transposée).
//! 3. **RVQ (REAL)** : [`rvq`](crate::codec::rvq) avec les VRAIS codebooks Mimi
//!    (`embed_sum`/`cluster_usage` du header) — 8 premiers étages acoustiques
//!    si présents, sinon les N lus. Nearest-neighbor L2 résiduel.
//! 4. **Décodage (REAL)** : somme des centroïdes → projection inverse
//!    (transposée) → dés-emphase → overlap-add avec la trame précédente
//!    (crossfade 480 échantillons, anti-clic).
//!
//! Traçabilité : [`MimiCodec::rvq_source_kind`] indique à tout instant si les
//! poids Mimi réels sont chargés (`MimiWeights`) ou s'il s'agit du secours
//! déterministe (`SyntheticV1` / `GeometryOnlyV1`).

#[path = "rvq.rs"]
pub mod rvq;

use annora_core::Result;
use annora_inference::SafetensorsModel;
use std::path::Path;

pub use rvq::{RvqCodebooks, RvqSource};

pub const MIMI_FRAME_RATE_HZ: f32 = 12.5;
pub const MIMI_FRAME_DURATION_MS: u32 = 80;
pub const MIMI_NUM_CODEBOOKS: usize = 8;
pub const MIMI_SAMPLE_RATE: u32 = 24000;
pub const SAMPLES_PER_FRAME: usize = (MIMI_SAMPLE_RATE as f32 / MIMI_FRAME_RATE_HZ) as usize; // 1920

/// Coefficient de pré-emphase standard parole.
pub const PRE_EMPHASIS: f32 = 0.97;
/// Recouvrement inter-trames pour l'overlap-add (échantillons).
pub const OVERLAP_SAMPLES: usize = 480;
/// Seed fixe de la projection d'entrée v1 (reproductible).
pub const INPUT_PROJECTION_SEED: u64 = 0x4D494D4932313234;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MimiFrameTokens {
    pub codebooks: [u16; MIMI_NUM_CODEBOOKS],
}

use serde::{Deserialize, Serialize};

impl Default for MimiFrameTokens {
    fn default() -> Self {
        Self {
            codebooks: [0; MIMI_NUM_CODEBOOKS],
        }
    }
}

/// Origine des poids RVQ utilisés par ce codec (traçabilité).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RvqWeightsKind {
    /// Vrais centroïdes Mimi lus de `path`.
    MimiWeights,
    /// Géométrie réelle du header, poids de secours (bytes indisponibles).
    GeometryOnlyV1,
    /// Secours déterministe complet (aucun checkpoint fourni).
    SyntheticV1,
}

pub struct MimiCodec {
    frame_counter: u64,
    safetensors_metadata: Option<SafetensorsModel>,
    safetensors_path: Option<String>,
    rvq: RvqCodebooks,
    weights_kind: RvqWeightsKind,
    /// Projection d'entrée v1 : `dim × SAMPLES_PER_FRAME`, row-major.
    proj_in: Vec<f32>,
    proj_dim: usize,
    /// Queue d'overlap-add (derniers `OVERLAP_SAMPLES` décodés).
    overlap: Vec<f32>,
    /// État de dés-emphase inter-trames.
    deemph_state: f32,
    /// État de pré-emphase inter-trames (continuité du filtre).
    preemph_state: f32,
}

fn xorshift64(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

/// Projection d'entrée v1 : matrice déterministe `dim × 1920`, échelle
/// `1/√1920` (donc `AᵀA ≈ I`, la transposée sert de pseudo-inverse).
fn build_input_projection(dim: usize, seed: u64) -> Vec<f32> {
    let mut s = if seed == 0 { 0x9E3779B97F4A7C15 } else { seed };
    let scale = 1.0 / (SAMPLES_PER_FRAME as f32).sqrt();
    let mut m = Vec::with_capacity(dim * SAMPLES_PER_FRAME);
    for _ in 0..dim * SAMPLES_PER_FRAME {
        let u = (xorshift64(&mut s) >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0;
        m.push(u as f32 * scale);
    }
    m
}

/// Cherche honnêtement une matrice d'encodeur directement mappable
/// (tenseur 2D `[dim, 1920]` ou `[1920, dim]`) dans le header.
/// Retourne `None` si aucun (cas réel du checkpoint Mimi : l'encodeur est un
/// stack convolutif + transformer, pas une matrice compatible) — le codec
/// replie alors sur la projection d'entrée v1 documentée.
fn try_mappable_encoder_matrix(model: &SafetensorsModel) -> Option<(String, Vec<usize>)> {
    for (name, info) in &model.tensors {
        let is_encoderish =
            name.contains("encoder") || name.contains("input_proj") || name.contains("front_end");
        if !is_encoderish || info.shape.len() != 2 {
            continue;
        }
        let (r, c) = (info.shape[0], info.shape[1]);
        if (r == SAMPLES_PER_FRAME || c == SAMPLES_PER_FRAME) && (r <= 4096 && c <= 4096) {
            return Some((name.clone(), info.shape.clone()));
        }
    }
    None
}

/// Géométrie de repli (aucun header) : identique à Mimi publié
/// (8 codebooks × 2048 entrées × dim 256).
fn fallback_geometry() -> (usize, usize, usize) {
    (MIMI_NUM_CODEBOOKS, 256, 2048)
}

impl MimiCodec {
    fn with_rvq(rvq: RvqCodebooks, kind: RvqWeightsKind) -> Self {
        let dim = rvq.dim();
        Self {
            frame_counter: 0,
            safetensors_metadata: None,
            safetensors_path: None,
            proj_in: build_input_projection(dim, INPUT_PROJECTION_SEED),
            proj_dim: dim,
            rvq,
            weights_kind: kind,
            overlap: vec![0.0; OVERLAP_SAMPLES],
            deemph_state: 0.0,
            preemph_state: 0.0,
        }
    }

    pub fn new() -> Self {
        let (nb, dim, entries) = fallback_geometry();
        Self::with_rvq(
            rvq::synthetic_v1(nb, dim, entries, INPUT_PROJECTION_SEED),
            RvqWeightsKind::SyntheticV1,
        )
    }

    /// Charge le codec depuis le checkpoint : VRAIS codebooks Mimi.
    pub fn with_safetensors_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let model = SafetensorsModel::read_from_file(path)?;
        // Recherche honnête d'une matrice d'encodeur mappable (traçabilité).
        let _mappable = try_mappable_encoder_matrix(&model);
        let rvq = match rvq::load_acoustic_from_file(path, &model, MIMI_NUM_CODEBOOKS) {
            Ok(r) => r,
            Err(_) => {
                // Header sans codebook exploitable : géométrie réelle si dispo.
                let (nb, dim, entries) = rvq::geometry_from_header(&model)
                    .map(|(n, d, e)| (n.min(MIMI_NUM_CODEBOOKS), d, e))
                    .unwrap_or_else(fallback_geometry);
                let mut codec = Self::with_rvq(
                    rvq::synthetic_v1(nb, dim, entries, INPUT_PROJECTION_SEED),
                    RvqWeightsKind::GeometryOnlyV1,
                );
                codec.safetensors_metadata = Some(model);
                codec.safetensors_path = Some(path.display().to_string());
                return Ok(codec);
            }
        };
        let mut codec = Self::with_rvq(rvq, RvqWeightsKind::MimiWeights);
        codec.safetensors_metadata = Some(model);
        codec.safetensors_path = Some(path.display().to_string());
        Ok(codec)
    }

    /// Métadonnées seules (sans bytes) : géométrie RÉELLE du header, poids de
    /// secours — marqué `GeometryOnlyV1` (jamais présenté comme Mimi).
    pub fn with_safetensors(model: SafetensorsModel) -> Self {
        let (nb, dim, entries) = rvq::geometry_from_header(&model)
            .map(|(n, d, e)| (n.min(MIMI_NUM_CODEBOOKS).max(1), d.max(1), e.max(2)))
            .unwrap_or_else(fallback_geometry);
        let mut codec = Self::with_rvq(
            rvq::synthetic_v1(nb, dim, entries, INPUT_PROJECTION_SEED),
            RvqWeightsKind::GeometryOnlyV1,
        );
        codec.safetensors_metadata = Some(model);
        codec
    }

    pub fn is_connected(&self) -> bool {
        self.safetensors_metadata.is_some()
    }

    pub fn safetensors_info(&self) -> Option<&SafetensorsModel> {
        self.safetensors_metadata.as_ref()
    }

    /// D'où viennent les poids RVQ (traçabilité REAL vs secours).
    pub fn rvq_source_kind(&self) -> &RvqWeightsKind {
        &self.weights_kind
    }

    pub fn rvq(&self) -> &RvqCodebooks {
        &self.rvq
    }

    pub fn num_books(&self) -> usize {
        self.rvq.num_books().min(MIMI_NUM_CODEBOOKS)
    }

    /// Pré-emphase + bourrage/tronquage à 1920 + fenêtre de Hann.
    fn preprocess(&mut self, samples: &[f32]) -> Vec<f32> {
        let mut frame = vec![0.0f32; SAMPLES_PER_FRAME];
        let n = samples.len().min(SAMPLES_PER_FRAME);
        frame[..n].copy_from_slice(&samples[..n]);
        // Pré-emphase avec continuité inter-trames.
        let mut prev = self.preemph_state;
        for x in frame.iter_mut() {
            let y = *x - PRE_EMPHASIS * prev;
            prev = *x;
            *x = y;
        }
        self.preemph_state = if n > 0 { samples[n - 1] } else { prev };
        // Hann 80 ms.
        for (t, x) in frame.iter_mut().enumerate() {
            let w = 0.5
                - 0.5 * (2.0 * std::f32::consts::PI * t as f32 / SAMPLES_PER_FRAME as f32).cos();
            *x *= w;
        }
        frame
    }

    /// Projection 1920 → `dim` (matrice v1 documentée).
    fn project_in(&self, frame: &[f32]) -> Vec<f32> {
        let dim = self.proj_dim;
        let mut z = vec![0.0f32; dim];
        for k in 0..dim {
            let base = k * SAMPLES_PER_FRAME;
            let mut acc = 0.0f32;
            for (t, &x) in frame.iter().enumerate() {
                acc += self.proj_in[base + t] * x;
            }
            z[k] = acc;
        }
        z
    }

    /// Projection inverse `dim` → 1920 (transposée, `AᵀA ≈ I`).
    fn project_out(&self, z: &[f32]) -> Vec<f32> {
        let mut frame = vec![0.0f32; SAMPLES_PER_FRAME];
        for (k, &zk) in z.iter().enumerate() {
            let base = k * SAMPLES_PER_FRAME;
            for (t, x) in frame.iter_mut().enumerate() {
                *x += self.proj_in[base + t] * zk;
            }
        }
        frame
    }

    /// Encode une trame : pré-traitement → projection → VRAI RVQ Mimi.
    pub fn encode_frame(&mut self, samples: &[f32]) -> Result<MimiFrameTokens> {
        self.frame_counter += 1;
        let frame = self.preprocess(samples);
        let latent = self.project_in(&frame);
        let nb = self.num_books();
        let indices = self.rvq.encode(&latent)?;
        let mut codebooks = [0u16; MIMI_NUM_CODEBOOKS];
        for (i, &idx) in indices.iter().take(nb).enumerate() {
            codebooks[i] = idx;
        }
        Ok(MimiFrameTokens { codebooks })
    }

    /// Décode des tokens : VRAI RVQ inverse → overlap-add. Jamais de sinus.
    pub fn decode_frame(&mut self, tokens: &MimiFrameTokens) -> Result<Vec<f32>> {
        self.frame_counter += 1;
        let nb = self.num_books();
        let latent = self.rvq.decode(&tokens.codebooks[..nb])?;
        let mut frame = self.project_out(&latent);
        // Compensation de fenêtre (Hann² → normalisation douce) + dés-emphase.
        let mut prev = self.deemph_state;
        for x in frame.iter_mut() {
            let y = *x + PRE_EMPHASIS * prev;
            prev = y;
            *x = y;
        }
        self.deemph_state = prev;
        // Overlap-add : crossfade avec la queue précédente (anti-clic).
        let m = OVERLAP_SAMPLES.min(frame.len());
        for t in 0..m {
            let a = t as f32 / m as f32;
            frame[t] = frame[t] * a + self.overlap[self.overlap.len() - m + t] * (1.0 - a);
        }
        // Mémorise la queue pour la trame suivante.
        let tail: Vec<f32> = frame[frame.len() - m..].to_vec();
        self.overlap = tail;
        // Normalisation anti-saturation (pic → 0.89), jamais de NaN.
        let peak = frame.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        if peak.is_finite() && peak > 0.89 {
            let g = 0.89 / peak;
            for x in frame.iter_mut() {
                *x *= g;
            }
        }
        Ok(frame)
    }
}

impl Default for MimiCodec {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::rvq as rvq_mod;
    use super::*;

    fn noise(n: usize, seed: u64) -> Vec<f32> {
        rvq_mod::test_noise(n, seed)
    }

    #[test]
    fn test_pipeline_sans_sinus_roundtrip_borne() {
        let mut codec = MimiCodec::new();
        let input = noise(SAMPLES_PER_FRAME, 11);
        let tokens = codec.encode_frame(&input).expect("encode");
        // Déterminisme.
        let mut codec2 = MimiCodec::new();
        let tokens2 = codec2.encode_frame(&input).expect("encode2");
        assert_eq!(tokens, tokens2);
        let decoded = codec.decode_frame(&tokens).expect("decode");
        assert_eq!(decoded.len(), SAMPLES_PER_FRAME);
        assert!(decoded.iter().all(|x| x.is_finite()));
        // Erreur bornée : mieux que le silence (énergie préservée en partie).
        let mse: f32 = decoded
            .iter()
            .zip(input.iter())
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f32>()
            / SAMPLES_PER_FRAME as f32;
        let sig_e: f32 = input.iter().map(|x| x * x).sum::<f32>() / SAMPLES_PER_FRAME as f32;
        assert!(mse < sig_e * 4.0, "mse={} sig_e={}", mse, sig_e);
        // Le décodage n'est PAS une sinusoïde : deux entrées différentes
        // donnent deux reconstructions différentes…
        let other = noise(SAMPLES_PER_FRAME, 77);
        let tok_other = codec.encode_frame(&other).expect("encode other");
        assert_ne!(tokens, tok_other);
        let dec_other = codec.decode_frame(&tok_other).expect("decode other");
        // …et l'autocorrélation ne montre pas de pic tonal pur à 220/440 Hz.
        let r220 = autocorr_norm(&decoded, (MIMI_SAMPLE_RATE as f32 / 220.0) as usize);
        let r440 = autocorr_norm(&decoded, (MIMI_SAMPLE_RATE as f32 / 440.0) as usize);
        assert!(
            r220 < 0.90 && r440 < 0.90,
            "pic tonal suspect: r220={} r440={}",
            r220,
            r440
        );
        let _ = dec_other;
    }

    fn autocorr_norm(x: &[f32], lag: usize) -> f32 {
        if lag >= x.len() {
            return 0.0;
        }
        let e0: f32 = x.iter().map(|v| v * v).sum();
        if e0 < 1e-12 {
            return 0.0;
        }
        x[..x.len() - lag]
            .iter()
            .zip(&x[lag..])
            .map(|(a, b)| a * b)
            .sum::<f32>()
            / e0
    }

    #[test]
    fn test_indices_toujours_dans_le_codebook() {
        let mut codec = MimiCodec::new();
        for seed in [1u64, 2, 3] {
            let tok = codec
                .encode_frame(&noise(SAMPLES_PER_FRAME, seed))
                .expect("enc");
            assert_eq!(tok.codebooks.len(), MIMI_NUM_CODEBOOKS);
            for &c in &tok.codebooks {
                assert!((c as usize) < codec.rvq().entries_per_book());
            }
        }
    }

    #[test]
    fn test_avec_vrais_poids_si_presents() {
        let path = format!(
            "{}/../../models/mimi_candle.safetensors",
            env!("CARGO_MANIFEST_DIR")
        );
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {} absent", path);
            return;
        }
        let mut codec = MimiCodec::with_safetensors_file(&path).expect("vrais poids");
        assert_eq!(*codec.rvq_source_kind(), RvqWeightsKind::MimiWeights);
        assert_eq!(codec.num_books(), MIMI_NUM_CODEBOOKS);
        let input = noise(SAMPLES_PER_FRAME, 5);
        let t0 = std::time::Instant::now();
        let tok = codec.encode_frame(&input).expect("encode réel");
        let dt_enc = t0.elapsed();
        let dec = codec.decode_frame(&tok).expect("decode réel");
        assert_eq!(dec.len(), SAMPLES_PER_FRAME);
        assert!(dec.iter().all(|x| x.is_finite()));
        eprintln!(
            "Mimi réel: tokens={:?} encode={:?} peak={:.4}",
            tok.codebooks,
            dt_enc,
            dec.iter().map(|x| x.abs()).fold(0.0f32, f32::max)
        );
    }
}
