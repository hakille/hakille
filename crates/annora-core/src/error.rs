use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnnoraError {
    #[error("Capability denied: {0}")]
    CapabilityDenied(String),
    #[error("Capability expired")]
    CapabilityExpired,
    #[error("Attenuation violation")]
    AttenuationViolation,
    #[error("Crypto error: {0}")]
    CryptoError(String),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Inference error: {0}")]
    InferenceError(String),
    #[error("Substrate error: {0}")]
    SubstrateError(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Authentication failed")]
    AuthenticationFailed,
    #[error("Trust violation: {0}")]
    TrustViolation(String),
    #[error("Constitution violation: {0}")]
    ConstitutionViolation(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Contradiction detected: existing '{existing}', incoming '{incoming}'")]
    ContradictionDetected {
        existing: String,
        incoming: String,
    },
}

pub type Result<T> = std::result::Result<T, AnnoraError>;
