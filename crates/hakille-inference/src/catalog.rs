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
                description: "Sub-1ms streaming neural voice activity detection (VAD).".to_string(),
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
                description: "7.2M parameter speaker verification network for cryptographic capability token gating.".to_string(),
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
                auxiliary_artifacts: vec![],
                expected_ram_mb: 90,
                expected_vram_mb: 0,
                description: "High-speed semantic embedding generation for KnowledgeGraph HNSW vector queries.".to_string(),
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
                description: "Non-autoregressive ASR, emotion recognition, and acoustic event detection under 250MB RAM.".to_string(),
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
                auxiliary_artifacts: vec![],
                expected_ram_mb: 550,
                expected_vram_mb: 500,
                description: "Sub-100ms local reasoning engine for conversation turns and intention routing.".to_string(),
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
                auxiliary_artifacts: vec![],
                expected_ram_mb: 1_800,
                expected_vram_mb: 1_600,
                description: "Vision-Language-Action agent for direct desktop and mobile GUI interaction without OS hooks.".to_string(),
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
                description: "24kHz causal streaming neural audio codec running natively in pure Rust via Candle.".to_string(),
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
