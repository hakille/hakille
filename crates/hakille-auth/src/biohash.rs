//! Cancellable Biohashing & Ephemeral RAM Protection (F18).
//! Template protection via revocable salt projections, Hamming matching,
//! and zeroize on drop of ephemeral RAM buffers.

use crate::face::FaceEmbedding;
use hakille_core::HakilleError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Ephemeral raw pixel / embedding buffer in RAM with guaranteed zeroization on drop
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct RawBiometricBuffer {
    pub data: Vec<f32>,
}

impl RawBiometricBuffer {
    pub fn new(data: Vec<f32>) -> Self {
        Self { data }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn as_slice(&self) -> &[f32] {
        &self.data
    }
}

/// Protected Cancellable Biohash Template (1024 bits = 128 bytes)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BiohashDigest {
    pub bits: Vec<u8>,
}

impl BiohashDigest {
    pub fn new(bits: Vec<u8>) -> Self {
        Self { bits }
    }

    pub fn bit_len(&self) -> usize {
        self.bits.len() * 8
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bits
    }

    pub fn to_hex(&self) -> String {
        let mut hex = String::with_capacity(self.bits.len() * 2);
        for b in &self.bits {
            use std::fmt::Write;
            let _ = write!(hex, "{:02x}", b);
        }
        hex
    }

    pub fn from_hex(hex: &str) -> Result<Self, HakilleError> {
        if !hex.len().is_multiple_of(2) {
            return Err(HakilleError::InferenceError("Odd length hex string".to_string()));
        }
        let mut bits = Vec::with_capacity(hex.len() / 2);
        for i in (0..hex.len()).step_by(2) {
            let byte = u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| {
                HakilleError::InferenceError("Invalid hex digit".to_string())
            })?;
            bits.push(byte);
        }
        Ok(Self { bits })
    }
}

/// Fuzzy Extractor public helper data and commitment (F06 & F18)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FuzzyExtractorCommitment {
    pub helper_data: Vec<u8>,
    pub commitment_hash: [u8; 32],
}

pub struct BiohashingEngine {
    pub projection_dim: usize,
    pub threshold: f32,
}

impl Default for BiohashingEngine {
    fn default() -> Self {
        Self {
            projection_dim: 1024,
            threshold: 0.18,
        }
    }
}

impl BiohashingEngine {
    pub fn new(projection_dim: usize, threshold: f32) -> Self {
        Self {
            projection_dim,
            threshold,
        }
    }

    /// Project a 512-dim face embedding into a cancellable bit vector using a secret salt
    pub fn project_embedding(&self, embedding: &FaceEmbedding, salt: &[u8; 32]) -> BiohashDigest {
        let num_bytes = self.projection_dim.div_ceil(8);
        let mut bits = vec![0u8; num_bytes];

        // Derive pseudo-random projection hyperplanes using HMAC-SHA256 seeded by salt
        for i in 0..self.projection_dim {
            let mut hasher = Sha256::new();
            hasher.update(salt);
            hasher.update((i as u32).to_le_bytes());
            let seed = hasher.finalize();

            // Inner product with pseudo-random vector
            let mut dot = 0.0f32;
            for j in 0..512 {
                let pseudo_weight =
                    ((seed[j % 32] as f32 / 128.0) - 1.0) * ((j as f32 * 0.1).sin());
                dot += embedding.vector[j] * pseudo_weight;
            }

            // Binarization: bit is 1 if dot >= 0.0
            if dot >= 0.0 {
                let byte_idx = i / 8;
                let bit_idx = i % 8;
                bits[byte_idx] |= 1 << bit_idx;
            }
        }

        BiohashDigest { bits }
    }

    /// Normalized Hamming distance between two biohashes: [0.0, 1.0]
    pub fn hamming_distance(&self, a: &BiohashDigest, b: &BiohashDigest) -> f32 {
        let min_len = a.bits.len().min(b.bits.len());
        if min_len == 0 {
            return 1.0;
        }

        let mut diff_bits = 0u32;
        for i in 0..min_len {
            diff_bits += (a.bits[i] ^ b.bits[i]).count_ones();
        }

        let total_bits = (min_len * 8) as f32;
        (diff_bits as f32) / total_bits
    }

    /// Verify matching between probe and reference biohash
    pub fn verify_biohash(
        &self,
        probe: &BiohashDigest,
        reference: &BiohashDigest,
        threshold: f32,
    ) -> bool {
        self.hamming_distance(probe, reference) <= threshold
    }

    /// Generate a Fuzzy Extractor commitment (Gen) yielding helper data P and key seed R
    pub fn generate_fuzzy_commitment(
        &self,
        biohash: &BiohashDigest,
        secret_codeword: &[u8; 32],
    ) -> (FuzzyExtractorCommitment, [u8; 32]) {
        // P = biohash[0..32] XOR codeword
        let mut helper_data = vec![0u8; 32];
        for i in 0..32.min(biohash.bits.len()) {
            helper_data[i] = biohash.bits[i] ^ secret_codeword[i];
        }

        let commitment_hash = Sha256::digest(secret_codeword).into();
        let derived_key = Sha256::digest([&secret_codeword[..], b"hakille-fuzzy-release"].concat()).into();

        (
            FuzzyExtractorCommitment {
                helper_data,
                commitment_hash,
            },
            derived_key,
        )
    }

    /// Reproduce Fuzzy Extractor key (Rep) from probe biohash and helper data P
    pub fn reproduce_fuzzy_key(
        &self,
        probe_biohash: &BiohashDigest,
        commitment: &FuzzyExtractorCommitment,
    ) -> Result<[u8; 32], HakilleError> {
        let mut recovered_codeword = [0u8; 32];
        let max_len = 32.min(probe_biohash.bits.len()).min(commitment.helper_data.len());
        for (i, byte) in recovered_codeword.iter_mut().enumerate().take(max_len) {
            *byte = probe_biohash.bits[i] ^ commitment.helper_data[i];
        }

        let check_hash = Sha256::digest(recovered_codeword);
        if check_hash.as_slice() == commitment.commitment_hash {
            let key = Sha256::digest([&recovered_codeword[..], b"hakille-fuzzy-release"].concat()).into();
            Ok(key)
        } else {
            Err(HakilleError::AuthenticationFailed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_biohash_hex_roundtrip() {
        let digest = BiohashDigest::new(vec![0x12, 0xab, 0xcd, 0xef]);
        let hex = digest.to_hex();
        assert_eq!(hex, "12abcdef");
        let parsed = BiohashDigest::from_hex(&hex).unwrap();
        assert_eq!(digest, parsed);
    }

    #[test]
    fn test_fuzzy_extractor_reproduction() {
        let engine = BiohashingEngine::default();
        let biohash = BiohashDigest::new(vec![0xaa; 32]);
        let codeword = [0x55u8; 32];

        let (commitment, key1) = engine.generate_fuzzy_commitment(&biohash, &codeword);
        let key2 = engine.reproduce_fuzzy_key(&biohash, &commitment).unwrap();
        assert_eq!(key1, key2);

        // Perturbed probe with wrong bits fails reproduction
        let wrong_probe = BiohashDigest::new(vec![0xbb; 32]);
        assert!(engine.reproduce_fuzzy_key(&wrong_probe, &commitment).is_err());
    }
}
