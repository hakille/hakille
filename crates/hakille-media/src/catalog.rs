//! Catalogue honnête des modèles génératifs locaux (Image, Vidéo, Audio).
//!
//! Contrat d'honnêteté : aucun SHA-256 factice n'est inventé ici. Les artefacts
//! sont localisés sous `<models_dir>/generative/<model_id>/` et vérifiés
//! dynamiquement avant chargement ; si un fichier manque, le moteur rapporte
//! [`crate::types::EngineAvailability::ModelMissing`].

use crate::types::{EngineAvailability, EngineKind, MediaModality};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Spécification d'un modèle génératif local supporté par `hakille-media`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerativeModelProfile {
    pub id: String,
    pub name: String,
    pub modality: MediaModality,
    pub engine_kind: EngineKind,
    /// Sous-répertoire relatif dans `<models_dir>/generative/`.
    pub subdir: String,
    /// Fichiers obligatoires pour exécuter un forward réel.
    pub required_files: Vec<String>,
    pub expected_ram_mb: u64,
    pub expected_vram_mb: u64,
    pub source_repo: String,
    pub description: String,
}

impl GenerativeModelProfile {
    pub fn model_dir(&self, models_dir: &Path) -> PathBuf {
        models_dir.join("generative").join(&self.subdir)
    }

    /// Vérifie la présence réelle sur disque de tous les artefacts requis.
    pub fn probe_availability(&self, models_dir: &Path) -> EngineAvailability {
        let dir = self.model_dir(models_dir);
        let missing: Vec<String> = self
            .required_files
            .iter()
            .filter(|f| !dir.join(f).is_file())
            .cloned()
            .collect();
        if missing.is_empty() {
            let primary_sha = self
                .required_files
                .first()
                .and_then(|f| sha256_file(&dir.join(f)).ok());
            EngineAvailability::Ready {
                device: "CPU (Rust natif)".to_string(),
                model_sha256: primary_sha,
            }
        } else {
            EngineAvailability::ModelMissing {
                model_id: self.id.clone(),
                missing_files: missing,
                hint: format!(
                    "Placer les poids vérifiés de {} dans {}",
                    self.source_repo,
                    dir.display()
                ),
            }
        }
    }
}

pub struct MediaModelCatalog;

impl MediaModelCatalog {
    pub fn profiles() -> Vec<GenerativeModelProfile> {
        vec![
            GenerativeModelProfile {
                id: "sd-1.5-candle".into(),
                name: "Stable Diffusion 1.5 (Candle Pur Rust)".into(),
                modality: MediaModality::Image,
                engine_kind: EngineKind::NeuralLocalCandle,
                subdir: "sd-1.5".into(),
                required_files: vec![
                    "clip.safetensors".into(),
                    "unet.safetensors".into(),
                    "vae.safetensors".into(),
                    "tokenizer.json".into(),
                ],
                expected_ram_mb: 3_500,
                expected_vram_mb: 3_200,
                source_repo: "runwayml/stable-diffusion-v1-5".into(),
                description: "Diffusion latente 2D complète en Rust pur (CLIP + UNet2D + AutoEncoderKL + DDIM/Euler-A/UniPC), T2I & I2I.".into(),
            },
            GenerativeModelProfile {
                id: "sd-turbo-candle".into(),
                name: "Stable Diffusion Turbo (Candle Pur Rust)".into(),
                modality: MediaModality::Image,
                engine_kind: EngineKind::NeuralLocalCandle,
                subdir: "sd-turbo".into(),
                required_files: vec![
                    "clip.safetensors".into(),
                    "unet.safetensors".into(),
                    "vae.safetensors".into(),
                    "tokenizer.json".into(),
                ],
                expected_ram_mb: 3_600,
                expected_vram_mb: 3_400,
                source_repo: "stabilityai/sd-turbo".into(),
                description: "Diffusion distillée 1 à 4 steps en Rust pur via Candle (idéal CPU/Natif).".into(),
            },
            GenerativeModelProfile {
                id: "flux-schnell-candle".into(),
                name: "FLUX.1 Schnell (Candle Pur Rust)".into(),
                modality: MediaModality::Image,
                engine_kind: EngineKind::NeuralLocalCandle,
                subdir: "flux-schnell".into(),
                required_files: vec![
                    "flux.safetensors".into(),
                    "ae.safetensors".into(),
                    "t5.safetensors".into(),
                    "t5_tokenizer.json".into(),
                    "clip.safetensors".into(),
                    "clip_tokenizer.json".into(),
                ],
                expected_ram_mb: 16_000,
                expected_vram_mb: 14_000,
                source_repo: "black-forest-labs/FLUX.1-schnell".into(),
                description: "Rectified Flow Transformer 12B (Tier Souverain) exécuté nativement via candle_transformers::models::flux.".into(),
            },
            GenerativeModelProfile {
                id: "onnx-diffusion".into(),
                name: "Diffusion Latente ONNX Runtime (ort)".into(),
                modality: MediaModality::Image,
                engine_kind: EngineKind::NeuralLocalOnnx,
                subdir: "onnx-diffusion".into(),
                required_files: vec![
                    "text_encoder.onnx".into(),
                    "unet.onnx".into(),
                    "vae_decoder.onnx".into(),
                    "tokenizer.json".into(),
                ],
                expected_ram_mb: 2_400,
                expected_vram_mb: 0,
                source_repo: "checkpoint ONNX exporté (SDXS / LCM / SD-Turbo ONNX)".into(),
                description: "Pipeline de diffusion générique ONNX Runtime CPU/GPU (INT8/FP16/FP32).".into(),
            },
            GenerativeModelProfile {
                id: "parler-tts-candle".into(),
                name: "Parler-TTS + DAC (Candle Pur Rust)".into(),
                modality: MediaModality::Audio,
                engine_kind: EngineKind::NeuralLocalCandle,
                subdir: "parler-tts".into(),
                required_files: vec![
                    "model.safetensors".into(),
                    "config.json".into(),
                    "tokenizer.json".into(),
                ],
                expected_ram_mb: 2_200,
                expected_vram_mb: 1_800,
                source_repo: "parler-tts/parler-tts-mini-v1".into(),
                description: "Synthèse vocale neurale contrôlée par description stylistique (T5 + décodeur multi-codebooks + codec DAC).".into(),
            },
            GenerativeModelProfile {
                id: "onnx-tts".into(),
                name: "TTS Neural ONNX (Kokoro / VITS / Piper)".into(),
                modality: MediaModality::Audio,
                engine_kind: EngineKind::NeuralLocalOnnx,
                subdir: "onnx-tts".into(),
                required_files: vec!["model.onnx".into(), "vocab.json".into()],
                expected_ram_mb: 350,
                expected_vram_mb: 0,
                source_repo: "checkpoint TTS ONNX (ex. Kokoro-82M / Piper ONNX)".into(),
                description: "Synthèse vocale neurale rapide CPU via ONNX Runtime (`ort`).".into(),
            },
            GenerativeModelProfile {
                id: "onnx-video".into(),
                name: "Vidéo Diffusion Spatio-Temporelle ONNX".into(),
                modality: MediaModality::Video,
                engine_kind: EngineKind::NeuralLocalOnnx,
                subdir: "onnx-video".into(),
                required_files: vec![
                    "video_unet.onnx".into(),
                    "vae_decoder.onnx".into(),
                    "text_encoder.onnx".into(),
                    "tokenizer.json".into(),
                ],
                expected_ram_mb: 6_000,
                expected_vram_mb: 4_000,
                source_repo: "checkpoint vidéo diffusion ONNX (AnimateDiff / LTX / Wan ONNX)".into(),
                description: "Génération vidéo neurale spatio-temporelle `[B, C, T, H, W]` via ONNX Runtime.".into(),
            },
        ]
    }

    pub fn by_id(id: &str) -> Option<GenerativeModelProfile> {
        Self::profiles().into_iter().find(|p| p.id == id)
    }
}

/// Calcule le SHA-256 hexadécimal d'un fichier en lecture streamée (bornée en RAM).
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex_encode(&digest)
}

pub fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

pub fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    for pair in bytes.as_chunks::<2>().0 {
        let s = std::str::from_utf8(pair).ok()?;
        let val = u8::from_str_radix(s, 16).ok()?;
        out.push(val);
    }
    Some(out)
}
