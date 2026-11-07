use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CryptoError {
    #[error("Argon2 KDF derivation error: {0}")]
    KdfError(String),

    #[error("HKDF subkey derivation error: {0}")]
    SubkeyDerivationError(String),

    #[error("XChaCha20-Poly1305 encryption error: {0}")]
    EncryptionFailed(String),

    #[error("XChaCha20-Poly1305 decryption error: {0}")]
    DecryptionFailed(String),

    #[error("AAD fingerprint mismatch: cross-tenant or cross-table splicing detected")]
    AadMismatch,

    #[error("Invalid nonce length: expected {expected}, actual {actual}")]
    InvalidNonceLength { expected: usize, actual: usize },

    #[error("Invalid key length: expected {expected}, actual {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },

    #[error("Invalid secret share parameters: threshold {threshold}, total {total}")]
    InvalidShareParameters { threshold: u8, total: u8 },

    #[error("Corrupted or invalid secret share: {0}")]
    CorruptedShare(String),

    #[error(
        "Insufficient shares for secret reconstruction: required {required}, provided {provided}"
    )]
    InsufficientShares { required: usize, provided: usize },

    #[error("Duplicate share index {0}")]
    DuplicateShareIndex(u8),

    #[error("Biometric quantization error: {0}")]
    BiometricQuantizationError(String),

    #[error("Biometric verification failed: noise exceeds allowable threshold")]
    BiometricVerificationFailed,

    #[error("Invalid DID format: {0}")]
    InvalidDid(String),

    #[error("Invalid Verifiable Credential: {0}")]
    InvalidCredential(String),

    #[error("Signature verification failed: {0}")]
    SignatureVerificationFailed(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

impl From<CryptoError> for crate::error::HakilleError {
    fn from(err: CryptoError) -> Self {
        crate::error::HakilleError::CryptoError(err.to_string())
    }
}
