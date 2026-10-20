use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The tier of the Hakille instance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Tier {
    /// Full sovereign agent capable of making long-term decisions.
    Sovereign,
    /// Native desktop/mobile companion.
    Native,
    /// Guest agent in a restricted environment.
    Guest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HakilleConfig {
    pub tier: Tier,
    pub cognitive_manifest_path: String,
    pub constitution_path: String,
    pub data_dir: String,
    pub log_level: String,
}

impl Default for HakilleConfig {
    fn default() -> Self {
        Self {
            tier: Tier::Native,
            cognitive_manifest_path: "cognitive_manifest.toml".to_string(),
            constitution_path: "constitution.toml".to_string(),
            data_dir: "data".to_string(),
            log_level: "info".to_string(),
        }
    }
}

impl HakilleConfig {
    /// Charge la configuration depuis un fichier.
    ///
    /// Stratégie sans nouvelle dépendance lourde :
    /// 1. Tente d'abord un parse JSON via `serde_json`.
    /// 2. Sinon, parse TOML minimal (paires `clé = "valeur"`, section-less ou
    ///    avec sections ignorées) pour les champs connus.
    pub fn load_from_file(path: &std::path::Path) -> crate::error::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        // 1. JSON fallback (serde_json déjà dans le workspace).
        if let Ok(cfg) = serde_json::from_str::<HakilleConfig>(&content) {
            cfg.validate()?;
            return Ok(cfg);
        }
        // 2. Parse TOML minimal manuel.
        let cfg = Self::parse_minimal_toml(&content)?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn parse_minimal_toml(content: &str) -> crate::error::Result<Self> {
        let mut base = Self::default();
        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
                continue;
            }
            let Some(eq) = line.find('=') else { continue };
            let key = line[..eq].trim().trim_matches('"').to_string();
            let mut value = line[eq + 1..].trim().to_string();
            // Strip inline comments (naïf : hors guillemets c'est suffisant ici).
            if let Some(hash) = value.find('#') {
                // Ne coupe que si le `#` est hors guillemets.
                let quote_count = value[..hash].matches('"').count();
                if quote_count % 2 == 0 {
                    value = value[..hash].trim().to_string();
                }
            }
            let value = value.trim().trim_matches('"').to_string();
            match key.as_str() {
                "tier" => {
                    base.tier = match value.to_lowercase().as_str() {
                        "sovereign" => Tier::Sovereign,
                        "native" => Tier::Native,
                        "guest" => Tier::Guest,
                        _ => {
                            return Err(crate::error::HakilleError::SerializationError(format!(
                                "unknown tier: {value}"
                            )));
                        }
                    };
                }
                "cognitive_manifest_path" => base.cognitive_manifest_path = value,
                "constitution_path" => base.constitution_path = value,
                "data_dir" => base.data_dir = value,
                "log_level" => base.log_level = value,
                _ => {}
            }
        }
        Ok(base)
    }

    /// Vérifie la cohérence de la configuration.
    pub fn validate(&self) -> crate::error::Result<()> {
        if self.cognitive_manifest_path.trim().is_empty() {
            return Err(crate::error::HakilleError::SerializationError(
                "cognitive_manifest_path must not be empty".into(),
            ));
        }
        if self.constitution_path.trim().is_empty() {
            return Err(crate::error::HakilleError::SerializationError(
                "constitution_path must not be empty".into(),
            ));
        }
        if self.data_dir.trim().is_empty() {
            return Err(crate::error::HakilleError::SerializationError(
                "data_dir must not be empty".into(),
            ));
        }
        if self.log_level.trim().is_empty() {
            return Err(crate::error::HakilleError::SerializationError(
                "log_level must not be empty".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveManifestConfig {
    pub capabilities: HashMap<String, CapabilityBindingConfig>,
}

impl CognitiveManifestConfig {
    /// Valide le manifest cognitif : au moins une capability et
    /// `max_memory_mb > 0` pour chaque binding.
    pub fn validate(&self) -> crate::error::Result<()> {
        for (name, binding) in &self.capabilities {
            if binding.max_memory_mb == 0 {
                return Err(crate::error::HakilleError::SerializationError(format!(
                    "capability '{name}': max_memory_mb must be > 0"
                )));
            }
            if binding.model_ref.trim().is_empty() {
                return Err(crate::error::HakilleError::SerializationError(format!(
                    "capability '{name}': model_ref must not be empty"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityBindingConfig {
    pub model_ref: String,
    pub runtime: RuntimeType,
    pub quantization: Option<String>,
    pub max_memory_mb: u64,
    pub loading_strategy: LoadingStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuntimeType {
    Candle,
    OnnxRuntime,
    Gguf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LoadingStrategy {
    AlwaysResident,
    OnDemand,
    Paged,
}
