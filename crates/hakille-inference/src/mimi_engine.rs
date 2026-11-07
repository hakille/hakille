//! Codec neuronal Mimi CPU : SEANet, transformers causaux et quantification
//! résiduelle du checkpoint Kyutai. Aucun poids synthétique de secours.
//!
//! Une instance représente un flux mono 24 kHz. L'état encodeur et décodeur
//! est conservé entre les trames de 80 ms. Utiliser une instance distincte par
//! locuteur/source pour ne pas mélanger les historiques acoustiques.

use candle_core::{DType, Device, StreamTensor, Tensor};
use candle_transformers::models::mimi::{Config, Model};
use hakille_core::{HakilleError, Result};
use std::path::Path;

pub const MIMI_NEURAL_SAMPLE_RATE: u32 = 24_000;
pub const MIMI_NEURAL_FRAME_SAMPLES: usize = 1_920;
pub const MIMI_NEURAL_CODEBOOKS: usize = 8;
pub const MIMI_NEURAL_BINS: u16 = 2_048;

/// Encodeur/décodeur neuronal chargé depuis un vrai checkpoint compatible
/// Candle/Kyutai. Les poids sont partagés par les clones, pas les états de flux.
pub struct MimiEngine {
    model: Model,
    pristine: Model,
    poisoned: bool,
    encoded_frames: u64,
    decoded_frames: u64,
}

fn failure(message: impl std::fmt::Display) -> HakilleError {
    HakilleError::InferenceError(format!("Mimi neuronal : {message}"))
}

fn validate_samples(samples: &[f32]) -> Result<()> {
    if samples.len() != MIMI_NEURAL_FRAME_SAMPLES {
        return Err(failure(format!(
            "une trame doit contenir exactement {MIMI_NEURAL_FRAME_SAMPLES} échantillons mono à 24 kHz"
        )));
    }
    if samples
        .iter()
        .any(|value| !value.is_finite() || value.abs() > 1.0)
    {
        return Err(failure("PCM attendu fini et normalisé dans [-1, 1]"));
    }
    Ok(())
}

fn validate_codes(codes: &[u16; MIMI_NEURAL_CODEBOOKS]) -> Result<()> {
    if codes.iter().any(|&code| code >= MIMI_NEURAL_BINS) {
        return Err(failure("indice de codebook hors de [0, 2047]"));
    }
    Ok(())
}

impl MimiEngine {
    /// Charge les poids complets, jamais uniquement le header ou les codebooks.
    /// Un fichier absent, corrompu ou incompatible est une erreur bloquante.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let bytes =
            std::fs::read(path).map_err(|e| failure(format!("lecture du checkpoint : {e}")))?;
        let device = Device::Cpu;
        let vb = candle_nn::VarBuilder::from_buffered_safetensors(bytes, DType::F32, &device)
            .map_err(failure)?;
        let model = Model::new(Config::v0_1(Some(MIMI_NEURAL_CODEBOOKS)), vb).map_err(failure)?;
        Ok(Self {
            pristine: model.clone(),
            model,
            poisoned: false,
            encoded_frames: 0,
            decoded_frames: 0,
        })
    }

    /// Nouveau flux indépendant utilisant les mêmes poids immuables en mémoire.
    pub fn fresh_stream(&self) -> Self {
        Self {
            model: self.pristine.clone(),
            pristine: self.pristine.clone(),
            poisoned: false,
            encoded_frames: 0,
            decoded_frames: 0,
        }
    }

    /// Réinitialise aussi les convolutions de rééchantillonnage. Restaurer le
    /// modèle vierge évite de dépendre d'un reset partiel du runtime amont.
    pub fn reset(&mut self) {
        self.model = self.pristine.clone();
        self.poisoned = false;
        self.encoded_frames = 0;
        self.decoded_frames = 0;
    }

    pub fn encoded_frames(&self) -> u64 {
        self.encoded_frames
    }
    pub fn decoded_frames(&self) -> u64 {
        self.decoded_frames
    }

    fn check_state(&self) -> Result<()> {
        if self.poisoned {
            Err(failure(
                "état de flux invalide après échec ; appeler reset avant de continuer",
            ))
        } else {
            Ok(())
        }
    }

    pub fn encode_frame(&mut self, samples: &[f32]) -> Result<[u16; MIMI_NEURAL_CODEBOOKS]> {
        self.check_state()?;
        validate_samples(samples)?;
        let result = (|| -> Result<_> {
            let tensor = Tensor::from_slice(samples, (1, 1, samples.len()), &Device::Cpu)
                .map_err(failure)?;
            let output = self
                .model
                .encode_step(&StreamTensor::from_tensor(tensor))
                .map_err(failure)?;
            let codes = output
                .as_option()
                .ok_or_else(|| failure("aucun token produit pour une trame complète"))?;
            if codes.dims() != [1, MIMI_NEURAL_CODEBOOKS, 1] {
                return Err(failure(format!(
                    "forme des tokens inattendue : {:?}",
                    codes.dims()
                )));
            }
            let values = codes
                .flatten_all()
                .and_then(|v| v.to_vec1::<u32>())
                .map_err(failure)?;
            let mut frame = [0u16; MIMI_NEURAL_CODEBOOKS];
            for (target, value) in frame.iter_mut().zip(values) {
                *target =
                    u16::try_from(value).map_err(|_| failure("indice de codebook trop grand"))?;
            }
            validate_codes(&frame)?;
            Ok(frame)
        })();
        match result {
            Ok(codes) => {
                self.encoded_frames += 1;
                Ok(codes)
            }
            Err(error) => {
                self.poisoned = true;
                Err(error)
            }
        }
    }

    pub fn decode_frame(&mut self, codes: &[u16; MIMI_NEURAL_CODEBOOKS]) -> Result<Vec<f32>> {
        self.check_state()?;
        validate_codes(codes)?;
        let result = (|| -> Result<_> {
            let codes: Vec<u32> = codes.iter().map(|&value| u32::from(value)).collect();
            let tensor = Tensor::from_vec(codes, (1, MIMI_NEURAL_CODEBOOKS, 1), &Device::Cpu)
                .map_err(failure)?;
            let output = self
                .model
                .decode_step(&StreamTensor::from_tensor(tensor))
                .map_err(failure)?;
            let audio = output
                .as_option()
                .ok_or_else(|| failure("aucun audio produit pour les tokens"))?;
            if audio.dims() != [1, 1, MIMI_NEURAL_FRAME_SAMPLES] {
                return Err(failure(format!(
                    "forme audio inattendue : {:?}",
                    audio.dims()
                )));
            }
            let samples = audio
                .flatten_all()
                .and_then(|v| v.to_vec1::<f32>())
                .map_err(failure)?;
            if samples.iter().any(|value| !value.is_finite()) {
                return Err(failure("sortie neuronale non finie"));
            }
            Ok(samples)
        })();
        match result {
            Ok(samples) => {
                self.decoded_frames += 1;
                Ok(samples)
            }
            Err(error) => {
                self.poisoned = true;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_pcm_frame_contract_without_loading_weights() {
        assert!(validate_samples(&vec![0.0; MIMI_NEURAL_FRAME_SAMPLES]).is_ok());
        assert!(validate_samples(&[]).is_err());
        assert!(validate_samples(&vec![0.0; MIMI_NEURAL_FRAME_SAMPLES + 1]).is_err());
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.01, -1.01] {
            let mut frame = vec![0.0; MIMI_NEURAL_FRAME_SAMPLES];
            frame[17] = invalid;
            assert!(validate_samples(&frame).is_err());
        }
    }

    #[test]
    fn validates_all_token_indices() {
        assert!(validate_codes(&[MIMI_NEURAL_BINS - 1; MIMI_NEURAL_CODEBOOKS]).is_ok());
        for index in 0..MIMI_NEURAL_CODEBOOKS {
            let mut frame = [0; MIMI_NEURAL_CODEBOOKS];
            frame[index] = MIMI_NEURAL_BINS;
            assert!(validate_codes(&frame).is_err());
        }
    }

    #[test]
    fn absent_checkpoint_never_creates_a_synthetic_codec() {
        assert!(MimiEngine::load("/nonexistent/hakille-mimi-checkpoint.safetensors").is_err());
    }

    #[test]
    #[ignore = "nécessite le checkpoint Mimi complet (~385 Mo), forward CPU réel"]
    fn neural_mimi_stream_roundtrip_and_reset() {
        let path = std::env::var_os("HAKILLE_MODELS_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models"))
            .join("mimi_candle.safetensors");
        let mut engine = MimiEngine::load(path).expect("checkpoint Mimi réel requis");
        let input: Vec<f32> = (0..MIMI_NEURAL_FRAME_SAMPLES)
            .map(|i| 0.2 * (i as f32 * 440.0 * std::f32::consts::TAU / 24_000.0).sin())
            .collect();
        let first = engine.encode_frame(&input).expect("encode neuronal");
        let first_audio = engine.decode_frame(&first).expect("décodeur neuronal");
        assert_eq!(first_audio.len(), MIMI_NEURAL_FRAME_SAMPLES);
        assert!(first_audio.iter().all(|v| v.is_finite()));
        let energy: f32 = first_audio.iter().map(|v| v * v).sum();
        assert!(
            energy > 1e-9,
            "le décodeur réel ne doit pas rendre uniquement du silence"
        );
        for _ in 0..2 {
            let codes = engine.encode_frame(&input).unwrap();
            assert_eq!(
                engine.decode_frame(&codes).unwrap().len(),
                MIMI_NEURAL_FRAME_SAMPLES
            );
        }
        assert_eq!(engine.encoded_frames(), 3);
        assert_eq!(engine.decoded_frames(), 3);
        // Les rejets de données avant forward ne modifient pas l'état.
        assert!(
            engine
                .encode_frame(&[f32::NAN; MIMI_NEURAL_FRAME_SAMPLES])
                .is_err()
        );
        assert!(
            engine
                .decode_frame(&[MIMI_NEURAL_BINS; MIMI_NEURAL_CODEBOOKS])
                .is_err()
        );
        assert_eq!(engine.encoded_frames(), 3);
        engine.reset();
        assert_eq!(engine.encoded_frames(), 0);
        assert_eq!(engine.encode_frame(&input).unwrap(), first);
        let reset_audio = engine.decode_frame(&first).unwrap();
        assert_eq!(reset_audio, first_audio);
        let mut independent = engine.fresh_stream();
        assert_eq!(independent.encode_frame(&input).unwrap(), first);
        eprintln!(
            "Mimi neuronal réel : 3 trames encodées/décodées, 1920 PCM/trame, énergie={energy:.6}, reset déterministe"
        );
    }
}
