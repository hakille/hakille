use crate::crypto::error::CryptoError;
use crate::crypto::kdf::MasterKey;
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Number of bits in the secret key R (256 bits = 32 bytes)
pub const SECRET_BITS: usize = 256;
/// Repetition factor per secret bit for noise tolerance
pub const REPETITION_FACTOR: usize = 31;
/// Total number of bits in the error-correcting codeword (7936 bits = 992 bytes)
pub const CODEWORD_BITS: usize = SECRET_BITS * REPETITION_FACTOR;
pub const CODEWORD_BYTES: usize = CODEWORD_BITS / 8;

/// Persistent record of biometric commitment and helper data (P, salt, C)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BiometricCommitmentRecord {
    /// Public helper data P = w XOR C_R
    pub helper_data: Vec<u8>,
    /// Keystore salt used in commitment hash
    pub salt: [u8; 32],
    /// Cryptographic commitment C = SHA-256(R || Salt)
    pub commitment: [u8; 32],
}

/// Helper Data struct for code-offset fuzzy extractor
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelperData {
    pub syndrome: Vec<u8>,
    pub salt: [u8; 32],
}

/// Commitment hash wrapper
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitmentHash(pub [u8; 32]);

/// Code-Offset Fuzzy Extractor with interleaved repetition error correction
pub struct FuzzyExtractor {
    /// Allowable noise threshold (e.g. 0.20 for 20% error tolerance)
    pub noise_threshold: f32,
}

impl Default for FuzzyExtractor {
    fn default() -> Self {
        Self {
            noise_threshold: 0.20,
        }
    }
}

impl FuzzyExtractor {
    pub fn new(noise_threshold: f32) -> Self {
        Self { noise_threshold }
    }

    /// Quantize a continuous biometric embedding into discrete binary representation
    /// using pseudo-random hyperplane projections (Locality-Sensitive Hashing).
    pub fn quantize(embedding: &[f32]) -> Vec<u8> {
        let mut bits = vec![0u8; CODEWORD_BYTES];
        if embedding.is_empty() {
            return bits;
        }

        let dim = embedding.len();
        // Deterministic PRNG seed for reproducible hyperplane projection
        let mut seed = [0u8; 32];
        let mut hasher = Sha256::new();
        hasher.update(b"hakille:fuzzy:hyperplanes:v1:");
        hasher.update(dim.to_le_bytes());
        seed.copy_from_slice(&hasher.finalize());

        for bit_idx in 0..CODEWORD_BITS {
            // Compute projection onto hyperplane bit_idx
            let mut dot_product = 0.0f32;
            let num_chunks = (dim + 31) / 32;

            for chunk in 0..num_chunks {
                let mut h_hasher = Sha256::new();
                h_hasher.update(seed);
                h_hasher.update((bit_idx as u32).to_le_bytes());
                h_hasher.update((chunk as u32).to_le_bytes());
                let prn = h_hasher.finalize();

                let start_d = chunk * 32;
                let end_d = (start_d + 32).min(dim);
                for d in start_d..end_d {
                    let byte_val = prn[d - start_d];
                    let weight = (byte_val as f32 - 128.0) / 128.0;
                    dot_product += embedding[d] * weight;
                }
            }

            if dot_product >= 0.0 {
                let byte_pos = bit_idx / 8;
                let bit_pos = bit_idx % 8;
                bits[byte_pos] |= 1 << bit_pos;
            }
        }

        bits
    }

    /// Calculate Hamming distance between two binary byte slices
    pub fn hamming_distance(a: &[u8], b: &[u8]) -> usize {
        a.iter()
            .zip(b.iter())
            .map(|(&x, &y)| (x ^ y).count_ones() as usize)
            .sum()
    }

    /// Encode 32-byte secret R into interleaved repetition codeword
    fn encode_secret(r: &[u8; 32]) -> Vec<u8> {
        let mut codeword = vec![0u8; CODEWORD_BYTES];
        for secret_bit_idx in 0..SECRET_BITS {
            let secret_byte = secret_bit_idx / 8;
            let secret_bit = (r[secret_byte] >> (secret_bit_idx % 8)) & 1;

            if secret_bit == 1 {
                for rep in 0..REPETITION_FACTOR {
                    let code_bit_idx = rep * SECRET_BITS + secret_bit_idx;
                    codeword[code_bit_idx / 8] |= 1 << (code_bit_idx % 8);
                }
            }
        }
        codeword
    }

    /// Decode candidate codeword via majority voting across replicas
    fn decode_codeword(codeword: &[u8]) -> [u8; 32] {
        let mut r = [0u8; 32];
        for secret_bit_idx in 0..SECRET_BITS {
            let mut ones_count = 0;
            for rep in 0..REPETITION_FACTOR {
                let code_bit_idx = rep * SECRET_BITS + secret_bit_idx;
                let bit = (codeword[code_bit_idx / 8] >> (code_bit_idx % 8)) & 1;
                ones_count += bit as usize;
            }

            // Majority voting
            if ones_count > REPETITION_FACTOR / 2 {
                let secret_byte = secret_bit_idx / 8;
                let secret_bit = secret_bit_idx % 8;
                r[secret_byte] |= 1 << secret_bit;
            }
        }
        r
    }

    /// Gen(B, Salt) -> (MasterKey, BiometricCommitmentRecord)
    pub fn generate_commitment(
        &self,
        biometric: &[f32],
        salt: &[u8; 32],
    ) -> Result<(MasterKey, BiometricCommitmentRecord), CryptoError> {
        let w = Self::quantize(biometric);

        // Generate uniformly random 32-byte secret R
        let mut r_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut r_bytes);

        // Encode R into error-correcting codeword C_R
        let c_r = Self::encode_secret(&r_bytes);

        // Compute helper data P = w XOR C_R
        let mut helper_data = vec![0u8; CODEWORD_BYTES];
        for i in 0..CODEWORD_BYTES {
            helper_data[i] = w[i] ^ c_r[i];
        }

        // Compute cryptographic commitment C = SHA-256(R || Salt)
        let mut hasher = Sha256::new();
        hasher.update(r_bytes);
        hasher.update(salt);
        let mut commitment = [0u8; 32];
        commitment.copy_from_slice(&hasher.finalize());

        let record = BiometricCommitmentRecord {
            helper_data,
            salt: *salt,
            commitment,
        };

        Ok((MasterKey::new(r_bytes), record))
    }

    /// Rep(B', Record) -> MasterKey
    pub fn reproduce_key(
        &self,
        probe: &[f32],
        record: &BiometricCommitmentRecord,
    ) -> Result<MasterKey, CryptoError> {
        if record.helper_data.len() != CODEWORD_BYTES {
            return Err(CryptoError::BiometricQuantizationError(
                "Helper data length mismatch".to_string(),
            ));
        }

        let w_prime = Self::quantize(probe);
        self.reproduce_from_bits(&w_prime, record)
    }

    /// Reproduce secret from discrete quantized bit vector w'
    pub fn reproduce_from_bits(
        &self,
        w_prime: &[u8],
        record: &BiometricCommitmentRecord,
    ) -> Result<MasterKey, CryptoError> {
        if w_prime.len() != CODEWORD_BYTES || record.helper_data.len() != CODEWORD_BYTES {
            return Err(CryptoError::BiometricQuantizationError(
                "Invalid bit vector length for fuzzy reproduction".to_string(),
            ));
        }

        // C'_R = w' XOR P
        let mut candidate_codeword = vec![0u8; CODEWORD_BYTES];
        for i in 0..CODEWORD_BYTES {
            candidate_codeword[i] = w_prime[i] ^ record.helper_data[i];
        }

        // Decode candidate secret R' via majority voting
        let r_prime = Self::decode_codeword(&candidate_codeword);

        // Verify that the error rate does not exceed the allowable noise threshold
        let reconstructed_codeword = Self::encode_secret(&r_prime);
        let error_bits = Self::hamming_distance(&candidate_codeword, &reconstructed_codeword);
        let error_rate = error_bits as f32 / CODEWORD_BITS as f32;
        if error_rate > self.noise_threshold {
            return Err(CryptoError::BiometricVerificationFailed);
        }

        // Verify cryptographic commitment C' == C
        let mut hasher = Sha256::new();
        hasher.update(r_prime);
        hasher.update(record.salt);
        let check_commitment = hasher.finalize();

        if check_commitment.as_slice() == record.commitment {
            Ok(MasterKey::new(r_prime))
        } else {
            Err(CryptoError::BiometricVerificationFailed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_extractor_exact_roundtrip() {
        let extractor = FuzzyExtractor::default();
        let embedding = vec![0.35f32; 128];
        let salt = [0x11; 32];

        let (master_key, record) = extractor.generate_commitment(&embedding, &salt).unwrap();
        let recovered_key = extractor.reproduce_key(&embedding, &record).unwrap();

        assert_eq!(master_key.as_bytes(), recovered_key.as_bytes());
    }

    #[test]
    fn test_fuzzy_extractor_noise_tolerance_up_to_20_percent() {
        let extractor = FuzzyExtractor::default();
        let embedding = vec![0.42f32; 128];
        let salt = [0x22; 32];

        let (master_key, record) = extractor.generate_commitment(&embedding, &salt).unwrap();

        // Quantize original embedding
        let mut w = FuzzyExtractor::quantize(&embedding);

        // Perturb exactly 15% of the total bits uniformly
        let total_bits = CODEWORD_BITS;
        let bits_to_flip = (total_bits as f32 * 0.15) as usize;
        for i in 0..bits_to_flip {
            let bit_pos = (i * 6) % total_bits;
            w[bit_pos / 8] ^= 1 << (bit_pos % 8);
        }

        let recovered = extractor.reproduce_from_bits(&w, &record).unwrap();
        assert_eq!(master_key.as_bytes(), recovered.as_bytes());
    }

    #[test]
    fn test_fuzzy_extractor_rejects_excessive_noise_40_percent() {
        let extractor = FuzzyExtractor::default();
        let embedding = vec![0.42f32; 128];
        let salt = [0x33; 32];

        let (_master_key, record) = extractor.generate_commitment(&embedding, &salt).unwrap();

        let mut w = FuzzyExtractor::quantize(&embedding);
        // Perturb 40% of the bits
        let total_bits = CODEWORD_BITS;
        let bits_to_flip = (total_bits as f32 * 0.40) as usize;
        for i in 0..bits_to_flip {
            let bit_pos = (i * 2) % total_bits;
            w[bit_pos / 8] ^= 1 << (bit_pos % 8);
        }

        let result = extractor.reproduce_from_bits(&w, &record);
        assert_eq!(
            result.unwrap_err(),
            CryptoError::BiometricVerificationFailed
        );
    }
}
