use crate::error::{AnnoraError, Result};
use crate::types::Timestamp;
use chacha20poly1305::{
    ChaCha20Poly1305, Key as ChaChaKey, Nonce as ChaChaNonce,
    aead::{Aead, KeyInit},
};
use ed25519_dalek::{Signature as EdSignature, Signer, SigningKey, VerifyingKey};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};

// Re-exported Ed25519 verifier types so downstream crates (agents, sync) can
// verify signatures against a bare public key WITHOUT adding a direct
// `ed25519-dalek` dependency. Purely additive: no existing signature changes.
pub use ed25519_dalek::{
    Signature as Ed25519Signature, Verifier as VerifyEd25519, VerifyingKey as Ed25519VerifyingKey,
};

#[derive(Clone)]
pub struct KeyPair {
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
}

impl std::fmt::Debug for KeyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyPair")
            .field("verifying_key", &self.verifying_key)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Signature(pub Vec<u8>);

impl KeyPair {
    pub fn generate() -> Self {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        Self {
            signing_key,
            verifying_key,
        }
    }

    pub fn sign(&self, data: &[u8]) -> Signature {
        let sig: EdSignature = self.signing_key.sign(data);
        Signature(sig.to_vec())
    }

    pub fn verify(&self, data: &[u8], signature: &Signature) -> bool {
        if let Ok(sig) = EdSignature::from_slice(&signature.0) {
            use ed25519_dalek::Verifier;
            self.verifying_key.verify(data, &sig).is_ok()
        } else {
            false
        }
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        let signing_key = SigningKey::from_bytes(bytes);
        let verifying_key = signing_key.verifying_key();
        Self {
            signing_key,
            verifying_key,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedData<T> {
    pub data: T,
    pub signature: Signature,
    pub signer: Vec<u8>, // Public key bytes
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedBlob {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

pub fn encrypt_at_rest(data: &[u8], key: &[u8]) -> Result<EncryptedBlob> {
    if key.len() != 32 {
        return Err(AnnoraError::CryptoError("Invalid key length".into()));
    }
    let cipher = ChaCha20Poly1305::new(ChaChaKey::from_slice(key));
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = ChaChaNonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, data)
        .map_err(|e| AnnoraError::CryptoError(e.to_string()))?;

    Ok(EncryptedBlob {
        nonce: nonce_bytes.to_vec(),
        ciphertext,
    })
}

pub fn decrypt_at_rest(blob: &EncryptedBlob, key: &[u8]) -> Result<Vec<u8>> {
    if key.len() != 32 {
        return Err(AnnoraError::CryptoError("Invalid key length".into()));
    }
    if blob.nonce.len() != 12 {
        return Err(AnnoraError::CryptoError("Invalid nonce length".into()));
    }
    let cipher = ChaCha20Poly1305::new(ChaChaKey::from_slice(key));
    let nonce = ChaChaNonce::from_slice(&blob.nonce);

    cipher
        .decrypt(nonce, blob.ciphertext.as_ref())
        .map_err(|e| AnnoraError::CryptoError(e.to_string()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedAttestation {
    pub manifest_hash: String,
    pub platform_id: String,
    pub timestamp: Timestamp,
    pub signature: Signature,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_sign_and_verify() {
        let kp = KeyPair::generate();
        let data = b"Annora Sovereign OS cryptographic integrity check";
        let sig = kp.sign(data);
        assert!(kp.verify(data, &sig));

        // Tampered data must fail verification
        let tampered_data = b"Annora Sovereign OS cryptographic integrity CHECK";
        assert!(!kp.verify(tampered_data, &sig));
    }

    #[test]
    fn test_encryption_at_rest_roundtrip() {
        let key = [42u8; 32];
        let plaintext = b"Confidential biometric voice embedding vector";
        let encrypted = encrypt_at_rest(plaintext, &key).expect("encryption failed");
        assert_ne!(encrypted.ciphertext, plaintext);

        let decrypted = decrypt_at_rest(&encrypted, &key).expect("decryption failed");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encryption_wrong_key_fails() {
        let key1 = [42u8; 32];
        let key2 = [99u8; 32];
        let plaintext = b"Sensitive memory triplet";
        let encrypted = encrypt_at_rest(plaintext, &key1).unwrap();
        assert!(decrypt_at_rest(&encrypted, &key2).is_err());
    }

    #[test]
    fn test_encryption_invalid_key_length() {
        let bad_key = [1u8; 16]; // Only 16 bytes instead of 32
        let plaintext = b"Test";
        assert!(encrypt_at_rest(plaintext, &bad_key).is_err());
    }
}
