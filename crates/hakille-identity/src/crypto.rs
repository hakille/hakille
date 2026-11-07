//! Cryptographic Primitives: Argon2id, HKDF-SHA256, XChaCha20-Poly1305,
//! Blind Indexing, Shamir Secret Sharing, and Fuzzy Extractor.

use chacha20poly1305::{
    Key as XKey, XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit},
};
use hakille_core::HakilleError;
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop};

// =============================================================================
// F01: Argon2id KDF (RFC 9106)
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Argon2Params {
    pub memory_kib: u32,
    pub passes: u32,
    pub parallelism: u32,
}

impl Default for Argon2Params {
    fn default() -> Self {
        Self {
            memory_kib: 65536, // 64 MiB standard production parameter
            passes: 3,         // t = 3
            parallelism: 4,    // p = 4 threads
        }
    }
}

/// 32-byte Master Key with Zeroize on drop
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct MasterKey {
    pub key: [u8; 32],
}

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MasterKey([REDACTED])")
    }
}

impl MasterKey {
    pub fn new(key: [u8; 32]) -> Self {
        Self { key }
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.key
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.key
    }
}

/// Zeroized cryptographic subkey hierarchy for an entity or tenant
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SubkeyHierarchy {
    pub identity_key: [u8; 32],
    pub memory_key: [u8; 32],
    pub credentials_key: [u8; 32],
    pub biometrics_key: [u8; 32],
    pub blind_index_key: [u8; 32],
}

impl std::fmt::Debug for SubkeyHierarchy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SubkeyHierarchy([REDACTED])")
    }
}

impl SubkeyHierarchy {
    pub fn identity_key(&self) -> &[u8; 32] {
        &self.identity_key
    }

    pub fn memory_key(&self) -> &[u8; 32] {
        &self.memory_key
    }

    pub fn credentials_key(&self) -> &[u8; 32] {
        &self.credentials_key
    }

    pub fn biometrics_key(&self) -> &[u8; 32] {
        &self.biometrics_key
    }

    pub fn blind_index_key(&self) -> &[u8; 32] {
        &self.blind_index_key
    }
}

pub struct MasterKeyKdf;

impl MasterKeyKdf {
    /// Generate a 32-byte CSPRNG salt
    pub fn generate_salt() -> [u8; 32] {
        hakille_core::crypto::kdf::generate_salt()
    }

    /// Derive master key using default RFC 9106 production parameters (64MB, t=3, p=4)
    pub fn derive_master_key(passphrase: &[u8], salt: &[u8; 32]) -> Result<MasterKey, HakilleError> {
        Self::derive_master_key_with_params(passphrase, salt, &Argon2Params::default())
    }

    /// Memory-hard key derivation adhering to Argon2id RFC 9106
    pub fn derive_master_key_with_params(
        passphrase: &[u8],
        salt: &[u8; 32],
        params: &Argon2Params,
    ) -> Result<MasterKey, HakilleError> {
        if params.memory_kib == 0 || params.passes == 0 || params.parallelism == 0 {
            return Err(HakilleError::CryptoError(
                "Argon2 parameters must be non-zero".to_string(),
            ));
        }

        let core_params = hakille_core::crypto::kdf::Argon2Params {
            memory_kib: params.memory_kib,
            passes: params.passes,
            parallelism: params.parallelism,
            output_len: 32,
        };

        let core_master = hakille_core::crypto::kdf::MasterKeyKdf::derive_master_key_with_params(
            passphrase,
            salt,
            &core_params,
        )
        .map_err(|e| HakilleError::CryptoError(e.to_string()))?;

        Ok(MasterKey::new(core_master.to_bytes()))
    }

    /// Derive default subkey hierarchy from a derived master key
    pub fn derive_subkeys(master: &MasterKey) -> SubkeyHierarchy {
        let k_id = HkdfSubkeyTree::derive_subkey(master, &SubkeyDomain::Identity, "default");
        let k_mem = HkdfSubkeyTree::derive_subkey(
            master,
            &SubkeyDomain::MemoryPartition("default".to_string()),
            "default",
        );
        let k_cred = HkdfSubkeyTree::derive_subkey(master, &SubkeyDomain::Credentials, "default");
        let k_bio = HkdfSubkeyTree::derive_subkey(master, &SubkeyDomain::Biometrics, "default");
        let k_blind = HkdfSubkeyTree::derive_subkey(master, &SubkeyDomain::BlindIndex, "default");

        SubkeyHierarchy {
            identity_key: k_id,
            memory_key: k_mem,
            credentials_key: k_cred,
            biometrics_key: k_bio,
            blind_index_key: k_blind,
        }
    }
}

// =============================================================================
// F02: HKDF-SHA256 Subkey Tree (RFC 5869)
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SubkeyDomain {
    Identity,
    MemoryPartition(String),
    Credentials,
    Biometrics,
    BlindIndex,
}

impl SubkeyDomain {
    pub fn as_str(&self) -> String {
        match self {
            SubkeyDomain::Identity => "identity".to_string(),
            SubkeyDomain::MemoryPartition(p) => format!("memory-partition:{}", p),
            SubkeyDomain::Credentials => "credentials".to_string(),
            SubkeyDomain::Biometrics => "biometrics".to_string(),
            SubkeyDomain::BlindIndex => "blind-index".to_string(),
        }
    }
}

pub struct HkdfSubkeyTree;

impl HkdfSubkeyTree {
    /// Pure Rust HMAC-SHA256
    pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
        let mut k = [0u8; 64];
        if key.len() > 64 {
            let mut h = Sha256::new();
            h.update(key);
            k[..32].copy_from_slice(&h.finalize());
        } else {
            k[..key.len()].copy_from_slice(key);
        }

        let mut i_pad = [0x36u8; 64];
        let mut o_pad = [0x5cu8; 64];
        for idx in 0..64 {
            i_pad[idx] ^= k[idx];
            o_pad[idx] ^= k[idx];
        }

        let mut inner_hasher = Sha256::new();
        inner_hasher.update(i_pad);
        inner_hasher.update(data);
        let inner_hash = inner_hasher.finalize();

        let mut outer_hasher = Sha256::new();
        outer_hasher.update(o_pad);
        outer_hasher.update(inner_hash);
        let mut out = [0u8; 32];
        out.copy_from_slice(&outer_hasher.finalize());
        out
    }

    /// HKDF-Extract: PRK = HMAC-Hash(salt, IKM)
    pub fn extract(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
        Self::hmac_sha256(salt, ikm)
    }

    /// HKDF-Expand: OKM = HMAC-Hash(PRK, info || 0x01)
    pub fn expand(prk: &[u8; 32], info: &[u8]) -> [u8; 32] {
        let mut data = Vec::with_capacity(info.len() + 1);
        data.extend_from_slice(info);
        data.push(0x01);
        Self::hmac_sha256(prk, &data)
    }

    /// Derive domain-separated subkey:
    /// context = "hakille:v1:" || Domain || TenantID
    pub fn derive_subkey(
        master: &MasterKey,
        domain: &SubkeyDomain,
        tenant_id: &str,
    ) -> [u8; 32] {
        let salt = b"hakille:v1:domain-salt";
        let prk = Self::extract(salt, master.as_bytes());
        let info = format!("hakille:v1:{}:{}", domain.as_str(), tenant_id);
        Self::expand(&prk, info.as_bytes())
    }
}

// =============================================================================
// F03: XChaCha20-Poly1305 AEAD (24-byte Nonce & AAD Binding)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct XChaChaBlob {
    pub nonce: [u8; 24],
    pub ciphertext: Vec<u8>,
    pub tag: [u8; 16],
    pub aad_fingerprint: [u8; 16],
}

/// Type alias for encrypted payloads adhering to PROJECT.md
pub type EncryptedPayload = XChaChaBlob;

pub struct XChaChaCipher;

impl XChaChaCipher {
    pub fn encrypt(
        plaintext: &[u8],
        key: &[u8; 32],
        aad: &[u8],
    ) -> Result<XChaChaBlob, HakilleError> {
        let mut nonce = [0u8; 24];
        OsRng.fill_bytes(&mut nonce);
        Self::encrypt_with_nonce(plaintext, key, &nonce, aad)
    }

    pub fn encrypt_with_nonce(
        plaintext: &[u8],
        key: &[u8; 32],
        nonce: &[u8; 24],
        aad: &[u8],
    ) -> Result<XChaChaBlob, HakilleError> {
        let cipher = XChaCha20Poly1305::new(XKey::from_slice(key));
        let xnonce = XNonce::from_slice(nonce);

        let payload_to_encrypt = chacha20poly1305::aead::Payload {
            msg: plaintext,
            aad,
        };

        let mut ct_with_tag = cipher
            .encrypt(xnonce, payload_to_encrypt)
            .map_err(|e| HakilleError::CryptoError(format!("XChaCha encryption failed: {e}")))?;

        if ct_with_tag.len() < 16 {
            return Err(HakilleError::CryptoError("Ciphertext too short".to_string()));
        }

        let tag_offset = ct_with_tag.len() - 16;
        let mut tag = [0u8; 16];
        tag.copy_from_slice(&ct_with_tag[tag_offset..]);
        ct_with_tag.truncate(tag_offset);

        // Compute Blake3/Sha256 fingerprint of AAD
        let aad_hash = Sha256::digest(aad);
        let mut aad_fingerprint = [0u8; 16];
        aad_fingerprint.copy_from_slice(&aad_hash[..16]);

        Ok(XChaChaBlob {
            nonce: *nonce,
            ciphertext: ct_with_tag,
            tag,
            aad_fingerprint,
        })
    }

    pub fn decrypt(
        blob: &XChaChaBlob,
        key: &[u8; 32],
        aad: &[u8],
    ) -> Result<Vec<u8>, HakilleError> {
        // Verify AAD fingerprint using constant-time comparison
        let aad_hash = Sha256::digest(aad);
        let mut diff = 0u8;
        for (a, b) in aad_hash[..16].iter().zip(blob.aad_fingerprint.iter()) {
            diff |= a ^ b;
        }
        if diff != 0 {
            return Err(HakilleError::CryptoError(
                "AAD fingerprint mismatch: cross-tenant or cross-table splicing detected"
                    .to_string(),
            ));
        }

        let cipher = XChaCha20Poly1305::new(XKey::from_slice(key));
        let xnonce = XNonce::from_slice(&blob.nonce);

        let mut ct_with_tag = Vec::with_capacity(blob.ciphertext.len() + 16);
        ct_with_tag.extend_from_slice(&blob.ciphertext);
        ct_with_tag.extend_from_slice(&blob.tag);

        let payload = chacha20poly1305::aead::Payload {
            msg: &ct_with_tag,
            aad,
        };

        cipher
            .decrypt(xnonce, payload)
            .map_err(|e| HakilleError::CryptoError(format!("XChaCha decryption failed: {e}")))
    }
}

// =============================================================================
// F04: HMAC-SHA256 Blind Indexing (Searchable ZK)
// =============================================================================

pub fn compute_blind_index(plaintext_term: &str, blind_key: &[u8; 32]) -> [u8; 32] {
    let normalized = plaintext_term.trim().to_lowercase();
    HkdfSubkeyTree::hmac_sha256(blind_key, normalized.as_bytes())
}

pub fn compute_prefix_blind_index(
    plaintext_term: &str,
    prefix_len: usize,
    blind_key: &[u8; 32],
) -> [u8; 16] {
    let normalized = plaintext_term.trim().to_lowercase();
    let prefix: String = normalized.chars().take(prefix_len).collect();
    let full = HkdfSubkeyTree::hmac_sha256(blind_key, prefix.as_bytes());
    let mut out = [0u8; 16];
    out.copy_from_slice(&full[..16]);
    out
}

#[derive(Default)]
pub struct BlindIndexStore {
    exact_index: std::collections::HashMap<[u8; 32], Vec<String>>,
    prefix_index: std::collections::HashMap<[u8; 16], Vec<String>>,
}

impl BlindIndexStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, term: &str, record_id: &str, blind_key: &[u8; 32]) {
        let exact = compute_blind_index(term, blind_key);
        self.exact_index
            .entry(exact)
            .or_default()
            .push(record_id.to_string());

        let prefix = compute_prefix_blind_index(term, 4, blind_key);
        self.prefix_index
            .entry(prefix)
            .or_default()
            .push(record_id.to_string());
    }

    pub fn search_exact(&self, term: &str, blind_key: &[u8; 32]) -> Vec<String> {
        let token = compute_blind_index(term, blind_key);
        self.exact_index.get(&token).cloned().unwrap_or_default()
    }

    pub fn search_prefix(&self, prefix: &str, blind_key: &[u8; 32]) -> Vec<String> {
        let token = compute_prefix_blind_index(prefix, 4, blind_key);
        self.prefix_index.get(&token).cloned().unwrap_or_default()
    }
}

// =============================================================================
// F05: Shamir's Secret Sharing (k-of-n over GF(2^8))
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct SecretShare {
    pub index: u8,
    pub data: Vec<u8>,
    pub commitment: [u8; 32], // Cryptographic VSS root commitment binding threshold, total, and secret
}

// GF(2^8) arithmetic with irreducible polynomial x^8 + x^4 + x^3 + x + 1 (0x11b)
fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0u8;
    for _ in 0..8 {
        if (b & 1) != 0 {
            p ^= a;
        }
        let hi_bit_set = (a & 0x80) != 0;
        a <<= 1;
        if hi_bit_set {
            a ^= 0x1b; // polynomial 0x11b
        }
        b >>= 1;
    }
    p
}

fn gf_inv(a: u8) -> u8 {
    if a == 0 {
        return 0;
    }
    // In GF(2^8), a^254 is the multiplicative inverse
    let mut res = 1u8;
    let mut base = a;
    let mut exp = 254;
    while exp > 0 {
        if (exp & 1) != 0 {
            res = gf_mul(res, base);
        }
        base = gf_mul(base, base);
        exp >>= 1;
    }
    res
}

pub fn split_secret(
    secret: &[u8; 32],
    threshold: u8,
    total: u8,
) -> Result<Vec<SecretShare>, HakilleError> {
    if threshold == 0 || total == 0 {
        return Err(HakilleError::CryptoError(
            "Threshold and total must be non-zero".to_string(),
        ));
    }
    if threshold > total {
        return Err(HakilleError::CryptoError(
            "Threshold cannot exceed total shares".to_string(),
        ));
    }

    let mut shares = Vec::with_capacity(total as usize);
    let mut share_data: Vec<Vec<u8>> = (0..total).map(|_| Vec::with_capacity(32)).collect();

    // For each byte in the 32-byte secret, generate a random polynomial of degree (threshold - 1)
    for &secret_byte in secret.iter() {
        let mut poly = vec![0u8; threshold as usize];
        poly[0] = secret_byte;
        for coeff in poly.iter_mut().take(threshold as usize).skip(1) {
            *coeff = (OsRng.next_u32() & 0xff) as u8;
        }

        // Evaluate polynomial for each share index x in 1..=total
        for x in 1..=total {
            let mut val = 0u8;
            let mut x_pow = 1u8;
            for &coeff in poly.iter() {
                val ^= gf_mul(coeff, x_pow);
                x_pow = gf_mul(x_pow, x);
            }
            share_data[(x - 1) as usize].push(val);
        }
    }

    // Cryptographic root commitment binding the threshold k, total n, and secret
    let mut commit_hasher = Sha256::new();
    commit_hasher.update(b"hakille:shamir:vss:v1:");
    commit_hasher.update([threshold, total]);
    commit_hasher.update(secret);
    let digest = commit_hasher.finalize();

    let mut commitment = [0u8; 32];
    commitment[0] = threshold;
    commitment[1] = total;
    commitment[2..].copy_from_slice(&digest[..30]);

    for (i, data) in share_data.into_iter().enumerate() {
        let index = (i + 1) as u8;
        shares.push(SecretShare {
            index,
            data,
            commitment,
        });
    }

    Ok(shares)
}

pub fn reconstruct_secret(shares: &[SecretShare]) -> Result<[u8; 32], HakilleError> {
    if shares.is_empty() {
        return Err(HakilleError::CryptoError("No shares provided".to_string()));
    }

    let expected_commitment = shares[0].commitment;
    let threshold = expected_commitment[0] as usize;

    if threshold == 0 {
        return Err(HakilleError::CryptoError(
            "Share failed commitment verification: invalid threshold 0".to_string(),
        ));
    }

    if shares.len() < threshold {
        return Err(HakilleError::CryptoError(format!(
            "Insufficient shares provided: required {threshold}, provided {}",
            shares.len()
        )));
    }

    // Check share integrity and duplicates
    let mut seen_indices = std::collections::HashSet::new();
    for share in shares {
        if share.index == 0 {
            return Err(HakilleError::CryptoError("Invalid share index 0".to_string()));
        }
        if !seen_indices.insert(share.index) {
            return Err(HakilleError::CryptoError(format!(
                "Duplicate share index {}",
                share.index
            )));
        }
        if share.data.len() != 32 {
            return Err(HakilleError::CryptoError(
                "Invalid share length (must be 32 bytes)".to_string(),
            ));
        }
        if share.commitment != expected_commitment {
            return Err(HakilleError::CryptoError(format!(
                "Share {} failed commitment verification",
                share.index
            )));
        }
    }

    let k = shares.len();
    let mut secret = [0u8; 32];

    // Lagrange interpolation at x = 0 over GF(2^8)
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
                    li = gf_mul(li, gf_mul(num, gf_inv(den)));
                }
            }
            secret_byte ^= gf_mul(yi, li);
        }
        *secret_slot = secret_byte;
    }

    // Verify reconstructed secret against commitment
    let mut verify_hasher = Sha256::new();
    verify_hasher.update(b"hakille:shamir:vss:v1:");
    verify_hasher.update([expected_commitment[0], expected_commitment[1]]);
    verify_hasher.update(secret);
    let verify_digest = verify_hasher.finalize();

    if verify_digest[..30] != expected_commitment[2..] {
        return Err(HakilleError::CryptoError(
            "Share failed commitment verification".to_string(),
        ));
    }

    Ok(secret)
}

// =============================================================================
// F06: Fuzzy Extractor Biometric Commitments
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelperData {
    pub syndrome: Vec<u8>,
    pub salt: [u8; 16],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitmentHash(pub [u8; 32]);

pub struct FuzzyExtractor {
    pub delta_threshold: usize,
}

impl Default for FuzzyExtractor {
    fn default() -> Self {
        Self {
            delta_threshold: 16,
        }
    }
}

impl FuzzyExtractor {
    pub fn new(delta_threshold: usize) -> Self {
        Self { delta_threshold }
    }

    /// Quantize a continuous biometric vector into a 256-bit string
    pub fn quantize(embedding: &[f32]) -> [u8; 32] {
        let mut bits = [0u8; 32];
        for (i, &val) in embedding.iter().take(256).enumerate() {
            if val > 0.0 {
                bits[i / 8] |= 1 << (i % 8);
            }
        }
        bits
    }

    /// Hamming distance between two bitstrings
    pub fn hamming_distance(a: &[u8; 32], b: &[u8; 32]) -> usize {
        let mut dist = 0;
        for (x, y) in a.iter().zip(b.iter()) {
            dist += (x ^ y).count_ones() as usize;
        }
        dist
    }

    /// Gen(B) -> (R, HelperData, CommitmentHash)
    pub fn generate(
        &self,
        biometric: &[f32],
    ) -> Result<([u8; 32], HelperData, CommitmentHash), HakilleError> {
        let b = Self::quantize(biometric);

        // 4-byte seed repeated 8 times as a [256, 32, 8] linear repetition code
        let mut seed = [0u8; 4];
        OsRng.fill_bytes(&mut seed);
        let mut c = [0u8; 32];
        for i in 0..32 {
            c[i] = seed[i % 4];
        }

        let r: [u8; 32] = Sha256::digest(seed).into();

        // Helper syndrome P = b ^ c
        let mut syndrome = vec![0u8; 32];
        for (i, (b_byte, c_byte)) in b.iter().zip(c.iter()).enumerate() {
            syndrome[i] = b_byte ^ c_byte;
        }

        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);

        let mut hasher = Sha256::new();
        hasher.update(r);
        hasher.update(salt);
        let mut commit = [0u8; 32];
        commit.copy_from_slice(&hasher.finalize());

        Ok((
            r,
            HelperData { syndrome, salt },
            CommitmentHash(commit),
        ))
    }

    /// Rep(B', HelperData) -> R'
    pub fn reproduce(
        &self,
        probe: &[f32],
        helper: &HelperData,
        expected_commitment: &CommitmentHash,
    ) -> Result<[u8; 32], HakilleError> {
        if helper.syndrome.len() != 32 {
            return Err(HakilleError::CryptoError(
                "Invalid helper data length".to_string(),
            ));
        }

        let b_prime = Self::quantize(probe);
        let mut noisy_c = [0u8; 32];
        for (i, (bp_byte, syn_byte)) in b_prime.iter().zip(helper.syndrome.iter()).enumerate() {
            noisy_c[i] = bp_byte ^ syn_byte;
        }

        // Majority decode each of the 32 bits from 8 repetitions
        let mut recovered_seed = [0u8; 4];
        for bit_idx in 0..32 {
            let mut ones = 0;
            for rep in 0..8 {
                let byte_idx = rep * 4 + (bit_idx / 8);
                let bit_in_byte = bit_idx % 8;
                if (noisy_c[byte_idx] & (1 << bit_in_byte)) != 0 {
                    ones += 1;
                }
            }
            if ones >= 5 {
                recovered_seed[bit_idx / 8] |= 1 << (bit_idx % 8);
            }
        }

        let mut reconstructed_c = [0u8; 32];
        for i in 0..32 {
            reconstructed_c[i] = recovered_seed[i % 4];
        }

        let mut original_b = [0u8; 32];
        for i in 0..32 {
            original_b[i] = helper.syndrome[i] ^ reconstructed_c[i];
        }

        let dist = Self::hamming_distance(&b_prime, &original_b);
        if dist > self.delta_threshold {
            return Err(HakilleError::AuthenticationFailed);
        }

        let recovered_r: [u8; 32] = Sha256::digest(recovered_seed).into();

        // Verify commitment: Hash(R' || salt) == Commitment
        let mut hasher = Sha256::new();
        hasher.update(recovered_r);
        hasher.update(helper.salt);
        let check_commit = hasher.finalize();

        if check_commit.as_slice() == expected_commitment.0 {
            Ok(recovered_r)
        } else {
            Err(HakilleError::AuthenticationFailed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_argon2id_genuine_derivation_and_zeroize() {
        let salt = [0x5au8; 32];
        let master = MasterKeyKdf::derive_master_key(b"test_genuine_argon2id", &salt).unwrap();
        assert_eq!(master.as_bytes().len(), 32);
        assert!(master.as_bytes().iter().any(|&b| b != 0));

        let mut key_to_zero = MasterKey::new([0x37u8; 32]);
        key_to_zero.zeroize();
        assert_eq!(key_to_zero.as_bytes(), &[0u8; 32]);
    }

    #[test]
    fn test_derive_subkeys_and_hierarchy_zeroize() {
        let master = MasterKey::new([0x88u8; 32]);
        let hierarchy = MasterKeyKdf::derive_subkeys(&master);

        assert_ne!(hierarchy.identity_key(), &[0u8; 32]);
        assert_ne!(hierarchy.identity_key(), hierarchy.memory_key());
        assert_ne!(hierarchy.credentials_key(), hierarchy.biometrics_key());
        assert_ne!(hierarchy.blind_index_key(), hierarchy.identity_key());

        let mut h_zero = hierarchy.clone();
        h_zero.zeroize();
        assert_eq!(h_zero.identity_key(), &[0u8; 32]);
        assert_eq!(h_zero.memory_key(), &[0u8; 32]);
        assert_eq!(h_zero.credentials_key(), &[0u8; 32]);
    }

    #[test]
    fn test_secret_share_zeroize() {
        let mut share = SecretShare {
            index: 1,
            data: vec![0x99u8; 32],
            commitment: [0x55u8; 32],
        };
        share.zeroize();
        assert_eq!(share.index, 0);
        assert!(share.data.is_empty());
        assert_eq!(share.commitment, [0u8; 32]);
    }

    #[test]
    fn test_shamir_threshold_enforcement_strict() {
        let secret = [0x7cu8; 32];
        let shares = split_secret(&secret, 3, 5).unwrap();

        // 2 shares provided when 3 required -> must return Err
        let err = reconstruct_secret(&shares[0..2]).unwrap_err();
        match err {
            HakilleError::CryptoError(msg) => {
                assert!(msg.contains("Insufficient shares provided"));
            }
            other => panic!("Expected Insufficient shares CryptoError, got: {:?}", other),
        }

        // 3 shares -> succeeds
        let rec = reconstruct_secret(&shares[0..3]).unwrap();
        assert_eq!(rec, secret);
    }

    #[test]
    fn test_fuzzy_extractor_delta_threshold_wiring() {
        let fe = FuzzyExtractor::new(4);
        let sample = vec![0.5f32; 256];
        let (key, helper, commit) = fe.generate(&sample).unwrap();

        // 2 errors (dist <= 4) -> succeeds
        let mut probe_ok = sample.clone();
        probe_ok[0] = -0.5;
        probe_ok[1] = -0.5;
        let res_ok = fe.reproduce(&probe_ok, &helper, &commit);
        assert_eq!(res_ok.unwrap(), key);

        // 6 errors (dist > 4) -> fails
        let mut probe_fail = sample.clone();
        for i in 0..6 {
            probe_fail[i] = -0.5;
        }
        let res_fail = fe.reproduce(&probe_fail, &helper, &commit);
        assert!(res_fail.is_err());
    }

    #[test]
    fn test_xchacha_payload_type_alias_and_aad() {
        let key = [0x12u8; 32];
        let payload: EncryptedPayload = XChaChaCipher::encrypt(b"secret", &key, b"aad1").unwrap();
        let decrypted = XChaChaCipher::decrypt(&payload, &key, b"aad1").unwrap();
        assert_eq!(decrypted, b"secret");

        // Wrong AAD rejected
        assert!(XChaChaCipher::decrypt(&payload, &key, b"wrong_aad").is_err());
    }
}

