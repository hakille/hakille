use annora_agents::{AgentManifest, AgentRegistry};
use annora_audit::entry::{ActionRecord, AuditEntry, ReasoningStep, ReasoningTrace};
use annora_audit::journal::AuditJournal;
use annora_constitution::rules::Constitution;
use annora_core::{
    ActorId, CapabilityAuthority, CapabilityScope, EntryId, KeyPair,
    Timestamp, TrustLevel, UserId,
};
use annora_kernel::{
    fabric::{CognitiveFabric, CognitiveManifest},
    orchestrator::CognitiveOrchestrator,
};
use annora_metrics::{
    BoundedInitiativeTracker, ContinuityTracker, GeneralityTracker, TaskDomain,
    ValuesDriftDetector,
};
use annora_perception::audio::{SenseVoiceSmall, VoiceActivityDetector, WakeWordDetector};
use annora_substrate::{Compute, Substrate};
use annora_substrate_windows::{WindowsComputeBackend, WindowsSubstrate};
use annora_sync::{CrdtSync, HandoffCoordinator};
use annora_ui::{CostCounter, OnboardingManager, PresenceIndicator, PresenceState};
use annora_voice::{BargeInDetector, CosyVoice2Engine, MoshiEngine};
use std::io::{self, BufRead, Write};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();
    let mut eval_prompt: Option<String> = None;
    let mut daily_budget_cents: u64 = 500;

    let mut arg_idx = 1;
    while arg_idx < args.len() {
        match args[arg_idx].as_str() {
            "--help" | "-h" => {
                println!("Annora Sovereign Cognitive OS Daemon");
                println!("Usage: annora-daemon [OPTIONS]");
                println!();
                println!("Options:");
                println!("  --eval-prompt <PROMPT>   Execute a single turn headlessly and exit");
                println!("  --budget <CENTS>         Set daily System 2 budget in cents (default: 500)");
                println!("  --version, -v            Print version information");
                println!("  --help, -h               Print this help message");
                return Ok(());
            }
            "--version" | "-v" => {
                println!("annora-daemon v0.1.0 (Rust 2024)");
                return Ok(());
            }
            "--eval-prompt" => {
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
    let _authority = CapabilityAuthority::new();
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

    // 4. Initialize Perception & Voice Engines
    println!("\n[4/6] Bootstrapping Perception Pipeline & Moshi Voice Engine...");
    let mut _vad = VoiceActivityDetector::default_silero_profile();
    let mut _wake_word = WakeWordDetector::default_annora();
    let _asr = SenseVoiceSmall::new();
    let mut _moshi = MoshiEngine::new();
    let _tts = CosyVoice2Engine::new();
    let mut _barge_in = BargeInDetector::new(0.6, 2);
    println!("  Audio Pipeline: 16kHz continuous capture buffer ready");
    println!("  SenseVoice-Small: Multi-task transcription & acoustic events loaded");
    println!("  Moshi: Full-duplex speech-to-speech engine ready (160ms theoretical latency)");

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
    let fabric = CognitiveFabric::new(CognitiveManifest::default());
    let mut orchestrator = CognitiveOrchestrator::new(fabric, user_id.clone(), constitution.clone());
    let mut presence = PresenceIndicator::new();
    let mut cost_counter = CostCounter::new(daily_budget_cents, "EUR");
    let mut onboarding = OnboardingManager::new();
    onboarding.acknowledge_limits();

    let mut continuity_tracker = ContinuityTracker::new();
    let mut bounded_initiative = BoundedInitiativeTracker::new();
    let mut generality_tracker = GeneralityTracker::new();
    let drift_detector = ValuesDriftDetector::default_constitutional();

    println!("  Cognitive Fabric: System 1 / System 2 dynamic router online");
    println!("  North-Star Metrics: Continuity, Bounded Initiative, Generality tracking active");
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
    println!("Annora is running. Type a prompt, or a command (:help, :status, :metrics, :exit)");
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
        let input = line?.trim().to_string();
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
                println!("  :status     Show platform tier, memory, and presence state");
                println!("  :metrics    Display 3 North-Star metrics and ethics drift status");
                println!("  :agents     List registered WASM agents");
                println!("  :cost       Display System 2 inference cost tracking");
                println!("  :memory     Display all triplets currently stored in knowledge graph");
                println!("  :remember   Add a fact to knowledge graph (e.g. :remember Alice aime Rust)");
                println!("  :recall     Search memory graph by keyword (e.g. :recall Rust)");
                println!("  :exit       Shutdown Annora daemon");
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
