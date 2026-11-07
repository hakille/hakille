use crate::crypto::error::CryptoError;
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// A single cryptographic share in a (k, n) threshold scheme
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct SecretShare {
    /// Share evaluation index x in 1..=255
    pub index: u8,
    /// Share data bytes y = f(x)
    pub data: [u8; 32],
    /// Cryptographic checksum validating share integrity
    pub checksum: [u8; 4],
}

impl std::fmt::Debug for SecretShare {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretShare")
            .field("index", &self.index)
            .field("data", &"[REDACTED]")
            .field("checksum", &self.checksum)
            .finish()
    }
}

impl SecretShare {
    pub fn compute_checksum(index: u8, data: &[u8; 32]) -> [u8; 4] {
        let mut hasher = Sha256::new();
        hasher.update(b"hakille:shamir:v1:");
        hasher.update([index]);
        hasher.update(data);
        let digest = hasher.finalize();
        let mut checksum = [0u8; 4];
        checksum.copy_from_slice(&digest[..4]);
        checksum
    }

    pub fn verify_checksum(&self) -> bool {
        self.checksum == Self::compute_checksum(self.index, &self.data)
    }
}

/// Multiplication in Galois Field GF(2^8) with irreducible polynomial x^8 + x^4 + x^3 + x + 1 (0x11b)
pub fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0u8;
    for _ in 0..8 {
        if (b & 1) != 0 {
            p ^= a;
        }
        let hi_bit_set = (a & 0x80) != 0;
        a <<= 1;
        if hi_bit_set {
            a ^= 0x1b; // polynomial 0x11b reduced
        }
        b >>= 1;
    }
    p
}

/// Multiplicative inverse in GF(2^8) via Fermat's Little Theorem: a^-1 = a^254
pub fn gf_inv(a: u8) -> u8 {
    if a == 0 {
        return 0;
    }
    let mut res = 1u8;
    let mut base = a;
    let mut exp = 254u8;
    while exp > 0 {
        if (exp & 1) != 0 {
            res = gf_mul(res, base);
        }
        base = gf_mul(base, base);
        exp >>= 1;
    }
    res
}

/// Split a 32-byte secret into `total` shares with reconstruction threshold `threshold`
pub fn split_secret(
    secret: &[u8; 32],
    threshold: u8,
    total: u8,
) -> Result<Vec<SecretShare>, CryptoError> {
    if threshold == 0 || total == 0 || threshold > total {
        return Err(CryptoError::InvalidShareParameters { threshold, total });
    }

    let mut shares_data = vec![[0u8; 32]; total as usize];

    // For each byte position of the 32-byte secret, generate a random polynomial of degree (threshold - 1)
    for byte_idx in 0..32 {
        let mut poly = vec![0u8; threshold as usize];
        poly[0] = secret[byte_idx]; // constant term is the secret byte

        for item in poly.iter_mut().take(threshold as usize).skip(1) {
            *item = (OsRng.next_u32() & 0xff) as u8;
        }

        // Evaluate polynomial for each share index x in 1..=total
        for x_idx in 1..=total {
            let x = x_idx;
            let mut val = 0u8;
            let mut x_pow = 1u8;
            for &coeff in poly.iter() {
                val ^= gf_mul(coeff, x_pow);
                x_pow = gf_mul(x_pow, x);
            }
            shares_data[(x_idx - 1) as usize][byte_idx] = val;
        }
    }

    let mut shares = Vec::with_capacity(total as usize);
    for (i, data) in shares_data.into_iter().enumerate() {
        let index = (i + 1) as u8;
        let checksum = SecretShare::compute_checksum(index, &data);
        shares.push(SecretShare {
            index,
            data,
            checksum,
        });
    }

    Ok(shares)
}

/// Reconstruct the original 32-byte secret from any k valid shares
pub fn reconstruct_secret(shares: &[SecretShare]) -> Result<[u8; 32], CryptoError> {
    if shares.is_empty() {
        return Err(CryptoError::InsufficientShares {
            required: 1,
            provided: 0,
        });
    }

    let mut seen_indices = HashSet::new();
    for share in shares {
        if share.index == 0 {
            return Err(CryptoError::CorruptedShare(
                "Share index 0 is invalid".to_string(),
            ));
        }
        if !seen_indices.insert(share.index) {
            return Err(CryptoError::DuplicateShareIndex(share.index));
        }
        if !share.verify_checksum() {
            return Err(CryptoError::CorruptedShare(format!(
                "Share {} failed integrity checksum verification",
                share.index
            )));
        }
    }

    let k = shares.len();
    let mut secret = [0u8; 32];

    // Lagrange polynomial interpolation at x = 0 over GF(2^8)
    for (byte_idx, secret_slot) in secret.iter_mut().enumerate() {
        let mut secret_byte = 0u8;
        for i in 0..k {
            let xi = shares[i].index;
            let yi = shares[i].data[byte_idx];

            let mut li = 1u8;
            for (j, share_j) in shares.iter().enumerate().take(k) {
                if i != j {
                    let xj = share_j.index;
                    let num = xj;
                    let den = xi ^ xj;
                    let term = gf_mul(num, gf_inv(den));
                    li = gf_mul(li, term);
                }
            }
            secret_byte ^= gf_mul(yi, li);
        }
        *secret_slot = secret_byte;
    }

    Ok(secret)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gf_inversion() {
        for a in 1..=255 {
            let inv = gf_inv(a);
            assert_eq!(gf_mul(a, inv), 1, "Failed for a = {a}");
        }
    }

    #[test]
    fn test_shamir_3_of_5_roundtrip() {
        let secret = [0x79; 32];
        let shares = split_secret(&secret, 3, 5).unwrap();
        assert_eq!(shares.len(), 5);

        // Reconstruct with shares [0, 2, 4]
        let subset1 = vec![shares[0].clone(), shares[2].clone(), shares[4].clone()];
        let reconstructed1 = reconstruct_secret(&subset1).unwrap();
        assert_eq!(reconstructed1, secret);

        // Reconstruct with shares [1, 3, 4]
        let subset2 = vec![shares[1].clone(), shares[3].clone(), shares[4].clone()];
        let reconstructed2 = reconstruct_secret(&subset2).unwrap();
        assert_eq!(reconstructed2, secret);
    }

    #[test]
    fn test_shamir_corrupted_share_rejected() {
        let secret = [0x55; 32];
        let mut shares = split_secret(&secret, 2, 3).unwrap();
        shares[0].data[0] ^= 0x01; // Corrupt share data

        let err = reconstruct_secret(&shares[0..2]).unwrap_err();
        assert!(matches!(err, CryptoError::CorruptedShare(_)));
    }
}
