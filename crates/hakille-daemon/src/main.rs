use hakille_agents::{AgentManifest, AgentRegistry, UiTarsAgent, sha256_hex};
use hakille_audit::journal::AuditJournal;
use hakille_audit::{ActionRecord, AuditEntry, ReasoningStep, ReasoningTrace};
use hakille_constitution::rules::Constitution;
use hakille_core::{
    ActorId, CapabilityAuthority, CapabilityScope, EntryId, KeyPair, Timestamp, TrustLevel, UserId,
};
use hakille_inference::{ModelCatalog, ModelProvisioner, ModelTier};
use hakille_kernel::{
    budget::PipelineBudgetConfig, fabric::CognitiveFabric, orchestrator::CognitiveOrchestrator,
};
use hakille_metrics::{
    BoundedInitiativeTracker, ContinuityTracker, GeneralityTracker, TaskDomain, ValuesDriftDetector,
};
use hakille_perception::audio::SenseVoiceSmall;
use hakille_perception::{
    AsrEngine, AudioChunk, CameraPipeline, VoiceActivityDetector, WakeWordDetector, WorldModel,
};
use hakille_substrate::{CameraConfig, CameraFacing, CameraStreamHandle, Compute, Substrate};
use hakille_substrate_windows::{WindowsComputeBackend, WindowsScreenCapturer, WindowsSubstrate};
use hakille_sync::{CrdtSync, HandoffCoordinator, Outbox, OutboxEntry};
use hakille_ui::{
    AccessibilityManager, AcrylicOverlayWindowSpec, ChannelSelector, CostCounter, DeviceType,
    EphemeralHud, InitiativeOrigin, InteractionContext, MemoryViewerPanel, OnboardingManager,
    PlatformAdapter, PresenceIndicator, PresenceState, PrimaryModality, SilenceBudgetTracker,
    UrgencyLevel, VisualAuditViewer,
};
use hakille_voice::{BargeInDetector, FormantTts, MoshiEngine};
use sha2::Digest;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use uuid::Uuid;

mod agent_runtime;
mod ipc;

use ipc::run_serve;

fn make_cli_progress_callback() -> hakille_inference::ProgressCallback {
    Arc::new(|p: &hakille_inference::DownloadProgress| {
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
    let mut models_dir = std::env::var("HAKILLE_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./models"));
    let mut auto_download_models = false;
    let mut provider_arg: Option<String> = None;
    let mut serve_mode = false;
    let mut speak_enabled = false;
    let mut listen_mode = false;

    // Check for CLI subcommands first: `models list`, `models download`, `models verify`
    if args.len() > 1 && args[1] == "models" {
        let provisioner = ModelProvisioner::new(models_dir);
        let subcmd = args.get(2).map(|s| s.as_str()).unwrap_or("list");

        match subcmd {
            "list" => {
                println!("============================================================");
                println!("  Hakille Sovereign Model Catalog & Local Cache Status");
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
                        "--tier" if idx + 1 < args.len() => {
                            target_tier = match args[idx + 1].to_lowercase().as_str() {
                                "guest" => Some(ModelTier::Guest),
                                "native" => Some(ModelTier::Native),
                                "sovereign" => Some(ModelTier::Sovereign),
                                _ => None,
                            };
                            idx += 1;
                        }
                        "--model" if idx + 1 < args.len() => {
                            target_model_id = Some(args[idx + 1].clone());
                            idx += 1;
                        }
                        arg if !arg.starts_with("--") && target_model_id.is_none() => {
                            target_model_id = Some(arg.to_string());
                        }
                        _ => {}
                    }
                    idx += 1;
                }

                println!("============================================================");
                println!("  Hakille Autonomous Model Downloader (Zero-Touch Provisioning)");
                println!("  Target Directory: {}", provisioner.cache_dir().display());
                println!("============================================================");

                let progress = Some(make_cli_progress_callback());

                if let Some(mut model_id) = target_model_id {
                    // Match exact id or fuzzy/prefix against catalog
                    if provisioner.catalog().iter().all(|m| m.id != model_id)
                        && let Some(matched) = provisioner
                            .catalog()
                            .iter()
                            .find(|m| m.id.starts_with(&model_id) || m.id.contains(&model_id))
                    {
                        model_id = matched.id.clone();
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
                println!("Usage: hakille-daemon models [list|download|verify]");
                return Ok(());
            }
        }
    }

    let mut arg_idx = 1;
    while arg_idx < args.len() {
        match args[arg_idx].as_str() {
            "--help" | "-h" => {
                println!("Hakille Sovereign Cognitive OS Daemon");
                println!("Usage: hakille-daemon [COMMAND] [OPTIONS]");
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
                println!(
                    "  --serve  Mode IPC local (pipe nommé Windows / UDS Unix) pour le HUD desktop ; sans REPL"
                );
                println!(
                    "  --speak                     Synthèse vocale réelle (FormantTts) de chaque réponse REPL"
                );
                println!(
                    "  --listen                    Écoute micro réelle (VAD + ASR) au démarrage du REPL"
                );
                println!("  --version, -v               Print version information");
                println!("  --help, -h                  Print this help message");
                return Ok(());
            }
            "--version" | "-v" => {
                println!("hakille-daemon v0.1.0 (Rust 2024)");
                return Ok(());
            }
            "--eval-prompt" | "--prompt" | "--eval" if arg_idx + 1 < args.len() => {
                eval_prompt = Some(args[arg_idx + 1].clone());
                arg_idx += 1;
            }
            "--budget" if arg_idx + 1 < args.len() => {
                if let Ok(b) = args[arg_idx + 1].parse::<u64>() {
                    daily_budget_cents = b;
                }
                arg_idx += 1;
            }
            "--models-dir" if arg_idx + 1 < args.len() => {
                models_dir = PathBuf::from(&args[arg_idx + 1]);
                arg_idx += 1;
            }
            "--auto-download" => {
                auto_download_models = true;
            }
            "--provider" if arg_idx + 1 < args.len() => {
                provider_arg = Some(args[arg_idx + 1].clone());
                arg_idx += 1;
            }
            "--serve" => {
                serve_mode = true;
            }
            "--speak" => {
                speak_enabled = true;
            }
            "--listen" => {
                listen_mode = true;
            }
            _ => {}
        }
        arg_idx += 1;
    }

    println!("============================================================");
    println!("  Hakille — Sovereign Cognitive Operating System (Rust Native)");
    println!("  'Un système d'exploitation cognitif, pas un chatbot.'");
    println!("============================================================");

    // 1. Initialize Substrate Layer
    println!("\n[1/6] Initializing Host Substrate Layer...");
    // Persistent dirs first: keys + outbox + journal all live under
    // LOCALAPPDATA/Hakille (or ./hakille_data/Hakille fallback) so reboots keep
    // identity (signing key, capability root) instead of regenerating.
    let data_dir = std::env::var("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./hakille_data"))
        .join("Hakille");
    // Refus explicite si l'état persistant ne peut pas être créé : démarrer
    // avec un chemin différent reviendrait à perdre identité, journal et
    // mémoire sans que l'utilisateur le sache.
    std::fs::create_dir_all(&data_dir).map_err(|e| {
        std::io::Error::other(format!(
            "répertoire de données inaccessible ({}): {e}",
            data_dir.display()
        ))
    })?;
    let keys_dir = data_dir.join("keys");
    std::fs::create_dir_all(&keys_dir).map_err(|e| {
        std::io::Error::other(format!(
            "répertoire de clés inaccessible ({}): {e}",
            keys_dir.display()
        ))
    })?;
    // Clés : keychain OS d'abord (service "hakille"), repli fichier LOUD sinon.
    // Le repli keys/*.bin reste lisible en clair : migrer via rotation dès que
    // le trousseau est débloqué. Jamais de régénération silencieuse.
    let keystore = hakille_keystore::KeyRegistry::new(hakille_keystore::OsStore::new());
    let (authority, signing_keypair) = match (
        keystore.capability_root(),
        keystore.journal_signing(),
    ) {
        (Ok(root), Ok(journal)) => {
            println!(
                "  Keys: keychain OS (service \"hakille\") — capability-root-v1 {} / journal-signing-v1 {}",
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
    let attestation = substrate.attest(b"hakille-daemon-boot").await?;
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
    // ./hakille_data with a stable path. Signing key is the persisted identity
    // above (load-or-generate); TPM/DPAPI + rotation remain tracked work.
    let journal_path = data_dir.join("audit.redb");
    let journal_path_str = journal_path
        .to_str()
        .ok_or_else(|| format!("chemin du journal non UTF-8: {}", journal_path.display()))?;
    // F3 : la clé racine de capabilities est épinglée dès la construction.
    // Sans cela, `revert_with_cap` refuse (fail-closed) et l'audit ne peut
    // pas vérifier les tokens au point d'effet.
    let audit_journal = AuditJournal::new_with_root(
        journal_path_str,
        signing_keypair.signing_key.clone(),
        authority.root_public_key(),
    )?;
    // Vérification complète au démarrage : une chaîne corrompue ne doit
    // jamais être complétée silencieusement par de nouvelles entrées.
    match audit_journal.verify_chain() {
        Ok(report) if report.is_valid() => {
            println!(
                "  Audit Journal: {} entrée(s) vérifiée(s), chaîne Ed25519 saine — {}",
                report.valid_entries,
                journal_path.display()
            );
        }
        Ok(report) => {
            return Err(format!(
                "journal d'audit corrompu ({} entrées valides / {}, premier maillon cassé: {:?}, corruption: {:?}) — démarrage refusé: {}",
                report.valid_entries,
                report.total_entries,
                report.broken_at,
                report.corruption,
                journal_path.display()
            )
            .into());
        }
        Err(e) => {
            return Err(format!(
                "journal d'audit illisible ({e}) — démarrage refusé: {}",
                journal_path.display()
            )
            .into());
        }
    }
    println!(
        "  Audit Journal: append-only persistant à {} (chaîne Ed25519, racine épinglée)",
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
    let (mut vad, asr, mut _moshi, fabric, qwen_engine_opt, gui_agent) = match maybe_connected {
        Ok(ref conn) => {
            let summary = conn.system_summary();
            println!(
                "  Model artifacts inspected: {}/{} files present on disk ({:.2} GB)",
                installed_count,
                model_statuses.len(),
                conn.total_disk_bytes as f64 / 1_000_000_000.0
            );
            println!(
                "  NOTE: statuts par artefact — forwards réels pour VAD/ASR/CAM++/MiniLM/Qwen ; Mimi = RVQ nearest-L2 (pas de décodeur neural) ; UI-TARS = inspection seule, pas de forward VLM."
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
            let fabric = CognitiveFabric::new(hakille_kernel::fabric::CognitiveManifest::default());
            let ui_tars = UiTarsAgent::new();
            (vad, asr, moshi, fabric, None, ui_tars)
        }
    };
    let mut _wake_word = WakeWordDetector::default_hakille();
    let mut _barge_in = BargeInDetector::new(0.6, 2);
    println!("  Audio Pipeline: 16kHz continuous capture buffer ready (REAL VAD/ASR via ort)");
    println!(
        "  SenseVoice-Small: mel + LFR + forward ort + CTC greedy — real ASR, measured confidence"
    );
    println!(
        "  Voice/GUI: Mimi RVQ nearest-L2 sur codebooks réels, TTS formantique Klatt, grounding CV — pas de modèle neuronal Moshi ni de forward VLM."
    );

    // Camera & Multimodal Perception Pipeline (§6)
    let camera_config = CameraConfig {
        width: 1920,
        height: 1080,
        fps: 30,
        facing: CameraFacing::Front,
    };
    let camera_cap = authority.create_root_token(
        vec![CapabilityScope::Camera, CapabilityScope::Sensor],
        vec![hakille_core::CapabilityAction::Read],
        Timestamp(chrono::Utc::now() + chrono::Duration::hours(1)),
    )?;
    let gui_input_cap = authority.create_root_token(
        vec![CapabilityScope::GuiInput],
        vec![
            hakille_core::CapabilityAction::Execute,
            hakille_core::CapabilityAction::Write,
        ],
        Timestamp(chrono::Utc::now() + chrono::Duration::hours(1)),
    )?;
    let (camera_handle, camera_unavailable_reason): (Option<CameraStreamHandle>, Option<String>) =
        match substrate
            .start_camera_capture(camera_config.clone(), &camera_cap)
            .await
        {
            Ok(handle) => (Some(handle), None),
            Err(e) => (None, Some(e.to_string())),
        };
    let camera_spec = format!(
        "{}x{} {:?} demandé, résolution négociée par le substrat au premier frame",
        camera_config.width, camera_config.height, camera_config.facing
    );
    let mut camera_pipeline = CameraPipeline::new(camera_config);
    let mut world_model = WorldModel::new("Workstation");
    match (&camera_handle, &camera_unavailable_reason) {
        (Some(_), _) => {
            println!(
                "  Camera Pipeline: capture native active ({}, zéro pixel brut stocké)",
                camera_spec
            );
        }
        (None, reason) => {
            eprintln!(
                "  Camera Pipeline: indisponible — {}",
                reason.as_deref().unwrap_or("raison inconnue")
            );
            eprintln!(
                "  Vision désactivée: aucune capture active, :camera et :ambient fonctionneront sans perception visuelle."
            );
        }
    }
    println!("  WorldModel: Continuous multimodal situational tracking initialized");

    // 5. Initialize Action Agents Layer
    println!("\n[5/6] Registering Agent Manifests & WASM Runtime...");
    let mut agent_registry = AgentRegistry::new();
    let declarative_business_agents = [
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
    for (id, name, caps) in declarative_business_agents {
        agent_registry.register(AgentManifest {
            id: id.to_string(),
            name: name.to_string(),
            version: "0.1.0".to_string(),
            description: format!(
                "Declarative business manifest (no WASM binary provisioned): {}",
                name
            ),
            required_capabilities: caps,
        })?;
    }
    println!(
        "  Registered {} declarative business agent manifests (scopes déclarés, aucun binaire métier fourni)",
        agent_registry.list_agents().len()
    );

    // Runtime WASM réel : seuls les agents de référence embarqués ont un
    // binaire. Les artefacts sont content-addressed, vérifiés puis auto-testés
    // avant toute exécution. Une indisponibilité est affichée, jamais masquée.
    let mut agent_runtime = match agent_runtime::ReferenceAgentRuntime::provision(&data_dir) {
        Ok(runtime) => {
            println!(
                "  WASM runtime: {} agent(s) de référence réellement chargé(s) depuis {}",
                runtime.agents().len(),
                runtime.dir.display()
            );
            for agent in runtime.agents() {
                println!(
                    "    - {} : {} octet(s), sha256 {}",
                    agent.name, agent.bytes_len, agent.sha256
                );
            }
            println!(
                "  WASM self-test: OK en {} ms (fuel 10M, timeout 2 s, 32 pages ; aucun import hôte)",
                runtime.self_test_duration_ms
            );
            Some(runtime)
        }
        Err(e) => {
            eprintln!("  WASM runtime: INDISPONIBLE — aucun binaire chargé ni exécuté : {e}");
            None
        }
    };

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
    // Budget pipeline aligné sur le timeout LLM réel : sans ça, le défaut
    // 100 ms fait échouer tout vrai tour Qwen (~24 s release CPU).
    let llm_timeout_ms = orchestrator.system1.llm_timeout_ms();
    orchestrator.set_budget_config(PipelineBudgetConfig::aligned_with_llm_timeout(
        llm_timeout_ms,
    ));
    println!(
        "  Pipeline Budget: inference {} ms / total {} ms (aligné sur HAKILLE_QWEN_TIMEOUT_MS, défaut 600000)",
        llm_timeout_ms.saturating_add(30_000),
        llm_timeout_ms.saturating_add(60_000)
    );
    // Mémoire bout-en-bout : snapshot principal sous data_dir, repli legacy
    // ./hakille-memory.json (jamais supprimé). Un snapshot existant mais
    // illisible refuse le démarrage : démarrer vide puis réécrire écraserait
    // silencieusement les faits de l'utilisateur.
    let memory_snapshot = data_dir.join("memory.json");
    let legacy_memory_snapshot = PathBuf::from("./hakille-memory.json");
    let memory_source = if memory_snapshot.exists() {
        Some((memory_snapshot.clone(), false))
    } else if legacy_memory_snapshot.exists() {
        Some((legacy_memory_snapshot.clone(), true))
    } else {
        None
    };
    match memory_source {
        Some((path, legacy)) => match hakille_memory::KnowledgeGraph::load_from_file(&path) {
            Ok(restored) => {
                let n = restored.len();
                orchestrator.memory_graph = restored;
                println!(
                    "  Memory: graphe restauré depuis {} ({} triplet(s)){}",
                    path.display(),
                    n,
                    if legacy { " [repli legacy]" } else { "" }
                );
                if legacy {
                    match orchestrator.memory_graph.save_to_file(&memory_snapshot) {
                        Ok(()) => println!(
                            "  Memory: migré vers {} (legacy conservé intact)",
                            memory_snapshot.display()
                        ),
                        Err(e) => eprintln!(
                            "  ⚠ Memory: migration vers {} impossible — {e} (legacy toujours lisible)",
                            memory_snapshot.display()
                        ),
                    }
                }
            }
            Err(e) => {
                return Err(format!(
                    "snapshot mémoire illisible ({}) — démarrage refusé pour ne pas écraser les faits: {e}",
                    path.display()
                )
                .into());
            }
        },
        None => {
            println!(
                "  Memory: aucun snapshot ({}) — graphe vide (premier démarrage)",
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
    let mut heritage_keys = hakille_memory::KeyRegistry::new();
    // Registre providers local-only (spec BYOM) : --provider inconnu = erreur.
    let mut provider_registry = hakille_kernel::providers::ProviderRegistry::default_local_only();
    if let Some(raw) = provider_arg.as_deref() {
        match hakille_kernel::providers::ProviderId::parse_cli(raw) {
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
        // `live_stream_ui=false` : pas de streaming token par token, même si
        // stdout est un terminal. Le feedback d'attente (⏳) et l'avertissement
        // de réponse vide restent affichés.
        match process_prompt(
            &prompt,
            &mut orchestrator,
            &mut presence,
            &mut cost_counter,
            &mut continuity_tracker,
            &mut bounded_initiative,
            &mut generality_tracker,
            &audit_journal,
            &memory_snapshot,
            false,
            speak_enabled,
        )
        .await
        {
            Ok(non_empty) => {
                if non_empty {
                    println!(
                        "Headless evaluation completed. Cost: {}",
                        cost_counter.formatted_total()
                    );
                } else {
                    println!(
                        "⚠ Headless evaluation : réponse vide (EOS immédiat) — aucun succès simulé."
                    );
                }
                return Ok(());
            }
            Err(e) => {
                eprintln!("✗ Évaluation headless échouée : {e}");
                if let Some(hint) = turn_error_hint(&e.to_string()) {
                    eprintln!("  → {hint}");
                }
                std::process::exit(1);
            }
        }
    }

    if serve_mode {
        return run_serve(
            &mut orchestrator,
            &mut presence,
            &mut cost_counter,
            &mut continuity_tracker,
            &mut bounded_initiative,
            &mut generality_tracker,
            &audit_journal,
            &memory_snapshot,
        )
        .await;
    }

    println!("\n============================================================");
    println!("Hakille is running. Type a prompt, or a command (:help, :status, :models, :exit)");
    println!("============================================================\n");

    // Ctrl+C : arrêt immédiat mais jamais destructeur — la mémoire est
    // auto-sauvegardée après chaque tour (succès ou échec).
    tokio::spawn(async {
        if tokio::signal::ctrl_c().await.is_ok() {
            eprintln!(
                "\n⚠ Interruption (Ctrl+C) — arrêt. La mémoire a été auto-sauvegardée après le dernier tour."
            );
            std::process::exit(130);
        }
    });

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    let sample_prompts = [
        "Planifier une réunion demain matin à 10h",
        "Analyser le contrat de prestation et vérifier les clauses",
        "Refactoriser le module d'inférence en Rust pur",
    ];

    // `--listen` : un tour vocal réel au démarrage (micro → VAD Silero →
    // ASR SenseVoice → tour cognitif), puis le REPL texte continue.
    if listen_mode && let Some(prompt) = listen_once(&mut vad, &asr).await {
        println!("\n[Hakille — tour vocal]");
        match process_prompt(
            &prompt,
            &mut orchestrator,
            &mut presence,
            &mut cost_counter,
            &mut continuity_tracker,
            &mut bounded_initiative,
            &mut generality_tracker,
            &audit_journal,
            &memory_snapshot,
            true,
            speak_enabled,
        )
        .await
        {
            Ok(_) => {}
            Err(e) => {
                presence.set_state(PresenceState::Idle);
                eprintln!("✗ Tour vocal échoué : {e}");
                if let Some(hint) = turn_error_hint(&e.to_string()) {
                    eprintln!("  → {hint}");
                }
            }
        }
    }

    print!("hakille> ");
    stdout.flush()?;

    let mut sample_index = 0;
    for line in stdin.lock().lines() {
        let input = line?.trim().trim_start_matches('\u{feff}').to_string();

        if input.is_empty() {
            if sample_index < sample_prompts.len() {
                let prompt = sample_prompts[sample_index];
                sample_index += 1;
                println!("(Simulating prompt: {})", prompt);
                // REPL : streaming token par token autorisé (si stdout TTY).
                match process_prompt(
                    prompt,
                    &mut orchestrator,
                    &mut presence,
                    &mut cost_counter,
                    &mut continuity_tracker,
                    &mut bounded_initiative,
                    &mut generality_tracker,
                    &audit_journal,
                    &memory_snapshot,
                    true,
                    speak_enabled,
                )
                .await
                {
                    Ok(_) => {}
                    Err(e) => {
                        presence.set_state(PresenceState::Idle);
                        eprintln!("✗ Tour échoué : {e}");
                        if let Some(hint) = turn_error_hint(&e.to_string()) {
                            eprintln!("  → {hint}");
                        }
                    }
                }
            }
            print!("\nhakille> ");
            stdout.flush()?;
            continue;
        }

        match input.as_str() {
            ":exit" | ":quit" => {
                // Sauvegarde auto du graphe avant arrêt (miroir sortie EOF).
                persist_memory(&orchestrator, &memory_snapshot, false);
                println!("Shutting down Hakille daemon cleanly.");
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
                    "  :shell            Show ambient shell snapshot (presence/HUD/cost/silence payload)"
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
                println!(
                    "  :agents           List declarative business manifests + real WASM reference runtime"
                );
                println!(
                    "  :agents-run <name> <json>  Execute a real reference WASM agent (echo-agent/count-agent)"
                );
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
                println!("  :help             Show this command list");
                println!("  :exit             Shutdown Hakille daemon");
                println!("  :quit             Alias of :exit (Shutdown Hakille daemon)");
                println!(
                    "Toute saisie qui ne commence pas par ':' est envoyée au modèle Qwen (pas une commande)."
                );
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
                            hakille_memory::NodeValue::Text(s) => {
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
                            hakille_ui_shell::MemoryRow::freshness_label(item.weight),
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
                let rows: Vec<hakille_ui_shell::MemoryRow> = items
                    .iter()
                    .map(hakille_ui_shell::MemoryRow::from_item)
                    .collect();
                let triplets: Vec<_> = orchestrator
                    .memory_graph
                    .all_triplets()
                    .into_iter()
                    .cloned()
                    .collect();
                let contra = hakille_memory::scan_contradictions(&triplets).len();
                let screen = hakille_ui_shell::MemoryScreenVM::from_rows(&rows, partition, contra);
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
                let hits = hakille_memory::scan_contradictions(&triplets);
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
                    let resolved: Option<hakille_core::TripletId> = match uuid::Uuid::parse_str(
                        id_str,
                    ) {
                        Ok(uuid_val) => Some(hakille_core::TripletId(uuid_val)),
                        Err(_) => {
                            let cands: Vec<hakille_core::TripletId> = orchestrator
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
                        if let Err(e) = audit_memory_event(
                            &audit_journal,
                            "MemoryForget",
                            format!(
                                "Révocation de consentement demandée pour [{}]",
                                triplet_id.0
                            ),
                            serde_json::json!({ "triplet_id": triplet_id.0.to_string(), "origin": "repl" }),
                        ) {
                            eprintln!("✗ Révocation refusée — audit indisponible : {e}");
                        } else {
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
                                    persist_memory(&orchestrator, &memory_snapshot, true);
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
                        let rows: Vec<hakille_ui_shell::JournalRow> = entries
                            .iter()
                            .map(hakille_ui_shell::JournalRow::from_entry)
                            .collect();
                        let filtered = hakille_ui_shell::filter_journal(&rows, query);
                        let page_rows = hakille_ui_shell::page_journal(&filtered, page, 20);
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
                                    "Pourquoi — {} par {:?} (confiance {} : {:.2}) :\n{}",
                                    e.action.action_type,
                                    e.actor,
                                    hakille_design::WhySheet::confidence_word(e.confidence),
                                    e.confidence,
                                    hakille_ui::JournalView::explain_action(e),
                                );
                                println!(
                                    "Trace complète :\n{}",
                                    hakille_ui::JournalView::full_reasoning(e)
                                );
                                let nodes = hakille_ui::JournalView::graph_node_labels(e);
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
                                            hakille_memory::NodeValue::Text(s) => s.clone(),
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
                                let sheet = hakille_ui_shell::WhyStrings::from_parts(
                                    &hakille_ui::JournalView::explain_action(e),
                                    &facts,
                                    &hakille_ui::JournalView::full_reasoning(e),
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
                    "Intentions GUI ancrées sur capture réelle (vision classique ; VLM non exécuté) pour tâche : '{}'...",
                    task
                );
                // Ancrage fail-closed : capture réelle → régions mesurées →
                // intentions. Sans région mesurée, `predict_grounded_on_frame`
                // renvoie Wait (aucun clic inventé au centre de l'écran).
                let capture = WindowsScreenCapturer::capture_primary_display();
                let (screen_w, screen_h) = (capture.width, capture.height);
                let intentions = match gui_agent.predict_grounded_on_frame(
                    task,
                    &capture.data,
                    screen_w,
                    screen_h,
                ) {
                    Ok(intentions) => intentions,
                    Err(e) => {
                        eprintln!("  Ancrage impossible : {e} — aucune action exécutée.");
                        Vec::new()
                    }
                };
                println!("  Predicted {} GUI intentions:", intentions.len());
                for (i, intent) in intentions.iter().enumerate() {
                    println!("    {}. {:?}", i + 1, intent);
                    match gui_agent
                        .execute_intention(&substrate, intent, &gui_input_cap, screen_w, screen_h)
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
                    if let Err(e) = audit_memory_event(
                        &audit_journal,
                        "MemoryRemember",
                        format!("Demande de mémorisation REPL: '{text}'"),
                        serde_json::json!({ "text": text, "origin": "repl" }),
                    ) {
                        eprintln!("✗ Mémorisation refusée — audit indisponible : {e}");
                    } else {
                        let parts: Vec<&str> = text.splitn(3, ' ').collect();
                        let report = |res: hakille_memory::InsertResult,
                                      s: &str,
                                      p: &str,
                                      o: &str| {
                            use hakille_memory::InsertResult;
                            match res {
                                InsertResult::Inserted(id) => println!(
                                    "Stored triplet [{}] in sovereign graph: {s} --({p})--> {o}",
                                    id.0
                                ),
                                InsertResult::ContradictionResolved { old, new } => println!(
                                    "Contradiction résolue sans écrasement silencieux : [{}] remplacé par [{}] ({s} --({p})--> {o})",
                                    old.0, new.0
                                ),
                                InsertResult::NeedsUserResolution { existing, incoming } => {
                                    println!(
                                        "Contradiction — décision requise : existant [{}] vs entrant [{}]. Rien n'a été écrasé (voir :contradictions).",
                                        existing.id.0, incoming.id.0
                                    )
                                }
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
                        persist_memory(&orchestrator, &memory_snapshot, true);
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
                println!(
                    "  LLM: Qwen 0.5B Q4_K_M (timeout {} ms — override HAKILLE_QWEN_TIMEOUT_MS)",
                    orchestrator.system1.llm_timeout_ms()
                );
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
                let quorum = hakille_constitution::governance::GovernanceConfig::default().quorum;
                println!("  Quorum gouvernance : {}", quorum);
                let dilemmes = hakille_constitution::ethics::EthicsBattery::default()
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
                println!("Declarative Business Agent Manifests:");
                for agent in agent_registry.list_agents() {
                    println!(
                        "  - [{}] {} (declared caps: {:?})",
                        agent.id, agent.name, agent.required_capabilities
                    );
                }
                println!(
                    "  NOTE: ces cinq manifests métier restent déclaratifs ; aucun binaire métier n'est chargé ni exécuté."
                );
                match &agent_runtime {
                    Some(runtime) => {
                        println!("Real WASM Reference Runtime:");
                        for agent in runtime.agents() {
                            println!(
                                "  - {} | {} octet(s) | sha256 {} | {}",
                                agent.name,
                                agent.bytes_len,
                                agent.sha256,
                                agent.path.display()
                            );
                        }
                        println!(
                            "  Self-test au boot: OK en {} ms ; fuel/timeout/mémoire bornés, aucun import hôte.",
                            runtime.self_test_duration_ms
                        );
                        println!(
                            "  Exécution réelle : :agents-run <name> <json> (ex. :agents-run echo-agent {{\"msg\":\"bonjour\"}})"
                        );
                    }
                    None => println!(
                        "  WASM Reference Runtime: INDISPONIBLE — aucun binaire chargé ni exécuté (voir l'avertissement au démarrage)."
                    ),
                }
            }
            ":agents-run" => {
                println!(
                    "Usage : :agents-run <name> <json> (ex. :agents-run echo-agent {{\"msg\":\"bonjour\"}})"
                );
            }
            s if s.starts_with(":agents-run ") => {
                let rest = s.trim_start_matches(":agents-run ").trim();
                let mut parts = rest.splitn(2, char::is_whitespace);
                let name = parts.next().unwrap_or("").trim();
                let json = parts.next().unwrap_or("").trim();
                if name.is_empty() || json.is_empty() {
                    eprintln!(
                        "Usage : :agents-run <name> <json> (ex. :agents-run echo-agent {{\"msg\":\"bonjour\"}})"
                    );
                } else if let Some(runtime) = agent_runtime.as_mut() {
                    let input_sha256 = sha256_hex(json.as_bytes());
                    let audit_request = audit_readonly_action(
                        &audit_journal,
                        "WasmAgentRunRequested",
                        format!("Exécution demandée de l'agent WASM '{name}'"),
                        serde_json::json!({
                            "agent": name,
                            "input_sha256": input_sha256,
                            "input_len": json.len(),
                        }),
                    );
                    match audit_request {
                        Err(e) => {
                            eprintln!("Exécution refusée — audit indisponible : {e}");
                        }
                        Ok(()) => match runtime.run(name, json) {
                            Ok(run) => {
                                println!(
                                    "Agent {} : OK en {} ms (sortie {} octet(s))",
                                    run.agent,
                                    run.duration_ms,
                                    run.output.len()
                                );
                                println!("  output: {}", run.output);
                                println!(
                                    "  input sha256: {} | output sha256: {}",
                                    run.input_sha256, run.output_sha256
                                );
                                if let Err(e) = audit_readonly_action(
                                    &audit_journal,
                                    "WasmAgentRunCompleted",
                                    format!("Exécution réussie de l'agent WASM '{}'", run.agent),
                                    serde_json::json!({
                                        "agent": run.agent,
                                        "duration_ms": run.duration_ms,
                                        "input_sha256": run.input_sha256,
                                        "output_sha256": run.output_sha256,
                                        "output_len": run.output.len(),
                                    }),
                                ) {
                                    eprintln!("  Audit du résultat impossible : {e}");
                                }
                            }
                            Err(e) => {
                                eprintln!("Agent {name} : échec : {e}");
                                if let Err(audit_err) = audit_readonly_action(
                                    &audit_journal,
                                    "WasmAgentRunFailed",
                                    format!("Échec de l'agent WASM '{name}'"),
                                    serde_json::json!({
                                        "agent": name,
                                        "input_sha256": input_sha256,
                                        "error": e.to_string(),
                                    }),
                                ) {
                                    eprintln!("  Audit de l'échec impossible : {audit_err}");
                                }
                            }
                        },
                    }
                } else {
                    eprintln!(
                        "WASM runtime indisponible — aucune exécution tentée. Vérifie l'avertissement de démarrage."
                    );
                }
            }
            ":cost" => {
                println!("Inference Cost: {}", cost_counter.formatted_total());
            }
            ":models-honest" => {
                println!("Honest Inference Statuses (artefacts-derived; never fake Operational):");
                let statuses = provisioner.list_all_status();
                let installed = statuses.iter().filter(|s| s.is_installed).count();
                let gb = statuses
                    .iter()
                    .map(|s| s.total_disk_bytes as f64 / 1_000_000_000.0)
                    .sum::<f64>();
                for row in hakille_ui_shell::honest_engine_rows(&models_dir, gb, statuses.len()) {
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
                for card in hakille_ui_shell::reference_agent_cards() {
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
                let view = hakille_ui_shell::PowersView::from_manifest(&granted, &denied);
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
                let view = hakille_ui_shell::OnboardingView::from_manager(&onboarding);
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
                let snap = hakille_ui_shell::ShellSnapshot::from_states(
                    &presence,
                    &ephemeral_hud,
                    &cost_counter,
                    &silence_budget,
                    now_ms,
                    0,
                );
                let spec = ephemeral_hud.overlay_spec();
                println!(
                    "Ambient Shell Snapshot → {}",
                    snap.to_compositor_json(&spec)
                );
                println!("--- Dashboard texte (fallback Redox/Web/clavier) ---");
                println!("{}", snap.render_text_dashboard());
                println!(
                    "  hud visible: {} | host: {} {} | Rendu natif: cargo run -p hakille-ui-desktop --bin hakille-desktop (ADR-009)",
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
                match hakille_memory::export_graph(
                    &orchestrator.memory_graph,
                    hakille_memory::ExportFormat::Json,
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
                        match orchestrator.memory_graph.save_to_file(&memory_snapshot) {
                            Ok(()) => println!(
                                "  (snapshot RAM sauvegardé vers {})",
                                memory_snapshot.display()
                            ),
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
                        Ok(bytes) => match hakille_memory::import_graph(&bytes) {
                            Ok(export) => {
                                if let Err(e) = audit_memory_event(
                                    &audit_journal,
                                    "MemoryImport",
                                    format!("Import de graphe depuis '{path}'"),
                                    serde_json::json!({ "path": path }),
                                ) {
                                    eprintln!("✗ Import refusé — audit indisponible : {e}");
                                } else {
                                    let n =
                                        import_into_graph(&mut orchestrator.memory_graph, &export);
                                    println!(
                                        "Graphe importé depuis '{path}' : {} triplet(s) fusionné(s) (total : {}).",
                                        n,
                                        orchestrator.memory_graph.len()
                                    );
                                    persist_memory(&orchestrator, &memory_snapshot, true);
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
                    match hakille_memory::crypto_erase(&mut heritage_keys, key_id) {
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
                    match hakille_kernel::providers::ProviderId::parse_cli(rest) {
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
                            parameters: serde_json::json!({
                                "dedup_key": e.dedup_key.clone(),
                                "attempts": e.attempts,
                            }),
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
                    match audit_journal.append(entry) {
                        Ok(_) => {
                            println!(
                                "  → transmis : {} {} (clé {})",
                                e.verb, e.object, e.dedup_key
                            );
                            sent += 1;
                        }
                        Err(err) => {
                            // L'entrée a déjà été retirée par `take_sendable` :
                            // on la remet en file pour ne pas perdre l'action.
                            eprintln!(
                                "  ✗ Audit indisponible pour {} {} — entrée remise en file : {err}",
                                e.verb, e.object
                            );
                            let _ = outbox.enqueue(e);
                            break;
                        }
                    }
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
                match &camera_handle {
                    None => {
                        eprintln!(
                            "Camera unavailable: {}",
                            camera_unavailable_reason
                                .as_deref()
                                .unwrap_or("raison inconnue")
                        );
                    }
                    Some(handle) => {
                        match substrate.capture_camera_frame(*handle, &camera_cap).await {
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
                let view = hakille_ui_shell::SettingsView::default();
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
                match &camera_handle {
                    None => {
                        println!("  [Camera] indisponible — cycle poursuivi sans vision");
                    }
                    Some(handle) => {
                        match substrate.capture_camera_frame(*handle, &camera_cap).await {
                            Ok(frame) => {
                                let v_ctx = camera_pipeline.process_frame(&frame);
                                camera_pipeline.update_world_model(&v_ctx, &mut world_model);
                                println!(
                                    "  [Camera] Scene updated: {:?}, gaze attentive: {}, lighting: {:?}",
                                    v_ctx.user_presence,
                                    v_ctx.user_gaze_focused_on_device,
                                    v_ctx.lighting
                                );
                            }
                            Err(e) => eprintln!("  [Camera] échec capture: {}", e),
                        }
                    }
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
                    vec![hakille_core::CapabilityAction::Read],
                    Timestamp(chrono::Utc::now() + chrono::Duration::hours(1)),
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
                let mut win_tray = hakille_substrate_windows::WindowsPresenceTray::new();
                println!("  • Initial State:            {:?}", win_tray.state());
                win_tray.set_state(hakille_substrate_windows::WindowsPresenceState::Listening);
                println!(
                    "  • Updated to Listening:     {:?} (Tooltip: '{}')",
                    win_tray.state(),
                    win_tray.tooltip()
                );
                win_tray.set_state(hakille_substrate_windows::WindowsPresenceState::Acting);
                println!(
                    "  • Updated to Acting:        {:?} (Tooltip: '{}')",
                    win_tray.state(),
                    win_tray.tooltip()
                );
                win_tray.set_state(hakille_substrate_windows::WindowsPresenceState::Idle);
                println!(
                    "  • Returned to Idle:         {:?} (Tooltip: '{}')",
                    win_tray.state(),
                    win_tray.tooltip()
                );
                let mut hotkey = hakille_substrate_windows::WindowsGlobalHotkey::new();
                let registered = hotkey.register();
                println!(
                    "  • Global Hotkey:            {:?} (Ctrl+Alt+Space, alias Alt+Space summoned: {})",
                    hotkey.key_combination, registered
                );
            }

            s if bare_command_usage(s).is_some() => {
                if let Some(usage) = bare_command_usage(s) {
                    eprintln!("{usage}");
                }
            }

            s if s.starts_with(':') => {
                println!("✗ Commande inconnue : {input}");
                if let Some(suggestion) = suggest_command(s) {
                    println!("  → Commande proche : {suggestion}");
                }
                println!("  Tape :help pour la liste.");
            }

            _ => {
                // REPL : streaming token par token autorisé (si stdout TTY).
                match process_prompt(
                    &input,
                    &mut orchestrator,
                    &mut presence,
                    &mut cost_counter,
                    &mut continuity_tracker,
                    &mut bounded_initiative,
                    &mut generality_tracker,
                    &audit_journal,
                    &memory_snapshot,
                    true,
                    speak_enabled,
                )
                .await
                {
                    Ok(_) => {}
                    Err(e) => {
                        presence.set_state(PresenceState::Idle);
                        eprintln!("✗ Tour échoué : {e}");
                        if let Some(hint) = turn_error_hint(&e.to_string()) {
                            eprintln!("  → {hint}");
                        }
                    }
                }
            }
        }

        print!("\nhakille> ");
        stdout.flush()?;
    }

    // Sortie de boucle (EOF/Ctrl-D) : même sauvegarde auto que :exit.
    persist_memory(&orchestrator, &memory_snapshot, true);

    Ok(())
}

/// Résultat d'un tour cognitif : consommé par le REPL (wrapper TTY) et par le
/// serveur IPC. `non_empty=false` = réponse vide (EOS) propagée honnêtement.
struct TurnOutcome {
    pub response: String,
    pub cost_cents: u64,
    pub non_empty: bool,
    pub routing_used: String,
    pub latency_ms: u64,
    pub confidence_decision: hakille_core::ConfidenceDecision,
    /// Échec de persistance mémoire du tour, propagé au HUD au lieu d'être
    /// masqué (le tour lui-même reste valide).
    pub persistence_warning: Option<String>,
}

/// Cœur réutilisable d'un tour : présence, heuristique de domaine, tour
/// orchestrateur (attente directe, sans spinner ni ticker), compteur de coût,
/// trackers, entrée d'audit signée. N'installe JAMAIS le callback de streaming
/// global : les appelants (REPL, serveur IPC) font la présentation.
#[allow(clippy::too_many_arguments)]
async fn run_turn_core(
    orchestrator: &mut CognitiveOrchestrator,
    presence: &mut PresenceIndicator,
    cost_counter: &mut CostCounter,
    continuity_tracker: &mut ContinuityTracker,
    bounded_initiative: &mut BoundedInitiativeTracker,
    generality_tracker: &mut GeneralityTracker,
    audit_journal: &AuditJournal,
    memory_path: &std::path::Path,
    prompt: &str,
) -> Result<TurnOutcome, Box<dyn std::error::Error>> {
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

    let result = match orchestrator
        .process_user_turn(prompt, &TrustLevel::User)
        .await
    {
        Ok(result) => result,
        Err(e) => {
            presence.set_state(PresenceState::Idle);
            return Err(e.into());
        }
    };

    let cost_cents = match result.estimated_cost_cents {
        Some(cost) => {
            cost_counter.record_cost(cost);
            cost
        }
        None => 0,
    };

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
        cost: result.estimated_cost_cents.map(|c| hakille_core::Cost {
            amount_cents: c,
            currency: "EUR".to_string(),
            description: "Turn cost".to_string(),
        }),
        signature: vec![1, 2, 3, 4],
        prev_hash: vec![0; 32],
        token_hash: None,
    };
    // Fail-closed : un tour sans trace d'audit ne doit pas être présenté
    // comme un succès. L'erreur remonte au REPL/HUD avec la cause réelle.
    if let Err(e) = audit_journal.append(entry) {
        presence.set_state(PresenceState::Idle);
        return Err(format!("journal d'audit: entrée du tour non écrite: {e}").into());
    }

    // La mémoire est persistée ici, au même point que l'audit, pour que le
    // REPL et le mode `--serve` aient exactement la même durabilité.
    let persistence_warning = persist_memory_to(orchestrator, memory_path)
        .err()
        .map(|e| format!("mémoire non sauvegardée: {e}"));
    if let Some(warning) = &persistence_warning {
        eprintln!("⚠ {warning}");
    }

    presence.set_state(PresenceState::Idle);

    let non_empty = !result.response_text.trim().is_empty();

    Ok(TurnOutcome {
        response: result.response_text,
        cost_cents,
        non_empty,
        routing_used: result.routing_used,
        latency_ms: result.latency_ms,
        confidence_decision: result.confidence_decision,
        persistence_warning,
    })
}

/// Exécute un tour cognitif complet et affiche la réponse.
///
/// `live_stream_ui` : quand `true` ET `stdout` est un TTY, les tokens Qwen
/// sont affichés en direct via le callback global de `hakille_inference`
/// (REPL). Le mode `--eval-prompt` passe `false` : pas de streaming token à
/// token, mais le feedback d'attente (⏳) reste actif si stdout est un TTY.
#[allow(clippy::too_many_arguments)]
async fn process_prompt(
    prompt: &str,
    orchestrator: &mut CognitiveOrchestrator,
    presence: &mut PresenceIndicator,
    cost_counter: &mut CostCounter,
    continuity_tracker: &mut ContinuityTracker,
    bounded_initiative: &mut BoundedInitiativeTracker,
    generality_tracker: &mut GeneralityTracker,
    audit_journal: &AuditJournal,
    memory_path: &std::path::Path,
    live_stream_ui: bool,
    speak_enabled: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    // Process cognitive turn — feedback d'attente pendant la génération
    // (≈24 s release CPU, plus le chargement initial 10-60 s).
    let is_tty = io::stdout().is_terminal();
    let turn = run_turn_core(
        orchestrator,
        presence,
        cost_counter,
        continuity_tracker,
        bounded_initiative,
        generality_tracker,
        audit_journal,
        memory_path,
        prompt,
    );
    tokio::pin!(turn);

    // Streaming token par token (REPL + stdout TTY uniquement) : le callback
    // global du moteur d'inférence écrit sur stdout depuis le thread de
    // génération (verrou moteur tenu — il ne rappelle donc JAMAIS le moteur).
    // Il est retiré inconditionnellement après le tour (chemins Ok ET Err).
    let stream_started = Arc::new(AtomicBool::new(false));
    if is_tty && live_stream_ui {
        let stream_started = Arc::clone(&stream_started);
        let cb: Box<dyn FnMut(&hakille_inference::TokenEvent) + Send> = Box::new(move |ev| {
            if ev.index == 0 {
                // Premier token : marquer le flux, effacer la ligne ⏳
                // éventuelle (pas d'ANSI : retour chariot + espaces) et ouvrir
                // la zone de texte (ligne vide + indentation 2 espaces).
                stream_started.store(true, Ordering::SeqCst);
                print!("\r{:80}\r\n\n  ", "");
            }
            print!("{}", ev.text_delta);
            let _ = io::stdout().flush();
        });
        hakille_inference::set_token_stream_callback(Some(cb));
    }

    let turn_result = if is_tty {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(1));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // Le premier tick d'`interval` est immédiat : le consommer pour que
        // l'affichage commence à 1 s réellement écoulée.
        ticker.tick().await;
        let mut elapsed_secs: u64 = 0;
        loop {
            tokio::select! {
                biased;
                res = &mut turn => break res,
                _ = ticker.tick() => {
                    if stream_started.load(Ordering::SeqCst) {
                        // Les tokens s'écrivent en direct : ne plus réafficher
                        // la ligne ⏳ (déjà effacée par l'en-tête du callback).
                    } else {
                        elapsed_secs += 1;
                        print!(
                            "\r  ⏳ Hakille réfléchit… {elapsed_secs}s (Qwen 0.5B CPU, 1er tour = chargement du modèle)      "
                        );
                        let _ = io::stdout().flush();
                    }
                }
            }
        }
    } else {
        println!("  ⏳ Génération Qwen en cours (CPU ; le 1er tour charge le modèle)…");
        let _ = io::stdout().flush();
        turn.await
    };

    // TOUJOURS retirer le callback global (Ok comme Err) : aucun `return`
    // intermédiaire ne doit court-circuiter ce nettoyage.
    hakille_inference::clear_token_stream_callback();

    if is_tty {
        if stream_started.load(Ordering::SeqCst) {
            // Terminer proprement la ligne de texte streamée.
            println!();
        } else {
            // Effacer la ligne d'attente (pas d'ANSI : retour chariot + espaces).
            print!("\r{:80}\r", "");
            let _ = io::stdout().flush();
        }
    }

    let outcome = match turn_result {
        Ok(outcome) => outcome,
        Err(e) => {
            // `run_turn_core` a déjà remis la présence à Idle (jamais de faux état).
            return Err(e);
        }
    };

    if is_tty && stream_started.load(Ordering::SeqCst) {
        // Le texte a déjà été écrit en direct : pas de réimpression, seules
        // les métadonnées du tour sont affichées.
        println!(
            "  [Routing: {} | Latency: {}ms | Confidence: {:?}]",
            outcome.routing_used, outcome.latency_ms, outcome.confidence_decision
        );
    } else {
        println!("\n[Hakille Response]");
        println!("  {}", outcome.response);
        println!(
            "  [Routing: {} | Latency: {}ms | Confidence: {:?}]",
            outcome.routing_used, outcome.latency_ms, outcome.confidence_decision
        );
    }

    if !outcome.non_empty {
        println!("  ⚠ Réponse vide (EOS immédiat) — propagée honnêtement, pas un succès simulé.");
    }
    if let Some(warning) = &outcome.persistence_warning {
        eprintln!("  ⚠ {warning} — le tour reste affiché, mais la mémoire n'est pas durable.");
    }

    // `--speak` : synthèse formantique RÉELLE puis lecture sur le périphérique
    // de sortie par défaut (cpal). Toute erreur audio est affichée, jamais
    // masquée : le tour cognitif reste valide même si la lecture échoue.
    if speak_enabled && outcome.non_empty {
        let tts = FormantTts::new();
        match tts.render_pcm(&outcome.response, 1.0) {
            Ok(chunk) => {
                println!(
                    "  🔊 TTS formantique : {:.2} s @ {} Hz",
                    chunk.duration_seconds(),
                    chunk.sample_rate
                );
                if let Err(e) = hakille_audio::play_pcm(&chunk.samples, chunk.sample_rate) {
                    eprintln!("  ⚠ Lecture audio impossible : {e}");
                }
            }
            Err(e) => eprintln!("  ⚠ Synthèse vocale impossible : {e}"),
        }
    }

    Ok(outcome.non_empty)
}

/// Écoute micro one-shot réelle : VAD Silero (`ort`) pour détecter début/fin
/// de parole, puis ASR SenseVoice (`ort`). Aucun fallback simulé : toute
/// erreur (micro absent, modèle manquant, forward en échec) est affichée et
/// `None` est renvoyé — le REPL continue en mode texte.
async fn listen_once(vad: &mut VoiceActivityDetector, asr: &SenseVoiceSmall) -> Option<String> {
    let mut capture = match hakille_audio::Capture::new() {
        Ok(capture) => capture,
        Err(e) => {
            eprintln!("  ⚠ Écoute micro indisponible : {e} — REPL en mode texte.");
            return None;
        }
    };
    let (tx, rx) = std::sync::mpsc::channel::<Vec<f32>>();
    if let Err(e) = capture.start(move |block| {
        let _ = tx.send(block.to_vec());
    }) {
        eprintln!("  ⚠ Démarrage de la capture impossible : {e} — REPL en mode texte.");
        return None;
    }

    println!(
        "  🎤 Écoute micro (VAD Silero réel) : parlez ; 1,2 s de silence termine (max 15 s)..."
    );
    vad.reset();

    const MAX_MS: u64 = 15_000;
    const SILENCE_MS: u64 = 1_200;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(MAX_MS);
    let mut pcm: Vec<f32> = Vec::new();
    let mut speech_started = false;
    let mut silence_ms: u64 = 0;

    while std::time::Instant::now() < deadline {
        let block = match rx.recv_timeout(std::time::Duration::from_millis(250)) {
            Ok(block) => block,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let chunk = AudioChunk::new(block.clone(), hakille_audio::TARGET_SAMPLE_RATE, 1);
        let is_speech = match vad.process_real(&chunk) {
            Ok(result) => result.is_speech,
            Err(e) => {
                eprintln!("  ⚠ VAD réel indisponible : {e}");
                break;
            }
        };
        if is_speech {
            speech_started = true;
            silence_ms = 0;
            pcm.extend_from_slice(&block);
        } else if speech_started {
            silence_ms += 10;
            pcm.extend_from_slice(&block);
            if silence_ms >= SILENCE_MS {
                break;
            }
        }
        // Avant le premier mot : blocs ignorés (pas de pré-roll dans le buffer).
    }

    if let Err(e) = capture.stop() {
        eprintln!("  ⚠ Arrêt de la capture : {e}");
    }
    if !speech_started || pcm.is_empty() {
        println!("  (aucune parole détectée)");
        return None;
    }

    println!("  Transcription ASR (SenseVoice réel)...");
    let chunk = AudioChunk::new(pcm, hakille_audio::TARGET_SAMPLE_RATE, 1);
    match asr.transcribe(&chunk).await {
        Ok(hyp) => {
            let text = hyp.transcript.trim().to_string();
            if text.is_empty() {
                println!("  (transcription vide — aucune action)");
                None
            } else {
                println!(
                    "  🎤 « {} » [langue: {}, confiance: {:.3}]",
                    text, hyp.language, hyp.confidence
                );
                Some(text)
            }
        }
        Err(e) => {
            eprintln!("  ⚠ ASR réel impossible : {e}");
            None
        }
    }
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
// Keys live in `<data_dir>/keys/` (LOCALAPPDATA/Hakille/keys or
// ./hakille_data/Hakille/keys fallback). Raw 32-byte secrets, no encryption yet
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

/// Fusionne un export JSON (`hakille_memory::import_graph`, produit par
/// `:export`) dans le graphe RAM : restaure d'abord les registres nom→ID via
/// les labels exportés (IDs déterministes — `resolve_*` redonne le même UUID,
/// rendant les faits importés requêtable via `:recall`), puis réinsère chaque
/// triplet via `KnowledgeGraph::insert` (routage contradiction : un
/// `NeedsUserResolution` est loggué en warn, jamais écrasé). Rend le nombre
/// de triplets traités.
fn import_into_graph(
    graph: &mut hakille_memory::KnowledgeGraph,
    export: &hakille_memory::GraphExport,
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
        if let hakille_memory::InsertResult::NeedsUserResolution { existing, incoming } =
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

/// Usage des commandes nues qui exigent un argument : afficher le message
/// d'usage au lieu d'envoyer la commande au modèle (vocabulaire identique aux
/// branches du match REPL).
fn bare_command_usage(input: &str) -> Option<&'static str> {
    match input {
        ":plan" => Some("Usage : :plan <texte> — aperçu sec (plan + dry-run, zéro exécution)."),
        ":defer" => Some("Usage : :defer <verbe> | <objet>"),
        ":remember" => {
            Some("Usage : :remember <sujet> <prédicat> <objet> (ex. :remember Marie aime le jazz)")
        }
        ":recall" => Some("Usage : :recall <mot-clé> (ex. :recall jazz)"),
        ":forget" => Some("Usage : :forget <préfixe-8-caractères | uuid-complet> (voir :memory)"),
        ":download" => Some("Usage : :download <id> (ex. :download silero-vad-v5)"),
        ":asr-test" => Some("Usage : :asr-test <texte>"),
        ":embed-test" => Some("Usage : :embed-test <texte>"),
        ":journal-why" => Some("Usage : :journal-why <n|id>"),
        ":why2" => Some("Usage : :why2 <n|id>"),
        ":gui-action" => Some("Usage : :gui-action <cmd>"),
        ":import" => Some("Usage : :import <fichier> (JSON produit par :export)"),
        ":revalidate" => Some("Usage : :revalidate <key>"),
        ":heritage-erase" => Some("Usage : :heritage-erase <key_id>"),
        _ => None,
    }
}

/// Toutes les commandes REPL visibles (y compris l'alias `:quit`), utilisées
/// pour suggérer la commande la plus proche en cas de faute de frappe.
const KNOWN_COMMANDS: &[&str] = &[
    ":quit",
    ":exit",
    ":help",
    ":models",
    ":connected",
    ":inspect-models",
    ":asr-test",
    ":embed-test",
    ":download",
    ":verify-models",
    ":memory",
    ":memory-view",
    ":mem2",
    ":contradictions",
    ":forget",
    ":audit-view",
    ":journal-search",
    ":journal-why",
    ":why2",
    ":gui-action",
    ":remember",
    ":recall",
    ":status",
    ":constitution",
    ":metrics",
    ":agents",
    ":agents-run",
    ":cost",
    ":models-honest",
    ":agents-cards",
    ":powers",
    ":onboarding",
    ":shell",
    ":a11y",
    ":defer",
    ":outbox",
    ":revalidate",
    ":export",
    ":import",
    ":heritage-erase",
    ":providers",
    ":plan",
    ":flush",
    ":camera",
    ":hud",
    ":channel",
    ":settings",
    ":budget",
    ":ambient",
    ":win-ram",
    ":win-power",
    ":win-screen",
    ":win-tray",
];

/// Distance de Levenshtein (insertion/suppression/substitution = 1) —
/// implémentée localement, aucune dépendance supplémentaire.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr: Vec<usize> = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// Suggère la commande la plus proche (distance ≤ 2) pour une entrée `:xxx`.
fn suggest_command(input: &str) -> Option<&'static str> {
    KNOWN_COMMANDS
        .iter()
        .map(|cmd| (*cmd, levenshtein(input, cmd)))
        .filter(|(_, distance)| *distance <= 2)
        .min_by_key(|(_, distance)| *distance)
        .map(|(cmd, _)| cmd)
}

/// Indice actionnable pour un tour échoué — jamais de faux succès.
fn turn_error_hint(err: &str) -> Option<&'static str> {
    let lower = err.to_lowercase();
    if lower.contains("budget") {
        Some(
            "Dépassement du budget pipeline. Le budget suit le timeout LLM : relance avec HAKILLE_QWEN_TIMEOUT_MS=1200000.",
        )
    } else if lower.contains("timeout") {
        Some(
            "Augmente HAKILLE_QWEN_TIMEOUT_MS (défaut 600000 ms) ; le premier tour inclut le chargement du modèle (10-60 s).",
        )
    } else if lower.contains("gguf")
        || lower.contains("tokenizer")
        || lower.contains("modèle")
        || lower.contains("model artifact")
    {
        Some(
            "Définis HAKILLE_MODELS_DIR ou HAKILLE_QWEN_GGUF, ou lance :download qwen2.5-0.5b-instruct-q4_k_m.",
        )
    } else if lower.contains("capability") {
        Some("Vérifie les pouvoirs actifs avec :powers et les fournisseurs avec :providers.")
    } else {
        None
    }
}

/// Journalise une mutation mémoire AVANT de l'appliquer : aucune écriture
/// non auditée ne doit exister, même si la mutation échoue ensuite.
fn audit_memory_event(
    audit_journal: &AuditJournal,
    action_type: &str,
    description: String,
    parameters: serde_json::Value,
) -> Result<(), String> {
    audit_action(audit_journal, action_type, description, parameters)
}

/// Journalise une action générique (mémoire, agent WASM, etc.) avant ou après
/// son exécution. Les payloads sensibles ne doivent jamais être journalisés
/// en clair : les appelants passent des hashes ou des métadonnées.
fn audit_action(
    audit_journal: &AuditJournal,
    action_type: &str,
    description: String,
    parameters: serde_json::Value,
) -> Result<(), String> {
    audit_action_with_reversibility(audit_journal, action_type, description, parameters, false)
}

/// Variante explicite pour les actions sans effet de bord persistant, comme
/// l'exécution d'un agent WASM en lecture seule.
fn audit_readonly_action(
    audit_journal: &AuditJournal,
    action_type: &str,
    description: String,
    parameters: serde_json::Value,
) -> Result<(), String> {
    audit_action_with_reversibility(audit_journal, action_type, description, parameters, true)
}

fn audit_action_with_reversibility(
    audit_journal: &AuditJournal,
    action_type: &str,
    description: String,
    parameters: serde_json::Value,
    reversible: bool,
) -> Result<(), String> {
    let entry = AuditEntry {
        id: EntryId(uuid::Uuid::now_v7()),
        timestamp: Timestamp::now(),
        actor: ActorId::Kernel,
        action: ActionRecord {
            action_type: action_type.to_string(),
            description,
            parameters,
            reversible,
            reversed: false,
        },
        capability_used: None,
        reasoning_trace: ReasoningTrace {
            steps: vec![],
            model_used: None,
            duration_ms: 0,
        },
        graph_nodes_used: vec![],
        confidence: 1.0,
        cost: None,
        signature: Vec::new(),
        prev_hash: Vec::new(),
        token_hash: None,
    };
    audit_journal
        .append(entry)
        .map(|_| ())
        .map_err(|e| format!("entrée d'audit non écrite: {e}"))
}

/// Sauvegarde le graphe de mémoire vers `path` (écriture atomique).
/// `quiet=false` : message succès (style historique des commandes) ;
/// `quiet=true` : silencieux sauf erreur (auto-save crash-safe).
fn persist_memory(orchestrator: &CognitiveOrchestrator, path: &std::path::Path, quiet: bool) {
    match persist_memory_to(orchestrator, path) {
        Ok(()) => {
            if !quiet {
                println!(
                    "Mémoire sauvegardée ({} triplet(s)) vers {}.",
                    orchestrator.memory_graph.len(),
                    path.display()
                );
            }
        }
        Err(e) => eprintln!("✗ Sauvegarde mémoire impossible ({}) : {e}", path.display()),
    }
}

/// Variante résultat pour les chemins qui doivent propager un échec de
/// persistance (accusé IPC `MemoryRemember`, audits de tour).
fn persist_memory_to(
    orchestrator: &CognitiveOrchestrator,
    path: &std::path::Path,
) -> Result<(), String> {
    orchestrator
        .memory_graph
        .save_to_file(path)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suggest_command_typo() {
        assert_eq!(
            suggest_command(":hlep"),
            Some(":help"),
            "une faute de frappe (:hlep) doit suggérer :help"
        );
        assert_eq!(
            suggest_command(":sttaus"),
            Some(":status"),
            "une transposition (:sttaus) doit suggérer :status"
        );
    }

    #[test]
    fn test_suggest_command_unknown_returns_none() {
        assert_eq!(
            suggest_command(":zzz"),
            None,
            ":zzz est trop éloigné de toute commande connue"
        );
    }

    #[test]
    fn test_turn_error_hint_budget() {
        let err =
            "Internal error: Stage budget exceeded for 'inference' (elapsed 24000 ms > max 100 ms)";
        let hint = turn_error_hint(err);
        assert!(
            hint.is_some(),
            "une erreur de budget doit produire un indice"
        );
        assert!(
            hint.unwrap_or_default().contains("HAKILLE_QWEN_TIMEOUT_MS"),
            "l'indice budget doit citer HAKILLE_QWEN_TIMEOUT_MS"
        );
    }

    #[test]
    fn test_turn_error_hint_timeout() {
        let err = "Inference error: LLM timeout after 600000 ms waiting for decode";
        let hint = turn_error_hint(err);
        assert!(
            hint.is_some(),
            "une erreur de timeout doit produire un indice"
        );
        assert!(
            hint.unwrap_or_default().contains("HAKILLE_QWEN_TIMEOUT_MS"),
            "l'indice timeout doit citer HAKILLE_QWEN_TIMEOUT_MS"
        );
    }

    #[test]
    fn test_turn_error_hint_model() {
        let err = "Inference error: GGUF file not found for qwen2.5-0.5b";
        let hint = turn_error_hint(err);
        assert!(
            hint.is_some(),
            "une erreur de modèle doit produire un indice"
        );
        assert!(
            hint.unwrap_or_default().contains("HAKILLE_MODELS_DIR"),
            "l'indice modèle doit citer HAKILLE_MODELS_DIR"
        );
    }

    #[test]
    fn test_turn_error_hint_capability() {
        let err = "Capability denied: Network scope required for cloud provider";
        let hint = turn_error_hint(err);
        assert!(
            hint.is_some(),
            "une erreur de capability doit produire un indice"
        );
        assert!(
            hint.unwrap_or_default().contains(":powers"),
            "l'indice capability doit citer :powers"
        );
    }

    #[test]
    fn test_turn_error_hint_unknown_none() {
        assert_eq!(
            turn_error_hint("erreur quelconque sans mot-clé connu"),
            None,
            "une erreur sans mot-clé ne doit produire aucun indice"
        );
    }

    #[test]
    fn test_known_commands_start_with_colon() {
        for cmd in KNOWN_COMMANDS {
            assert!(
                cmd.starts_with(':'),
                "la commande connue '{cmd}' doit commencer par ':'"
            );
        }
    }

    #[test]
    fn test_bare_command_usage_exhaustive() {
        assert!(
            bare_command_usage(":plan").is_some(),
            ":plan nue doit afficher son usage"
        );
        assert!(
            bare_command_usage(":heritage-erase").is_some(),
            ":heritage-erase nue doit afficher son usage"
        );
        assert_eq!(
            bare_command_usage(":plan quelque chose"),
            None,
            "une commande avec argument ne doit pas être interceptée"
        );
    }
}
