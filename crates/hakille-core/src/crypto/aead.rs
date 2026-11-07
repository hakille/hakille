use crate::crypto::error::CryptoError;
use chacha20poly1305::{
    Key as XKey, XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Encrypted payload using XChaCha20-Poly1305 with 24-byte extended nonce
/// and cryptographically bound Additional Authenticated Data (AAD) fingerprint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct XChaChaBlob {
    /// 24-byte extended nonce (immune to birthday collision attacks)
    pub nonce: [u8; 24],
    /// Encrypted ciphertext payload (without 16-byte Poly1305 authentication tag)
    pub ciphertext: Vec<u8>,
    /// 16-byte Poly1305 MAC tag
    pub tag: [u8; 16],
    /// 32-byte SHA-256 fingerprint of the Additional Authenticated Data
    pub aad_fingerprint: [u8; 32],
}

/// Type alias for encrypted payload
pub type EncryptedPayload = XChaChaBlob;

/// Canonical binding structure for Additional Authenticated Data (AAD)
/// preventing cross-tenant and cross-table ciphertext splicing attacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AadBinding<'a> {
    pub tenant_id: &'a str,
    pub user_id: &'a str,
    pub table_id: &'a str,
    pub record_uuid: &'a str,
    pub schema_version: u32,
}

impl<'a> AadBinding<'a> {
    pub fn new(
        tenant_id: &'a str,
        user_id: &'a str,
        table_id: &'a str,
        record_uuid: &'a str,
        schema_version: u32,
    ) -> Self {
        Self {
            tenant_id,
            user_id,
            table_id,
            record_uuid,
            schema_version,
        }
    }

    /// Serialize into deterministic AAD bytes with unit separator delimiters
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"hakille:aad:v1:");
        bytes.extend_from_slice(self.tenant_id.as_bytes());
        bytes.push(0x1f); // Unit separator
        bytes.extend_from_slice(self.user_id.as_bytes());
        bytes.push(0x1f);
        bytes.extend_from_slice(self.table_id.as_bytes());
        bytes.push(0x1f);
        bytes.extend_from_slice(self.record_uuid.as_bytes());
        bytes.push(0x1f);
        bytes.extend_from_slice(&self.schema_version.to_le_bytes());
        bytes
    }
}

/// Authenticated Symmetric Cipher using XChaCha20-Poly1305
pub struct XChaChaCipher;

impl XChaChaCipher {
    /// Encrypt plaintext using a CSPRNG-generated 24-byte extended nonce and bound AAD
    pub fn encrypt(
        plaintext: &[u8],
        key: &[u8; 32],
        aad: &[u8],
    ) -> Result<XChaChaBlob, CryptoError> {
        let mut nonce = [0u8; 24];
        OsRng.fill_bytes(&mut nonce);
        Self::encrypt_with_nonce(plaintext, key, &nonce, aad)
    }

    /// Encrypt with an explicit 24-byte nonce (primarily for deterministic test vectors)
    pub fn encrypt_with_nonce(
        plaintext: &[u8],
        key: &[u8; 32],
        nonce: &[u8; 24],
        aad: &[u8],
    ) -> Result<XChaChaBlob, CryptoError> {
        let cipher = XChaCha20Poly1305::new(XKey::from_slice(key));
        let xnonce = XNonce::from_slice(nonce);

        let payload = Payload {
            msg: plaintext,
            aad,
        };

        let mut ct_with_tag = cipher
            .encrypt(xnonce, payload)
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

        if ct_with_tag.len() < 16 {
            return Err(CryptoError::EncryptionFailed(
                "Ciphertext payload length is smaller than tag length".to_string(),
            ));
        }

        let tag_offset = ct_with_tag.len() - 16;
        let mut tag = [0u8; 16];
        tag.copy_from_slice(&ct_with_tag[tag_offset..]);
        ct_with_tag.truncate(tag_offset);

        let aad_hash = Sha256::digest(aad);
        let mut aad_fingerprint = [0u8; 32];
        aad_fingerprint.copy_from_slice(&aad_hash);

        Ok(XChaChaBlob {
            nonce: *nonce,
            ciphertext: ct_with_tag,
            tag,
            aad_fingerprint,
        })
    }

    /// Decrypt ciphertext payload and strictly verify AAD binding
    pub fn decrypt(blob: &XChaChaBlob, key: &[u8; 32], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
        // Fast-fail: verify AAD fingerprint before expensive decryption
        let aad_hash = Sha256::digest(aad);
        if aad_hash.as_slice() != blob.aad_fingerprint {
            return Err(CryptoError::AadMismatch);
        }

        let cipher = XChaCha20Poly1305::new(XKey::from_slice(key));
        let xnonce = XNonce::from_slice(&blob.nonce);

        let mut ct_with_tag = Vec::with_capacity(blob.ciphertext.len() + 16);
        ct_with_tag.extend_from_slice(&blob.ciphertext);
        ct_with_tag.extend_from_slice(&blob.tag);

        let payload = Payload {
            msg: &ct_with_tag,
            aad,
        };

        cipher
            .decrypt(xnonce, payload)
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xchacha20_roundtrip_with_aad() {
        let key = [0x42; 32];
        let plaintext = b"Hakille Sovereign Operating System secure payload";
        let binding = AadBinding::new("tenant-100", "user-001", "records", "uuid-123", 1);
        let aad = binding.to_bytes();

        let encrypted = XChaChaCipher::encrypt(plaintext, &key, &aad).unwrap();
        assert_ne!(encrypted.ciphertext, plaintext);
        assert_eq!(encrypted.nonce.len(), 24);

        let decrypted = XChaChaCipher::decrypt(&encrypted, &key, &aad).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_xchacha20_cross_tenant_splicing_detected() {
        let key = [0x42; 32];
        let plaintext = b"Confidential organization memorandum";
        let aad_tenant_a =
            AadBinding::new("tenant-A", "user-001", "records", "uuid-123", 1).to_bytes();
        let aad_tenant_b =
            AadBinding::new("tenant-B", "user-001", "records", "uuid-123", 1).to_bytes();

        let encrypted = XChaChaCipher::encrypt(plaintext, &key, &aad_tenant_a).unwrap();

        // Attempting to decrypt in tenant-B context must fail with AadMismatch
        let err = XChaChaCipher::decrypt(&encrypted, &key, &aad_tenant_b).unwrap_err();
        assert_eq!(err, CryptoError::AadMismatch);
    }

    #[test]
    fn test_xchacha20_tampered_ciphertext_fails() {
        let key = [0x42; 32];
        let plaintext = b"Critical instruction";
        let aad = b"test:aad";

        let mut encrypted = XChaChaCipher::encrypt(plaintext, &key, aad).unwrap();
        if let Some(byte) = encrypted.ciphertext.first_mut() {
            *byte ^= 0x01; // tamper 1 bit
        }

        assert!(matches!(
            XChaChaCipher::decrypt(&encrypted, &key, aad),
            Err(CryptoError::DecryptionFailed(_))
        ));
    }
}
