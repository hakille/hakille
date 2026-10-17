pub mod audio_resample;
pub mod candle_runtime;
pub mod catalog;
pub mod downloader;
pub mod embed_onnx;
pub mod gguf_runtime;
pub mod llm_engine;
pub mod manager;
pub mod mel;
pub mod model_registry;
pub mod onnx_runtime;
pub mod ort_env;
pub mod ort_vad;
pub mod quantization;
pub mod sensevoice_onnx;
pub mod speaker_onnx;

pub use audio_resample::*;
pub use candle_runtime::*;
pub use catalog::*;
pub use downloader::*;
pub use embed_onnx::*;
pub use gguf_runtime::*;
pub use llm_engine::*;
pub use manager::*;
pub use mel::*;
pub use model_registry::*;
pub use onnx_runtime::*;
pub use ort_env::shared_ort_env;
pub use ort_vad::*;
pub use quantization::*;
pub use sensevoice_onnx::*;
pub use speaker_onnx::*;

// ---------------------------------------------------------------------------
// Honnêteté Inspector vs Engine (RÉEL prouvé par goldens, sauf legacy).
// ---------------------------------------------------------------------------

/// Statut honnête d'un modèle/moteur : inspection seule vs simulation vs réel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ModelStatus {
    /// Fichiers présents + headers parsés (GGUF magic, Safetensors JSON, ONNX meta).
    Inspected,
    /// Heuristique locale (identité ONNX, RMS+ZCR, sine hash, gabarits, mots-clés).
    Simulated,
    /// Forward neural réellement exécuté (ort/Candle) — prouvé par goldens
    /// (VAD, SenseVoice, CAM++, MiniLM, Qwen). Reste SIMULÉ/INSPECTÉ : les
    /// moteurs legacy heuristiques et le VLM UI-TARS (pas de forward VLM).
    Real,
}

impl ModelStatus {
    /// `true` uniquement pour une inférence neurale réellement exécutée.
    pub fn is_real(&self) -> bool {
        matches!(self, Self::Real)
    }

    pub fn label_fr(&self) -> &'static str {
        match self {
            Self::Inspected => "Inspecté — métadonnées seules",
            Self::Simulated => "Simulé — heuristique locale",
            Self::Real => "Réel — inférence neurale",
        }
    }

    pub fn badge(&self) -> &'static str {
        match self {
            Self::Inspected => "INSPECTÉ",
            Self::Simulated => "SIMULÉ",
            Self::Real => "RÉEL",
        }
    }

    /// Alias honnête : tout statut non-réel doit être rendu comme SIMULÉ/INSPECTÉ.
    pub fn status(&self) -> Self {
        *self
    }
}

/// Inspection seule : lecture de métadonnées / headers, jamais de forward neural.
pub trait ModelInspector {
    fn inspector_id(&self) -> &str;
    /// Statut honnête — par défaut `Inspected` (jamais `Real` sans preuve).
    fn status(&self) -> ModelStatus {
        ModelStatus::Inspected
    }
    fn is_real(&self) -> bool {
        false
    }
}

/// Moteur d'inférence : seul `run_real` compte comme exécution neurale.
/// Tant que le backend (`ort`/Candle forward) n'existe pas, `is_real()` vaut
/// `false` et `run_real()` retourne toujours une erreur honnête.
pub trait InferenceEngine {
    fn engine_id(&self) -> &str;
    fn is_real(&self) -> bool;
    /// Statut honnête du moteur (`Simulated` tant que le backend est absent).
    fn status(&self) -> ModelStatus;
    fn run_real(
        &self,
        inputs: std::collections::HashMap<String, crate::candle_runtime::TensorBuffer>,
    ) -> annora_core::Result<std::collections::HashMap<String, crate::candle_runtime::TensorBuffer>>;
}

impl ModelInspector for crate::onnx_runtime::OnnxSession {
    fn inspector_id(&self) -> &str {
        &self.model_path
    }
    fn status(&self) -> ModelStatus {
        ModelStatus::Simulated
    }
    fn is_real(&self) -> bool {
        false
    }
}

impl InferenceEngine for crate::onnx_runtime::OnnxSession {
    fn engine_id(&self) -> &str {
        &self.model_path
    }
    fn is_real(&self) -> bool {
        false
    }
    fn status(&self) -> ModelStatus {
        ModelStatus::Simulated
    }
    fn run_real(
        &self,
        _inputs: std::collections::HashMap<String, crate::candle_runtime::TensorBuffer>,
    ) -> annora_core::Result<std::collections::HashMap<String, crate::candle_runtime::TensorBuffer>>
    {
        Err(annora_core::AnnoraError::InferenceError(format!(
            "SIMULÉ — inspection-only : run_real indisponible pour '{}' (backend `ort` non branché, voir onnx_runtime.rs)",
            self.model_path
        )))
    }
}

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
        registry
            .load_model(resident_model.clone(), 250, LoadingStrategy::AlwaysResident)
            .unwrap();
        assert_eq!(registry.memory_used_mb(), 250);
        assert!(registry.is_loaded(&resident_model));

        // Load OnDemand model A (5,500 MB)
        registry
            .load_model(ondemand_a.clone(), 5_500, LoadingStrategy::OnDemand)
            .unwrap();
        assert_eq!(registry.memory_used_mb(), 5_750);
        assert!(registry.is_loaded(&ondemand_a));

        // Load OnDemand model B (6,000 MB) -> Total would be 11,750 MB > 10,000 MB
        // ondemand_a must be evicted, but resident_model must NOT be evicted!
        registry
            .load_model(ondemand_b.clone(), 6_000, LoadingStrategy::OnDemand)
            .unwrap();

        assert!(
            registry.is_loaded(&resident_model),
            "Resident model should never be evicted"
        );
        assert!(
            registry.is_loaded(&ondemand_b),
            "New model should be loaded"
        );
        assert!(
            !registry.is_loaded(&ondemand_a),
            "Oldest on-demand model should have been evicted"
        );
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
        assert!(
            !ModelDownloader::verify_sha256(
                &test_file,
                "deadbeef00000000000000000000000000000000000000000000000000000000"
            )
            .unwrap()
        );

        let _ = std::fs::remove_file(test_file);
    }

    #[test]
    fn test_connect_all_models_real_binaries() {
        let models_dir = if std::path::Path::new("./models").exists() {
            std::path::PathBuf::from("./models")
        } else if std::path::Path::new("../../models").exists() {
            std::path::PathBuf::from("../../models")
        } else {
            return; // Skip if models not present in environment
        };

        let provisioner = ModelProvisioner::new(models_dir);
        let connected = provisioner
            .connect_all_models()
            .expect("All 7 models should connect and parse cleanly");

        // 1. Verify Qwen2.5 GGUF parsing
        assert_eq!(connected.qwen2.architecture(), Some("qwen2"));
        assert!(
            connected.qwen2.tensor_count() >= 280,
            "Expected ~291 tensors for Qwen2.5 0.5B, got {}",
            connected.qwen2.tensor_count()
        );
        assert!(connected.qwen2.context_length().unwrap_or(0) >= 4096);

        // 2. Verify UI-TARS 2B GGUF parsing
        assert!(
            connected.ui_tars.tensor_count() >= 300,
            "Expected ~339 tensors for UI-TARS 2B, got {}",
            connected.ui_tars.tensor_count()
        );

        // 3. Verify Mimi Safetensors parsing
        assert!(
            connected.mimi.tensor_count() >= 50,
            "Expected Mimi multi-layer tensors, got {}",
            connected.mimi.tensor_count()
        );
        assert!(connected.mimi.codebook_count() >= 1);

        // 4. Verify SenseVoice Tokenizer
        assert_eq!(
            connected.sensevoice_tokenizer.vocab_size(),
            25055,
            "SenseVoice vocabulary must contain 25,055 tokens"
        );
        let decoded = connected.sensevoice_tokenizer.decode(&[3, 5, 8]);
        assert!(!decoded.is_empty(), "Tokenizer should decode token ids");

        // 5. Verify System Summary
        let summary = connected.system_summary();
        assert_eq!(summary.total_models_connected, 7);
        assert!(summary.ready_for_inspection);
        assert!(
            summary.total_disk_bytes > 2_000_000_000,
            "Total size should be ~2.4GB"
        );
    }

    #[test]
    fn test_tensor_buffer_math_and_argmax() {
        let t1 = TensorBuffer::new(vec![4], vec![10.0, 50.0, 30.0, 5.0]);
        assert_eq!(
            t1.argmax(),
            1,
            "argmax must return index 1 for highest value 50.0"
        );

        let t2 = TensorBuffer::new(vec![3], vec![100.0, 0.0, 1.0]);
        assert_eq!(
            t2.argmax(),
            0,
            "argmax must return index 0 for highest value 100.0"
        );

        let t3 = TensorBuffer::new(vec![3], vec![1.0, 2.0, 300.0]);
        assert_eq!(
            t3.argmax(),
            2,
            "argmax must return index 2 for highest value 300.0"
        );

        let empty = TensorBuffer::new(vec![0], vec![]);
        assert_eq!(empty.argmax(), 0);

        // Vector math: dot, l2_norm, normalize, cosine_similarity
        let a = TensorBuffer::new(vec![2], vec![3.0, 4.0]);
        assert_eq!(a.l2_norm(), 5.0);
        let an = a.normalize();
        assert!((an.l2_norm() - 1.0).abs() < 1e-6);

        let b = TensorBuffer::new(vec![2], vec![6.0, 8.0]);
        let sim = a.cosine_similarity(&b).unwrap();
        assert!(
            (sim - 1.0).abs() < 1e-5,
            "Collinear vectors should have cosine similarity 1.0"
        );

        // Greedy sampler
        assert_eq!(NeuralSampler::sample_greedy(&t1), 1);
    }
}
