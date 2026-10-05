pub mod candle_runtime;
pub mod catalog;
pub mod downloader;
pub mod gguf_runtime;
pub mod manager;
pub mod model_registry;
pub mod onnx_runtime;
pub mod quantization;

pub use candle_runtime::*;
pub use catalog::*;
pub use downloader::*;
pub use gguf_runtime::*;
pub use manager::*;
pub use model_registry::*;
pub use onnx_runtime::*;
pub use quantization::*;

#[cfg(test)]
mod tests {
    use super::*;
    use annora_core::{LoadingStrategy, ModelReference};
    use std::io::Write;

    #[test]
    fn test_model_registry_budget_and_lru_eviction() {
        // 10 GB total budget
        let mut registry = ModelRegistry::new(10_000);

        let resident_model = ModelReference("SenseVoice-Small".into());
        let ondemand_a = ModelReference("UI-TARS-7B".into());
        let ondemand_b = ModelReference("VibeVoice-ASR".into());

        // Load AlwaysResident model (250 MB)
        registry.load_model(resident_model.clone(), 250, LoadingStrategy::AlwaysResident).unwrap();
        assert_eq!(registry.memory_used_mb(), 250);
        assert!(registry.is_loaded(&resident_model));

        // Load OnDemand model A (5,500 MB)
        registry.load_model(ondemand_a.clone(), 5_500, LoadingStrategy::OnDemand).unwrap();
        assert_eq!(registry.memory_used_mb(), 5_750);
        assert!(registry.is_loaded(&ondemand_a));

        // Load OnDemand model B (6,000 MB) -> Total would be 11,750 MB > 10,000 MB
        // ondemand_a must be evicted, but resident_model must NOT be evicted!
        registry.load_model(ondemand_b.clone(), 6_000, LoadingStrategy::OnDemand).unwrap();

        assert!(registry.is_loaded(&resident_model), "Resident model should never be evicted");
        assert!(registry.is_loaded(&ondemand_b), "New model should be loaded");
        assert!(!registry.is_loaded(&ondemand_a), "Oldest on-demand model should have been evicted");
        assert_eq!(registry.memory_used_mb(), 6_250);
    }

    #[test]
    fn test_catalog_and_provisioner_status() {
        let catalog = ModelCatalog::official_catalog();
        assert!(!catalog.is_empty(), "Official catalog must have entries");

        let guest_models = ModelCatalog::models_for_tier(ModelTier::Guest);
        let native_models = ModelCatalog::models_for_tier(ModelTier::Native);
        let sovereign_models = ModelCatalog::models_for_tier(ModelTier::Sovereign);

        assert!(guest_models.len() <= native_models.len());
        assert!(native_models.len() <= sovereign_models.len());

        let temp_dir = std::env::temp_dir().join("annora_test_models");
        let provisioner = ModelProvisioner::new(temp_dir.clone());
        let statuses = provisioner.list_all_status();
        assert_eq!(statuses.len(), catalog.len());

        // Test SHA-256 verification utility
        let test_file = temp_dir.join("test_sha256.bin");
        let _ = std::fs::create_dir_all(&temp_dir);
        let mut f = std::fs::File::create(&test_file).unwrap();
        f.write_all(b"Annora Sovereign Cognitive OS").unwrap();
        f.flush().unwrap();
        drop(f);

        let hash = ModelDownloader::compute_sha256(&test_file).unwrap();
        assert!(ModelDownloader::verify_sha256(&test_file, &hash).unwrap());
        assert!(!ModelDownloader::verify_sha256(&test_file, "deadbeef00000000000000000000000000000000000000000000000000000000").unwrap());

        let _ = std::fs::remove_file(test_file);
    }
}
