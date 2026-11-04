use hakille_core::{ModelReference, RuntimeType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelTier {
    Guest,
    Native,
    Sovereign,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PipelineRole {
    VoiceActivity,
    Biometrics,
    PerceptionAsr,
    MemoryEmbeddings,
    System1Reasoner,
    System2Reasoner,
    GuiAutomation,
    VoiceSpeechToSpeech,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelFormat {
    Onnx,
    Gguf,
    Safetensors,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelArtifact {
    pub filename: String,
    pub primary_url: String,
    pub fallback_url: Option<String>,
    pub size_bytes: u64,
    pub sha256_checksum: Option<String>,
    pub is_essential: bool,
}

impl ModelArtifact {
    /// Nom de fichier portable : aucun sous-répertoire ni chemin absolu.
    pub fn validate(&self) -> hakille_core::Result<()> {
        validate_artifact_filename(&self.filename)?;
        if self.size_bytes == 0 {
            return Err(hakille_core::HakilleError::InferenceError(
                "Taille d'artefact nulle dans le catalogue".into(),
            ));
        }
        if let Some(hash) = &self.sha256_checksum
            && (hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(hakille_core::HakilleError::InferenceError(
                "SHA-256 invalide dans le catalogue".into(),
            ));
        }
        Ok(())
    }
}

pub fn validate_artifact_filename(filename: &str) -> hakille_core::Result<()> {
    if filename.is_empty()
        || matches!(filename, "." | "..")
        || filename.contains(['/', '\\', ':', '\0'])
        || filename.ends_with(['.', ' '])
        || filename.to_ascii_lowercase().ends_with(".part")
        || filename.to_ascii_lowercase().ends_with(".download.lock")
        || filename
            .chars()
            .any(|c| c.is_control() || matches!(c, '<' | '>' | '"' | '|' | '?' | '*'))
    {
        return Err(hakille_core::HakilleError::StorageError(
            "Nom d'artefact non portable ou traversée de chemin refusée".into(),
        ));
    }
    let stem = filename
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    {
        return Err(hakille_core::HakilleError::StorageError(
            "Nom de périphérique réservé refusé".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub model_ref: ModelReference,
    pub role: PipelineRole,
    pub tier: ModelTier,
    pub runtime: RuntimeType,
    pub format: ModelFormat,
    pub primary_artifact: ModelArtifact,
    pub auxiliary_artifacts: Vec<ModelArtifact>,
    pub expected_ram_mb: u64,
    pub expected_vram_mb: u64,
    pub description: String,
}

pub struct ModelCatalog;

impl ModelCatalog {
    pub fn official_catalog() -> Vec<ModelEntry> {
        vec![
            // 1. Silero VAD v5 (Ultra-low latency speech activity detector)
            ModelEntry {
                id: "silero-vad-v5".to_string(),
                name: "Silero Voice Activity Detector v5".to_string(),
                model_ref: ModelReference("silero-vad-v5".to_string()),
                role: PipelineRole::VoiceActivity,
                tier: ModelTier::Guest,
                runtime: RuntimeType::OnnxRuntime,
                format: ModelFormat::Onnx,
                primary_artifact: ModelArtifact {
                    filename: "silero_vad.onnx".to_string(),
                    primary_url: "https://github.com/snakers4/silero-vad/raw/master/src/silero_vad/data/silero_vad.onnx".to_string(),
                    fallback_url: Some("https://raw.githubusercontent.com/snakers4/silero-vad/master/src/silero_vad/data/silero_vad.onnx".to_string()),
                    size_bytes: 2_327_524,
                    sha256_checksum: Some("1a153a22f4509e292a94e67d6f9b85e8deb25b4988682b7e174c65279d8788e3".to_string()),
                    is_essential: true,
                },
                auxiliary_artifacts: vec![],
                expected_ram_mb: 15,
                expected_vram_mb: 0,
                description: "Détection locale d'activité vocale avec état récurrent ONNX ; latence à mesurer sur l'hôte.".to_string(),
            },

            // 2. CAM++ Speaker Verification (Biometric Identity)
            ModelEntry {
                id: "campplus-speaker-id".to_string(),
                name: "CAM++ Neural Speaker Verifier".to_string(),
                model_ref: ModelReference("campplus-speaker-id".to_string()),
                role: PipelineRole::Biometrics,
                tier: ModelTier::Guest,
                runtime: RuntimeType::OnnxRuntime,
                format: ModelFormat::Onnx,
                primary_artifact: ModelArtifact {
                    filename: "campplus_zh_en_16k_advanced.onnx".to_string(),
                    primary_url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx".to_string(),
                    fallback_url: None,
                    size_bytes: 28_281_164,
                    sha256_checksum: Some("aa3cfc16963a10586a9393f5035d6d6b57e98d358b347f80c2a30bf4f00ceba2".to_string()),
                    is_essential: true,
                },
                auxiliary_artifacts: vec![],
                expected_ram_mb: 30,
                expected_vram_mb: 0,
                description: "Encodeur ONNX d'embeddings de locuteur normalisés pour comparaison.".to_string(),
            },

            // 3. All-MiniLM-L6-v2 (Vector Embeddings for Sovereign Memory Knowledge Graph)
            ModelEntry {
                id: "all-minilm-l6-v2".to_string(),
                name: "All-MiniLM-L6-v2 Vector Embeddings".to_string(),
                model_ref: ModelReference("all-minilm-l6-v2".to_string()),
                role: PipelineRole::MemoryEmbeddings,
                tier: ModelTier::Native,
                runtime: RuntimeType::OnnxRuntime,
                format: ModelFormat::Onnx,
                primary_artifact: ModelArtifact {
                    filename: "all-minilm-l6-v2.onnx".to_string(),
                    primary_url: "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx".to_string(),
                    fallback_url: None,
                    size_bytes: 90_405_214,
                    sha256_checksum: Some("6fd5d72fe4589f189f8ebc006442dbb529bb7ce38f8082112682524616046452".to_string()),
                    is_essential: true,
                },
                auxiliary_artifacts: vec![ModelArtifact {
                    filename: "minilm_tokenizer.json".to_string(),
                    primary_url: "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json".to_string(),
                    fallback_url: None,
                    size_bytes: 466_247,
                    sha256_checksum: Some("be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037".to_string()),
                    is_essential: true,
                }],
                expected_ram_mb: 90,
                expected_vram_mb: 0,
                description: "Encodage sémantique de textes en 384 dimensions avec modèle ONNX et tokenizer WordPiece associés.".to_string(),
            },

            // 4. SenseVoice-Small INT8 (Front-End Multi-Task ASR)
            ModelEntry {
                id: "sensevoice-small-int8".to_string(),
                name: "SenseVoice-Small Multi-Task ASR".to_string(),
                model_ref: ModelReference("sensevoice-small-int8".to_string()),
                role: PipelineRole::PerceptionAsr,
                tier: ModelTier::Native,
                runtime: RuntimeType::OnnxRuntime,
                format: ModelFormat::Onnx,
                primary_artifact: ModelArtifact {
                    filename: "sensevoice_small.int8.onnx".to_string(),
                    primary_url: "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main/model.int8.onnx".to_string(),
                    fallback_url: None,
                    size_bytes: 239_233_841,
                    sha256_checksum: Some("c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51".to_string()),
                    is_essential: true,
                },
                auxiliary_artifacts: vec![
                    ModelArtifact {
                        filename: "tokens.txt".to_string(),
                        primary_url: "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main/tokens.txt".to_string(),
                        fallback_url: None,
                        size_bytes: 315_894,
                        sha256_checksum: Some("f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc".to_string()),
                        is_essential: true,
                    }
                ],
                expected_ram_mb: 230,
                expected_vram_mb: 0,
                description: "Reconnaissance vocale multilingue locale avec checkpoint ONNX INT8, prétraitement acoustique et décodage CTC.".to_string(),
            },

            // 5. Qwen2.5 0.5B Instruct Q4_K_M (Fast System 1 Local Reasoner)
            ModelEntry {
                id: "qwen2.5-0.5b-instruct-q4km".to_string(),
                name: "Qwen2.5 0.5B Instruct GGUF (System 1)".to_string(),
                model_ref: ModelReference("qwen2.5-0.5b-instruct-q4km".to_string()),
                role: PipelineRole::System1Reasoner,
                tier: ModelTier::Native,
                runtime: RuntimeType::Gguf,
                format: ModelFormat::Gguf,
                primary_artifact: ModelArtifact {
                    filename: "qwen2.5-0.5b-instruct-q4_k_m.gguf".to_string(),
                    primary_url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf".to_string(),
                    fallback_url: None,
                    size_bytes: 491_400_032,
                    sha256_checksum: Some("74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db".to_string()),
                    is_essential: true,
                },
                auxiliary_artifacts: vec![ModelArtifact {
                    filename: "tokenizer.json".to_string(),
                    primary_url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct/resolve/main/tokenizer.json".to_string(),
                    fallback_url: None,
                    size_bytes: 7_031_645,
                    sha256_checksum: Some("c0382117ea329cdf097041132f6d735924b697924d6f6fc3945713e96ce87539".to_string()),
                    is_essential: true,
                }],
                expected_ram_mb: 550,
                expected_vram_mb: 500,
                description: "Génération de texte Qwen 2.5 0.5B Q4_K_M via Candle CPU ; poids GGUF et tokenizer vérifiés séparément.".to_string(),
            },

            // 6. UI-TARS 2B SFT Q4_K_M (GUI Computer-Use Agent)
            ModelEntry {
                id: "ui-tars-2b-sft-q4km".to_string(),
                name: "UI-TARS 2B SFT GGUF (GUI Agent)".to_string(),
                model_ref: ModelReference("ui-tars-2b-sft-q4km".to_string()),
                role: PipelineRole::GuiAutomation,
                tier: ModelTier::Native,
                runtime: RuntimeType::Gguf,
                format: ModelFormat::Gguf,
                primary_artifact: ModelArtifact {
                    filename: "ui-tars-2b-sft-q4_k_m.gguf".to_string(),
                    primary_url: "https://huggingface.co/bartowski/UI-TARS-2B-SFT-GGUF/resolve/main/UI-TARS-2B-SFT-Q4_K_M.gguf".to_string(),
                    fallback_url: None,
                    size_bytes: 1_117_319_712,
                    sha256_checksum: Some("f316ecef2e2e8fa9a29e81ba2c9efa8ea6f408241e175331332960c7160e9c53".to_string()),
                    is_essential: false,
                },
                auxiliary_artifacts: vec![ModelArtifact {
                    filename: "mmproj-UI-TARS-2B-SFT-f16.gguf".to_string(),
                    primary_url: "https://huggingface.co/bartowski/UI-TARS-2B-SFT-GGUF/resolve/5a0ef8290ddab04e691efca7fce30105bda2decb/mmproj-UI-TARS-2B-SFT-f16.gguf".to_string(),
                    fallback_url: None,
                    size_bytes: 1_331_656_192,
                    sha256_checksum: Some("b9c248c4ff9770b3cd82d0ed5b5d377ea02500d8d22368db9b7c2d4476b32ce8".to_string()),
                    is_essential: true,
                }],
                expected_ram_mb: 1_800,
                expected_vram_mb: 1_600,
                description: "Analyse d'image UI-TARS avec poids GGUF, projecteur Qwen2-VL et backend multimodal compatible ; validation par forward distincte de l'inspection.".to_string(),
            },

            // 7. Kyutai Mimi Streaming Neural Codec (Candle Native S2S)
            ModelEntry {
                id: "moshi-mimi-s2s".to_string(),
                name: "Kyutai Mimi Streaming Audio Codec (Candle Native)".to_string(),
                model_ref: ModelReference("moshi-mimi-s2s".to_string()),
                role: PipelineRole::VoiceSpeechToSpeech,
                tier: ModelTier::Sovereign,
                runtime: RuntimeType::Candle,
                format: ModelFormat::Safetensors,
                primary_artifact: ModelArtifact {
                    filename: "mimi_candle.safetensors".to_string(),
                    primary_url: "https://huggingface.co/kyutai/mimi/resolve/main/model.safetensors".to_string(),
                    fallback_url: None,
                    size_bytes: 384_649_828,
                    sha256_checksum: Some("bac7e85083dcded655d24eaadde7e6eea34c0da1b35fa2d284e641bd2b942a5e".to_string()),
                    is_essential: false,
                },
                auxiliary_artifacts: vec![],
                expected_ram_mb: 450,
                expected_vram_mb: 400,
                description: "Checkpoint Kyutai Mimi pour codec audio neural ; le diagnostic distingue inspection, chargement et encodage/décodage réels.".to_string(),
            },
            // 8. Whisper tiny multilingue : chargé uniquement par la voie vocale choisie.
            ModelEntry {
                id: "whisper-tiny-multilingual".to_string(),
                name: "OpenAI Whisper tiny multilingue".to_string(),
                model_ref: ModelReference("whisper-tiny-multilingual".to_string()),
                role: PipelineRole::PerceptionAsr,
                tier: ModelTier::Native,
                runtime: RuntimeType::Candle,
                format: ModelFormat::Safetensors,
                primary_artifact: ModelArtifact {
                    filename: "whisper-tiny.safetensors".to_string(),
                    primary_url: "https://huggingface.co/openai/whisper-tiny/resolve/169d4a4341b33bc18d8881c4b69c2e104e1cc0af/model.safetensors".to_string(),
                    fallback_url: None,
                    size_bytes: 151_061_672,
                    sha256_checksum: Some("7ebd0e69e78190ffe1438491fa05cc1f5c1aa3a4c4db3bc1723adbb551ea2395".to_string()),
                    is_essential: true,
                },
                auxiliary_artifacts: vec![
                    ModelArtifact {
                        filename: "whisper-tiny-config.json".to_string(),
                        primary_url: "https://huggingface.co/openai/whisper-tiny/resolve/169d4a4341b33bc18d8881c4b69c2e104e1cc0af/config.json".to_string(),
                        fallback_url: None,
                        size_bytes: 1_983,
                        sha256_checksum: Some("ffdccec4f3211f4c63310f2b7098f309fe70f3952cedc5e4d11e43f5b2379b98".to_string()),
                        is_essential: true,
                    },
                    ModelArtifact {
                        filename: "whisper-tiny-tokenizer.json".to_string(),
                        primary_url: "https://huggingface.co/openai/whisper-tiny/resolve/169d4a4341b33bc18d8881c4b69c2e104e1cc0af/tokenizer.json".to_string(),
                        fallback_url: None,
                        size_bytes: 2_480_466,
                        sha256_checksum: Some("27fc476bfe7f17299480be2273fc0608e4d5a99aba2ab5dec5374b4482d1a566".to_string()),
                        is_essential: true,
                    },
                ],
                expected_ram_mb: 400,
                expected_vram_mb: 0,
                description: "Reconnaissance vocale locale Whisper tiny multilingue (99 langues du tokenizer), PCM mono 16 kHz ; performances variables selon la langue et l'audio. Chargement indépendant des sept modèles historiques.".to_string(),
            },
        ]
    }

    pub fn get_by_id(id: &str) -> Option<ModelEntry> {
        Self::official_catalog().into_iter().find(|m| m.id == id)
    }

    pub fn models_for_tier(tier: ModelTier) -> Vec<ModelEntry> {
        Self::official_catalog()
            .into_iter()
            .filter(|m| match tier {
                ModelTier::Guest => m.tier == ModelTier::Guest,
                ModelTier::Native => m.tier == ModelTier::Guest || m.tier == ModelTier::Native,
                ModelTier::Sovereign => true,
            })
            .collect()
    }
}

#[cfg(test)]
mod artifact_metadata_tests {
    use super::*;

    #[test]
    fn official_artifacts_have_valid_portable_metadata() {
        let mut filenames = std::collections::HashSet::new();
        for entry in ModelCatalog::official_catalog() {
            for artifact in std::iter::once(entry.primary_artifact).chain(entry.auxiliary_artifacts)
            {
                artifact.validate().unwrap();
                assert!(artifact.sha256_checksum.is_some());
                assert!(filenames.insert(artifact.filename));
            }
        }
    }
}
