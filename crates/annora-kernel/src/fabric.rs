use annora_core::{LoadingStrategy, ModelReference, RuntimeType};
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
