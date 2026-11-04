use hakille_core::{HakilleError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::candle_runtime::SafetensorsModel;
use crate::catalog::{
    ModelArtifact, ModelCatalog, ModelEntry, ModelTier, validate_artifact_filename,
};
use crate::downloader::{ModelDownloader, ProgressCallback};
use crate::embed_onnx::MiniLmOnnx;
use crate::gguf_runtime::{GgufInferenceEngine, GgufModel};
use crate::onnx_runtime::{OnnxModelInfo, OnnxModelInspector, SenseVoiceTokenizer};
use crate::ort_vad::OrtVadSession;
use crate::sensevoice_onnx::SenseVoiceOnnx;
use crate::speaker_onnx::CampplusOnnx;

/// High-level provision status of a model in the local system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProvisionStatus {
    pub entry: ModelEntry,
    pub is_installed: bool,
    pub primary_path: Option<PathBuf>,
    pub total_disk_bytes: u64,
    pub missing_artifacts: Vec<String>,
    /// Présents mais taille incorrecte, illisibles, non réguliers ou chemin refusé.
    #[serde(default)]
    pub invalid_artifacts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStatusSummary {
    pub id: String,
    pub name: String,
    pub runtime: String,
    pub format: String,
    pub disk_size_bytes: u64,
    pub tensor_count: usize,
    pub parameter_estimate: u64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectedSystemSummary {
    pub total_models_connected: usize,
    pub total_disk_bytes: u64,
    pub total_parameters_estimate: u64,
    pub models: Vec<ModelStatusSummary>,
    pub ready_for_inspection: bool,
    pub connected_at: chrono::DateTime<chrono::Utc>,
}

/// Rapport d'intégrité SHA-256 par modèle (vérification cryptographique réelle).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityReport {
    pub model_id: String,
    pub filename: String,
    pub verified: bool,
    pub note: String,
}

/// Artefacts modèles connectés (headers parsés + SHA vérifié) ; l'inférence
/// RÉELLE est assurée par les moteurs dédiés (`ort_vad`, `sensevoice_onnx`,
/// `speaker_onnx`, `embed_onnx`, `llm_engine`, `rvq`, `formant`, `cv`).
#[derive(Clone)]
pub struct InspectedModels {
    pub qwen2: GgufModel,
    pub ui_tars: GgufModel,
    pub mimi: SafetensorsModel,
    pub sensevoice_tokenizer: SenseVoiceTokenizer,
    pub sensevoice_info: OnnxModelInfo,
    pub silero_vad_info: OnnxModelInfo,
    pub campplus_info: OnnxModelInfo,
    pub minilm_info: OnnxModelInfo,
    pub models_dir: PathBuf,
    pub total_disk_bytes: u64,
    pub connected_at: chrono::DateTime<chrono::Utc>,
    /// Un rapport par artefact primaire ou auxiliaire, vérifié avant parsing.
    pub integrity: Vec<IntegrityReport>,
    /// `true` uniquement si chaque artefact possède un checksum et a passé sa vérification.
    pub sha_verified: bool,
}

/// Compatibilité historique : `ConnectedModels` === `InspectedModels`
/// (artefacts inspectés + SHA ; statuts d'exécution dans `system_summary()`).
pub type ConnectedModels = InspectedModels;

impl InspectedModels {
    /// Statut de l'inventaire : ouvrir une session et réussir un forward sont
    /// des étapes distinctes, qui ne sont pas exécutées par cette inspection.
    pub fn status(&self) -> crate::ModelStatus {
        crate::ModelStatus::Inspected
    }

    /// Ce conteneur n'exécute aucun forward lui-même (`false`) ; les moteurs
    /// réels (`OrtVadSession`, `SenseVoiceOnnx`, `QwenEngine`…) portent leur
    /// propre `is_real() == true`.
    pub fn is_real(&self) -> bool {
        false
    }

    /// Résumé d'intégrité SHA : `Verified` quand le checksum catalogue passe,
    /// `Unverified` quand le catalogue ne fournit aucun checksum.
    pub fn integrity_summary(&self) -> String {
        let ok = self.integrity.iter().filter(|r| r.verified).count();
        format!(
            "INTÉGRITÉ — SHA vérifié {}/{} (aucune preuve de forward dans cet inventaire)",
            ok,
            self.integrity.len()
        )
    }
    pub fn qwen_engine(&self) -> GgufInferenceEngine {
        GgufInferenceEngine::new(self.qwen2.clone())
    }

    pub fn ui_tars_engine(&self) -> GgufInferenceEngine {
        GgufInferenceEngine::new(self.ui_tars.clone())
    }

    /// Ouvre le VAD réel (forward `ort`, fenêtres 512, état récurrent).
    /// Erreur LOUD si le checkpoint manque — aucun moteur simulé.
    pub fn vad_engine(&self) -> Result<OrtVadSession> {
        OrtVadSession::open(&self.silero_vad_info.path)
    }

    /// Ouvre le moteur ASR réel SenseVoice (ONNX + tokenizer SentencePiece).
    pub fn asr_engine(&self) -> Result<SenseVoiceOnnx> {
        SenseVoiceOnnx::open(
            &self.sensevoice_info.path,
            self.models_dir.join("tokens.txt"),
        )
    }

    /// Ouvre l'encodeur biométrique réel CAM++ (embeddings 192-D).
    pub fn speaker_engine(&self) -> Result<CampplusOnnx> {
        CampplusOnnx::open(&self.campplus_info.path)
    }

    /// Ouvre l'encodeur d'embeddings réel All-MiniLM-L6-v2 (384-D).
    pub fn embedding_engine(&self) -> Result<MiniLmOnnx> {
        MiniLmOnnx::open(
            &self.minilm_info.path,
            self.models_dir.join("minilm_tokenizer.json"),
        )
    }

    pub fn system_summary(&self) -> ConnectedSystemSummary {
        let mut models = Vec::new();

        // 1. Silero VAD — métadonnées inspectées.
        models.push(ModelStatusSummary {
            id: "silero-vad-v5".to_string(),
            name: "Silero Voice Activity Detector v5".to_string(),
            runtime: "OnnxRuntime".to_string(),
            format: "ONNX".to_string(),
            disk_size_bytes: self.silero_vad_info.file_size_bytes,
            tensor_count: 0, // Non mesuré par l’inspecteur ONNX.
            parameter_estimate: 2_000_000,
            status:
                "INSPECTÉ — graphe ONNX ; session VAD et forward non exécutés par cet inventaire"
                    .to_string(),
        });

        // 2. CAM++ — métadonnées inspectées.
        models.push(ModelStatusSummary {
            id: "campplus-speaker-id".to_string(),
            name: "CAM++ Neural Speaker Verifier".to_string(),
            runtime: "OnnxRuntime".to_string(),
            format: "ONNX".to_string(),
            disk_size_bytes: self.campplus_info.file_size_bytes,
            tensor_count: 0, // Non mesuré par l’inspecteur ONNX.
            parameter_estimate: 7_200_000,
            status:
                "INSPECTÉ — graphe ONNX ; extraction d'embeddings de locuteur à valider séparément"
                    .to_string(),
        });

        // 3. MiniLM — métadonnées inspectées.
        models.push(ModelStatusSummary {
            id: "all-minilm-l6-v2".to_string(),
            name: "All-MiniLM-L6-v2 Vector Embeddings".to_string(),
            runtime: "OnnxRuntime".to_string(),
            format: "ONNX".to_string(),
            disk_size_bytes: self.minilm_info.file_size_bytes,
            tensor_count: 0, // Non mesuré par l’inspecteur ONNX.
            parameter_estimate: 22_700_000,
            status: "INSPECTÉ — graphe ONNX et tokenizer ; encodage de texte à valider séparément"
                .to_string(),
        });

        // 4. SenseVoice — métadonnées et vocabulaire inspectés.
        models.push(ModelStatusSummary {
            id: "sensevoice-small-int8".to_string(),
            name: "SenseVoice-Small Multi-Task ASR".to_string(),
            runtime: "OnnxRuntime".to_string(),
            format: "ONNX INT8".to_string(),
            disk_size_bytes: self.sensevoice_info.file_size_bytes,
            tensor_count: 0, // Non mesuré par l’inspecteur ONNX.
            parameter_estimate: 150_000_000,
            status: format!(
                "INSPECTÉ — graphe ONNX et vocabulaire ({} tokens) ; transcription à valider séparément",
                self.sensevoice_tokenizer.vocab_size()
            ),
        });

        // 5. Qwen2.5 0.5B — métadonnées GGUF inspectées.
        models.push(ModelStatusSummary {
            id: "qwen2.5-0.5b-instruct-q4km".to_string(),
            name: self
                .qwen2
                .name()
                .unwrap_or("Qwen2.5-0.5B-Instruct")
                .to_string(),
            runtime: "Candle Native".to_string(),
            format: "GGUF Q4_K_M".to_string(),
            disk_size_bytes: std::fs::metadata(self.models_dir.join("qwen2.5-0.5b-instruct-q4_k_m.gguf")).map(|m| m.len()).unwrap_or(0),
            tensor_count: self.qwen2.tensor_count(),
            parameter_estimate: self.qwen2.estimated_params,
            status: format!(
                "INSPECTÉ — GGUF ({} tenseurs) ; chargement Candle et génération à valider séparément",
                self.qwen2.tensor_count()
            ),
        });

        // 6. UI-TARS 2B — métadonnées GGUF inspectées.
        models.push(ModelStatusSummary {
            id: "ui-tars-2b-sft-q4km".to_string(),
            name: self.ui_tars.name().unwrap_or("UI-TARS-2B-SFT").to_string(),
            runtime: "GGUF v3".to_string(),
            format: "GGUF Q4_K_M".to_string(),
            disk_size_bytes: std::fs::metadata(self.models_dir.join("ui-tars-2b-sft-q4_k_m.gguf")).map(|m| m.len()).unwrap_or(0),
            tensor_count: self.ui_tars.tensor_count(),
            parameter_estimate: self.ui_tars.estimated_params,
            status: format!(
                "INSPECTÉ — ({} tenseurs) forward VLM non exécuté par cet inventaire ; backend vision et projecteur à valider séparément",
                self.ui_tars.tensor_count()
            ),
        });

        // 7. Mimi — métadonnées du codec audio inspectées.
        models.push(ModelStatusSummary {
            id: "moshi-mimi-s2s".to_string(),
            name: "Kyutai Mimi Streaming Audio Codec".to_string(),
            runtime: "Candle Native".to_string(),
            format: "Safetensors".to_string(),
            disk_size_bytes: std::fs::metadata(self.models_dir.join("mimi_candle.safetensors")).map(|m| m.len()).unwrap_or(0),
            tensor_count: self.mimi.tensor_count(),
            parameter_estimate: self.mimi.total_parameters as u64,
            status: format!(
                "INSPECTÉ — checkpoint ({} tenseurs, {} codebooks) ; encodage et décodage audio neuraux à valider séparément",
                self.mimi.tensor_count(),
                self.mimi.codebook_count()
            ),
        });

        let total_parameters_estimate: u64 = models.iter().map(|m| m.parameter_estimate).sum();

        ConnectedSystemSummary {
            total_models_connected: models.len(),
            total_disk_bytes: self.total_disk_bytes,
            total_parameters_estimate,
            models,
            ready_for_inspection: true,
            connected_at: self.connected_at,
        }
    }
}

/// Coordinates local model storage, integrity verification, and zero-touch autonomous downloads.
#[derive(Clone)]
pub struct ModelProvisioner {
    cache_dir: PathBuf,
    downloader: ModelDownloader,
    catalog: Vec<ModelEntry>,
}

impl Default for ModelProvisioner {
    fn default() -> Self {
        let default_cache = std::env::var("HAKILLE_MODELS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./models"));
        Self::new(default_cache)
    }
}

impl ModelProvisioner {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            downloader: ModelDownloader::new(),
            catalog: ModelCatalog::official_catalog(),
        }
    }

    pub fn with_catalog(cache_dir: PathBuf, catalog: Vec<ModelEntry>) -> Self {
        Self {
            cache_dir,
            downloader: ModelDownloader::new(),
            catalog,
        }
    }

    /// The base directory where model artifacts are cached.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Chemin sûr d'un artefact plat ; aucun lien symbolique n'est accepté.
    pub fn artifact_path(&self, filename: &str) -> Result<PathBuf> {
        validate_artifact_filename(filename)?;
        let path = self.cache_dir.join(filename);
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if !meta.file_type().is_file() => {
                return Err(HakilleError::StorageError(format!(
                    "Artefact non régulier : {filename}"
                )));
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(HakilleError::StorageError(e.to_string()));
            }
            _ => {}
        }
        Ok(path)
    }

    pub fn catalog(&self) -> &[ModelEntry] {
        &self.catalog
    }

    /// Contrôle rapide de présence et taille ; ne lit pas les gigaoctets de poids.
    /// `is_installed` ne signifie ni SHA vérifié, ni modèle chargé, ni inférence réelle.
    pub fn status(&self, entry: &ModelEntry) -> ModelProvisionStatus {
        let mut missing = Vec::new();
        let mut invalid = Vec::new();
        let mut total_bytes = 0u64;
        for artifact in std::iter::once(&entry.primary_artifact).chain(&entry.auxiliary_artifacts) {
            if artifact.validate().is_err() {
                invalid.push(artifact.filename.clone());
                continue;
            }
            let path = match self.artifact_path(&artifact.filename) {
                Ok(path) => path,
                Err(_) => {
                    invalid.push(artifact.filename.clone());
                    continue;
                }
            };
            match std::fs::metadata(&path) {
                Ok(meta) => {
                    total_bytes = total_bytes.saturating_add(meta.len());
                    if meta.len() != artifact.size_bytes {
                        invalid.push(artifact.filename.clone());
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    missing.push(artifact.filename.clone())
                }
                Err(_) => invalid.push(artifact.filename.clone()),
            }
        }
        let is_installed = missing.is_empty() && invalid.is_empty();
        ModelProvisionStatus {
            entry: entry.clone(),
            is_installed,
            primary_path: if is_installed {
                self.artifact_path(&entry.primary_artifact.filename).ok()
            } else {
                None
            },
            total_disk_bytes: total_bytes,
            missing_artifacts: missing,
            invalid_artifacts: invalid,
        }
    }

    pub fn list_all_status(&self) -> Vec<ModelProvisionStatus> {
        self.catalog.iter().map(|m| self.status(m)).collect()
    }

    /// Vérification indépendante par modèle, avant toute ouverture du moteur.
    /// Un rapport par artefact, y compris les auxiliaires. Une absence de SHA
    /// reste explicitement non vérifiée, même si la taille correspond.
    /// `Ok(reports)` n'est pas une réussite d'intégrité : l'appelant doit exiger
    /// `reports.iter().all(|report| report.verified)` avant de charger un moteur.
    /// Les erreurs de catalogue/chemin/I/O sont retournées via `Err`.
    pub fn verify_model_by_id(&self, id: &str) -> Result<Vec<IntegrityReport>> {
        let entry = self
            .catalog
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| HakilleError::InferenceError(format!("Modèle inconnu : {id}")))?;
        self.verify_entry(entry)
    }

    fn verify_entry(&self, entry: &ModelEntry) -> Result<Vec<IntegrityReport>> {
        let mut reports = Vec::new();
        let mut filenames = std::collections::HashSet::new();
        for artifact in std::iter::once(&entry.primary_artifact).chain(&entry.auxiliary_artifacts) {
            artifact.validate()?;
            if !filenames.insert(artifact.filename.to_ascii_lowercase()) {
                return Err(HakilleError::StorageError(
                    "Noms d'artefacts dupliqués dans le modèle".into(),
                ));
            }
            let path = self.artifact_path(&artifact.filename)?;
            let (verified, note) = match std::fs::metadata(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    (false, "Artefact absent".to_string())
                }
                Err(error) => return Err(HakilleError::StorageError(error.to_string())),
                Ok(meta) if meta.len() != artifact.size_bytes => (
                    false,
                    format!(
                        "Taille incorrecte : {} octets, {} attendus",
                        meta.len(),
                        artifact.size_bytes
                    ),
                ),
                Ok(_) => match &artifact.sha256_checksum {
                    None => (
                        false,
                        "Non vérifié : aucun SHA-256 au catalogue".to_string(),
                    ),
                    Some(hash) => {
                        let matches = ModelDownloader::verify_sha256(&path, hash)?;
                        (
                            matches,
                            if matches {
                                "Taille et SHA-256 vérifiés ; inférence non exécutée"
                            } else {
                                "SHA-256 incorrect"
                            }
                            .to_string(),
                        )
                    }
                },
            };
            reports.push(IntegrityReport {
                model_id: entry.id.clone(),
                filename: artifact.filename.clone(),
                verified,
                note,
            });
        }
        Ok(reports)
    }

    pub fn verify_integrity(&self, entry: &ModelEntry) -> Result<bool> {
        Ok(self
            .verify_entry(entry)?
            .iter()
            .all(|report| report.verified))
    }

    /// Connect, inspect, and link all 7 downloaded foundation models across the cognitive pipeline.
    pub fn connect_all_models(&self) -> Result<InspectedModels> {
        let mut integrity = Vec::new();
        for id in [
            "silero-vad-v5",
            "campplus-speaker-id",
            "all-minilm-l6-v2",
            "sensevoice-small-int8",
            "qwen2.5-0.5b-instruct-q4km",
            "ui-tars-2b-sft-q4km",
            "moshi-mimi-s2s",
        ] {
            let reports = self.verify_model_by_id(id)?;
            if let Some(failed) = reports.iter().find(|report| !report.verified) {
                return Err(HakilleError::CryptoError(format!(
                    "Inspection refusée pour {} : {} ({})",
                    id, failed.filename, failed.note
                )));
            }
            integrity.extend(reports);
        }
        let qwen_path = self.artifact_path("qwen2.5-0.5b-instruct-q4_k_m.gguf")?;
        let ui_tars_path = self.artifact_path("ui-tars-2b-sft-q4_k_m.gguf")?;
        let mimi_path = self.artifact_path("mimi_candle.safetensors")?;
        let tokens_path = self.artifact_path("tokens.txt")?;
        let sensevoice_path = self.artifact_path("sensevoice_small.int8.onnx")?;
        let vad_path = self.artifact_path("silero_vad.onnx")?;
        let campplus_path = self.artifact_path("campplus_zh_en_16k_advanced.onnx")?;
        let minilm_path = self.artifact_path("all-minilm-l6-v2.onnx")?;
        let qwen_tokenizer_path = self.artifact_path("tokenizer.json")?;
        let minilm_tokenizer_path = self.artifact_path("minilm_tokenizer.json")?;

        let required = [
            (&qwen_path, "Qwen2.5 0.5B GGUF"),
            (&qwen_tokenizer_path, "Qwen tokenizer.json"),
            (&ui_tars_path, "UI-TARS 2B GGUF"),
            (&mimi_path, "Mimi Safetensors"),
            (&tokens_path, "SenseVoice Vocabulary tokens.txt"),
            (&sensevoice_path, "SenseVoice INT8 ONNX"),
            (&vad_path, "Silero VAD ONNX"),
            (&campplus_path, "CAM++ Speaker ID ONNX"),
            (&minilm_path, "All-MiniLM-L6-v2 ONNX"),
            (&minilm_tokenizer_path, "MiniLM tokenizer.json"),
        ];

        for (path, label) in &required {
            if !integrity.iter().any(|report| {
                path.file_name()
                    .is_some_and(|name| name == std::ffi::OsStr::new(&report.filename))
                    && report.verified
            }) {
                return Err(HakilleError::CryptoError(format!(
                    "Aucun checksum vérifié pour l'artefact utilisé : {label}"
                )));
            }
            if !path.exists() {
                return Err(HakilleError::InferenceError(format!(
                    "Model artifact missing for {}: expected at {}",
                    label,
                    path.display()
                )));
            }
        }

        // 1. Parse Qwen2.5 GGUF
        let qwen2 = GgufModel::read_from_file(&qwen_path)?;

        // 2. Parse UI-TARS 2B GGUF
        let ui_tars = GgufModel::read_from_file(&ui_tars_path)?;

        // 3. Parse Mimi Safetensors
        let mimi = SafetensorsModel::read_from_file(&mimi_path)?;

        // 4. Parse SenseVoice tokens.txt
        let sensevoice_tokenizer = SenseVoiceTokenizer::from_file(&tokens_path)?;

        // 5. Inspect ONNX models
        let sensevoice_info = OnnxModelInspector::inspect_file(&sensevoice_path)?;
        let silero_vad_info = OnnxModelInspector::inspect_file(&vad_path)?;
        let campplus_info = OnnxModelInspector::inspect_file(&campplus_path)?;
        let minilm_info = OnnxModelInspector::inspect_file(&minilm_path)?;

        // Inclure tous les auxiliaires vérifiés, notamment le projecteur vision.
        let mut seen = std::collections::HashSet::new();
        let mut total_disk_bytes = 0u64;
        for report in &integrity {
            if seen.insert(&report.filename) {
                let path = self.artifact_path(&report.filename)?;
                total_disk_bytes += std::fs::metadata(path)
                    .map_err(|e| HakilleError::StorageError(e.to_string()))?
                    .len();
            }
        }
        let sha_verified = true;

        Ok(InspectedModels {
            qwen2,
            ui_tars,
            mimi,
            sensevoice_tokenizer,
            sensevoice_info,
            silero_vad_info,
            campplus_info,
            minilm_info,
            models_dir: self.cache_dir.clone(),
            total_disk_bytes,
            connected_at: chrono::Utc::now(),
            integrity,
            sha_verified,
        })
    }

    /// Download and provision all artifacts for a given model entry.
    pub async fn provision_model(
        &self,
        entry: &ModelEntry,
        progress: Option<ProgressCallback>,
    ) -> Result<PathBuf> {
        tokio::fs::create_dir_all(&self.cache_dir)
            .await
            .map_err(|e| {
                HakilleError::StorageError(format!(
                    "Failed to create model cache dir {}: {}",
                    self.cache_dir.display(),
                    e
                ))
            })?;

        // Valider tous les chemins avant de commencer les effets sur disque.
        let mut filenames = std::collections::HashSet::new();
        for artifact in std::iter::once(&entry.primary_artifact).chain(&entry.auxiliary_artifacts) {
            artifact.validate()?;
            if !filenames.insert(artifact.filename.to_ascii_lowercase()) {
                return Err(HakilleError::StorageError(
                    "Noms d'artefacts dupliqués dans le modèle".into(),
                ));
            }
            self.artifact_path(&artifact.filename)?;
        }
        for artifact in entry
            .auxiliary_artifacts
            .iter()
            .chain(std::iter::once(&entry.primary_artifact))
        {
            self.provision_artifact(artifact, progress.clone()).await?;
        }
        self.artifact_path(&entry.primary_artifact.filename)
    }

    async fn provision_artifact(
        &self,
        artifact: &ModelArtifact,
        progress: Option<ProgressCallback>,
    ) -> Result<PathBuf> {
        let destination = self.artifact_path(&artifact.filename)?;
        // Toujours passer par le vérificateur, même pour un cache déjà présent.
        match self
            .downloader
            .download_file(
                &artifact.primary_url,
                &destination,
                Some(artifact.size_bytes),
                artifact.sha256_checksum.as_deref(),
                progress.clone(),
            )
            .await
        {
            Ok(path) => Ok(path),
            Err(error) => match &artifact.fallback_url {
                Some(fallback) => {
                    self.downloader
                        .download_file(
                            fallback,
                            &destination,
                            Some(artifact.size_bytes),
                            artifact.sha256_checksum.as_deref(),
                            progress,
                        )
                        .await
                }
                None => Err(error),
            },
        }
    }

    /// Provision a model by its identifier.
    pub async fn provision_by_id(
        &self,
        id: &str,
        progress: Option<ProgressCallback>,
    ) -> Result<PathBuf> {
        let entry = self
            .catalog
            .iter()
            .find(|m| m.id == id)
            .cloned()
            .ok_or_else(|| {
                HakilleError::InferenceError(format!("Model ID '{}' not found in catalog", id))
            })?;
        self.provision_model(&entry, progress).await
    }

    /// Provision all models required for a given deployment tier (Guest, Native, Sovereign).
    pub async fn provision_tier(
        &self,
        tier: ModelTier,
        progress: Option<ProgressCallback>,
    ) -> Result<Vec<PathBuf>> {
        let target_models = self.catalog.iter().filter(|entry| match tier {
            ModelTier::Guest => entry.tier == ModelTier::Guest,
            ModelTier::Native => entry.tier != ModelTier::Sovereign,
            ModelTier::Sovereign => true,
        });
        let mut provisioned = Vec::new();

        for entry in target_models {
            let path = self.provision_model(entry, progress.clone()).await?;
            provisioned.push(path);
        }

        Ok(provisioned)
    }

    /// Provision all models defined in the official catalog.
    pub async fn provision_all(&self, progress: Option<ProgressCallback>) -> Result<Vec<PathBuf>> {
        let mut provisioned = Vec::new();
        for entry in &self.catalog {
            let path = self.provision_model(entry, progress.clone()).await?;
            provisioned.push(path);
        }
        Ok(provisioned)
    }

    /// Remove a model and its associated artifacts from local disk storage.
    pub fn remove_model(&self, entry: &ModelEntry) -> Result<()> {
        let mut locks = Vec::new();
        for artifact in std::iter::once(&entry.primary_artifact).chain(&entry.auxiliary_artifacts) {
            artifact.validate()?;
            self.artifact_path(&artifact.filename)?;
        }
        for artifact in std::iter::once(&entry.primary_artifact).chain(&entry.auxiliary_artifacts) {
            let path = self.artifact_path(&artifact.filename)?;
            if self.cache_dir.exists() {
                locks.push(crate::downloader::acquire_download_lock(&path)?);
            }
        }
        let primary_dest = self.artifact_path(&entry.primary_artifact.filename)?;
        if primary_dest.exists() {
            std::fs::remove_file(&primary_dest).map_err(|e| {
                HakilleError::StorageError(format!(
                    "Failed to delete {}: {}",
                    primary_dest.display(),
                    e
                ))
            })?;
        }

        for aux in &entry.auxiliary_artifacts {
            let aux_dest = self.artifact_path(&aux.filename)?;
            if aux_dest.exists() {
                std::fs::remove_file(&aux_dest).map_err(|e| {
                    HakilleError::StorageError(format!(
                        "Failed to delete {}: {}",
                        aux_dest.display(),
                        e
                    ))
                })?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod provisioning_tests {
    use super::*;
    use sha2::{Digest, Sha256};

    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("hakille-provision-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn entry() -> ModelEntry {
        let mut entry = ModelCatalog::official_catalog().remove(0);
        entry.primary_artifact.size_bytes = 4;
        entry.primary_artifact.sha256_checksum = Some(format!("{:x}", Sha256::digest(b"good")));
        let mut aux = entry.primary_artifact.clone();
        aux.filename = "tokens.json".into();
        entry.auxiliary_artifacts = vec![aux];
        entry
    }

    #[tokio::test]
    async fn provision_repairs_existing_primary_and_auxiliary_artifacts() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let dir = TestDir::new();
        let mut entry = entry();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        entry.primary_artifact.primary_url = format!("{base}/primary");
        entry.primary_artifact.fallback_url = None;
        entry.auxiliary_artifacts[0].primary_url = format!("{base}/auxiliary");
        entry.auxiliary_artifacts[0].fallback_url = None;
        for artifact in std::iter::once(&entry.primary_artifact).chain(&entry.auxiliary_artifacts) {
            std::fs::write(dir.0.join(&artifact.filename), b"evil").unwrap();
        }
        let server = tokio::spawn(async move {
            for route in ["/auxiliary", "/primary"] {
                let (mut socket, _) =
                    tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept())
                        .await
                        .unwrap()
                        .unwrap();
                let mut header = Vec::new();
                let mut buffer = [0u8; 1024];
                while !header.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    let read = socket.read(&mut buffer).await.unwrap();
                    assert!(read > 0);
                    header.extend_from_slice(&buffer[..read]);
                }
                assert!(String::from_utf8(header).unwrap().contains(route));
                socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\ngood",
                    )
                    .await
                    .unwrap();
            }
        });
        let mut provisioner = ModelProvisioner::with_catalog(dir.0.clone(), vec![entry.clone()]);
        provisioner.downloader = ModelDownloader::local_test_client();
        provisioner.provision_model(&entry, None).await.unwrap();
        server.await.unwrap();
        assert!(provisioner.verify_integrity(&entry).unwrap());
        // Le serveur est arrêté : une seconde demande doit réutiliser le cache vérifié.
        provisioner.provision_model(&entry, None).await.unwrap();
    }

    #[test]
    fn installation_inventory_is_fast_but_integrity_checks_all_artifacts() {
        let dir = TestDir::new();
        let entry = entry();
        let provisioner = ModelProvisioner::with_catalog(dir.0.clone(), vec![entry.clone()]);
        std::fs::write(dir.0.join(&entry.primary_artifact.filename), b"good").unwrap();
        let status = provisioner.status(&entry);
        assert!(!status.is_installed);
        assert_eq!(status.missing_artifacts, ["tokens.json"]);
        std::fs::write(dir.0.join("tokens.json"), b"evil").unwrap();
        // La taille est correcte ; seul le SHA explicite peut détecter l'altération.
        assert!(provisioner.status(&entry).is_installed);
        let reports = provisioner.verify_model_by_id(&entry.id).unwrap();
        assert!(reports[0].verified);
        assert!(!reports[1].verified);
        assert!(!provisioner.verify_integrity(&entry).unwrap());
        std::fs::write(dir.0.join("tokens.json"), b"good").unwrap();
        assert!(provisioner.verify_integrity(&entry).unwrap());
    }

    #[test]
    fn wrong_sizes_and_missing_checksums_are_never_verified() {
        let dir = TestDir::new();
        let mut entry = entry();
        entry.auxiliary_artifacts.clear();
        let provisioner = ModelProvisioner::with_catalog(dir.0.clone(), vec![entry.clone()]);
        std::fs::write(dir.0.join(&entry.primary_artifact.filename), b"too long").unwrap();
        assert!(!provisioner.status(&entry).is_installed);
        assert_eq!(provisioner.status(&entry).invalid_artifacts.len(), 1);
        assert!(!provisioner.verify_integrity(&entry).unwrap());
        std::fs::write(dir.0.join(&entry.primary_artifact.filename), b"good").unwrap();
        entry.primary_artifact.sha256_checksum = None;
        assert!(!provisioner.verify_integrity(&entry).unwrap());
    }

    #[tokio::test]
    async fn unsafe_auxiliary_path_is_rejected_before_primary_deletion_or_download() {
        let dir = TestDir::new();
        let mut entry = entry();
        std::fs::write(dir.0.join(&entry.primary_artifact.filename), b"good").unwrap();
        let provisioner = ModelProvisioner::new(dir.0.clone());
        for filename in [
            "../outside",
            "/absolute",
            "C:\\outside",
            "folder/file",
            "folder\\file",
            "NUL.onnx",
            "model.part",
            "model.download.lock",
            "a?b",
        ] {
            assert!(provisioner.artifact_path(filename).is_err(), "{filename}");
            entry.auxiliary_artifacts[0].filename = filename.into();
            assert!(provisioner.remove_model(&entry).is_err());
            assert!(provisioner.provision_model(&entry, None).await.is_err());
            assert_eq!(
                std::fs::read(dir.0.join(&entry.primary_artifact.filename)).unwrap(),
                b"good"
            );
        }
    }

    #[test]
    fn connection_refuses_bad_integrity_before_parsing_any_model() {
        let dir = TestDir::new();
        let mut entry = entry();
        entry.auxiliary_artifacts.clear();
        std::fs::write(dir.0.join(&entry.primary_artifact.filename), b"evil").unwrap();
        let provisioner = ModelProvisioner::with_catalog(dir.0.clone(), vec![entry]);
        assert!(matches!(
            provisioner.connect_all_models(),
            Err(HakilleError::CryptoError(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_artifacts_are_not_installed_or_verified() {
        let dir = TestDir::new();
        let mut entry = entry();
        entry.auxiliary_artifacts.clear();
        let outside = dir.0.join("outside");
        std::fs::write(&outside, b"good").unwrap();
        std::os::unix::fs::symlink(&outside, dir.0.join(&entry.primary_artifact.filename)).unwrap();
        let provisioner = ModelProvisioner::with_catalog(dir.0.clone(), vec![entry.clone()]);
        assert!(!provisioner.status(&entry).is_installed);
        assert!(provisioner.verify_integrity(&entry).is_err());
        assert!(provisioner.remove_model(&entry).is_err());
        assert_eq!(std::fs::read(outside).unwrap(), b"good");
    }
}
