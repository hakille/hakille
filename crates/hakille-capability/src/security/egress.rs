//! Zero-knowledge secret repository and egress proxy.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use zeroize::Zeroizing;

/// Errors arising during secret storage or header injection.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SecretVaultError {
    #[error("Secret reference '{0}' not found in vault")]
    SecretNotFound(String),

    #[error("Caller unauthorized to access secret '{0}'")]
    Unauthorized(String),

    #[error("Failed to construct header value for secret '{0}': {1}")]
    InvalidHeaderValue(String, String),
}

/// Opaque secret reference passed across tool boundaries without exposing raw values.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SecretRef(pub String);

impl SecretRef {
    /// Creates a new secret reference.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the reference identifier.
    pub fn id(&self) -> &str {
        &self.0
    }
}

/// Zero-knowledge secret repository holding credentials in zeroized memory.
#[derive(Default, Clone)]
pub struct SecretVault {
    secrets: Arc<RwLock<HashMap<String, Zeroizing<String>>>>,
}

impl SecretVault {
    /// Creates a new empty secret vault.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores a secret securely in zeroized memory.
    pub async fn store_secret(&self, secret_id: impl Into<String>, secret_value: String) {
        let mut map = self.secrets.write().await;
        map.insert(secret_id.into(), Zeroizing::new(secret_value));
    }

    /// Removes a secret from the vault. Memory is zeroized on drop.
    pub async fn remove_secret(&self, secret_id: &str) -> bool {
        let mut map = self.secrets.write().await;
        map.remove(secret_id).is_some()
    }

    /// Checks if a secret exists in the vault.
    pub async fn has_secret(&self, secret_id: &str) -> bool {
        let map = self.secrets.read().await;
        map.contains_key(secret_id)
    }

    /// Injects an Authorization Bearer header directly without exposing raw bytes to guest runtime.
    pub async fn inject_authorization_header(
        &self,
        secret_ref: &SecretRef,
        headers: &mut reqwest::header::HeaderMap,
    ) -> Result<(), SecretVaultError> {
        self.inject_header(
            secret_ref,
            reqwest::header::AUTHORIZATION,
            Some("Bearer "),
            headers,
        )
        .await
    }

    /// Injects a custom HTTP header containing the referenced secret.
    pub async fn inject_header(
        &self,
        secret_ref: &SecretRef,
        header_name: reqwest::header::HeaderName,
        prefix: Option<&str>,
        headers: &mut reqwest::header::HeaderMap,
    ) -> Result<(), SecretVaultError> {
        let map = self.secrets.read().await;
        let secret = map
            .get(&secret_ref.0)
            .ok_or_else(|| SecretVaultError::SecretNotFound(secret_ref.0.clone()))?;

        let header_str = match prefix {
            Some(p) => format!("{}{}", p, secret.as_str()),
            None => secret.as_str().to_string(),
        };

        let mut header_value =
            reqwest::header::HeaderValue::from_str(&header_str).map_err(|e| {
                SecretVaultError::InvalidHeaderValue(secret_ref.0.clone(), e.to_string())
            })?;
        header_value.set_sensitive(true);

        headers.insert(header_name, header_value);
        Ok(())
    }
}
