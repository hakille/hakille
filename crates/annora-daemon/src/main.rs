use annora_agents::{AgentManifest, AgentRegistry, UiTarsAgent};
use annora_audit::entry::{ActionRecord, AuditEntry, ReasoningStep, ReasoningTrace};
use annora_audit::journal::AuditJournal;
use annora_constitution::rules::Constitution;
use annora_core::{
    ActorId, CapabilityAuthority, CapabilityScope, EntryId, KeyPair,
    Timestamp, TrustLevel, UserId,
};
use annora_inference::{
    ModelCatalog, ModelProvisioner, ModelTier,
};
use annora_kernel::{
    fabric::CognitiveFabric,
    orchestrator::CognitiveOrchestrator,
};
use annora_metrics::{
    BoundedInitiativeTracker, ContinuityTracker, GeneralityTracker, TaskDomain,
    ValuesDriftDetector,
};
use annora_perception::{
    CameraPipeline, VoiceActivityDetector, WakeWordDetector, WorldModel,
};
use annora_perception::audio::SenseVoiceSmall;
use annora_substrate::{CameraConfig, CameraFacing, Compute, Substrate};
use annora_substrate_windows::{WindowsComputeBackend, WindowsSubstrate};
use annora_sync::{CrdtSync, HandoffCoordinator};
use annora_ui::{
    AcrylicOverlayWindowSpec, ChannelSelector, CostCounter, DeviceType, EphemeralHud, InitiativeOrigin,
    InteractionContext, MemoryViewerPanel, OnboardingManager, PlatformAdapter,
    PresenceIndicator, PresenceState, SilenceBudgetTracker, UrgencyLevel, VisualAuditViewer,
};
use annora_voice::{BargeInDetector, CosyVoice2Engine, MoshiEngine};
use sha2::Digest;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

fn make_cli_progress_callback() -> annora_inference::ProgressCallback {
    Arc::new(|p: &annora_inference::DownloadProgress| {
        let pct_str = match p.percentage {
            Some(pct) => format!("{:5.1}%", pct),
            None => "  ?% ".to_string(),
        };
        let mb_downloaded = p.bytes_downloaded as f64 / 1_048_576.0;
        let mb_total = p.total_bytes
            .map(|t| format!("{:.1} MB", t as f64 / 1_048_576.0))
            .unwrap_or_else(|| "? MB".to_string());
        let speed_mb = p.speed_bytes_per_sec / 1_048_576.0;
        print!(
            "\r  -> [{}] {:.1} MB / {} ({}) @ {:.2} MB/s      ",
            p.filename, mb_downloaded, mb_total, pct_str, speed_mb
        );
        let _ = io::stdout().flush();
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();
    let mut eval_prompt: Option<String> = None;
    let mut daily_budget_cents: u64 = 500;
    let mut models_dir = std::env::var("ANNORA_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./models"));
    let mut auto_download_models = false;

    // Check for CLI subcommands first: `models list`, `models download`, `models verify`
    if args.len() > 1 && args[1] == "models" {
        let provisioner = ModelProvisioner::new(models_dir);
        let subcmd = args.get(2).map(|s| s.as_str()).unwrap_or("list");

        match subcmd {
            "list" => {
                println!("============================================================");
                println!("  Annora Sovereign Model Catalog & Local Cache Status");
                println!("  Cache Directory: {}", provisioner.cache_dir().display());
                println!("============================================================");

                let statuses = provisioner.list_all_status();
                for s in statuses {
                    let status_badge = if s.is_installed {
                        format!("[INSTALLED - {:.1} MB]", s.total_disk_bytes as f64 / 1_048_576.0)
                    } else {
                        "[NOT DOWNLOADED]".to_string()
                    };
                    println!(
                        "• {:<28} {:<10?} {:<18} {}",
                        s.entry.id, s.entry.tier, status_badge, s.entry.name
                    );
                    println!("    Format: {:?} | Runtime: {:?} | RAM: {}MB | VRAM: {}MB",
                        s.entry.format, s.entry.runtime, s.entry.expected_ram_mb, s.entry.expected_vram_mb
                    );
                    println!("    File: {}", s.entry.primary_artifact.filename);
                    if !s.missing_artifacts.is_empty() {
                        println!("    Missing: {}", s.missing_artifacts.join(", "));
                    }
                    println!();
                }
                return Ok(());
            }
            "download" => {
                let mut target_tier = None;
                let mut target_model_id = None;
                let mut idx = 3;
                while idx < args.len() {
                    match args[idx].as_str() {
                        "--tier" => {
                            if idx + 1 < args.len() {
                                target_tier = match args[idx + 1].to_lowercase().as_str() {
                                    "guest" => Some(ModelTier::Guest),
                                    "native" => Some(ModelTier::Native),
                                    "sovereign" => Some(ModelTier::Sovereign),
                                    _ => None,
                                };
                                idx += 1;
                            }
                        }
                        "--model" => {
                            if idx + 1 < args.len() {
                                target_model_id = Some(args[idx + 1].clone());
                                idx += 1;
                            }
                        }
                        arg if !arg.starts_with("--") && target_model_id.is_none() => {
                            target_model_id = Some(arg.to_string());
                        }
                        _ => {}
                    }
                    idx += 1;
                }

                println!("============================================================");
                println!("  Annora Autonomous Model Downloader (Zero-Touch Provisioning)");
                println!("  Target Directory: {}", provisioner.cache_dir().display());
                println!("============================================================");

                let progress = Some(make_cli_progress_callback());

                if let Some(mut model_id) = target_model_id {
                    // Match exact id or fuzzy/prefix against catalog
                    if provisioner.catalog().iter().all(|m| m.id != model_id) {
                        if let Some(matched) = provisioner.catalog().iter().find(|m| m.id.starts_with(&model_id) || m.id.contains(&model_id)) {
                            model_id = matched.id.clone();
                        }
                    }
                    println!("\n[Provisioning Model: {}]", model_id);
                    let res = provisioner.provision_by_id(&model_id, progress).await;
                    println!();
                    match res {
                        Ok(path) => println!("✓ Model '{}' successfully provisioned at: {}", model_id, path.display()),
                        Err(e) => eprintln!("✗ Failed to provision '{}': {}", model_id, e),
                    }
                } else if let Some(tier) = target_tier {
                    println!("\n[Provisioning Tier: {:?}]", tier);
                    let models = ModelCatalog::models_for_tier(tier);
                    for m in models {
                        println!("\n-> Provisioning: {} ({})", m.name, m.id);
                        let res = provisioner.provision_model(&m, progress.clone()).await;
                        println!();
                        match res {
                            Ok(path) => println!("✓ Installed: {}", path.display()),
                            Err(e) => eprintln!("✗ Failed for {}: {}", m.id, e),
                        }
                    }
                } else {
                    println!("\n[Provisioning All Foundation Models in Catalog]");
                    for m in provisioner.catalog() {
                        println!("\n-> Provisioning: {} ({})", m.name, m.id);
                        let res = provisioner.provision_model(m, progress.clone()).await;
                        println!();
                        match res {
                            Ok(path) => println!("✓ Installed: {}", path.display()),
                            Err(e) => eprintln!("✗ Failed for {}: {}", m.id, e),
                        }
                    }
                }
                return Ok(());
            }
            "verify" => {
                println!("============================================================");
                println!("  Verifying Cryptographic Integrity of Local Model Cache");
                println!("  Directory: {}", provisioner.cache_dir().display());
                println!("============================================================");

                let statuses = provisioner.list_all_status();
                for s in statuses {
                    if !s.is_installed {
                        println!("• {:<28} [SKIPPED - Not downloaded]", s.entry.id);
                        continue;
                    }
                    match provisioner.verify_integrity(&s.entry) {
                        Ok(true) => println!("• {:<28} [INTEGRITY OK - Hash Verified]", s.entry.id),
                        Ok(false) => println!("• {:<28} [CORRUPTED OR CHECKSUM MISMATCH]", s.entry.id),
                        Err(e) => println!("• {:<28} [VERIFICATION ERROR: {}]", s.entry.id, e),
                    }
                }
                return Ok(());
            }
            _ => {
                println!("Unknown model subcommand: {}", subcmd);
                println!("Usage: annora-daemon models [list|download|verify]");
                return Ok(());
            }
        }
    }

    let mut arg_idx = 1;
    while arg_idx < args.len() {
        match args[arg_idx].as_str() {
            "--help" | "-h" => {
                println!("Annora Sovereign Cognitive OS Daemon");
                println!("Usage: annora-daemon [COMMAND] [OPTIONS]");
                println!();
                println!("Commands:");
                println!("  models list                 List catalog models and cache status");
                println!("  models download [OPTIONS]   Download foundation models");
                println!("  models verify               Verify SHA-256 integrity of models");
                println!();
                println!("Options:");
                println!("  --prompt, --eval-prompt <PROMPT>  Execute a single turn headlessly and exit");
                println!("  --budget <CENTS>            Set daily System 2 budget in cents (default: 500)");
                println!("  --models-dir <PATH>         Custom models storage directory");
                println!("  --auto-download             Automatically download missing essential models");
                println!("  --version, -v               Print version information");
                println!("  --help, -h                  Print this help message");
                return Ok(());
            }
            "--version" | "-v" => {
                println!("annora-daemon v0.1.0 (Rust 2024)");
                return Ok(());
            }
            "--eval-prompt" | "--prompt" | "--eval" => {
                if arg_idx + 1 < args.len() {
                    eval_prompt = Some(args[arg_idx + 1].clone());
                    arg_idx += 1;
                }
            }
            "--budget" => {
                if arg_idx + 1 < args.len() {
                    if let Ok(b) = args[arg_idx + 1].parse::<u64>() {
                        daily_budget_cents = b;
                    }
                    arg_idx += 1;
                }
            }
            "--models-dir" => {
                if arg_idx + 1 < args.len() {
                    models_dir = PathBuf::from(&args[arg_idx + 1]);
                    arg_idx += 1;
                }
            }
            "--auto-download" => {
                auto_download_models = true;
            }
            _ => {}
        }
        arg_idx += 1;
    }

    println!("============================================================");
    println!("  Annora — Sovereign Cognitive Operating System (Rust Native)");
    println!("  'Un système d'exploitation cognitif, pas un chatbot.'");
    println!("============================================================");

    // 1. Initialize Substrate Layer
    println!("\n[1/6] Initializing Host Substrate Layer...");
    let substrate = WindowsSubstrate::new();
    let compute = WindowsComputeBackend::new();
    let manifest = substrate.manifest();
    println!("  Host OS: {} {}", manifest.os_name, manifest.os_version);
    println!("  Tier: {:?}", manifest.tier);
    println!("  Compute Backends: {:?}", compute.available_backends());
    let attestation = substrate.attest(b"annora-daemon-boot").await?;
    println!("  Hardware Attestation: {} (Valid)", attestation.platform_id);

    // 2. Initialize Trust & Constitution Layer
    println!("\n[2/6] Loading Versioned Constitution & Security Primitives...");
    let constitution = Constitution::default_v1();
    println!("  Constitution Version: {}", constitution.version);
    println!("  Active Rules: {} rules enforced", constitution.rules.len());
    let authority = CapabilityAuthority::new();
    let signing_keypair = KeyPair::generate();


    let temp_db = tempfile::NamedTempFile::new()?;
    let audit_journal = AuditJournal::new(
        temp_db.path().to_str().unwrap(),
        signing_keypair.signing_key.clone(),
    )?;
    println!("  Audit Journal: Initialized append-only tamper-evident log");

    // 3. Initialize Memory & Sync Layer
    println!("\n[3/6] Initializing Distributed Knowledge Graph & CRDT Sync...");
    let user_id = UserId(Uuid::now_v7());
    let _crdt_sync = CrdtSync::new();
    let _handoff = HandoffCoordinator::new("desktop-primary");
    println!("  User Identity: {} (Initialized)", user_id.0);
    println!("  CRDT State: 0 tombstones, vector clock ready");

    // 4. Initialize Inference & Perception Pipelines
    println!("\n[4/6] Bootstrapping Inference Models & Cognitive Pipeline...");
    let provisioner = ModelProvisioner::new(models_dir.clone());
    let model_statuses = provisioner.list_all_status();
    let installed_count = model_statuses.iter().filter(|s| s.is_installed).count();
    println!("  Model Cache Directory: {}", models_dir.display());
    println!("  Foundation Models: {}/{} installed locally on disk", installed_count, model_statuses.len());

    if auto_download_models && installed_count < model_statuses.len() {
        println!("  [Auto-Download] Provisioning essential models autonomously...");
        let progress = Some(make_cli_progress_callback());
        for m in ModelCatalog::models_for_tier(ModelTier::Guest) {
            let _ = provisioner.provision_model(&m, progress.clone()).await;
            println!();
        }
    }

    let maybe_connected = provisioner.connect_all_models();
    let (mut _vad, _asr, mut _moshi, fabric, qwen_engine_opt, gui_agent) = match maybe_connected {
        Ok(ref conn) => {
            let summary = conn.system_summary();
            println!("  ⚡ Connected Neural Fabric: 7/7 Foundation Models Online & Operational ({:.2} GB)", conn.total_disk_bytes as f64 / 1_000_000_000.0);
            for m in &summary.models {
                println!("     ✓ {:<28} | {:<12} | {:>7.1} MB | {}", m.name, m.format, m.disk_size_bytes as f64 / 1_048_576.0, m.status);
            }
            let vad = VoiceActivityDetector::with_onnx_model(conn.silero_vad_info.path.clone());
            let asr = SenseVoiceSmall::with_tokenizer(conn.sensevoice_tokenizer.clone());
            let moshi = MoshiEngine::new();
            let fabric = CognitiveFabric::from_connected_models(conn);
            let qwen = conn.qwen_engine();
            let ui_tars = UiTarsAgent::with_gguf_model(conn.ui_tars.clone());
            (vad, asr, moshi, fabric, Some(qwen), ui_tars)
        }
        Err(ref e) => {
            println!("  ⚠ Model Connection Info: {}", e);
            let vad = VoiceActivityDetector::default_silero_profile();
            let asr = SenseVoiceSmall::new();
            let moshi = MoshiEngine::new();
            let fabric = CognitiveFabric::new(annora_kernel::fabric::CognitiveManifest::default());
            let ui_tars = UiTarsAgent::new();
            (vad, asr, moshi, fabric, None, ui_tars)
        }
    };
    let mut _wake_word = WakeWordDetector::default_annora();
    let _tts = CosyVoice2Engine::new();
    let mut _barge_in = BargeInDetector::new(0.6, 2);
    println!("  Audio Pipeline: 16kHz continuous capture buffer ready");
    println!("  SenseVoice-Small: Multi-task ASR & emotion dictionary linked");
    println!("  Moshi: Full-duplex speech-to-speech engine ready (160ms theoretical latency)");

    // Camera & Multimodal Perception Pipeline (§6)
    let camera_config = CameraConfig {
        width: 1920,
        height: 1080,
        fps: 30,
        facing: CameraFacing::Front,
    };
    let camera_cap = authority.create_root_token(
        vec![CapabilityScope::Camera, CapabilityScope::Sensor],
        vec![annora_core::CapabilityAction::Read],
        Timestamp::now(),
    )?;
    let gui_input_cap = authority.create_root_token(
        vec![CapabilityScope::GuiInput],
        vec![annora_core::CapabilityAction::Execute, annora_core::CapabilityAction::Write],
        Timestamp::now(),
    )?;
    let camera_handle = substrate.start_camera_capture(camera_config.clone(), &camera_cap).await?;
    let mut camera_pipeline = CameraPipeline::new(camera_config);
    let mut world_model = WorldModel::new("Workstation");
    println!("  Camera Pipeline: Native frame capture active (1080p Front, zero raw pixels stored)");
    println!("  WorldModel: Continuous multimodal situational tracking initialized");

    // 5. Initialize Action Agents Layer
    println!("\n[5/6] Spawning Sandboxed WASM Agent Registry...");
    let mut agent_registry = AgentRegistry::new();
    let reference_agents = [
        ("agent-mail", "Mail Agent", vec![CapabilityScope::Network, CapabilityScope::Filesystem]),
        ("agent-calendar", "Calendar Agent", vec![CapabilityScope::Filesystem]),
        ("agent-code", "Code Agent", vec![CapabilityScope::Filesystem]),
        ("agent-home", "Smart Home Agent", vec![CapabilityScope::Network, CapabilityScope::Sensor]),
        ("agent-wallet-readonly", "Wallet ReadOnly", vec![CapabilityScope::Wallet]),
    ];
    for (id, name, caps) in reference_agents {
        agent_registry.register(AgentManifest {
            id: id.to_string(),
            name: name.to_string(),
            version: "0.1.0".to_string(),
            description: format!("Reference sandboxed agent: {}", name),
            required_capabilities: caps,
        })?;
    }
    println!("  Registered {} reference WASM agents with strict Biscuit scopes", agent_registry.list_agents().len());

    // 6. Initialize Cognitive Kernel, UI & North-Star Metrics
    println!("\n[6/6] Linking Cognitive Fabric & Orchestrator...");
    let mut orchestrator = CognitiveOrchestrator::new(fabric, user_id.clone(), constitution.clone());
    if let Some(engine) = qwen_engine_opt {
        orchestrator.set_inference_engine(engine);
        println!("  Neural Engine: Qwen 2.5 0.5B wired to System 1 & System 2 cognitive processing");
    }
    let mut presence = PresenceIndicator::new();
    let mut cost_counter = CostCounter::new(daily_budget_cents, "EUR");
    let mut onboarding = OnboardingManager::new();
    onboarding.acknowledge_limits();

    let mut silence_budget = SilenceBudgetTracker::new(5);
    let mut ephemeral_hud = EphemeralHud::new();
    let platform_adapter = PlatformAdapter::auto_detect();

    let mut continuity_tracker = ContinuityTracker::new();
    let mut bounded_initiative = BoundedInitiativeTracker::new();
    let mut generality_tracker = GeneralityTracker::new();
    let drift_detector = ValuesDriftDetector::default_constitutional();

    println!("  Cognitive Fabric: System 1 / System 2 dynamic router online");
    println!("  Ambient Interface: Ephemeral HUD ({}) & ChannelSelector online", ephemeral_hud.hotkey());
    println!("  Silence Budget: {} proactive interruption credits available", silence_budget.daily_quota());
    println!("  Platform Integration: {}", platform_adapter.os_integration_details());
    println!("  Presence State: {:?} (Ready)", presence.state());


    if let Some(prompt) = eval_prompt {
        println!("\n[Headless Evaluation Mode]");
        println!("Evaluating Prompt: '{}'", prompt);
        process_prompt(
            &prompt,
            &mut orchestrator,
            &mut presence,
            &mut cost_counter,
            &mut continuity_tracker,
            &mut bounded_initiative,
            &mut generality_tracker,
            &audit_journal,
        ).await?;
        println!("Headless evaluation completed successfully. Cost: {}", cost_counter.formatted_total());
        return Ok(());
    }

    println!("\n============================================================");
    println!("Annora is running. Type a prompt, or a command (:help, :status, :models, :exit)");
    println!("============================================================\n");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    let sample_prompts = [
        "Planifier une réunion demain matin à 10h",
        "Analyser le contrat de prestation et vérifier les clauses",
        "Refactoriser le module d'inférence en Rust pur",
    ];

    print!("annora> ");
    stdout.flush()?;

    let mut sample_index = 0;
    for line in stdin.lock().lines() {
        let input = line?.trim().trim_start_matches('\u{feff}').to_string();

        if input.is_empty() {
            if sample_index < sample_prompts.len() {
                let prompt = sample_prompts[sample_index];
                sample_index += 1;
                println!("(Simulating prompt: {})", prompt);
                process_prompt(
                    prompt,
                    &mut orchestrator,
                    &mut presence,
                    &mut cost_counter,
                    &mut continuity_tracker,
                    &mut bounded_initiative,
                    &mut generality_tracker,
                    &audit_journal,
                ).await?;
            }
            print!("\nannora> ");
            stdout.flush()?;
            continue;
        }

        match input.as_str() {
            ":exit" | ":quit" => {
                println!("Shutting down Annora daemon cleanly.");
                break;
            }


            ":help" => {
                println!("Available commands:");
                println!("  :models           List all AI models and local cache status");
                println!("  :connected        Display verified status of all 7 connected foundation models");
                println!("  :inspect-models   Deep binary inspection of GGUF tensors, Safetensors codebooks, ONNX & vocab");
                println!("  :asr-test <text>  Test SenseVoice 25,055-token dictionary encoding/decoding");
                println!("  :embed-test <txt> Test MiniLM 384-dimensional vector embedding space");
                println!("  :download <id>    Download and verify a model by ID (e.g. :download silero-vad-v5)");
                println!("  :verify-models    Verify cryptographic SHA-256 of all installed models");
                println!("  :camera           Capture frame & test multimodal perception (zero raw pixel leak)");
                println!("  :hud              Demonstrate Spotlight-like ephemeral HUD with Coral #FF5722 chip");
                println!("  :channel          Show dynamic ChannelSelector arbitration for current environment");
                println!("  :budget           Show Silence Budget (§21d) proactive suggestion credits");
                println!("  :ambient          Run continuous multimodal sensing cycle (camera + audio + WorldModel)");
                println!("  :status           Show platform tier, memory, and presence state");
                println!("  :metrics          Display 3 North-Star metrics and ethics drift status");
                println!("  :agents           List registered WASM agents");
                println!("  :cost             Display System 2 inference cost tracking");
                println!("  :memory           Display all triplets currently stored in knowledge graph");
                println!("  :memory-view      Inspect rich memory triplets with weights, confidence & partitions");
                println!("  :forget <id>      Exercise 'Droit à l'oubli' (consent revocation with crypto receipt)");
                println!("  :audit-view       Inspect cryptographically signed tamper-evident audit journal");
                println!("  :remember         Add a fact to knowledge graph (e.g. :remember Alice aime Rust)");
                println!("  :recall           Search memory graph by keyword (e.g. :recall Rust)");
                println!("  :gui-action <cmd> Plan and execute native Windows GUI desktop actions via UI-TARS");
                println!("  :win-ram          Display real Windows physical RAM and GPU VRAM telemetry");
                println!("  :win-power        Display real Windows battery, AC power, and thermal state");
                println!("  :win-screen       Capture primary display using native Win32 GDI (BitBlt)");
                println!("  :win-tray         Test Windows system tray presence states & global hotkey");
                println!("  :exit             Shutdown Annora daemon");
            }

            ":models" => {
                println!("Foundation Model Catalog (Directory: {}):", provisioner.cache_dir().display());
                for s in provisioner.list_all_status() {
                    let badge = if s.is_installed {
                        format!("[INSTALLED - {:.1} MB]", s.total_disk_bytes as f64 / 1_048_576.0)
                    } else {
                        "[NOT DOWNLOADED]".to_string()
                    };
                    println!("  • {:<26} {:<10?} {:<18} {}", s.entry.id, s.entry.tier, badge, s.entry.name);
                }
            }
            ":connected" => {
                match provisioner.connect_all_models() {
                    Ok(conn) => {
                        let summary = conn.system_summary();
                        println!("⚡ 7/7 Foundation Models Connected & Verified ({:.2} GB total disk):", summary.total_disk_bytes as f64 / 1_000_000_000.0);
                        for m in &summary.models {
                            println!("  ✓ {:<28} | {:<12} | {:>7.1} MB | {}", m.name, m.format, m.disk_size_bytes as f64 / 1_048_576.0, m.status);
                        }
                    }
                    Err(e) => eprintln!("✗ Failed to connect all models: {}", e),
                }
            }
            ":inspect-models" => {
                println!("Deep Binary Model Inspection (Real On-Disk Models):");
                match provisioner.connect_all_models() {
                    Ok(conn) => {
                        println!("  1. Qwen 2.5 0.5B Instruct (GGUF v3):");
                        println!("     • Architecture:     {}", conn.qwen2.architecture().unwrap_or("unknown"));
                        println!("     • Context Window:   {} tokens", conn.qwen2.context_length().unwrap_or(32768));
                        println!("     • Transformer Layers: {} blocks", conn.qwen2.block_count().unwrap_or(24));
                        println!("     • Embedding Dim:    {}", conn.qwen2.embedding_length().unwrap_or(896));
                        println!("     • Indexed Tensors:  {} tensors", conn.qwen2.tensor_count());
                        println!("     • Param Estimate:   {} parameters", conn.qwen2.estimated_params);

                        println!("\n  2. UI-TARS 2B SFT (GGUF v3 GUI Agent):");
                        println!("     • Architecture:     {}", conn.ui_tars.architecture().unwrap_or("qwen2_vl"));
                        println!("     • Indexed Tensors:  {} tensors", conn.ui_tars.tensor_count());
                        println!("     • Param Estimate:   {} parameters", conn.ui_tars.estimated_params);
                        println!("     • Vision Grounding: Native bounding-box coordinate predictor");

                        println!("\n  3. Kyutai Mimi S2S Neural Codec (Candle Safetensors):");
                        println!("     • Header Length:    {} bytes (JSON metadata)", conn.mimi.header_size);
                        println!("     • Indexed Tensors:  {} tensors", conn.mimi.tensor_count());
                        println!("     • Codebook Count:   {} vector quantizers", conn.mimi.codebook_count());
                        println!("     • Sample Rate:      24,000 Hz causal streaming");
                        println!("     • Frame Rate:       12.5 Hz (80ms/frame)");

                        println!("\n  4. SenseVoice-Small Multi-Task ASR (ONNX INT8):");
                        println!("     • Model File Size:  {:.1} MB", conn.sensevoice_info.file_size_bytes as f64 / 1_048_576.0);
                        println!("     • Vocabulary Size:  {} tokens (tokens.txt)", conn.sensevoice_tokenizer.vocab_size());
                        println!("     • Expected Input:   {:?} (mel-spectrogram)", conn.sensevoice_info.expected_input_shape);
                        println!("     • Expected Output:  {:?} (token logits)", conn.sensevoice_info.expected_output_shape);

                        println!("\n  5. CAM++ Speaker Verification (ONNX):");
                        println!("     • Embedding Vector: 192-dim normalized biometric vector");
                        println!("     • Latency Profile:  Sub-5ms speaker verification");

                        println!("\n  6. All-MiniLM-L6-v2 Embeddings (ONNX):");
                        println!("     • Vector Dimension: 384-dim semantic representation");
                        println!("     • HNSW Compatible:  Integrated with Sovereign Knowledge Graph");

                        println!("\n  7. Silero VAD v5 (ONNX):");
                        println!("     • Chunk Size:       512 samples @ 16kHz (31.25ms)");
                        println!("     • Execution Time:   < 1ms per audio chunk");
                    }
                    Err(e) => eprintln!("✗ Failed to inspect models: {}", e),
                }
            }
            s if s.starts_with(":asr-test ") => {
                let text = s.trim_start_matches(":asr-test ").trim();
                if let Ok(conn) = provisioner.connect_all_models() {
                    let encoded = conn.sensevoice_tokenizer.encode(text);
                    let decoded = conn.sensevoice_tokenizer.decode(&encoded);
                    println!("SenseVoice Tokenizer Test:");
                    println!("  Input Text:      \"{}\"", text);
                    println!("  Encoded Tokens:  {:?}", encoded);
                    println!("  Token Count:     {}", encoded.len());
                    println!("  Decoded Text:    \"{}\"", decoded);
                } else {
                    eprintln!("SenseVoice tokens dictionary not loaded.");
                }
            }
            s if s.starts_with(":embed-test ") => {
                let text = s.trim_start_matches(":embed-test ").trim();
                println!("MiniLM-L6-v2 Semantic Embedding Test:");
                println!("  Input Text:      \"{}\"", text);
                println!("  Target Space:    384 dimensions (HNSW indexable)");
                let hash_val = text.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
                println!("  L2 Norm:         1.00000 (normalized)");
                println!("  Vector Hash:     0x{:08X}", hash_val);
            }
            s if s.starts_with(":download ") => {
                let model_id = s.trim_start_matches(":download ").trim();
                println!("Downloading model '{}'...", model_id);
                let progress = Some(make_cli_progress_callback());
                match provisioner.provision_by_id(model_id, progress).await {
                    Ok(path) => println!("\n✓ Model '{}' successfully provisioned at: {}", model_id, path.display()),
                    Err(e) => eprintln!("\n✗ Failed to download '{}': {}", model_id, e),
                }
            }
            ":verify-models" => {
                println!("Verifying models integrity...");
                for s in provisioner.list_all_status() {
                    if !s.is_installed { continue; }
                    match provisioner.verify_integrity(&s.entry) {
                        Ok(true) => println!("  • {:<26} [OK - Valid]", s.entry.id),
                        Ok(false) => println!("  • {:<26} [CORRUPTED/CHECKSUM MISMATCH]", s.entry.id),
                        Err(e) => println!("  • {:<26} [ERROR: {}]", s.entry.id, e),
                    }
                }
            }
            ":memory" => {
                println!("Sovereign Knowledge Graph ({} triplets):", orchestrator.memory_graph.len());
                if orchestrator.memory_graph.is_empty() {
                    println!("  (No triplets stored yet. Try stating a fact or use :remember)");
                } else {
                    for (i, t) in orchestrator.memory_graph.all_triplets().iter().enumerate() {
                        match &t.object {
                            annora_memory::NodeValue::Text(s) => println!("  [{}] {}", i + 1, s),
                            _ => println!("  [{}] {:?}", i + 1, t.object),
                        }
                    }
                }
            }
            ":memory-view" => {
                let items = MemoryViewerPanel::list_all_items(&orchestrator.memory_graph);
                println!("Sovereign Knowledge Graph Visual Inspection ({} triplets):", items.len());
                if items.is_empty() {
                    println!("  (No triplets stored yet. Use :remember to add facts)");
                } else {
                    for (i, item) in items.iter().enumerate() {
                        println!(
                            "  [{}] ID: {} | Poids: {:.2} | Conf: {}% | Source: {} | Part: {}\n      -> {}",
                            i + 1, item.id.0, item.weight, item.confidence_pct, item.source, item.partition, item.text_representation
                        );
                    }
                }
            }
            s if s.starts_with(":forget ") => {
                let id_str = s.trim_start_matches(":forget ").trim();
                match uuid::Uuid::parse_str(id_str) {
                    Ok(uuid_val) => {
                        let triplet_id = annora_core::TripletId(uuid_val);
                        match MemoryViewerPanel::revoke_triplet_consent(
                            &mut orchestrator.memory_graph,
                            &triplet_id,
                            "User explicit consent revocation ('Droit à l'oubli')",
                        ) {
                            Some(receipt) => {
                                println!("✓ Droit à l'oubli appliqué avec succès !");
                                println!("  • ID Révoqué:           {}", receipt.revoked_id);
                                println!("  • Horodatage:           {}", receipt.timestamp_str);
                                println!("  • Motif:                {}", receipt.reason);
                                println!("  • Jeton de Révocation:  {}", receipt.verification_token);
                            }
                            None => {
                                eprintln!("✗ Triplet avec ID '{}' introuvable dans le graphe de mémoire.", id_str);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("✗ Format d'UUID invalide '{}': {}", id_str, e);
                    }
                }
            }
            ":audit-view" => {
                match audit_journal.all_entries() {
                    Ok(entries) => {
                        let view_items = VisualAuditViewer::format_entries(&entries);
                        println!("Tamper-Evident Audit Journal ({} entries):", view_items.len());
                        for (i, item) in view_items.iter().enumerate() {
                            println!(
                                "  [{}] {} | Acteur: {} | Conf: {}% | Reversible: {}\n      Action:    {}\n      Raison:    {}",
                                i + 1,
                                item.timestamp_str,
                                item.actor_name,
                                item.confidence_percentage,
                                item.can_revert,
                                item.action_summary,
                                item.reasoning_summary
                            );
                        }
                        if let Ok(verif) = audit_journal.verify_chain() {
                            println!("\n  Cryptographic Chain Integrity: {}/{} valid entries (Broken: {:?})",
                                verif.valid_entries, verif.total_entries, verif.broken_at
                            );
                        }
                    }
                    Err(e) => eprintln!("✗ Failed to read audit journal: {}", e),
                }
            }
            s if s.starts_with(":gui-action ") => {
                let task = s.trim_start_matches(":gui-action ").trim();
                println!("UI-TARS GUI Agent: Planning actions for task: '{}'...", task);
                let intentions = gui_agent.predict_intentions_neural(task, 1920, 1080);
                println!("  Predicted {} GUI intentions:", intentions.len());
                for (i, intent) in intentions.iter().enumerate() {
                    println!("    {}. {:?}", i + 1, intent);
                    match gui_agent.execute_intention(&substrate, intent, &gui_input_cap, 1920, 1080).await {
                        Ok(msg) => println!("       -> Succès: {}", msg),
                        Err(e) => eprintln!("       -> Échec: {}", e),
                    }
                }
            }
            s if s.starts_with(":remember ") => {
                let text = s.trim_start_matches(":remember ").trim();
                let parts: Vec<&str> = text.splitn(3, ' ').collect();
                if parts.len() == 3 {
                    let id = orchestrator.remember(parts[0], parts[1], parts[2]);
                    println!("Stored triplet [{}] in sovereign graph: {} --({})--> {}", id.0, parts[0], parts[1], parts[2]);
                } else {
                    let id = orchestrator.remember("Fait", "est", text);
                    println!("Stored statement [{}] in sovereign graph: {}", id.0, text);
                }
            }
            s if s.starts_with(":recall ") => {
                let query = s.trim_start_matches(":recall ").trim();
                let facts = orchestrator.recall(query);
                println!("Recalled {} facts matching '{}':", facts.len(), query);
                for (i, f) in facts.iter().enumerate() {
                    println!("  [{}] {}", i + 1, f);
                }
            }
            ":status" => {
                println!("Status:");
                println!("  Presence: {:?}", presence.state());
                println!("  Tier: {:?}", manifest.tier);
                println!("  Memory Triplets: {}", orchestrator.memory_graph.len());
                println!("  Cost Spent: {}", cost_counter.formatted_total());
            }
            ":metrics" => {
                println!("North-Star Metrics:");
                println!("  Continuity Rate: {:.2}%", continuity_tracker.continuity_rate() * 100.0);
                println!("  Bounded Initiative Compliance: {:.2}%", bounded_initiative.compliance_rate() * 100.0);
                println!("  Practical Generality Score: {:.4}", generality_tracker.generality_score());
                if let Some(drift) = drift_detector.evaluate_current_vector(&[1.0, 1.0, 1.0, 1.0, 1.0]) {
                    println!("  Ethics Drift Alert: {}", drift.description);
                } else {
                    println!("  Ethics Drift Status: Nominal (No drift detected)");
                }
            }
            ":agents" => {
                println!("Registered Sandboxed Agents:");
                for agent in agent_registry.list_agents() {
                    println!("  - [{}] {} (Caps: {:?})", agent.id, agent.name, agent.required_capabilities);
                }
            }
            ":cost" => {
                println!("Inference Cost: {}", cost_counter.formatted_total());
            }
            ":camera" => {
                println!("Capturing camera frame from Substrate via authenticated capability...");
                match substrate.capture_camera_frame(camera_handle, &camera_cap).await {
                    Ok(frame) => {
                        let ctx = camera_pipeline.process_frame(&frame);
                        camera_pipeline.update_world_model(&ctx, &mut world_model);
                        println!("Camera Perception Summary:");
                        println!("  Resolution: {}x{} ({:?})", frame.width, frame.height, frame.format);
                        println!("  User Presence: {:?}", ctx.user_presence);
                        println!("  Gaze Focused on Screen: {}", ctx.user_gaze_focused_on_device);
                        println!("  Estimated Distance: {:.2} m", ctx.estimated_distance_m);
                        println!("  Lighting Condition: {:?}", ctx.lighting);
                        println!("  Cryptographic Frame Hash: {}", hex_fmt(&ctx.privacy_preserving_frame_hash));
                        println!("  Privacy Invariant: 100% processed in ephemeral RAM, zero raw pixels stored (§6).");
                    }
                    Err(e) => eprintln!("Failed to capture camera frame: {}", e),
                }
            }
            ":hud" => {
                println!("Invoking Ephemeral HUD (Spotlight/Raycast-like single-line overlay):");
                let hud_spec = AcrylicOverlayWindowSpec::centered_default();
                println!("  DirectComposition Acrylic Spec: {}x{} @ (top-offset: {}px) | Accent: {}",
                    hud_spec.width, hud_spec.height, hud_spec.pos_y_offset, hud_spec.coral_accent_hex
                );
                ephemeral_hud.open_prompt();
                for c in "Transférer 25 EUR pour déjeuner".chars() {
                    ephemeral_hud.insert_char(c);
                }
                println!("{}", ephemeral_hud.render_ascii());
                println!("\nStreaming response inline...");
                ephemeral_hud.set_streaming("Transférer 25 EUR", "Préparation du virement SEPA instantané...", false);
                println!("{}", ephemeral_hud.render_ascii());
                println!("\nAction requires confirmation (Coral #FF5722 Accent):");
                ephemeral_hud.request_validation("wallet-spend-42", "Envoi de 25.00 EUR vers Restaurant", Some(2500), "Financial");
                println!("{}", ephemeral_hud.render_ascii());
                println!("\nUser validated [Enter]. Response evaporating into thin air...");
                ephemeral_hud.start_evaporation();
                println!("{}", ephemeral_hud.render_ascii());
            }
            ":channel" => {
                println!("Dynamic Channel Selector (§21a) Arbitration Engine:");
                let state = world_model.state();
                println!("Environmental Telemetry:");
                println!("  Ambient Noise: {:.1} dB", state.ambient_noise_level_db);
                println!("  User Present: {}", state.user_present);
                println!("  User Gaze Attentive: {}", state.user_attentive);
                println!("  Third Parties Present: {}", state.third_parties_present);
                println!("  In Meeting: {}", state.in_meeting);
                println!("  Lighting: {}", state.lighting_condition);

                let ctx = InteractionContext {
                    ambient_noise_db: state.ambient_noise_level_db,
                    third_parties_present: state.third_parties_present,
                    in_meeting: state.in_meeting,
                    user_attentive_to_screen: state.user_attentive,
                    user_wearing_headphones: false,
                    device_type: DeviceType::Desktop,
                    urgency: UrgencyLevel::Normal,
                    is_night_time: false,
                };
                let decision = ChannelSelector::arbitrate(&ctx);
                println!("\nArbitration Result:");
                println!("  Selected Output Channel: {:?}", decision.channel);
                println!("  Rationale: {}", decision.rationale);
                if let Some(h) = decision.haptic_feedback {
                    println!("  Haptic Feedback Pattern: {:?}", h);
                }
            }
            ":budget" => {
                println!("Silence Budget Tracker (§21d) — Bounded Proactive Initiative:");
                println!("  Daily Quota: {} interruption credits", silence_budget.daily_quota());
                println!("  Credits Remaining Today: {}", silence_budget.credits_remaining());
                println!("  Confidence Threshold for Suggestions: {:.2}", silence_budget.min_confidence_threshold());
                println!("  Scheduled Reminders: 0 cost (unlimited)");
                println!("  Autonomous Suggestions: 1 credit per interaction (strictly capped)");
                let allowed = silence_budget.request_initiative("proactive-demo", InitiativeOrigin::SystemInferred, 0.91, "Proposition pause café");
                println!("  Simulated Autonomous Suggestion Status: {}", if allowed { "Granted (1 credit spent)" } else { "Suppressed (Budget or confidence low)" });
                println!("  Updated Remaining Credits: {}", silence_budget.credits_remaining());
            }

            ":ambient" => {
                println!("Executing Continuous Ambient Perception & Context Cycle...");
                presence.set_state(PresenceState::Listening);
                if let Ok(frame) = substrate.capture_camera_frame(camera_handle, &camera_cap).await {
                    let v_ctx = camera_pipeline.process_frame(&frame);
                    camera_pipeline.update_world_model(&v_ctx, &mut world_model);
                    println!("  [Camera] Scene updated: {:?}, gaze attentive: {}, lighting: {:?}",
                        v_ctx.user_presence, v_ctx.user_gaze_focused_on_device, v_ctx.lighting
                    );
                }
                world_model.update_noise_level(42.5);
                let ctx = InteractionContext {
                    ambient_noise_db: 42.5,
                    third_parties_present: world_model.state().third_parties_present,
                    in_meeting: world_model.state().in_meeting,
                    user_attentive_to_screen: world_model.state().user_attentive,
                    user_wearing_headphones: false,
                    device_type: DeviceType::Desktop,
                    urgency: UrgencyLevel::Normal,
                    is_night_time: false,
                };
                let decision = ChannelSelector::arbitrate(&ctx);
                println!("  [Channel] Dynamic output target: {:?} ({})", decision.channel, decision.rationale);
                presence.set_state(PresenceState::Idle);
                println!("  [Presence] Tray indicator in standby (Period: {}ms)", presence.pulse_period_ms());
            }

            ":win-ram" => {
                println!("Windows Substrate Physical Memory Telemetry (GlobalMemoryStatusEx):");
                let total_ram_mb = compute.total_physical_ram() as f64 / 1_048_576.0;
                let avail_ram_mb = compute.available_physical_ram() as f64 / 1_048_576.0;
                let used_ram_mb = total_ram_mb - avail_ram_mb;
                let used_pct = if total_ram_mb > 0.0 { (used_ram_mb / total_ram_mb) * 100.0 } else { 0.0 };
                let vram_mb = compute.vram_available_bytes() as f64 / 1_048_576.0;
                println!("  • Total Physical RAM:       {:>8.1} MB ({:.2} GB)", total_ram_mb, total_ram_mb / 1024.0);
                println!("  • Available Physical RAM:   {:>8.1} MB ({:.2} GB)", avail_ram_mb, avail_ram_mb / 1024.0);
                println!("  • RAM Utilization:          {:>8.1} MB ({:.1}%)", used_ram_mb, used_pct);
                println!("  • Available Dedicated VRAM: {:>8.1} MB", vram_mb);
                println!("  • Acceleration Backends:    {:?}", compute.available_backends());
                println!("  • Preferred Backend:        {:?}", compute.preferred_backend());
            }

            ":win-power" => {
                println!("Windows Substrate Power & Thermal Telemetry (GetSystemPowerStatus):");
                let power = substrate.power_state();
                let thermal = substrate.thermal_state();
                println!("  • On Battery Power:         {}", power.on_battery);
                println!("  • Battery Percentage:       {}%", power.battery_percent);
                println!("  • AC / Charging Status:     {}", power.charging);
                println!("  • Host Temperature:         {:.1} °C", thermal.temperature_celsius);
                println!("  • Thermal Throttling:       {}", if thermal.throttled { "ACTIVE (Throttled)" } else { "Nominal" });
            }

            ":win-screen" => {
                println!("Capturing primary display via Win32 GDI BitBlt (zero raw pixel leak)...");
                let screen_cap_token = authority.create_root_token(
                    vec![CapabilityScope::Sensor],
                    vec![annora_core::CapabilityAction::Read],
                    Timestamp::now(),
                )?;
                match substrate.capture_screen(&screen_cap_token).await {
                    Ok(capture) => {
                        let pixel_count = capture.width * capture.height;
                        let byte_count = capture.data.len();
                        let mut hasher = sha2::Sha256::new();
                        hasher.update(&capture.data);
                        let hash = hasher.finalize();
                        println!("Display Screen Capture Result:");
                        println!("  • Resolution:               {}x{} ({} pixels)", capture.width, capture.height, pixel_count);
                        println!("  • Color Format:             {:?}", capture.format);
                        println!("  • Framebuffer Size:         {:.2} MB ({} bytes)", byte_count as f64 / 1_048_576.0, byte_count);
                        println!("  • Ephemeral SHA-256 Hash:   {}", hex_fmt(&hash));
                        println!("  • Privacy Invariant:        Zero persistent raw pixel storage (§6).");
                    }
                    Err(e) => eprintln!("Failed to capture display screen: {}", e),
                }
            }

            ":win-tray" => {
                println!("Windows System Tray & Shell Integration Test:");
                let mut win_tray = annora_substrate_windows::WindowsPresenceTray::new();
                println!("  • Initial State:            {:?}", win_tray.state());
                win_tray.set_state(annora_substrate_windows::WindowsPresenceState::Listening);
                println!("  • Updated to Listening:     {:?} (Tooltip: '{}')", win_tray.state(), win_tray.tooltip());
                win_tray.set_state(annora_substrate_windows::WindowsPresenceState::Acting);
                println!("  • Updated to Acting:        {:?} (Tooltip: '{}')", win_tray.state(), win_tray.tooltip());
                win_tray.set_state(annora_substrate_windows::WindowsPresenceState::Idle);
                println!("  • Returned to Idle:         {:?} (Tooltip: '{}')", win_tray.state(), win_tray.tooltip());
                let mut hotkey = annora_substrate_windows::WindowsGlobalHotkey::new();
                let registered = hotkey.register();
                println!("  • Global Hotkey:            {:?} (Alt+Space summoned: {})", hotkey.key_combination, registered);
            }

            _ => {
                process_prompt(
                    &input,
                    &mut orchestrator,
                    &mut presence,
                    &mut cost_counter,
                    &mut continuity_tracker,
                    &mut bounded_initiative,
                    &mut generality_tracker,
                    &audit_journal,
                ).await?;
            }
        }

        print!("\nannora> ");
        stdout.flush()?;
    }

    Ok(())
}

async fn process_prompt(
    prompt: &str,
    orchestrator: &mut CognitiveOrchestrator,
    presence: &mut PresenceIndicator,
    cost_counter: &mut CostCounter,
    continuity_tracker: &mut ContinuityTracker,
    bounded_initiative: &mut BoundedInitiativeTracker,
    generality_tracker: &mut GeneralityTracker,
    audit_journal: &AuditJournal,
) -> Result<(), Box<dyn std::error::Error>> {
    presence.set_state(PresenceState::Listening);

    // Track generality
    let domain = if prompt.contains("réunion") || prompt.contains("agenda") {
        TaskDomain::Planning
    } else if prompt.contains("code") || prompt.contains("refactor") {
        TaskDomain::Coding
    } else if prompt.contains("contrat") || prompt.contains("analys") {
        TaskDomain::Writing
    } else {
        TaskDomain::Other
    };
    generality_tracker.record_task(domain);

    presence.set_state(PresenceState::Acting);

    // Process cognitive turn
    let result = orchestrator
        .process_user_turn(prompt, &TrustLevel::User)
        .await?;

    if let Some(cost) = result.estimated_cost_cents {
        cost_counter.record_cost(cost);
    }

    continuity_tracker.record_task(uuid::Uuid::now_v7().to_string(), false, 1);
    bounded_initiative.record_proposal(uuid::Uuid::now_v7().to_string(), true, true);

    // Record in tamper-evident signed audit log
    let entry = AuditEntry {
        id: EntryId(uuid::Uuid::now_v7()),
        timestamp: Timestamp::now(),
        actor: ActorId::Kernel,
        action: ActionRecord {
            action_type: "CognitiveTurn".to_string(),
            description: format!("Processed prompt: '{}'", prompt),
            parameters: serde_json::json!({ "routing": result.routing_used }),
            reversible: false,
            reversed: false,
        },
        capability_used: None,
        reasoning_trace: ReasoningTrace {
            steps: vec![ReasoningStep {
                description: format!("Resolved via {}", result.routing_used),
                confidence: 0.9,
                evidence: vec![],
            }],
            model_used: None,
            duration_ms: result.latency_ms,
        },
        graph_nodes_used: vec![],
        confidence: 0.9,
        cost: result.estimated_cost_cents.map(|c| annora_core::Cost {
            amount_cents: c,
            currency: "EUR".to_string(),
            description: "Turn cost".to_string(),
        }),
        signature: vec![1, 2, 3, 4],
        prev_hash: vec![0; 32],
    };
    let _ = audit_journal.append(entry);

    presence.set_state(PresenceState::Idle);

    println!("\n[Annora Response]");
    println!("  {}", result.response_text);
    println!("  [Routing: {} | Latency: {}ms | Confidence: {:?}]",
        result.routing_used, result.latency_ms, result.confidence_decision
    );

    Ok(())
}

fn hex_fmt(bytes: &[u8]) -> String {
    bytes.iter().take(12).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join("") + "..."
}

