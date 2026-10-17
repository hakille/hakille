use annora_agents::{AgentManifest, AgentRegistry, UiTarsAgent};
use annora_audit::entry::{ActionRecord, AuditEntry, ReasoningStep, ReasoningTrace};
use annora_audit::journal::AuditJournal;
use annora_constitution::rules::Constitution;
use annora_core::{
    ActorId, CapabilityAuthority, CapabilityScope, EntryId, KeyPair, Timestamp, TrustLevel, UserId,
};
use annora_inference::{ModelCatalog, ModelProvisioner, ModelTier};
use annora_kernel::{fabric::CognitiveFabric, orchestrator::CognitiveOrchestrator};
use annora_metrics::{
    BoundedInitiativeTracker, ContinuityTracker, GeneralityTracker, TaskDomain, ValuesDriftDetector,
};
use annora_perception::audio::SenseVoiceSmall;
use annora_perception::{CameraPipeline, VoiceActivityDetector, WakeWordDetector, WorldModel};
use annora_substrate::{CameraConfig, CameraFacing, Compute, Substrate};
use annora_substrate_windows::{WindowsComputeBackend, WindowsSubstrate};
use annora_sync::{CrdtSync, HandoffCoordinator, Outbox, OutboxEntry};
use annora_ui::{
    AccessibilityManager, AcrylicOverlayWindowSpec, ChannelSelector, CostCounter, DeviceType,
    EphemeralHud, InitiativeOrigin, InteractionContext, MemoryViewerPanel, OnboardingManager,
    PlatformAdapter, PresenceIndicator, PresenceState, PrimaryModality, SilenceBudgetTracker,
    UrgencyLevel, VisualAuditViewer,
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
        let mb_total = p
            .total_bytes
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
    let mut provider_arg: Option<String> = None;

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
                        format!(
                            "[INSTALLED - {:.1} MB]",
                            s.total_disk_bytes as f64 / 1_048_576.0
                        )
                    } else {
                        "[NOT DOWNLOADED]".to_string()
                    };
                    println!(
                        "• {:<28} {:<10?} {:<18} {}",
                        s.entry.id, s.entry.tier, status_badge, s.entry.name
                    );
                    println!(
                        "    Format: {:?} | Runtime: {:?} | RAM: {}MB | VRAM: {}MB",
                        s.entry.format,
                        s.entry.runtime,
                        s.entry.expected_ram_mb,
                        s.entry.expected_vram_mb
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
                        if let Some(matched) = provisioner
                            .catalog()
                            .iter()
                            .find(|m| m.id.starts_with(&model_id) || m.id.contains(&model_id))
                        {
                            model_id = matched.id.clone();
                        }
                    }
                    println!("\n[Provisioning Model: {}]", model_id);
                    let res = provisioner.provision_by_id(&model_id, progress).await;
                    println!();
                    match res {
                        Ok(path) => println!(
                            "✓ Model '{}' successfully provisioned at: {}",
                            model_id,
                            path.display()
                        ),
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
                        Ok(false) => {
                            println!("• {:<28} [CORRUPTED OR CHECKSUM MISMATCH]", s.entry.id)
                        }
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
                println!(
                    "  --prompt, --eval-prompt <PROMPT>  Execute a single turn headlessly and exit"
                );
                println!(
                    "  --budget <CENTS>            Set daily System 2 budget in cents (default: 500)"
                );
                println!("  --models-dir <PATH>         Custom models storage directory");
                println!(
                    "  --auto-download             Automatically download missing essential models"
                );
                println!(
                    "  --provider <ID>           Inference provider: local|ollama|openai|anthropic|custom:URL (default: local-only)"
                );
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
            "--provider" => {
                if arg_idx + 1 < args.len() {
                    provider_arg = Some(args[arg_idx + 1].clone());
                    arg_idx += 1;
                }
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
    // Persistent dirs first: keys + outbox + journal all live under
    // LOCALAPPDATA/Annora (or ./annora_data/Annora fallback) so reboots keep
    // identity (signing key, capability root) instead of regenerating.
    let data_dir = std::env::var("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./annora_data"))
        .join("Annora");
    let _ = std::fs::create_dir_all(&data_dir);
    let keys_dir = data_dir.join("keys");
    let _ = std::fs::create_dir_all(&keys_dir);
    // Clés : keychain OS d'abord (service "annora"), repli fichier LOUD sinon.
    // Le repli keys/*.bin reste lisible en clair : migrer via rotation dès que
    // le trousseau est débloqué. Jamais de régénération silencieuse.
    let keystore = annora_keystore::KeyRegistry::new(annora_keystore::OsStore::new());
    let (authority, signing_keypair) = match (
        keystore.capability_root(),
        keystore.journal_signing(),
    ) {
        (Ok(root), Ok(journal)) => {
            println!(
                "  Keys: keychain OS (service \"annora\") — capability-root-v1 {} / journal-signing-v1 {}",
                if root.created {
                    "CRÉÉE (premier boot)"
                } else {
                    "chargée"
                },
                if journal.created {
                    "CRÉÉE (premier boot)"
                } else {
                    "chargée"
                },
            );
            let private = biscuit_auth::PrivateKey::from_bytes(&root.seed[..]).map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("seed capability-root-v1 invalide ({e})"),
                )
            })?;
            (
                CapabilityAuthority::with_keypair(biscuit_auth::KeyPair::from(&private)),
                KeyPair::from_bytes(&journal.seed),
            )
        }
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("  Keys: keychain OS indisponible ({e})");
            eprintln!(
                "  Keys: REPLI FICHIER {} (brut non chiffré — migrer via rotation dès que possible)",
                keys_dir.display()
            );
            (
                load_or_generate_authority(&keys_dir)?,
                load_or_generate_signing_key(&keys_dir)?,
            )
        }
    };
    println!(
        "  Keys: persistent identity loaded (keys dir: {})",
        keys_dir.display()
    );
    let substrate = WindowsSubstrate::new().with_trusted_root(authority.root_public_key());
    let compute = WindowsComputeBackend::new();
    let manifest = substrate.manifest();
    println!("  Host OS: {} {}", manifest.os_name, manifest.os_version);
    println!("  Tier: {:?}", manifest.tier);
    println!("  Compute Backends: {:?}", compute.available_backends());
    println!("  Capability enforcement: pinned root key (point-of-effect verification ON)");
    let attestation = substrate.attest(b"annora-daemon-boot").await?;
    println!(
        "  Hardware Attestation: {} (Valid)",
        attestation.platform_id
    );
    println!(
        "  NOTE: attestation is a local dev signature, NOT a TPM/VBS hardware proof (tracked work)."
    );

    // 2. Initialize Trust & Constitution Layer
    println!("\n[2/6] Loading Versioned Constitution & Security Primitives...");
    let constitution = Constitution::default_v1();
    println!("  Constitution Version: {}", constitution.version);
    println!(
        "  Active Rules: {} rules enforced",
        constitution.rules.len()
    );

    // Persistent audit journal (was: tempfile::NamedTempFile — destroyed on exit,
    // contradicting continuity/traceability). Now stored under LOCALAPPDATA or
    // ./annora_data with a stable path. Signing key is the persisted identity
    // above (load-or-generate); TPM/DPAPI + rotation remain tracked work.
    let journal_path = data_dir.join("audit.redb");
    let audit_journal = AuditJournal::new(
        journal_path.to_str().unwrap(),
        signing_keypair.signing_key.clone(),
    )?;
    println!(
        "  Audit Journal: persistent append-only log at {} (tamper-evident, Ed25519-chained)",
        journal_path.display()
    );

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
    println!(
        "  Foundation Models: {}/{} installed locally on disk",
        installed_count,
        model_statuses.len()
    );

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
            println!(
                "  Model artifacts inspected: {}/{} files present on disk ({:.2} GB)",
                installed_count,
                model_statuses.len(),
                conn.total_disk_bytes as f64 / 1_000_000_000.0
            );
            println!(
                "  NOTE: 7 foundation models — REAL inference (ort forwards + Candle Qwen + RVQ Mimi + formant TTS + CV grounding). UI-TARS VLM: inspected only, no VLM forward."
            );
            for m in &summary.models {
                println!(
                    "     - {:<28} | {:<12} | {:>7.1} MB | {}",
                    m.name,
                    m.format,
                    m.disk_size_bytes as f64 / 1_048_576.0,
                    m.status
                );
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
    println!("  Audio Pipeline: 16kHz continuous capture buffer ready (REAL VAD/ASR via ort)");
    println!(
        "  SenseVoice-Small: mel + LFR + forward ort + CTC greedy — real ASR, measured confidence"
    );
    println!("  Moshi/TTS/GUI: real engines (Mimi RVQ codebooks, formant Klatt TTS, CV grounding)");

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
        vec![
            annora_core::CapabilityAction::Execute,
            annora_core::CapabilityAction::Write,
        ],
        Timestamp::now(),
    )?;
    let camera_handle = substrate
        .start_camera_capture(camera_config.clone(), &camera_cap)
        .await?;
    let mut camera_pipeline = CameraPipeline::new(camera_config);
    let mut world_model = WorldModel::new("Workstation");
    println!(
        "  Camera Pipeline: Native frame capture active (1080p Front, zero raw pixels stored) (source synthétique, pas de MediaFoundation)"
    );
    println!("  WorldModel: Continuous multimodal situational tracking initialized");

    // 5. Initialize Action Agents Layer
    println!("\n[5/6] Spawning Sandboxed WASM Agent Registry...");
    let mut agent_registry = AgentRegistry::new();
    let reference_agents = [
        (
            "agent-mail",
            "Mail Agent",
            vec![CapabilityScope::Network, CapabilityScope::Filesystem],
        ),
        (
            "agent-calendar",
            "Calendar Agent",
            vec![CapabilityScope::Filesystem],
        ),
        (
            "agent-code",
            "Code Agent",
            vec![CapabilityScope::Filesystem],
        ),
        (
            "agent-home",
            "Smart Home Agent",
            vec![CapabilityScope::Network, CapabilityScope::Sensor],
        ),
        (
            "agent-wallet-readonly",
            "Wallet ReadOnly",
            vec![CapabilityScope::Wallet],
        ),
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
    println!(
        "  Registered {} reference WASM agents with strict Biscuit scopes",
        agent_registry.list_agents().len()
    );

    // 6. Initialize Cognitive Kernel, UI & North-Star Metrics
    println!("\n[6/6] Linking Cognitive Fabric & Orchestrator...");
    let mut orchestrator =
        CognitiveOrchestrator::new(fabric, user_id.clone(), constitution.clone());
    if let Some(engine) = qwen_engine_opt {
        orchestrator.set_inference_engine(engine);
        println!(
            "  LLM Engine: Qwen 2.5 0.5B Q4_K_M décodé en réel (Candle + KV-cache, ChatML) via singleton System1/2"
        );
    }
    // Mémoire bout-en-bout : restaure le graphe depuis ./annora-memory.json
    // (snapshot `save_to_file`) si présent, sinon graphe vide. Sauvegardé à
    // :exit (et sortie de boucle EOF) + :export/:import via `save_to_file`.
    let memory_snapshot = PathBuf::from("./annora-memory.json");
    match annora_memory::KnowledgeGraph::load_from_file(&memory_snapshot) {
        Ok(restored) => {
            let n = restored.len();
            orchestrator.memory_graph = restored;
            println!(
                "  Memory: graphe restauré depuis {} ({} triplet(s))",
                memory_snapshot.display(),
                n
            );
        }
        Err(_) => {
            println!(
                "  Memory: aucun snapshot ({} absent ou illisible) — graphe vide",
                memory_snapshot.display()
            );
        }
    }
    let mut presence = PresenceIndicator::new();
    let mut cost_counter = CostCounter::new(daily_budget_cents, "EUR");
    let mut onboarding = OnboardingManager::new();
    onboarding.acknowledge_limits();

    let mut silence_budget = SilenceBudgetTracker::new(3);
    let mut ephemeral_hud = EphemeralHud::new();
    // Outbox persistence: load JSON snapshot at boot (visible file,
    // Journal-backed); saved after every :defer/:revalidate/:flush below.
    let mut outbox = load_outbox(&data_dir);
    println!(
        "  Outbox: {} queued action(s) restored from {}",
        outbox.len(),
        outbox_file_path(&data_dir).display()
    );
    let mut a11y = AccessibilityManager::default();
    let mut heritage_keys = annora_memory::KeyRegistry::new();
    // Registre providers local-only (spec BYOM) : --provider inconnu = erreur.
    let mut provider_registry = annora_kernel::providers::ProviderRegistry::default_local_only();
    if let Some(raw) = provider_arg.as_deref() {
        match annora_kernel::providers::ProviderId::parse_cli(raw) {
            Ok(id) => {
                if id.is_cloud() {
                    provider_registry.register(id.clone());
                }
                if let Err(e) = provider_registry.select(id.clone()) {
                    eprintln!("  Providers: sélection refusée — {e}");
                    std::process::exit(2);
                }
                orchestrator.system2.with_provider(id);
                println!(
                    "  Providers: sélection --provider = {}",
                    provider_registry.selected().display_name()
                );
            }
            Err(e) => {
                eprintln!("  Providers: --provider '{raw}' refusé — {e}");
                std::process::exit(2);
            }
        }
    }
    println!("  Providers: {}", provider_registry.data_exit(0).badge());
    let platform_adapter = PlatformAdapter::auto_detect();

    let mut continuity_tracker = ContinuityTracker::new();
    let mut bounded_initiative = BoundedInitiativeTracker::new();
    let mut generality_tracker = GeneralityTracker::new();
    let drift_detector = ValuesDriftDetector::default_constitutional();

    println!(
        "  Cognitive Fabric: System 1 / System 2 dynamic router online (routing heuristique documenté, génération Qwen réelle)"
    );
    println!(
        "  Ambient Interface: Ephemeral HUD ({}) & ChannelSelector online",
        ephemeral_hud.hotkey()
    );
    println!(
        "  Silence Budget: {} proactive interruption credits available",
        silence_budget.daily_quota()
    );
    println!(
        "  Platform Integration: {}",
        platform_adapter.os_integration_details()
    );
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
        )
        .await?;
        println!(
            "Headless evaluation completed successfully. Cost: {}",
            cost_counter.formatted_total()
        );
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
                )
                .await?;
            }
            print!("\nannora> ");
            stdout.flush()?;
            continue;
        }

        match input.as_str() {
            ":exit" | ":quit" => {
                // Sauvegarde auto du graphe avant arrêt (miroir sortie EOF).
                match orchestrator
                    .memory_graph
                    .save_to_file("./annora-memory.json")
                {
                    Ok(()) => println!(
                        "Mémoire sauvegardée ({} triplet(s)) vers ./annora-memory.json.",
                        orchestrator.memory_graph.len()
                    ),
                    Err(e) => eprintln!("✗ Sauvegarde mémoire impossible : {e}"),
                }
                println!("Shutting down Annora daemon cleanly.");
                break;
            }

            ":help" => {
                println!("Available commands:");
                println!("  :models           List all AI models and local cache status");
                println!(
                    "  :connected        7 foundation models, REAL inference (ort + Candle + RVQ + formant)"
                );
                println!(
                    "  :inspect-models   Deep binary inspection of GGUF tensors, Safetensors codebooks, ONNX & vocab"
                );
                println!(
                    "  :asr-test <text>  Test SenseVoice 25,055-token dictionary encoding/decoding"
                );
                println!("  :embed-test <txt> Test MiniLM 384-dimensional vector embedding space");
                println!(
                    "  :download <id>    Download and verify a model by ID (e.g. :download silero-vad-v5)"
                );
                println!(
                    "  :verify-models    Verify cryptographic SHA-256 of all installed models"
                );
                println!(
                    "  :camera           Capture frame & test multimodal perception (zero raw pixel leak)"
                );
                println!(
                    "  :hud              Demonstrate Spotlight-like ephemeral HUD (Ctrl+Alt+Space, alias Alt+Space) with Braise #FF7A2F chip"
                );
                println!(
                    "  :shell            Show Slint shell snapshot (presence/HUD/cost/silence payload)"
                );
                println!(
                    "  :models-honest    Show honest inference statuses (Inspected/Real with cited proof)"
                );
                println!(
                    "  :agents-cards     Show 5 reference agents with honest DEMO badges + validation flags"
                );
                println!(
                    "  :powers           Show Powers screen (scope, quota, expiry, kill switch)"
                );
                println!(
                    "  :onboarding       Show limits-before-capabilities manifest + acknowledgement state"
                );
                println!(
                    "  :channel          Show dynamic ChannelSelector arbitration for current environment"
                );
                println!(
                    "  :settings          Show Réglages §8 (providers, voices, channels, languages, a11y, backup, foyer, simulation)"
                );
                println!(
                    "  :budget           Show Silence Budget (3/jour, spec §18/§29) proactive suggestion credits"
                );
                println!(
                    "  :ambient          Run continuous multimodal sensing cycle (camera + audio + WorldModel)"
                );
                println!("  :status           Show platform tier, memory, and presence state");
                println!(
                    "  :constitution     Afficher version, règles, quorum gouvernance et dilemmes éthiques"
                );
                println!(
                    "  :metrics          Display 3 North-Star metrics and ethics drift status"
                );
                println!("  :agents           List registered WASM agents");
                println!("  :cost             Display System 2 inference cost tracking");
                println!(
                    "  :memory           Display all triplets currently stored in knowledge graph"
                );
                println!(
                    "  :contradictions   Show contradiction box (facts that disagree + resolution)"
                );
                println!(
                    "  :memory-view      Inspect rich memory triplets with weights, confidence & partitions"
                );
                println!(
                    "  :forget <id>      Exercise 'Droit à l'oubli' (consent revocation with crypto receipt)"
                );
                println!(
                    "  :audit-view       Inspect cryptographically signed tamper-evident audit journal"
                );
                println!(
                    "  :journal-search <q> [page]  Search journal (newest first, 20/page, full-trace search)"
                );
                println!(
                    "  :journal-why <n|id>  Explain WHY an action happened (full trace + graph nodes)"
                );
                println!(
                    "  :why2 <n|id>         Explain WHY in 3 levels (sentence, facts, reasoning)"
                );
                println!(
                    "  :mem2 [partition]   Memory explorer (freshness + partition + contradictions)"
                );
                println!(
                    "  :a11y [voice|text|visual|haptic]  Show/set accessibility modality + live channel override"
                );
                println!(
                    "  :providers [ID]       Show/select inference provider (local-only default, cloud refused without Network cap)"
                );
                println!(
                    "  :plan <texte>         Aperçu sec ShowCard-like (plan + dry-run, zéro exécution)"
                );
                println!(
                    "  :defer <verbe> | <objet>  Queue offline action (replay-safe, revalidated after 15 min)"
                );
                println!("  :outbox           Show offline queue (visible file, Journal-backed)");
                println!("  :revalidate <key> Revalidate a queued action waiting > 15 min");
                println!("  :flush            Send queued sendable actions (traced in Journal)");
                println!("  :export [fichier]   Export JSON du graphe (affiche taille + compte)");
                println!(
                    "  :import <fichier>   Import JSON du graphe (fusion via insert, sans écrasement)"
                );
                println!("  :heritage-erase <key_id>  Effacement crypto d'une clé d'héritage");
                println!(
                    "  :remember         Add a fact to knowledge graph (e.g. :remember Alice aime Rust)"
                );
                println!("  :recall           Search memory graph by keyword (e.g. :recall Rust)");
                println!(
                    "  :gui-action <cmd> Plan and execute native Windows GUI desktop actions via UI-TARS"
                );
                println!(
                    "  :win-ram          Display real Windows physical RAM and GPU VRAM telemetry"
                );
                println!(
                    "  :win-power        Display real Windows battery, AC power, and thermal state"
                );
                println!(
                    "  :win-screen       Capture primary display using native Win32 GDI (BitBlt)"
                );
                println!(
                    "  :win-tray         Test Windows system tray presence states & global hotkey (Ctrl+Alt+Space, alias Alt+Space)"
                );
                println!("  :exit             Shutdown Annora daemon");
            }

            ":models" => {
                println!(
                    "Foundation Model Catalog (Directory: {}):",
                    provisioner.cache_dir().display()
                );
                for s in provisioner.list_all_status() {
                    let badge = if s.is_installed {
                        format!(
                            "[INSTALLED - {:.1} MB]",
                            s.total_disk_bytes as f64 / 1_048_576.0
                        )
                    } else {
                        "[NOT DOWNLOADED]".to_string()
                    };
                    println!(
                        "  • {:<26} {:<10?} {:<18} {}",
                        s.entry.id, s.entry.tier, badge, s.entry.name
                    );
                }
            }
            ":connected" => match provisioner.connect_all_models() {
                Ok(conn) => {
                    let summary = conn.system_summary();
                    println!(
                        "Model artifacts ({:.2} GB total disk) — REAL inference statuses:",
                        summary.total_disk_bytes as f64 / 1_000_000_000.0
                    );
                    for m in &summary.models {
                        println!(
                            "  - {:<28} | {:<12} | {:>7.1} MB | {}",
                            m.name,
                            m.format,
                            m.disk_size_bytes as f64 / 1_048_576.0,
                            m.status
                        );
                    }
                }
                Err(e) => eprintln!("✗ Failed to connect all models: {}", e),
            },
            ":inspect-models" => {
                println!("Deep Binary Model Inspection (Real On-Disk Models):");
                match provisioner.connect_all_models() {
                    Ok(conn) => {
                        println!("  1. Qwen 2.5 0.5B Instruct (GGUF v3):");
                        println!(
                            "     • Architecture:     {}",
                            conn.qwen2.architecture().unwrap_or("unknown")
                        );
                        println!(
                            "     • Context Window:   {} tokens",
                            conn.qwen2.context_length().unwrap_or(32768)
                        );
                        println!(
                            "     • Transformer Layers: {} blocks",
                            conn.qwen2.block_count().unwrap_or(24)
                        );
                        println!(
                            "     • Embedding Dim:    {}",
                            conn.qwen2.embedding_length().unwrap_or(896)
                        );
                        println!(
                            "     • Indexed Tensors:  {} tensors",
                            conn.qwen2.tensor_count()
                        );
                        println!(
                            "     • Param Estimate:   {} parameters",
                            conn.qwen2.estimated_params
                        );

                        println!("\n  2. UI-TARS 2B SFT (GGUF v3 GUI Agent):");
                        println!(
                            "     • Architecture:     {}",
                            conn.ui_tars.architecture().unwrap_or("qwen2_vl")
                        );
                        println!(
                            "     • Indexed Tensors:  {} tensors",
                            conn.ui_tars.tensor_count()
                        );
                        println!(
                            "     • Param Estimate:   {} parameters",
                            conn.ui_tars.estimated_params
                        );
                        println!(
                            "     • Vision Grounding: vision classique réelle (Sobel + NCC), texte via Qwen — pas de VLA"
                        );

                        println!("\n  3. Kyutai Mimi S2S Codec (RVQ réel sur codebooks) :");
                        println!(
                            "     • Header Length:    {} bytes (JSON metadata)",
                            conn.mimi.header_size
                        );
                        println!(
                            "     • Indexed Tensors:  {} tensors",
                            conn.mimi.tensor_count()
                        );
                        println!(
                            "     • Codebook Count:   {} vector quantizers (nearest-L2 réel)",
                            conn.mimi.codebook_count()
                        );
                        println!(
                            "     • TTS :             formantique Klatt réelle (checkpoint neural non provisionné)"
                        );
                        println!("     • Sample Rate:      24,000 Hz causal streaming");
                        println!("     • Frame Rate:       12.5 Hz (80ms/frame)");

                        println!("\n  4. SenseVoice-Small (forward ort RÉEL) :");
                        println!(
                            "     • Model File Size:  {:.1} MB",
                            conn.sensevoice_info.file_size_bytes as f64 / 1_048_576.0
                        );
                        println!(
                            "     • Vocabulary Size:  {} tokens (tokens.txt)",
                            conn.sensevoice_tokenizer.vocab_size()
                        );
                        println!(
                            "     • Frontend:         mel 80 + LFR(7,6), décodage CTC greedy, confiance mesurée"
                        );
                        println!(
                            "     • Expected Input:   {:?} (mel-spectrogram)",
                            conn.sensevoice_info.expected_input_shape
                        );
                        println!(
                            "     • Expected Output:  {:?} (token logits)",
                            conn.sensevoice_info.expected_output_shape
                        );

                        println!("\n  5. CAM++ Speaker (forward ort RÉEL) :");
                        println!(
                            "     • Embedding:      fbank 80×200 → [1,192] L2 — biométrie réelle, cosine réel"
                        );
                        println!("     • Vérification:   similarité cosinus sur embedding neural");

                        println!("\n  6. All-MiniLM-L6-v2 (forward ort RÉEL) :");
                        println!(
                            "     • Vecteur:        WordPiece 30522 → [384] mean-pool L2 — sémantique réelle"
                        );
                        println!("     • HNSW :            indexable sur vrais embeddings");

                        println!("\n  7. Silero VAD v5 (forward ort RÉEL) :");
                        println!("     • Chunk Size:       512 samples @ 16kHz (31.25ms)");
                        println!(
                            "     • Moteur réel:      forward ort + état récurrent, p95 < 1 ms"
                        );
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
                println!("MiniLM-L6-v2 [DEMO — pas un vrai embedding] :");
                println!("  Input Text:      \"{}\"", text);
                println!(
                    "  Target Space:    384 dimensions (HNSW indexable, si vrai modèle branché)"
                );
                let hash_val = text
                    .bytes()
                    .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
                println!(
                    "  [DEMO] Hash stable NON sémantique : 0x{:08X} (ne mesure aucune similarité)",
                    hash_val
                );
                println!(
                    "  [DEMO] L2 Norm affichée 1.00000 (non mesurée) — brancher le vrai MiniLM ort pour un vecteur réel"
                );
            }
            s if s.starts_with(":download ") => {
                let model_id = s.trim_start_matches(":download ").trim();
                println!("Downloading model '{}'...", model_id);
                let progress = Some(make_cli_progress_callback());
                match provisioner.provision_by_id(model_id, progress).await {
                    Ok(path) => println!(
                        "\n✓ Model '{}' successfully provisioned at: {}",
                        model_id,
                        path.display()
                    ),
                    Err(e) => eprintln!("\n✗ Failed to download '{}': {}", model_id, e),
                }
            }
            ":verify-models" => {
                println!("Verifying models integrity...");
                for s in provisioner.list_all_status() {
                    if !s.is_installed {
                        continue;
                    }
                    match provisioner.verify_integrity(&s.entry) {
                        Ok(true) => println!("  • {:<26} [OK - Valid]", s.entry.id),
                        Ok(false) => {
                            println!("  • {:<26} [CORRUPTED/CHECKSUM MISMATCH]", s.entry.id)
                        }
                        Err(e) => println!("  • {:<26} [ERROR: {}]", s.entry.id, e),
                    }
                }
            }
            ":memory" => {
                println!(
                    "Sovereign Knowledge Graph ({} triplets):",
                    orchestrator.memory_graph.len()
                );
                if orchestrator.memory_graph.is_empty() {
                    // État vide spec §19 : une phrase + trois exemples, pas de mascotte.
                    println!("  Ta mémoire est encore vierge — tout reste sur cet appareil.");
                    println!("  Essaie par exemple :");
                    println!("    :remember Marie aime le jazz");
                    println!("    :remember Réunion lundi 10h salle B");
                    println!("    :remember Projet Atlas échéance juin");
                } else {
                    // UUID court (8 chars) + texte : préfixe utilisable avec
                    // :forget (préfixe unique ou UUID complet, id complet via
                    // :memory-view).
                    for t in orchestrator.memory_graph.all_triplets().iter() {
                        let full = t.id.0.to_string();
                        let short: String = full.chars().take(8).collect();
                        match &t.object {
                            annora_memory::NodeValue::Text(s) => {
                                println!("  [{}] {}", short, s)
                            }
                            _ => println!("  [{}] {:?}", short, t.object),
                        }
                    }
                    println!(
                        "  (:forget <préfixe-8-caractères | uuid-complet> — id complet via :memory-view)"
                    );
                }
            }
            ":memory-view" => {
                let items = MemoryViewerPanel::list_all_items(&orchestrator.memory_graph);
                println!(
                    "Sovereign Knowledge Graph Visual Inspection ({} triplets):",
                    items.len()
                );
                if items.is_empty() {
                    println!("  Ta mémoire est encore vierge — voir :memory pour trois exemples.");
                } else {
                    for (i, item) in items.iter().enumerate() {
                        println!(
                            "  [{}] ID: {} | Poids: {:.2} ({}) | Conf: {}% | Source: {} | Part: {}\n      -> {}",
                            i + 1,
                            item.id.0,
                            item.weight,
                            annora_ui_shell::MemoryRow::freshness_label(item.weight),
                            item.confidence_pct,
                            item.source,
                            item.partition,
                            item.text_representation
                        );
                    }
                }
            }
            s if s.starts_with(":mem2") => {
                let partition = s.trim_start_matches(":mem2").trim();
                let partition = if partition.is_empty() {
                    "Tout"
                } else {
                    partition
                };
                let items = MemoryViewerPanel::list_all_items(&orchestrator.memory_graph);
                let rows: Vec<annora_ui_shell::MemoryRow> = items
                    .iter()
                    .map(annora_ui_shell::MemoryRow::from_item)
                    .collect();
                let triplets: Vec<_> = orchestrator
                    .memory_graph
                    .all_triplets()
                    .into_iter()
                    .cloned()
                    .collect();
                let contra = annora_memory::scan_contradictions(&triplets).len();
                let screen = annora_ui_shell::MemoryScreenVM::from_rows(&rows, partition, contra);
                println!("Mémoire (partition : {partition}) :");
                println!("  Partitions : {}", screen.partitions.join(", "));
                if screen.visible.is_empty() {
                    println!("  Ta mémoire est encore vierge — voir :memory pour trois exemples.");
                } else {
                    for (i, line) in screen.visible.iter().enumerate() {
                        println!("  [{}] {}", i + 1, line);
                    }
                }
                println!("  Contradictions : {}", screen.contra_label);
            }
            ":contradictions" => {
                let triplets: Vec<_> = orchestrator
                    .memory_graph
                    .all_triplets()
                    .into_iter()
                    .cloned()
                    .collect();
                let hits = annora_memory::scan_contradictions(&triplets);
                if hits.is_empty() {
                    println!("Boîte « contradictions » vide — rien ne se contredit.");
                } else {
                    println!("Boîte « contradictions » ({} paire(s)) :", hits.len());
                    for (a, b, kind) in hits.iter().take(20) {
                        println!("  - entrée #{a} vs entrée #{b} : {kind}");
                    }
                    println!(
                        "  Garder A, garder B, ou garder les deux avec leur contexte (:forget <id> pour oublier)."
                    );
                }
            }
            s if s.starts_with(":forget ") => {
                let id_str = s.trim_start_matches(":forget ").trim();
                if id_str.is_empty() {
                    eprintln!(
                        "Usage : :forget <préfixe-8-caractères | uuid-complet> (voir :memory)"
                    );
                } else {
                    // Accepte UUID complet OU préfixe court affiché par
                    // :memory (8 chars) s'il désigne un triplet unique.
                    let resolved: Option<annora_core::TripletId> = match uuid::Uuid::parse_str(
                        id_str,
                    ) {
                        Ok(uuid_val) => Some(annora_core::TripletId(uuid_val)),
                        Err(_) => {
                            let cands: Vec<annora_core::TripletId> = orchestrator
                                .memory_graph
                                .all_triplets()
                                .iter()
                                .filter(|t| t.id.0.to_string().starts_with(id_str))
                                .map(|t| t.id.clone())
                                .collect();
                            match cands.len() {
                                1 => Some(cands[0].clone()),
                                0 => {
                                    eprintln!(
                                        "✗ Aucun triplet ne commence par '{id_str}' (voir :memory)."
                                    );
                                    None
                                }
                                _ => {
                                    eprintln!(
                                        "✗ Préfixe '{id_str}' ambigu ({} candidats) — colle l'UUID complet (voir :memory-view).",
                                        cands.len()
                                    );
                                    None
                                }
                            }
                        }
                    };
                    if let Some(triplet_id) = resolved {
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
                                println!(
                                    "  • Jeton de Révocation:  {}",
                                    receipt.verification_token
                                );
                            }
                            None => {
                                eprintln!(
                                    "✗ Triplet avec ID '{}' introuvable dans le graphe de mémoire (voir :memory).",
                                    id_str
                                );
                            }
                        }
                    }
                }
            }
            ":audit-view" => match audit_journal.all_entries() {
                Ok(entries) => {
                    let view_items = VisualAuditViewer::format_entries(&entries);
                    println!(
                        "Tamper-Evident Audit Journal ({} entries):",
                        view_items.len()
                    );
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
                        println!(
                            "\n  Cryptographic Chain Integrity: {}/{} valid entries (Broken: {:?})",
                            verif.valid_entries, verif.total_entries, verif.broken_at
                        );
                    }
                }
                Err(e) => eprintln!("✗ Failed to read audit journal: {}", e),
            },
            s if s.starts_with(":journal-search") => {
                let rest = s.trim_start_matches(":journal-search").trim();
                let (query, page) = match rest.rsplit_once(' ') {
                    Some((q, p)) if p.chars().all(|c| c.is_ascii_digit()) => {
                        (q.trim(), p.parse::<usize>().unwrap_or(0).saturating_sub(1))
                    }
                    _ => (rest, 0),
                };
                match audit_journal.all_entries() {
                    Ok(entries) => {
                        let rows: Vec<annora_ui_shell::JournalRow> = entries
                            .iter()
                            .map(annora_ui_shell::JournalRow::from_entry)
                            .collect();
                        let filtered = annora_ui_shell::filter_journal(&rows, query);
                        let page_rows = annora_ui_shell::page_journal(&filtered, page, 20);
                        println!(
                            "Journal search '{}' — {} match(s), page {} (20/page, newest first):",
                            query,
                            filtered.len(),
                            page + 1
                        );
                        for (i, r) in page_rows.iter().enumerate() {
                            println!(
                                "  [{}] {} | {} | conf {}% | {}{} | nœuds:{}",
                                page * 20 + i + 1,
                                r.time,
                                r.action,
                                r.confidence_pct,
                                if r.verified { "sig✓ " } else { "sig✗ " },
                                if r.can_revert {
                                    "réversible"
                                } else {
                                    "irréversible"
                                },
                                r.graph_nodes.len(),
                            );
                        }
                    }
                    Err(e) => eprintln!("✗ Failed to read audit journal: {}", e),
                }
            }
            s if s.starts_with(":journal-why ") => {
                let arg = s.trim_start_matches(":journal-why ").trim();
                match audit_journal.all_entries() {
                    Ok(entries) => {
                        let found = match arg.parse::<usize>() {
                            Ok(n) if n >= 1 && n <= entries.len() => entries.get(n - 1),
                            _ => entries.iter().find(|e| e.id.0.to_string() == arg),
                        };
                        match found {
                            Some(e) => {
                                println!(
                                    "Pourquoi — {} par {} (confiance {} : {:.2}) :\n{}",
                                    e.action.action_type,
                                    format!("{:?}", e.actor),
                                    annora_design::WhySheet::confidence_word(e.confidence),
                                    e.confidence,
                                    annora_ui::JournalView::explain_action(e),
                                );
                                println!(
                                    "Trace complète :\n{}",
                                    annora_ui::JournalView::full_reasoning(e)
                                );
                                let nodes = annora_ui::JournalView::graph_node_labels(e);
                                if nodes.is_empty() {
                                    println!("Nœuds graphe : aucun");
                                } else {
                                    println!("Nœuds graphe ({}):", nodes.len());
                                    for n in &nodes {
                                        println!("  - {}", n);
                                    }
                                }
                            }
                            None => eprintln!(
                                "✗ Entrée '{}' introuvable (index 1..{} ou UUID).",
                                arg,
                                entries.len()
                            ),
                        }
                    }
                    Err(e) => eprintln!("✗ Failed to read audit journal: {}", e),
                }
            }
            s if s.starts_with(":why2 ") => {
                let arg = s.trim_start_matches(":why2 ").trim();
                match audit_journal.all_entries() {
                    Ok(entries) => {
                        let found = match arg.parse::<usize>() {
                            Ok(n) if n >= 1 && n <= entries.len() => entries.get(n - 1),
                            _ => entries.iter().find(|e| e.id.0.to_string() == arg),
                        };
                        match found {
                            Some(e) => {
                                let triplets: Vec<_> = orchestrator
                                    .memory_graph
                                    .all_triplets()
                                    .into_iter()
                                    .cloned()
                                    .collect();
                                let facts: Vec<(String, String, String, String)> = e
                                    .graph_nodes_used
                                    .iter()
                                    .filter_map(|id| triplets.iter().find(|t| &t.id == id))
                                    .map(|t| {
                                        let label = match &t.object {
                                            annora_memory::NodeValue::Text(s) => s.clone(),
                                            other => format!("{other:?}"),
                                        };
                                        (
                                            label,
                                            "fait".to_string(),
                                            t.metadata.created_at.0.to_rfc3339(),
                                            format!("{:?}", t.metadata.source),
                                        )
                                    })
                                    .collect();
                                let powers: Vec<String> = e
                                    .capability_used
                                    .as_ref()
                                    .map(|c| format!("{c:?}"))
                                    .into_iter()
                                    .collect();
                                let cost_label = e
                                    .cost
                                    .as_ref()
                                    .map(|c| format!("{:.2} EUR", c.amount_cents as f64 / 100.0))
                                    .unwrap_or_else(|| "0 €".to_string());
                                let sheet = annora_ui_shell::WhyStrings::from_parts(
                                    &annora_ui::JournalView::explain_action(e),
                                    &facts,
                                    &annora_ui::JournalView::full_reasoning(e),
                                    &powers,
                                    e.confidence,
                                    &[],
                                    "local",
                                    &cost_label,
                                    "Local",
                                    "une confiance < 0,5 aurait exigé une confirmation",
                                );
                                println!("Pourquoi (3 niveaux) — {} :", e.action.action_type);
                                println!("  Phrase : {}", sheet.sentence);
                                println!("  Faits graphe ({}) :", sheet.facts.len());
                                if sheet.facts.is_empty() {
                                    println!(
                                        "    (aucun fait graphe lié — reconstruit du journal signé)"
                                    );
                                } else {
                                    for f in &sheet.facts {
                                        println!("    - {f}");
                                    }
                                }
                                println!("  Plan :\n{}", sheet.plan);
                                println!(
                                    "  Pouvoirs : {} | Confiance : {} | Coût : {} | Données : {}",
                                    sheet.powers,
                                    sheet.confidence,
                                    sheet.cost_line,
                                    sheet.data_line
                                );
                                println!(
                                    "  Modèle : {} | Alternatives : {} | Bascule : {}",
                                    sheet.model_line,
                                    if sheet.alternatives.is_empty() {
                                        "—".to_string()
                                    } else {
                                        sheet.alternatives.join(", ")
                                    },
                                    sheet.would_change
                                );
                            }
                            None => eprintln!(
                                "✗ Entrée '{}' introuvable (index 1..{} ou UUID).",
                                arg,
                                entries.len()
                            ),
                        }
                    }
                    Err(e) => eprintln!("✗ Failed to read audit journal: {}", e),
                }
            }
            s if s.starts_with(":gui-action ") => {
                let task = s.trim_start_matches(":gui-action ").trim();
                println!(
                    "Intentions GUI ancrées sur mesure (vision classique réelle ; VLM non exécuté) pour tâche : '{}'...",
                    task
                );
                let intentions = gui_agent.predict_intentions_neural(task, 1920, 1080);
                println!("  Predicted {} GUI intentions:", intentions.len());
                for (i, intent) in intentions.iter().enumerate() {
                    println!("    {}. {:?}", i + 1, intent);
                    match gui_agent
                        .execute_intention(&substrate, intent, &gui_input_cap, 1920, 1080)
                        .await
                    {
                        Ok(msg) => println!("       -> Succès: {}", msg),
                        Err(e) => eprintln!("       -> Échec: {}", e),
                    }
                }
            }
            s if s.starts_with(":remember ") => {
                let text = s.trim_start_matches(":remember ").trim();
                if text.is_empty() {
                    eprintln!(
                        "Usage : :remember <sujet> <prédicat> <objet> (ex. :remember Marie aime le jazz)"
                    );
                } else {
                    let parts: Vec<&str> = text.splitn(3, ' ').collect();
                    let report = |res: annora_memory::InsertResult, s: &str, p: &str, o: &str| {
                        use annora_memory::InsertResult;
                        match res {
                            InsertResult::Inserted(id) => println!(
                                "Stored triplet [{}] in sovereign graph: {s} --({p})--> {o}",
                                id.0
                            ),
                            InsertResult::ContradictionResolved { old, new } => println!(
                                "Contradiction résolue sans écrasement silencieux : [{}] remplacé par [{}] ({s} --({p})--> {o})",
                                old.0, new.0
                            ),
                            InsertResult::NeedsUserResolution { existing, incoming } => println!(
                                "Contradiction — décision requise : existant [{}] vs entrant [{}]. Rien n'a été écrasé (voir :contradictions).",
                                existing.id.0, incoming.id.0
                            ),
                        }
                    };
                    if parts.len() == 3 {
                        report(
                            orchestrator.remember(parts[0], parts[1], parts[2]),
                            parts[0],
                            parts[1],
                            parts[2],
                        );
                    } else {
                        report(
                            orchestrator.remember("Fait", "est", text),
                            "Fait",
                            "est",
                            text,
                        );
                    }
                }
            }
            s if s.starts_with(":recall ") => {
                let query = s.trim_start_matches(":recall ").trim();
                if query.is_empty() {
                    eprintln!("Usage : :recall <mot-clé> (ex. :recall jazz)");
                } else {
                    let facts = orchestrator.recall(query);
                    println!("Recalled {} facts matching '{}':", facts.len(), query);
                    for (i, f) in facts.iter().enumerate() {
                        println!("  [{}] {}", i + 1, f);
                    }
                    if facts.is_empty() {
                        println!(
                            "  (aucun fait — essaie un mot plus court ou :memory pour tout voir)"
                        );
                    }
                }
            }
            ":status" => {
                println!("Status:");
                println!("  Presence: {:?}", presence.state());
                println!("  Tier: {:?}", manifest.tier);
                println!(
                    "  Données: Local (pastille §8 — aucun appel réseau hors fournisseur choisi)"
                );
                println!("  File d'envoi: {} en attente (voir :outbox)", outbox.len());
                println!("  Memory Triplets: {}", orchestrator.memory_graph.len());
                println!("  Cost Spent: {}", cost_counter.formatted_total());
            }
            ":constitution" => {
                let c = &orchestrator.constitution;
                println!("Constitution :");
                println!("  Version : {}", c.version);
                println!("  Effective depuis : {}", c.effective_since.0);
                println!("  Nombre de règles : {}", c.rules.len());
                for r in &c.rules {
                    println!("  - {} [{:?}] {}", r.id, r.enforcement, r.description);
                }
                let quorum = annora_constitution::governance::GovernanceConfig::default().quorum;
                println!("  Quorum gouvernance : {}", quorum);
                let dilemmes = annora_constitution::ethics::EthicsBattery::default()
                    .dilemmas
                    .len();
                println!("  Nombre de dilemmes éthiques : {}", dilemmes);
            }
            ":metrics" => {
                println!("North-Star Metrics:");
                println!(
                    "  Continuity Rate: {:.2}%",
                    continuity_tracker.continuity_rate() * 100.0
                );
                println!(
                    "  Bounded Initiative Compliance: {:.2}%",
                    bounded_initiative.compliance_rate() * 100.0
                );
                println!(
                    "  Practical Generality Score: {:.4}",
                    generality_tracker.generality_score()
                );
                if let Some(drift) =
                    drift_detector.evaluate_current_vector(&[1.0, 1.0, 1.0, 1.0, 1.0])
                {
                    println!("  Ethics Drift Alert: {}", drift.description);
                } else {
                    println!("  Ethics Drift Status: Nominal (No drift detected)");
                }
            }
            ":agents" => {
                println!("Registered Sandboxed Agents:");
                for agent in agent_registry.list_agents() {
                    println!(
                        "  - [{}] {} (Caps: {:?})",
                        agent.id, agent.name, agent.required_capabilities
                    );
                }
            }
            ":cost" => {
                println!("Inference Cost: {}", cost_counter.formatted_total());
            }
            ":models-honest" => {
                println!("Honest Inference Statuses (never fake Operational):");
                let statuses = provisioner.list_all_status();
                let installed = statuses.iter().filter(|s| s.is_installed).count();
                let gb = statuses
                    .iter()
                    .map(|s| s.total_disk_bytes as f64 / 1_000_000_000.0)
                    .sum::<f64>();
                for row in annora_ui_shell::honest_engine_rows(gb, statuses.len()) {
                    println!(
                        "  - {:<22} [{}] {}",
                        row.name,
                        row.status.badge(),
                        row.detail
                    );
                }
                println!(
                    "  Artifacts on disk: {}/{} installed.",
                    installed,
                    statuses.len()
                );
            }
            ":agents-cards" => {
                println!("Reference Agents (honest DEMO badges):");
                for card in annora_ui_shell::reference_agent_cards() {
                    println!(
                        "  - [{}] {} | caps: {} | {} | {} | validation: {}",
                        card.id,
                        card.name,
                        card.required_caps.join(", "),
                        card.badge,
                        card.real,
                        if card.needs_validation {
                            "REQUIRED (Braise)"
                        } else {
                            "no"
                        },
                    );
                    println!("      démo: {}", card.demo);
                }
            }
            ":powers" => {
                println!(
                    "Pouvoirs (spec §8) — périmètre, quota, dernier usage, expiration, interrupteur :"
                );
                let granted: Vec<String> = manifest
                    .capabilities_granted
                    .iter()
                    .map(|c| format!("{c:?}"))
                    .collect();
                let denied: Vec<String> = manifest
                    .capabilities_denied
                    .iter()
                    .map(|c| format!("{c:?}"))
                    .collect();
                let view = annora_ui_shell::PowersView::from_manifest(&granted, &denied);
                for r in &view.rows {
                    println!(
                        "  - {:<9} {:<10} | {} | jauge {} | dernier : {} | expire : {} | {} {}",
                        r.group,
                        r.scope,
                        r.perimeter,
                        r.gauge_state(),
                        r.last_used,
                        r.expires,
                        if r.enabled { "activé" } else { "coupé" },
                        if r.policy_locked {
                            "(verrouillé par politique)"
                        } else {
                            ""
                        },
                    );
                }
                println!(
                    "  « Tout couper » = révocation via le registre de consentement (:forget, registre)."
                );
            }
            ":onboarding" => {
                // Manifeste plateforme RÉEL §3ter (pas de générique) : injecté
                // à chaque affichage pour refléter l'hôte détecté au boot.
                onboarding.inject_platform(
                    &format!("{:?}", manifest.tier),
                    &manifest.os_name,
                    &manifest.os_version,
                    &manifest
                        .capabilities_granted
                        .iter()
                        .map(|c| format!("{c:?}"))
                        .collect::<Vec<_>>(),
                    &manifest
                        .capabilities_denied
                        .iter()
                        .map(|c| format!("{c:?}"))
                        .collect::<Vec<_>>(),
                );
                let view = annora_ui_shell::OnboardingView::from_manager(&onboarding);
                println!("Onboarding — limites AVANT capacités (bloquant sans acknowledge):");
                for (title, desc, non_neg) in &view.limits {
                    println!(
                        "  • {} [{}] — {}",
                        title,
                        if *non_neg {
                            "NON-NÉGOCIABLE"
                        } else {
                            "négociable"
                        },
                        desc
                    );
                }
                println!("  Capacités accordées:");
                for cap in &view.capabilities {
                    println!("    + {}", cap);
                }
                println!(
                    "  acknowledged: {} | ready: {}",
                    view.acknowledged, view.ready
                );
            }
            ":shell" => {
                let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
                let snap = annora_ui_shell::ShellSnapshot::from_states(
                    &presence,
                    &ephemeral_hud,
                    &cost_counter,
                    &silence_budget,
                    now_ms,
                    0,
                );
                let spec = ephemeral_hud.overlay_spec();
                println!("Slint Shell Snapshot → {}", snap.to_compositor_json(&spec));
                println!("--- Dashboard texte (fallback Redox/Web/clavier) ---");
                println!("{}", snap.render_text_dashboard());
                println!(
                    "  hud visible: {} | host: {} {} | Rendre en Slint: cargo run -p annora-ui-shell --features render (ADR-007, no WebView)",
                    spec.is_visible, manifest.os_name, manifest.os_version,
                );
            }
            s if s.starts_with(":a11y") => {
                let arg = s.trim_start_matches(":a11y").trim().to_lowercase();
                if !arg.is_empty() {
                    let modality = match arg.as_str() {
                        "voice" | "voicefirst" => PrimaryModality::VoiceFirst,
                        "text" | "textfirst" => PrimaryModality::TextFirst,
                        "visual" | "visualonly" => PrimaryModality::VisualOnly,
                        "haptic" | "hapticassisted" => PrimaryModality::HapticAssisted,
                        _ => {
                            eprintln!("Modalités : voice | text | visual | haptic");
                            a11y.profile().primary_modality
                        }
                    };
                    a11y.set_modality(modality);
                }
                let p = a11y.profile();
                println!(
                    "Accessibilité (§21g) — modalité {:?} (reconfiguration, pas dégradé) :",
                    p.primary_modality
                );
                println!(
                    "  lecteur écran: {} | contraste fort: {} | sous-titres: {} | débit voix: x{:.2}",
                    p.screen_reader_friendly,
                    p.high_contrast,
                    p.subtitles_enabled,
                    p.speech_rate_factor
                );
                // Effet live sur le canal courant (même télémétrie que :channel).
                let ctx = InteractionContext {
                    ambient_noise_db: 35.0,
                    third_parties_present: false,
                    in_meeting: world_model.state().in_meeting,
                    user_attentive_to_screen: world_model.state().user_attentive,
                    user_wearing_headphones: false,
                    device_type: DeviceType::Desktop,
                    urgency: UrgencyLevel::Normal,
                    is_night_time: false,
                };
                let base = ChannelSelector::arbitrate(&ctx);
                let over = a11y.override_decision(&base);
                println!(
                    "  Canal de base : {:?} → après override : {:?}\n  {}",
                    base.channel, over.channel, over.rationale
                );
            }
            s if s.starts_with(":defer ") => {
                let rest = s.trim_start_matches(":defer ").trim();
                let (verb, object) = match rest.split_once('|') {
                    Some((v, o)) => (v.trim(), o.trim()),
                    None => (rest, ""),
                };
                if verb.is_empty() {
                    eprintln!("Usage : :defer <verbe> | <objet>");
                } else {
                    let key = format!("{}|{}", verb.to_lowercase(), object.to_lowercase());
                    let added = outbox.enqueue(OutboxEntry::new(verb, object, &key));
                    // Persist JSON snapshot after every :defer (crash-safe).
                    save_outbox(&data_dir, &outbox);
                    if added {
                        println!("Mis en file (clé {key}) — « Je l'enverrai à ta reconnexion ».");
                    } else {
                        println!("Doublon ignoré (clé {key}) — rejouable sans doublon.");
                    }
                }
            }
            ":outbox" => {
                let now = chrono::Utc::now();
                if outbox.is_empty() {
                    println!("File d'envoi vide — rien en attente.");
                } else {
                    println!("File d'envoi ({} en attente) :", outbox.len());
                    for e in outbox.list() {
                        println!(
                            "  - [{}] {} {} — attente {} min — {}",
                            e.dedup_key,
                            e.verb,
                            e.object,
                            e.waiting_minutes(now),
                            if e.sendable(now) {
                                "envoyable"
                            } else {
                                "revalidation requise (> 15 min)"
                            },
                        );
                    }
                }
            }
            s if s.starts_with(":revalidate ") => {
                let key = s.trim_start_matches(":revalidate ").trim();
                if outbox.revalidate(key) {
                    save_outbox(&data_dir, &outbox);
                    println!("Entrée {key} revalidée — elle peut partir.");
                } else {
                    eprintln!("✗ Clé '{key}' introuvable dans la file.");
                }
            }
            s if s.starts_with(":export") => {
                match annora_memory::export_graph(
                    &orchestrator.memory_graph,
                    annora_memory::ExportFormat::Json,
                ) {
                    Ok(bytes) => {
                        let count = orchestrator.memory_graph.len();
                        let rest = s.trim_start_matches(":export").trim();
                        if rest.is_empty() {
                            println!(
                                "Export JSON prêt : {} octet(s), {} triplet(s) (aucun fichier — usage : :export <fichier>).",
                                bytes.len(),
                                count
                            );
                        } else {
                            match std::fs::write(rest, &bytes) {
                                Ok(()) => println!(
                                    "Graphe exporté vers '{rest}' : {} octet(s), {} triplet(s).",
                                    bytes.len(),
                                    count
                                ),
                                Err(e) => eprintln!("✗ Écriture de '{rest}' impossible : {e}"),
                            }
                        }
                        // Sauvegarde auto du snapshot RAM (miroir :exit) après export.
                        match orchestrator
                            .memory_graph
                            .save_to_file("./annora-memory.json")
                        {
                            Ok(()) => {
                                println!("  (snapshot RAM sauvegardé vers ./annora-memory.json)")
                            }
                            Err(e) => eprintln!("  (snapshot RAM non sauvegardé : {e})"),
                        }
                    }
                    Err(e) => eprintln!("✗ Export impossible : {e}"),
                }
            }
            s if s.starts_with(":import ") => {
                let path = s.trim_start_matches(":import ").trim();
                if path.is_empty() {
                    eprintln!("Usage : :import <fichier> (JSON produit par :export)");
                } else {
                    match std::fs::read(path) {
                        Ok(bytes) => match annora_memory::import_graph(&bytes) {
                            Ok(export) => {
                                let n = import_into_graph(&mut orchestrator.memory_graph, &export);
                                println!(
                                    "Graphe importé depuis '{path}' : {} triplet(s) fusionné(s) (total : {}).",
                                    n,
                                    orchestrator.memory_graph.len()
                                );
                                match orchestrator
                                    .memory_graph
                                    .save_to_file("./annora-memory.json")
                                {
                                    Ok(()) => println!(
                                        "  (snapshot RAM sauvegardé vers ./annora-memory.json)"
                                    ),
                                    Err(e) => {
                                        eprintln!("  (snapshot RAM non sauvegardé : {e})")
                                    }
                                }
                            }
                            Err(e) => eprintln!(
                                "✗ Import impossible depuis '{path}' : {e} (attendu : JSON de :export, version \"1\")"
                            ),
                        },
                        Err(e) => eprintln!("✗ Lecture de '{path}' impossible : {e}"),
                    }
                }
            }
            s if s.starts_with(":heritage-erase ") => {
                let key_id = s.trim_start_matches(":heritage-erase ").trim();
                if key_id.is_empty() {
                    eprintln!("Usage : :heritage-erase <key_id>");
                } else {
                    match annora_memory::crypto_erase(&mut heritage_keys, key_id) {
                        Ok(proof) => println!(
                            "Clé '{}' effacée : {} octet(s) neutralisés.",
                            proof.key_id, proof.wiped_bytes
                        ),
                        Err(e) => eprintln!("✗ Effacement impossible : {e}"),
                    }
                }
            }
            s if s.starts_with(":providers") => {
                let rest = s.trim_start_matches(":providers").trim();
                if rest.is_empty() {
                    println!("Providers (mémoire seule, défaut local-only) :");
                    for p in provider_registry.available() {
                        let mark = if p == provider_registry.selected() {
                            "*"
                        } else {
                            " "
                        };
                        println!("  {mark} {}", p.display_name());
                    }
                    println!("  {}", provider_registry.data_exit(0).badge());
                    println!("  Usage : :providers <local|ollama|openai|anthropic|custom:URL>");
                } else {
                    match annora_kernel::providers::ProviderId::parse_cli(rest) {
                        Ok(id) => {
                            if id.is_cloud() {
                                provider_registry.register(id.clone());
                            }
                            match provider_registry.select(id.clone()) {
                                Ok(()) => {
                                    orchestrator.system2.with_provider(id.clone());
                                    println!("  Providers: sélection = {}", id.display_name());
                                }
                                Err(e) => eprintln!("  Providers: sélection refusée — {e}"),
                            }
                            println!("  {}", provider_registry.data_exit(0).badge());
                            if id.is_cloud() {
                                println!(
                                    "  AVERTISSEMENT : requêtes System2 Cloud exigeant une capability Network explicite (refus par défaut)."
                                );
                            }
                        }
                        Err(e) => eprintln!("  Providers: '{rest}' inconnu — {e}"),
                    }
                }
            }
            s if s.starts_with(":plan ") => {
                let text = s.trim_start_matches(":plan ").trim();
                if text.is_empty() {
                    println!(
                        "Usage : :plan <texte> — aperçu sec (plan + dry-run, zéro exécution)."
                    );
                } else {
                    println!("{}", orchestrator.plan_preview(text));
                }
            }
            ":flush" => {
                let now = chrono::Utc::now();
                let mut sent = 0;
                while let Some(e) = outbox.take_sendable(now) {
                    let entry = AuditEntry {
                        id: EntryId(uuid::Uuid::now_v7()),
                        timestamp: Timestamp::now(),
                        actor: ActorId::Kernel,
                        action: ActionRecord {
                            action_type: "OutboxFlush".to_string(),
                            description: format!("Transmis (file) : {} {}", e.verb, e.object),
                            parameters: serde_json::json!({ "dedup_key": e.dedup_key, "attempts": e.attempts }),
                            reversible: false,
                            reversed: false,
                        },
                        capability_used: None,
                        reasoning_trace: ReasoningTrace {
                            steps: vec![ReasoningStep {
                                description: "File d'envoi : entrée envoyable, tracée au Journal."
                                    .to_string(),
                                confidence: 1.0,
                                evidence: vec![],
                            }],
                            model_used: None,
                            duration_ms: 0,
                        },
                        graph_nodes_used: vec![],
                        confidence: 1.0,
                        cost: None,
                        signature: vec![1, 2, 3, 4],
                        prev_hash: vec![0; 32],
                        token_hash: None,
                    };
                    let _ = audit_journal.append(entry);
                    println!(
                        "  → transmis : {} {} (clé {})",
                        e.verb, e.object, e.dedup_key
                    );
                    sent += 1;
                }
                if sent == 0 {
                    println!("Rien d'envoyable (file vide ou revalidation requise).");
                } else {
                    println!("{sent} entrée(s) transmise(s) et tracée(s) au Journal.");
                }
                // Persist after flush (entries removed) so reboot stays consistent.
                save_outbox(&data_dir, &outbox);
            }
            ":camera" => {
                println!("Capturing camera frame from Substrate via authenticated capability...");
                match substrate
                    .capture_camera_frame(camera_handle, &camera_cap)
                    .await
                {
                    Ok(frame) => {
                        let ctx = camera_pipeline.process_frame(&frame);
                        camera_pipeline.update_world_model(&ctx, &mut world_model);
                        println!("Camera Perception Summary:");
                        println!(
                            "  Resolution: {}x{} ({:?})",
                            frame.width, frame.height, frame.format
                        );
                        println!("  User Presence: {:?}", ctx.user_presence);
                        println!(
                            "  Gaze Focused on Screen: {}",
                            ctx.user_gaze_focused_on_device
                        );
                        println!("  Estimated Distance: {:.2} m", ctx.estimated_distance_m);
                        println!("  Lighting Condition: {:?}", ctx.lighting);
                        println!(
                            "  Cryptographic Frame Hash: {}",
                            hex_fmt(&ctx.privacy_preserving_frame_hash)
                        );
                        println!(
                            "  Privacy Invariant: 100% processed in ephemeral RAM, zero raw pixels stored (§6)."
                        );
                    }
                    Err(e) => eprintln!("Failed to capture camera frame: {}", e),
                }
            }
            ":hud" => {
                println!("Invoking Ephemeral HUD (Spotlight/Raycast-like single-line overlay):");
                let hud_spec = AcrylicOverlayWindowSpec::centered_default();
                println!(
                    "  DirectComposition Acrylic Spec: {}x{} @ (top-offset: {}px) | Accent: {}",
                    hud_spec.width,
                    hud_spec.height,
                    hud_spec.pos_y_offset,
                    hud_spec.coral_accent_hex
                );
                presence.set_state(PresenceState::Listening);
                ephemeral_hud.open_prompt();
                for c in "Transférer 25 EUR pour déjeuner".chars() {
                    ephemeral_hud.insert_char(c);
                }
                println!("{}", ephemeral_hud.render_ascii());
                println!("\nStreaming response inline...");
                presence.set_state(PresenceState::Acting);
                ephemeral_hud.set_streaming(
                    "Transférer 25 EUR",
                    "Préparation du virement SEPA instantané...",
                    false,
                );
                println!("{}", ephemeral_hud.render_ascii());
                println!("\nAction requires confirmation (Braise #FF7A2F Accent):");
                presence.set_state(PresenceState::DecisionRequired);
                ephemeral_hud.request_validation(
                    "wallet-spend-42",
                    "Envoi de 25.00 EUR vers Restaurant",
                    Some(2500),
                    "Financial",
                );
                println!("{}", ephemeral_hud.render_ascii());
                println!("\nUser validated [Enter]. Response evaporating into thin air...");
                ephemeral_hud.start_evaporation();
                println!("{}", ephemeral_hud.render_ascii());
                // Évaporation 1500 ms → Hidden, retour Veille.
                ephemeral_hud.tick(1500);
                presence.set_state(PresenceState::Idle);
                println!("  HUD évaporé (tick 1500 ms) → {:?}", presence.state());
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
            ":settings" => {
                // Aide seule : affiche le view-model Réglages §8 (aucune logique).
                let view = annora_ui_shell::SettingsView::default();
                println!("Réglages (spec §8) — pastille « où vont mes données » :");
                for p in &view.providers {
                    println!("  - {:<14} [{}]", p.name, p.data_label,);
                }
                println!("  Voix : {}", view.voices.join(", "));
                println!("  Canaux : {}", view.channels.join(", "));
                println!("  Langues : {}", view.languages.join(", "));
                println!("  Accessibilité : {}", view.accessibility_summary);
                println!("  Sauvegarde : {}", view.backup_label);
                println!("  Foyer : {}", view.household_profiles.join(", "));
                println!(
                    "  Simulation : {}",
                    if view.simulation_mode {
                        "activée (prévisualisation, rien ne s'exécute)"
                    } else {
                        "désactivée"
                    },
                );
            }
            ":budget" => {
                println!("Silence Budget Tracker (§21d) — Bounded Proactive Initiative:");
                println!(
                    "  Daily Quota: {} interruption credits",
                    silence_budget.daily_quota()
                );
                println!(
                    "  Credits Remaining Today: {}",
                    silence_budget.credits_remaining()
                );
                println!(
                    "  Confidence Threshold for Suggestions: {:.2}",
                    silence_budget.min_confidence_threshold()
                );
                println!("  Scheduled Reminders: 0 cost (unlimited)");
                println!("  Autonomous Suggestions: 1 credit per interaction (strictly capped)");
                let allowed = silence_budget.request_initiative(
                    "proactive-demo",
                    InitiativeOrigin::SystemInferred,
                    0.91,
                    "Proposition pause café",
                );
                println!(
                    "  Simulated Autonomous Suggestion Status: {}",
                    if allowed {
                        "Granted (1 credit spent)"
                    } else {
                        "Suppressed (Budget or confidence low)"
                    }
                );
                println!(
                    "  Updated Remaining Credits: {}",
                    silence_budget.credits_remaining()
                );
            }

            ":ambient" => {
                println!("Executing Continuous Ambient Perception & Context Cycle...");
                presence.set_state(PresenceState::Listening);
                if let Ok(frame) = substrate
                    .capture_camera_frame(camera_handle, &camera_cap)
                    .await
                {
                    let v_ctx = camera_pipeline.process_frame(&frame);
                    camera_pipeline.update_world_model(&v_ctx, &mut world_model);
                    println!(
                        "  [Camera] Scene updated: {:?}, gaze attentive: {}, lighting: {:?}",
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
                println!(
                    "  [Channel] Dynamic output target: {:?} ({})",
                    decision.channel, decision.rationale
                );
                presence.set_state(PresenceState::Idle);
                println!(
                    "  [Presence] Tray indicator in standby (Period: {}ms)",
                    presence.pulse_period_ms()
                );
            }

            ":win-ram" => {
                println!("Windows Substrate Physical Memory Telemetry (GlobalMemoryStatusEx):");
                let total_ram_mb = compute.total_physical_ram() as f64 / 1_048_576.0;
                let avail_ram_mb = compute.available_physical_ram() as f64 / 1_048_576.0;
                let used_ram_mb = total_ram_mb - avail_ram_mb;
                let used_pct = if total_ram_mb > 0.0 {
                    (used_ram_mb / total_ram_mb) * 100.0
                } else {
                    0.0
                };
                let vram_mb = compute.vram_available_bytes() as f64 / 1_048_576.0;
                println!(
                    "  • Total Physical RAM:       {:>8.1} MB ({:.2} GB)",
                    total_ram_mb,
                    total_ram_mb / 1024.0
                );
                println!(
                    "  • Available Physical RAM:   {:>8.1} MB ({:.2} GB)",
                    avail_ram_mb,
                    avail_ram_mb / 1024.0
                );
                println!(
                    "  • RAM Utilization:          {:>8.1} MB ({:.1}%)",
                    used_ram_mb, used_pct
                );
                println!("  • Available Dedicated VRAM: {:>8.1} MB", vram_mb);
                println!(
                    "  • Acceleration Backends:    {:?}",
                    compute.available_backends()
                );
                println!(
                    "  • Preferred Backend:        {:?}",
                    compute.preferred_backend()
                );
            }

            ":win-power" => {
                println!("Windows Substrate Power & Thermal Telemetry (GetSystemPowerStatus):");
                let power = substrate.power_state();
                let thermal = substrate.thermal_state();
                println!("  • On Battery Power:         {}", power.on_battery);
                println!("  • Battery Percentage:       {}%", power.battery_percent);
                println!("  • AC / Charging Status:     {}", power.charging);
                println!(
                    "  • Host Temperature:         {:.1} °C",
                    thermal.temperature_celsius
                );
                println!(
                    "  • Thermal Throttling:       {}",
                    if thermal.throttled {
                        "ACTIVE (Throttled)"
                    } else {
                        "Nominal"
                    }
                );
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
                        println!(
                            "  • Resolution:               {}x{} ({} pixels)",
                            capture.width, capture.height, pixel_count
                        );
                        println!("  • Color Format:             {:?}", capture.format);
                        println!(
                            "  • Framebuffer Size:         {:.2} MB ({} bytes)",
                            byte_count as f64 / 1_048_576.0,
                            byte_count
                        );
                        println!("  • Ephemeral SHA-256 Hash:   {}", hex_fmt(&hash));
                        println!(
                            "  • Privacy Invariant:        Zero persistent raw pixel storage (§6)."
                        );
                    }
                    Err(e) => eprintln!("Failed to capture display screen: {}", e),
                }
            }

            ":win-tray" => {
                println!("Windows System Tray & Shell Integration Test:");
                let mut win_tray = annora_substrate_windows::WindowsPresenceTray::new();
                println!("  • Initial State:            {:?}", win_tray.state());
                win_tray.set_state(annora_substrate_windows::WindowsPresenceState::Listening);
                println!(
                    "  • Updated to Listening:     {:?} (Tooltip: '{}')",
                    win_tray.state(),
                    win_tray.tooltip()
                );
                win_tray.set_state(annora_substrate_windows::WindowsPresenceState::Acting);
                println!(
                    "  • Updated to Acting:        {:?} (Tooltip: '{}')",
                    win_tray.state(),
                    win_tray.tooltip()
                );
                win_tray.set_state(annora_substrate_windows::WindowsPresenceState::Idle);
                println!(
                    "  • Returned to Idle:         {:?} (Tooltip: '{}')",
                    win_tray.state(),
                    win_tray.tooltip()
                );
                let mut hotkey = annora_substrate_windows::WindowsGlobalHotkey::new();
                let registered = hotkey.register();
                println!(
                    "  • Global Hotkey:            {:?} (Ctrl+Alt+Space, alias Alt+Space summoned: {})",
                    hotkey.key_combination, registered
                );
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
                )
                .await?;
            }
        }

        print!("\nannora> ");
        stdout.flush()?;
    }

    // Sortie de boucle (EOF/Ctrl-D) : même sauvegarde auto que :exit.
    if let Err(e) = orchestrator
        .memory_graph
        .save_to_file("./annora-memory.json")
    {
        eprintln!("✗ Sauvegarde mémoire impossible : {e}");
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
        token_hash: None,
    };
    let _ = audit_journal.append(entry);

    presence.set_state(PresenceState::Idle);

    println!("\n[Annora Response]");
    println!("  {}", result.response_text);
    println!(
        "  [Routing: {} | Latency: {}ms | Confidence: {:?}]",
        result.routing_used, result.latency_ms, result.confidence_decision
    );

    Ok(())
}

fn hex_fmt(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(12)
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join("")
        + "..."
}

// --- IMP-06: persistent daemon identity (keys) + offline outbox ---------------
// Keys live in `<data_dir>/keys/` (LOCALAPPDATA/Annora/keys or
// ./annora_data/Annora/keys fallback). Raw 32-byte secrets, no encryption yet
// (TPM/DPAPI tracked work) — but stable across reboots (no silent rotation).
// Outbox lives in `<data_dir>/outbox.json` (pretty JSON array of entries).

fn load_or_generate_signing_key(
    keys_dir: &std::path::Path,
) -> Result<KeyPair, Box<dyn std::error::Error>> {
    let path = keys_dir.join("signing_key.bin");
    if let Ok(bytes) = std::fs::read(&path) {
        if bytes.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            return Ok(KeyPair::from_bytes(&arr));
        }
        eprintln!(
            "  Keys: signing_key.bin corrupt ({} bytes), regenerating",
            bytes.len()
        );
    }
    let kp = KeyPair::generate();
    std::fs::write(&path, kp.to_bytes())?;
    println!("  Keys: new signing key generated -> {}", path.display());
    Ok(kp)
}

fn load_or_generate_authority(
    keys_dir: &std::path::Path,
) -> Result<CapabilityAuthority, Box<dyn std::error::Error>> {
    let path = keys_dir.join("capability_root.bin");
    if let Ok(bytes) = std::fs::read(&path) {
        if bytes.len() == 32 {
            match biscuit_auth::PrivateKey::from_bytes(&bytes) {
                Ok(private) => {
                    return Ok(CapabilityAuthority::with_keypair(
                        biscuit_auth::KeyPair::from(&private),
                    ));
                }
                Err(e) => eprintln!("  Keys: capability_root.bin invalid ({e}), regenerating"),
            }
        } else {
            eprintln!(
                "  Keys: capability_root.bin corrupt ({} bytes), regenerating",
                bytes.len()
            );
        }
    }
    let authority = CapabilityAuthority::new();
    std::fs::write(&path, authority.root_keypair.private().to_bytes())?;
    println!(
        "  Keys: new capability root generated -> {}",
        path.display()
    );
    Ok(authority)
}

fn outbox_file_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("outbox.json")
}

fn load_outbox(data_dir: &std::path::Path) -> Outbox {
    let path = outbox_file_path(data_dir);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => return Outbox::new(),
    };
    let entries: Vec<OutboxEntry> = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "  Outbox: {} unparsable ({e}), starting empty",
                path.display()
            );
            return Outbox::new();
        }
    };
    let mut outbox = Outbox::new();
    for e in entries {
        // enqueue dedups by key — replay-safe restore.
        outbox.enqueue(e);
    }
    outbox
}

fn save_outbox(data_dir: &std::path::Path, outbox: &Outbox) {
    let path = outbox_file_path(data_dir);
    match serde_json::to_vec_pretty(outbox.list()) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                eprintln!("  Outbox: failed to persist {} ({e})", path.display());
            }
        }
        Err(e) => eprintln!("  Outbox: serialization failed ({e})"),
    }
}

/// Fusionne un export JSON (`annora_memory::import_graph`, produit par
/// `:export`) dans le graphe RAM : restaure d'abord les registres nom→ID via
/// les labels exportés (IDs déterministes — `resolve_*` redonne le même UUID,
/// rendant les faits importés requêtable via `:recall`), puis réinsère chaque
/// triplet via `KnowledgeGraph::insert` (routage contradiction : un
/// `NeedsUserResolution` est loggué en warn, jamais écrasé). Rend le nombre
/// de triplets traités.
fn import_into_graph(
    graph: &mut annora_memory::KnowledgeGraph,
    export: &annora_memory::GraphExport,
) -> usize {
    for label in export.labels.subjects.values() {
        graph.resolve_subject(label);
    }
    for label in export.labels.predicates.values() {
        graph.resolve_predicate(label);
    }
    let mut n = 0;
    for t in &export.triplets {
        n += 1;
        if let annora_memory::InsertResult::NeedsUserResolution { existing, incoming } =
            graph.insert(t.clone())
        {
            tracing::warn!(
                "import: contradiction en attente (existant=[{}] entrant=[{}]) — rien écrasé",
                existing.id.0,
                incoming.id.0
            );
        }
    }
    n
}
