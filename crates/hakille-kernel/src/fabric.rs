use hakille_core::{LoadingStrategy, ModelReference, RuntimeType};
use hakille_inference::ConnectedModels;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CapabilityName {
    Listen,
    See,
    Speak,
    ActGui,
    Reason,
    ReasonHeavy,
    Identify,
    Diarize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityBinding {
    pub model_ref: ModelReference,
    pub runtime: RuntimeType,
    pub quantization: Option<String>,
    pub max_memory_mb: u64,
    pub loading_strategy: LoadingStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CognitiveManifest {
    pub capabilities: HashMap<CapabilityName, CapabilityBinding>,
}

pub struct CognitiveFabric {
    manifest: CognitiveManifest,
    fused_groups: Vec<HashSet<CapabilityName>>,
}

impl CognitiveFabric {
    pub fn new(manifest: CognitiveManifest) -> Self {
        let mut model_to_caps: HashMap<String, HashSet<CapabilityName>> = HashMap::new();
        for (cap, binding) in &manifest.capabilities {
            model_to_caps
                .entry(binding.model_ref.0.clone())
                .or_default()
                .insert(*cap);
        }

        let fused_groups = model_to_caps
            .into_values()
            .filter(|group| group.len() > 1)
            .collect();

        Self {
            manifest,
            fused_groups,
        }
    }

    /// Construct a fully integrated CognitiveFabric bound to the verified connected foundation models.
    pub fn from_connected_models(connected: &ConnectedModels) -> Self {
        let mut capabilities = HashMap::new();

        // 1. Listen (SenseVoice-Small INT8 ONNX)
        capabilities.insert(
            CapabilityName::Listen,
            CapabilityBinding {
                model_ref: ModelReference("sensevoice-small-int8".to_string()),
                runtime: RuntimeType::OnnxRuntime,
                quantization: Some("INT8".to_string()),
                max_memory_mb: 250,
                loading_strategy: LoadingStrategy::AlwaysResident,
            },
        );

        // 2. Identify & Diarize (CAM++ ONNX)
        capabilities.insert(
            CapabilityName::Identify,
            CapabilityBinding {
                model_ref: ModelReference("campplus-speaker-id".to_string()),
                runtime: RuntimeType::OnnxRuntime,
                quantization: None,
                max_memory_mb: 35,
                loading_strategy: LoadingStrategy::AlwaysResident,
            },
        );
        capabilities.insert(
            CapabilityName::Diarize,
            CapabilityBinding {
                model_ref: ModelReference("campplus-speaker-id".to_string()),
                runtime: RuntimeType::OnnxRuntime,
                quantization: None,
                max_memory_mb: 35,
                loading_strategy: LoadingStrategy::AlwaysResident,
            },
        );

        // 3. Reason & ReasonHeavy (Qwen2.5 0.5B GGUF)
        let qwen_name = connected
            .qwen2
            .name()
            .unwrap_or("qwen2.5-0.5b-instruct-q4km");
        capabilities.insert(
            CapabilityName::Reason,
            CapabilityBinding {
                model_ref: ModelReference(qwen_name.to_string()),
                runtime: RuntimeType::Gguf,
                quantization: Some("Q4_K_M".to_string()),
                max_memory_mb: 550,
                loading_strategy: LoadingStrategy::AlwaysResident,
            },
        );
        capabilities.insert(
            CapabilityName::ReasonHeavy,
            CapabilityBinding {
                model_ref: ModelReference(qwen_name.to_string()),
                runtime: RuntimeType::Gguf,
                quantization: Some("Q4_K_M".to_string()),
                max_memory_mb: 550,
                loading_strategy: LoadingStrategy::AlwaysResident,
            },
        );

        // 4. ActGui & See (UI-TARS 2B GGUF VLA)
        let ui_tars_name = connected.ui_tars.name().unwrap_or("ui-tars-2b-sft-q4km");
        capabilities.insert(
            CapabilityName::ActGui,
            CapabilityBinding {
                model_ref: ModelReference(ui_tars_name.to_string()),
                runtime: RuntimeType::Gguf,
                quantization: Some("Q4_K_M".to_string()),
                max_memory_mb: 1800,
                loading_strategy: LoadingStrategy::OnDemand,
            },
        );
        capabilities.insert(
            CapabilityName::See,
            CapabilityBinding {
                model_ref: ModelReference(ui_tars_name.to_string()),
                runtime: RuntimeType::Gguf,
                quantization: Some("Q4_K_M".to_string()),
                max_memory_mb: 1800,
                loading_strategy: LoadingStrategy::OnDemand,
            },
        );

        // 5. Speak (Kyutai Mimi Candle Native)
        capabilities.insert(
            CapabilityName::Speak,
            CapabilityBinding {
                model_ref: ModelReference("moshi-mimi-s2s".to_string()),
                runtime: RuntimeType::Candle,
                quantization: None,
                max_memory_mb: 450,
                loading_strategy: LoadingStrategy::AlwaysResident,
            },
        );

        Self::new(CognitiveManifest { capabilities })
    }

    pub fn manifest(&self) -> &CognitiveManifest {
        &self.manifest
    }

    pub fn get_binding(&self, capability: CapabilityName) -> Option<&CapabilityBinding> {
        self.manifest.capabilities.get(&capability)
    }

    pub fn is_fused(&self, cap1: CapabilityName, cap2: CapabilityName) -> bool {
        self.fused_groups
            .iter()
            .any(|group| group.contains(&cap1) && group.contains(&cap2))
    }

    pub fn supported_capabilities(&self) -> Vec<CapabilityName> {
        self.manifest.capabilities.keys().copied().collect()
    }
}
