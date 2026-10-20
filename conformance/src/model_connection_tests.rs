use hakille_agents::gui_agent::{GuiIntention, UiTarsAgent};
use hakille_inference::{
    GgufModel, ModelProvisioner, OnnxModelInspector, SafetensorsModel, SenseVoiceTokenizer,
};
use hakille_kernel::fabric::{CapabilityName, CognitiveFabric};
use hakille_perception::audio::{
    asr::{AsrEngine, SenseVoiceSmall},
    capture::AudioChunk,
    speaker_id::{CamPlusPlusIdentifier, SpeakerIdentifier},
    vad::VoiceActivityDetector,
};
use hakille_voice::codec::MimiCodec;
use std::path::{Path, PathBuf};

fn get_models_dir() -> PathBuf {
    if Path::new("./models").exists() {
        PathBuf::from("./models")
    } else if Path::new("../models").exists() {
        PathBuf::from("../models")
    } else if Path::new("../../models").exists() {
        PathBuf::from("../../models")
    } else {
        PathBuf::from("./models")
    }
}

#[tokio::test]
async fn test_real_qwen2_gguf_parsing() {
    let dir = get_models_dir();
    let qwen_path = dir.join("qwen2.5-0.5b-instruct-q4_k_m.gguf");
    if !qwen_path.exists() {
        eprintln!("Skipping: Qwen model not found at {}", qwen_path.display());
        return;
    }

    let model = GgufModel::read_from_file(&qwen_path)
        .expect("Real Qwen2.5 GGUF v3 file should parse flawlessly");

    // Verify GGUF binary invariants
    assert_eq!(model.header.magic, 0x46554747, "GGUF magic must be 'GGUF'");
    assert_eq!(model.header.version, 3, "Expected GGUF version 3");
    assert_eq!(model.architecture(), Some("qwen2"));
    assert!(
        model.tensor_count() >= 280,
        "Expected ~291 tensors, got {}",
        model.tensor_count()
    );
    assert!(model.context_length().unwrap_or(0) >= 4096);
    assert_eq!(
        model.block_count(),
        Some(24),
        "Qwen2.5 0.5B has 24 transformer blocks"
    );
    assert_eq!(
        model.embedding_length(),
        Some(896),
        "Qwen2.5 0.5B hidden size is 896"
    );
    assert!(
        model.estimated_params > 400_000_000,
        "Estimated params should be ~490M"
    );

    // Check token embedding tensor
    let token_embd = model.find_tensor("token_embd.weight");
    assert!(
        token_embd.is_some(),
        "Must contain token_embd.weight tensor"
    );
}

#[tokio::test]
async fn test_real_ui_tars_gguf_parsing() {
    let dir = get_models_dir();
    let ui_tars_path = dir.join("ui-tars-2b-sft-q4_k_m.gguf");
    if !ui_tars_path.exists() {
        eprintln!(
            "Skipping: UI-TARS model not found at {}",
            ui_tars_path.display()
        );
        return;
    }

    let model = GgufModel::read_from_file(&ui_tars_path)
        .expect("Real UI-TARS 2B SFT GGUF file should parse flawlessly");

    assert_eq!(model.header.magic, 0x46554747);
    assert_eq!(model.header.version, 3);
    assert!(
        model.tensor_count() >= 300,
        "Expected ~339 tensors, got {}",
        model.tensor_count()
    );
    assert!(
        model.estimated_params > 1_500_000_000,
        "Estimated params should be ~2B"
    );
}

#[tokio::test]
async fn test_real_mimi_safetensors_parsing() {
    let dir = get_models_dir();
    let mimi_path = dir.join("mimi_candle.safetensors");
    if !mimi_path.exists() {
        eprintln!("Skipping: Mimi model not found at {}", mimi_path.display());
        return;
    }

    let model = SafetensorsModel::read_from_file(&mimi_path)
        .expect("Real Mimi Safetensors file should parse flawlessly");

    assert!(
        model.header_size > 10_000,
        "Safetensors JSON header is ~44KB"
    );
    assert!(
        model.tensor_count() >= 50,
        "Expected multi-layer neural codec tensors"
    );
    assert!(
        model.total_parameters > 30_000_000,
        "Mimi neural codec has > 30M parameters"
    );
    assert!(
        model.codebook_count() >= 1,
        "Must contain vector quantizer codebooks"
    );

    // Verify presence of encoder and decoder weights
    assert!(
        model.find_tensor("encoder").is_some(),
        "Must contain encoder tensors"
    );
    assert!(
        model.find_tensor("decoder").is_some(),
        "Must contain decoder tensors"
    );
}

#[tokio::test]
async fn test_real_sensevoice_tokenizer_parsing() {
    let dir = get_models_dir();
    let tokens_path = dir.join("tokens.txt");
    if !tokens_path.exists() {
        eprintln!(
            "Skipping: tokens.txt not found at {}",
            tokens_path.display()
        );
        return;
    }

    let tokenizer = SenseVoiceTokenizer::from_file(&tokens_path)
        .expect("SenseVoice tokens.txt should parse into dictionary");

    assert_eq!(
        tokenizer.vocab_size(),
        25055,
        "SenseVoice vocabulary must be exactly 25,055 tokens"
    );
    assert_eq!(tokenizer.id_to_token(0), Some("<unk>"));
    assert_eq!(tokenizer.id_to_token(1), Some("<s>"));
    assert_eq!(tokenizer.id_to_token(2), Some("</s>"));

    // Test text encoding & decoding
    let text = "the and to";
    let encoded = tokenizer.encode(text);
    assert!(!encoded.is_empty(), "Encoded IDs should not be empty");
    let decoded = tokenizer.decode(&encoded);
    assert!(!decoded.is_empty(), "Decoded text should not be empty");
}

#[tokio::test]
async fn test_real_onnx_models_inspection() {
    let dir = get_models_dir();

    // 1. Silero VAD
    let vad_path = dir.join("silero_vad.onnx");
    if vad_path.exists() {
        let info = OnnxModelInspector::inspect_file(&vad_path).unwrap();
        assert_eq!(info.primary_role, "VoiceActivity");
        assert!(info.file_size_bytes > 2_000_000);
        assert_eq!(info.expected_input_shape, vec![1, 512]);
    }

    // 2. CAM++
    let campplus_path = dir.join("campplus_zh_en_16k_advanced.onnx");
    if campplus_path.exists() {
        let info = OnnxModelInspector::inspect_file(&campplus_path).unwrap();
        assert_eq!(info.primary_role, "Biometrics");
        assert_eq!(info.expected_output_shape, vec![1, 192]);
    }

    // 3. All-MiniLM-L6-v2
    let minilm_path = dir.join("all-minilm-l6-v2.onnx");
    if minilm_path.exists() {
        let info = OnnxModelInspector::inspect_file(&minilm_path).unwrap();
        assert_eq!(info.primary_role, "MemoryEmbeddings");
        assert_eq!(info.expected_output_shape, vec![1, 384]);
    }

    // 4. SenseVoice-Small INT8
    let sensevoice_path = dir.join("sensevoice_small.int8.onnx");
    if sensevoice_path.exists() {
        let info = OnnxModelInspector::inspect_file(&sensevoice_path).unwrap();
        assert_eq!(info.primary_role, "PerceptionAsr");
        assert!(info.file_size_bytes > 200_000_000);
    }
}

#[tokio::test]
async fn test_unified_connect_all_models() {
    let dir = get_models_dir();
    let provisioner = ModelProvisioner::new(dir);

    let connected = match provisioner.connect_all_models() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Skipping: Could not connect all models: {}", e);
            return;
        }
    };

    assert!(
        connected.total_disk_bytes > 2_000_000_000,
        "Total disk size should exceed 2GB"
    );
    let summary = connected.system_summary();
    assert_eq!(
        summary.total_models_connected, 7,
        "All 7 models must be connected"
    );
    assert!(
        summary.ready_for_inspection,
        "Cognitive OS must be flagged ready (inférence réelle, voir statuts RÉEL)"
    );
    assert!(
        summary.total_parameters_estimate > 2_000_000_000,
        "Total parameters > 2B"
    );
}

#[tokio::test]
async fn test_end_to_end_cross_crate_model_wiring() {
    let dir = get_models_dir();
    let provisioner = ModelProvisioner::new(dir.clone());

    let connected = match provisioner.connect_all_models() {
        Ok(c) => c,
        Err(_) => return,
    };

    // 1. Cross-crate ASR wiring: SenseVoiceSmall + SenseVoiceTokenizer
    let asr = SenseVoiceSmall::with_tokenizer(connected.sensevoice_tokenizer.clone());
    assert!(asr.is_tokenizer_loaded());
    let audio_chunk = AudioChunk::new(vec![0.15f32; 1600], 16000, 1);
    let hypothesis = asr.transcribe(&audio_chunk).await.unwrap();
    assert!(
        !hypothesis.transcript.is_empty(),
        "Transcript must be produced"
    );

    // 2. Cross-crate Biometrics wiring: CAM++ with model path
    let campplus = CamPlusPlusIdentifier::with_model_path(&connected.campplus_info.path);
    assert!(campplus.model_path().is_some());
    let embedding = campplus.extract_embedding(&audio_chunk).await.unwrap();
    assert_eq!(
        embedding.dimension, 192,
        "CAM++ embedding must be 192 dimensions"
    );
    let self_sim = embedding.cosine_similarity(&embedding);
    assert!(
        (self_sim - 1.0).abs() < 1e-4,
        "Self cosine similarity must be 1.0"
    );

    // 3. Cross-crate VAD wiring: VoiceActivityDetector with ONNX model path
    let mut vad = VoiceActivityDetector::with_onnx_model(&connected.silero_vad_info.path);
    assert!(vad.model_path().is_some());
    let vad_res = vad.process(&audio_chunk);
    assert!(vad_res.speech_probability > 0.0);

    // 4. Cross-crate Codec wiring: MimiCodec with Safetensors model
    let mut mimi = MimiCodec::with_safetensors(connected.mimi.clone());
    assert!(mimi.is_connected());
    let samples = vec![0.05f32; 1920];
    let tokens = mimi.encode_frame(&samples).unwrap();
    assert_eq!(tokens.codebooks.len(), 8, "Mimi uses 8 codebooks");
    let decoded_audio = mimi.decode_frame(&tokens).unwrap();
    assert_eq!(
        decoded_audio.len(),
        1920,
        "Mimi frame length is 1920 samples @ 24kHz"
    );

    // 5. Cross-crate GUI Agent wiring: UiTarsAgent with GgufModel.
    // Contrat honnête : sans capture, le parseur retourne un prior NEUTRE
    // (0.5, 0.5) interdit d'exécution tel quel ; les coordonnées exécutées
    // proviennent de la mesure sur frame (vision classique réelle).
    let ui_tars = UiTarsAgent::with_gguf_model(connected.ui_tars.clone());
    assert!(ui_tars.is_model_connected());
    assert!(ui_tars.tensor_count() >= 300);
    let intentions = ui_tars.predict_intent_from_task("Click the top right Submit button");
    assert!(!intentions.is_empty());
    match &intentions[0] {
        GuiIntention::Click { x, y, .. } => {
            assert_eq!(*x, 0.5, "prior neutre non mesuré");
            assert_eq!(*y, 0.5, "prior neutre non mesuré");
        }
        _ => panic!("Expected Click intention"),
    }
    // Ancrage RÉEL : rectangle blanc en haut-droite sur fond noir.
    let (fw, fh) = (200u32, 200u32);
    let mut rgba = vec![0u8; (fw * fh * 4) as usize];
    for yy in 20..60 {
        for xx in 120..180 {
            let i = ((yy * fw + xx) * 4) as usize;
            rgba[i] = 255;
            rgba[i + 1] = 255;
            rgba[i + 2] = 255;
            rgba[i + 3] = 255;
        }
    }
    let grounded = ui_tars
        .predict_grounded_on_frame("Click the top right Submit button", &rgba, fw, fh)
        .expect("grounding réel sur frame synthétique");
    assert!(!grounded.is_empty());
    match &grounded[0] {
        GuiIntention::Click { x, y, .. } => {
            assert!(*x > 0.5, "ancrage mesuré à droite, x={x}");
            assert!(*y < 0.5, "ancrage mesuré en haut, y={y}");
        }
        _ => panic!("Expected anchored Click intention"),
    }
    // Écran vide : Wait honnête, jamais de coordonnée inventée.
    let empty = vec![0u8; (fw * fh * 4) as usize];
    let idle = ui_tars
        .predict_grounded_on_frame("Click the button", &empty, fw, fh)
        .expect("frame vide sans panic");
    assert!(
        matches!(idle[0], GuiIntention::Wait { .. }),
        "frame vide → Wait honnête"
    );

    // 6. Cross-crate Kernel CognitiveFabric wiring
    let fabric = CognitiveFabric::from_connected_models(&connected);
    let supported = fabric.supported_capabilities();
    assert!(supported.contains(&CapabilityName::Listen));
    assert!(supported.contains(&CapabilityName::Identify));
    assert!(supported.contains(&CapabilityName::Diarize));
    assert!(supported.contains(&CapabilityName::Reason));
    assert!(supported.contains(&CapabilityName::ReasonHeavy));
    assert!(supported.contains(&CapabilityName::ActGui));
    assert!(supported.contains(&CapabilityName::See));
    assert!(supported.contains(&CapabilityName::Speak));

    // Verify capability bindings
    let reason_binding = fabric.get_binding(CapabilityName::Reason).unwrap();
    assert_eq!(reason_binding.quantization, Some("Q4_K_M".to_string()));
    assert_eq!(reason_binding.runtime, hakille_core::RuntimeType::Gguf);

    let listen_binding = fabric.get_binding(CapabilityName::Listen).unwrap();
    assert_eq!(listen_binding.quantization, Some("INT8".to_string()));
    assert_eq!(
        listen_binding.runtime,
        hakille_core::RuntimeType::OnnxRuntime
    );

    let speak_binding = fabric.get_binding(CapabilityName::Speak).unwrap();
    assert_eq!(speak_binding.runtime, hakille_core::RuntimeType::Candle);
}
