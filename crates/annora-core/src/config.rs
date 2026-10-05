use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The tier of the Annora instance.
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
pub struct AnnoraConfig {
    pub tier: Tier,
    pub cognitive_manifest_path: String,
    pub constitution_path: String,
    pub data_dir: String,
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveManifestConfig {
    pub capabilities: HashMap<String, CapabilityBindingConfig>,
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
