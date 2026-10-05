use annora_core::{CapabilityToken, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedoxSchemePacket {
    pub scheme: String,
    pub path: String,
    pub flags: usize,
    pub payload: Vec<u8>,
}

pub struct RedoxSchemeHandler {
    registered_schemes: HashMap<String, String>,
}

impl RedoxSchemeHandler {
    pub fn new() -> Self {
        let mut map = HashMap::new();
        map.insert("annora:".to_string(), "Cognitive Kernel Scheme".to_string());
        map.insert("memory:".to_string(), "Graph Storage Scheme".to_string());
        map.insert("agent:".to_string(), "Sandboxed WASM Agent Scheme".to_string());
        Self {
            registered_schemes: map,
        }
    }

    pub fn handle_open(
        &self,
        scheme: &str,
        _path: &str,
        _cap: &CapabilityToken,
    ) -> Result<usize> {
        if self.registered_schemes.contains_key(scheme) {
            Ok(42) // File descriptor
        } else {
            Err(annora_core::AnnoraError::SubstrateError(format!(
                "Redox scheme '{}' not found",
                scheme
            )))
        }
    }

    pub fn is_scheme_supported(&self, scheme: &str) -> bool {
        self.registered_schemes.contains_key(scheme)
    }
}

impl Default for RedoxSchemeHandler {
    fn default() -> Self {
        Self::new()
    }
}
