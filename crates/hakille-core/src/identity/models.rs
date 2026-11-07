//! Identity and cryptographic data models for zero-knowledge storage and credentials.

pub use crate::crypto::aead::{AadBinding, EncryptedPayload, XChaChaBlob};
pub use crate::crypto::fuzzy::{BiometricCommitmentRecord, CommitmentHash, HelperData};
pub use crate::crypto::shamir::SecretShare;
pub use crate::crypto::ssi::{CredentialProof, DidJwk, DidKey, VerifiableCredential};
