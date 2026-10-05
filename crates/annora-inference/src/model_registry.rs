use annora_core::{LoadingStrategy, ModelReference, Result, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub model_ref: ModelReference,
    pub memory_size_mb: u64,
    pub loading_strategy: LoadingStrategy,
    pub last_used: Timestamp,
    pub is_paged_out: bool,
}

pub struct ModelRegistry {
    max_memory_budget_mb: u64,
    current_memory_used_mb: u64,
    loaded_models: HashMap<ModelReference, ModelMetadata>,
}

impl ModelRegistry {
    pub fn new(max_memory_budget_mb: u64) -> Self {
        Self {
            max_memory_budget_mb,
            current_memory_used_mb: 0,
            loaded_models: HashMap::new(),
        }
    }

    pub fn is_loaded(&self, model_ref: &ModelReference) -> bool {
        self.loaded_models.contains_key(model_ref)
    }

    pub fn load_model(
        &mut self,
        model_ref: ModelReference,
        memory_size_mb: u64,
        strategy: LoadingStrategy,
    ) -> Result<()> {
        while self.current_memory_used_mb + memory_size_mb > self.max_memory_budget_mb {
            if !self.evict_lru() {
                break;
            }
        }

        self.current_memory_used_mb += memory_size_mb;
        self.loaded_models.insert(
            model_ref.clone(),
            ModelMetadata {
                model_ref,
                memory_size_mb,
                loading_strategy: strategy,
                last_used: Timestamp::now(),
                is_paged_out: false,
            },
        );

        Ok(())
    }

    pub fn evict_lru(&mut self) -> bool {
        let mut candidate = None;
        let mut oldest_ts = Timestamp::now();

        for (mref, meta) in &self.loaded_models {
            if meta.loading_strategy == LoadingStrategy::AlwaysResident {
                continue;
            }
            if meta.last_used < oldest_ts {
                oldest_ts = meta.last_used.clone();
                candidate = Some(mref.clone());
            }
        }

        if let Some(to_evict) = candidate {
            if let Some(removed) = self.loaded_models.remove(&to_evict) {
                self.current_memory_used_mb = self
                    .current_memory_used_mb
                    .saturating_sub(removed.memory_size_mb);
                return true;
            }
        }

        false
    }

    pub fn mark_used(&mut self, model_ref: &ModelReference) {
        if let Some(meta) = self.loaded_models.get_mut(model_ref) {
            meta.last_used = Timestamp::now();
        }
    }

    pub fn memory_used_mb(&self) -> u64 {
        self.current_memory_used_mb
    }

    pub fn budget_mb(&self) -> u64 {
        self.max_memory_budget_mb
    }
}
