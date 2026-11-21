//! Mode `--serve` : serveur IPC local (pipe nommé Windows / socket Unix)
//! exposant le contrat `hakille-ipc` au HUD desktop et à la passerelle mobile, sans REPL.
//!
//! Architecture multi-clients asynchrone non-bloquante : acceptation immédiate via `tokio::spawn`,
//! bus de diffusion `tokio::sync::broadcast` pour les événements système (`PresenceState`, `ShowCard`,
//! `CardDecisionAck`, `Toast`) avec `id = 0`, et routage unicast strict des réponses vers le client
//! émetteur (`id = request.id`). Aucun succès simulé : chaque requête reçoit une Reply ou une
//! Error/Toast explicite, et toute erreur réelle du tour est propagée telle quelle.
//!
//! Extension additive v1 : le HUD peut consulter le journal d'audit, la vue
//! mémoire / le recall, mémoriser un fait et lire un statut minimal — toutes
//! les données proviennent des API réelles (journal, graphe, orchestrateurs,
//! compteurs), jamais d'une liste inventée.

use crate::persistence::{copy_outbox, replace_outbox, update_outbox};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use hakille_audit::entry::AuditEntry;
use hakille_audit::journal::AuditJournal;
use hakille_ipc::transport::{self, IpcListener};
use hakille_ipc::{ErrorCode, Frame, FrameBody};
use hakille_kernel::orchestrator::{ActionCardPreview, CognitiveOrchestrator};
use hakille_metrics::{BoundedInitiativeTracker, ContinuityTracker, GeneralityTracker};
use hakille_sync::{Outbox, OutboxEntry};
use hakille_ui::{
    CostCounter, MemoryViewerPanel, PresenceIndicator, PresenceState as DaemonPresenceState,
};
use hakille_ui_protocol::{
    ActionCard, AgentDto, DataExit, JournalEntryDto, KernelToUi, MemoryRowDto, Money,
    PresenceState as ProtoPresenceState, PrivacyGuardEvent, PrivacyGuardTrigger, Provenance,
    StatusDto, UiToKernel,
};
use tokio::io::{BufReader, BufWriter};
use tokio::sync::{broadcast, mpsc};

use crate::{
    audit_action_with_id, audit_memory_event, persist_memory_to, run_turn_core, turn_error_hint,
};

/// Plafond de lignes renvoyées par consultation (marge sous `MAX_FRAME_BYTES`
/// et réponse rapide — le HUD peut redemander).
const MAX_LIST_ROWS: usize = 200;

/// DTO d'une entrée d'audit : résumé lisible, jamais la trace complète.
fn journal_entry_dto(entry: &AuditEntry) -> JournalEntryDto {
    JournalEntryDto {
        id: entry.id.0.to_string(),
        timestamp: entry.timestamp.0.to_rfc3339(),
        action: entry.action.action_type.clone(),
        summary: entry.action.description.clone(),
    }
}

/// DTO d'une ligne mémoire via le formateur réel d'`hakille-ui` (même texte
/// que `:memory-view` — une seule vérité d'affichage).
fn memory_row_dto(triplet: &hakille_memory::Triplet) -> MemoryRowDto {
    let item = MemoryViewerPanel::format_triplet(triplet);
    MemoryRowDto {
        id: item.id.0.to_string(),
        text: item.text_representation,
        tags: vec![item.partition, item.trust_level],
        source: item.source,
    }
}

/// Parse honnête du texte libre de `remember` (même règle que le REPL) :
/// « sujet prédicat objet » si 3 parties, sinon fait générique.
fn remember_parts(text: &str) -> (String, &str, &str) {
    let parts: Vec<&str> = text.splitn(3, ' ').collect();
    if parts.len() == 3 {
        (parts[0].to_owned(), parts[1], parts[2])
    } else {
        (
            format!(
                "Note {}",
                hakille_memory::KnowledgeGraph::exact_uuid("hakille-note", text)
            ),
            "contenu",
            text,
        )
    }
}

/// État matériel et cognitif partagé piloté par `UiToKernel::PerceptionControl` (Règle 8).
#[derive(Debug)]
pub struct PerceptionControlState {
    pub hardware_camera_enabled: std::sync::atomic::AtomicBool,
    pub hardware_mic_enabled: std::sync::atomic::AtomicBool,
    pub cognitive_inference_enabled: std::sync::atomic::AtomicBool,
    pub active_channels_mask: std::sync::atomic::AtomicU32,
}

impl Default for PerceptionControlState {
    fn default() -> Self {
        Self {
            hardware_camera_enabled: std::sync::atomic::AtomicBool::new(true),
            hardware_mic_enabled: std::sync::atomic::AtomicBool::new(true),
            cognitive_inference_enabled: std::sync::atomic::AtomicBool::new(true),
            active_channels_mask: std::sync::atomic::AtomicU32::new(0xFF),
        }
    }
}

/// Vue mémoire : `limit` dernières lignes visibles dans la partition active
/// (exclut `TrustLevel::Untrusted` et les tombstones fanés ; tri par dernière activation décroissante).
fn memory_view_rows(
    graph: &hakille_memory::KnowledgeGraph,
    active_partition: &hakille_memory::ContextPartition,
    limit: usize,
) -> Vec<MemoryRowDto> {
    let mut triplets: Vec<&hakille_memory::Triplet> = graph
        .all_triplets()
        .into_iter()
        .filter(|t| {
            t.metadata.trust_level != hakille_core::TrustLevel::Untrusted
                && hakille_memory::context_partition::is_visible(
                    &t.metadata.context,
                    active_partition,
                    false,
                )
                && !hakille_memory::DecayEngine::is_faded(
                    t.metadata.weight,
                    hakille_memory::DEFAULT_COGNITIVE_CUTOFF_WEIGHT,
                )
        })
        .collect();
    triplets.sort_by(|a, b| {
        b.metadata
            .last_activated
            .0
            .cmp(&a.metadata.last_activated.0)
    });
    triplets
        .into_iter()
        .take(limit.min(MAX_LIST_ROWS))
        .map(memory_row_dto)
        .collect()
}

/// Recall : les chaînes réelles de `orchestrator.recall` sont re-mappées vers
/// leurs triplets d'origine (id/tags/source réels). Chaîne introuvable =
/// ligne signalée avec id vide — jamais d'identifiant inventé.
#[cfg(test)]
fn recall_rows(
    graph: &hakille_memory::KnowledgeGraph,
    recalled: &[String],
    limit: usize,
) -> Vec<MemoryRowDto> {
    let indexed: Vec<(String, MemoryRowDto)> = graph
        .all_triplets()
        .into_iter()
        .map(|triplet| {
            let row = memory_row_dto(triplet);
            (row.text.clone(), row)
        })
        .collect();
    recalled
        .iter()
        .take(limit.min(MAX_LIST_ROWS))
        .map(|text| {
            indexed
                .iter()
                .find(|(representation, _)| representation == text)
                .map(|(_, row)| row.clone())
                .unwrap_or_else(|| MemoryRowDto {
                    id: String::new(),
                    text: text.clone(),
                    tags: vec!["recall".into()],
                    source: "recall".into(),
                })
        })
        .collect()
}

/// Mappe le DTO du noyau vers le contrat UI. Aucun champ n'est inventé :
/// le coût vient du plan, la provenance est Hakille, les données restent
/// locales et la carte est irréversible par construction.
fn card_from_preview(id: u64, preview: ActionCardPreview) -> ActionCard {
    ActionCard {
        id,
        verb: preview.verb,
        object: preview.object,
        reason: preview.reason,
        cost: Money {
            micros: preview
                .cost_cents
                .min((i64::MAX as u64) / 10_000)
                .saturating_mul(10_000) as i64,
            currency: preview.currency,
        },
        data_exit: if preview.data_exit_local {
            DataExit::Local
        } else {
            DataExit::Cloud {
                provider: "non précisé".to_string(),
            }
        },
        reversible: preview.reversible,
        provenance: Provenance::Hakille,
    }
}

/// Durée minimale obligatoire entre la proposition d'une carte et son approbation
/// (maintien 600 ms imposé par le contrat `hakille-ui-protocol`).
pub const MIN_APPROVAL_HOLD: std::time::Duration = std::time::Duration::from_millis(600);
/// Durée de vie maximale d'une carte en attente (15 minutes).
pub const PENDING_CARD_TTL: std::time::Duration = std::time::Duration::from_secs(15 * 60);
/// Nombre maximal de cartes en attente simultanées.
pub const MAX_PENDING_CARDS: usize = 64;

/// Carte proposée par le démon, en attente de décision.
///
/// Le PLAN D'EXÉCUTION est conservé avec la carte : à l'approbation, le
/// démon doit savoir QUOI exécuter (et l'auditer), pas seulement afficher un
/// libellé.
#[derive(Debug, Clone)]
pub struct PendingCard {
    pub card: ActionCard,
    pub plan: hakille_kernel::planner::ExecutionPlan,
    pub proposed_at: Instant,
}

/// Gestionnaire centralisé et partagé des cartes d'action multi-clients.
#[derive(Debug, Default)]
pub struct ActionCardManager {
    pub pending: HashMap<u64, PendingCard>,
    pub next_id: u64,
}

impl ActionCardManager {
    pub fn new(initial_id: u64) -> Self {
        Self {
            pending: HashMap::new(),
            next_id: initial_id.max(1),
        }
    }

    /// Alloue une nouvelle carte sans l'insérer dans `pending` (permet d'auditer
    /// `ActionCardProposed` AVANT de rendre la carte approuvable).
    pub fn allocate_preview(&mut self, preview: ActionCardPreview) -> ActionCard {
        let card = card_from_preview(self.next_id, preview);
        self.next_id = self.next_id.saturating_add(1);
        card
    }

    pub fn purge_expired(&mut self) {
        self.pending
            .retain(|_, p| p.proposed_at.elapsed() <= PENDING_CARD_TTL);
    }

    #[cfg(test)]
    pub fn insert(
        &mut self,
        preview: ActionCardPreview,
        plan: hakille_kernel::planner::ExecutionPlan,
    ) -> ActionCard {
        let card = self.allocate_preview(preview);
        self.insert_card(card.clone(), plan);
        card
    }

    pub fn insert_card(&mut self, card: ActionCard, plan: hakille_kernel::planner::ExecutionPlan) {
        self.purge_expired();
        if self.pending.len() >= MAX_PENDING_CARDS
            && let Some(oldest_id) = self
                .pending
                .iter()
                .min_by_key(|(_, p)| p.proposed_at)
                .map(|(&id, _)| id)
        {
            self.pending.remove(&oldest_id);
        }
        let id = card.id;
        self.pending.insert(
            id,
            PendingCard {
                card,
                plan,
                proposed_at: Instant::now(),
            },
        );
        if id >= self.next_id {
            self.next_id = id.saturating_add(1);
        }
    }

    pub fn decide(
        &mut self,
        audit_journal: &AuditJournal,
        outbox: &mut Outbox,
        data_dir: &std::path::Path,
        card_id: u64,
        approved: bool,
    ) -> KernelToUi {
        self.purge_expired();
        decide_card(
            &mut self.pending,
            audit_journal,
            outbox,
            data_dir,
            card_id,
            approved,
        )
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    #[allow(dead_code)]
    pub fn get(&self, card_id: u64) -> Option<&PendingCard> {
        self.pending.get(&card_id)
    }
}

/// Décision de carte : l'audit signé est écrit AVANT le retrait de la carte
/// et avant l'accusé. En cas d'échec d'audit, la carte reste en attente et le
/// HUD reçoit un refus explicite.
///
/// EFFET RÉEL : une carte **approuvée** est mise en file dans l'outbox
/// durable (`<data_dir>/outbox.json`, file idempotente et revalidable). Ce
/// n'est PAS une exécution métier — les agents métier ne sont pas encore
/// exécutables — mais c'est un effet durable, visible (`:outbox`) et audité,
/// au lieu d'un « aucun effet » muet. Le refus, lui, ne produit aucun effet.
fn decide_card(
    pending_cards: &mut HashMap<u64, PendingCard>,
    audit_journal: &AuditJournal,
    outbox: &mut Outbox,
    data_dir: &std::path::Path,
    card_id: u64,
    approved: bool,
) -> KernelToUi {
    let Some(pending) = pending_cards.get(&card_id).cloned() else {
        return KernelToUi::Toast {
            text: format!("carte #{card_id} inconnue ou déjà traitée — aucune décision"),
        };
    };
    if pending.proposed_at.elapsed() > PENDING_CARD_TTL {
        pending_cards.remove(&card_id);
        return KernelToUi::Toast {
            text: format!("carte #{card_id} expirée (> 15 min) — veuillez relancer la demande"),
        };
    }
    if approved && pending.proposed_at.elapsed() < MIN_APPROVAL_HOLD {
        let elapsed_ms = pending.proposed_at.elapsed().as_millis();
        return KernelToUi::Toast {
            text: format!(
                "approbation de la carte #{card_id} refusée : maintien minimal de 600 ms requis ({elapsed_ms} ms écoulés)"
            ),
        };
    }
    let card = pending.card;
    let plan = pending.plan;
    let action_type = if approved {
        "ActionCardApproved"
    } else {
        "ActionCardRejected"
    };
    let verb = if approved { "approuvée" } else { "refusée" };

    // Effet réel, calculé AVANT l'audit : l'entrée de journal doit décrire ce
    // qui a réellement eu lieu, pas une intention. L'action mise en file
    // PORTE le plan approuvé (étapes + risque) : un exécuteur futur saura
    // quoi faire.
    let before = copy_outbox(outbox);
    let mut added_to_outbox = false;
    let (effect_kind, effect_detail) = if approved {
        let dedup_key = format!(
            "card#{}|{}|{}",
            card.id,
            card.verb.to_lowercase(),
            card.object.to_lowercase()
        );
        let steps = plan
            .steps
            .iter()
            .map(|s| s.description.clone())
            .collect::<Vec<_>>()
            .join(" → ");
        let queued_object = format!(
            "{} [plan {} : {} étape(s), risque {:?} : {}]",
            card.object,
            plan.plan_id,
            plan.steps.len(),
            plan.estimated_risk,
            steps
        );
        let entry = OutboxEntry::new(&card.verb, &queued_object, &dedup_key);
        let added = match update_outbox(data_dir, outbox, |q| q.enqueue(entry)) {
            Ok(added) => added,
            Err(error) => {
                return KernelToUi::Toast {
                    text: format!("décision non confirmée — persistance indisponible : {error}"),
                };
            }
        };
        added_to_outbox = added;
        if added {
            (
                "outbox_enqueued",
                Some(format!("action mise en file (clé {dedup_key})")),
            )
        } else {
            (
                "outbox_already_queued",
                Some(format!("action déjà en file (clé {dedup_key})")),
            )
        }
    } else {
        ("none", None)
    };

    let description = format!(
        "Carte #{} {verb} : {} {} (effet : {effect_kind})",
        card.id, card.verb, card.object
    );
    match audit_action_with_id(
        audit_journal,
        action_type,
        description,
        serde_json::json!({
            "card_id": card.id,
            "verb": card.verb,
            "object": card.object,
            "reversible": card.reversible,
            "effect": effect_kind,
            "plan_id": plan.plan_id,
            "plan_steps": plan.steps.len(),
            "plan_risk": format!("{:?}", plan.estimated_risk),
        }),
        false,
    ) {
        Ok(audit_id) => {
            pending_cards.remove(&card_id);
            KernelToUi::CardDecisionAck {
                card_id,
                approved,
                audit_id,
                effect: effect_detail,
            }
        }
        Err(e) => {
            if added_to_outbox && let Err(rollback) = replace_outbox(data_dir, outbox, before) {
                return KernelToUi::Toast {
                    text: format!(
                        "décision non confirmée — audit indisponible : {e} ; restauration de l'outbox non confirmée : {rollback}. Vérifier la file avant de réessayer."
                    ),
                };
            }
            KernelToUi::Toast {
                text: format!("décision refusée — audit indisponible: {e}"),
            }
        }
    }
}

/// Exécute un agent WASM réel et renvoie la réponse protocole.
///
/// Isolé du transport pour être testable directement (le contrat est le
/// même : sortie JSON réelle + empreintes SHA-256, jamais de sortie inventée).
fn run_agent_request(
    runtime: &mut crate::agent_runtime::ReferenceAgentRuntime,
    name: &str,
    input_json: &str,
) -> Result<KernelToUi, String> {
    let run = runtime.run(name, input_json).map_err(|e| e.to_string())?;
    Ok(KernelToUi::AgentRunReply {
        name: run.agent,
        output_json: run.output,
        duration_ms: run.duration_ms as u64,
        input_sha256: run.input_sha256,
        output_sha256: run.output_sha256,
    })
}

/// Liste honnête des agents exposés au HUD.
///
/// `artifact_present = true` UNIQUEMENT pour un agent réellement exécutable
/// (binaire WASM vérifié, hash connu). Les manifests métier déclaratifs sont
/// listés avec `artifact_present = false` : le HUD ne peut pas les présenter
/// comme opérationnels.
fn agent_dtos(
    runtime: Option<&crate::agent_runtime::ReferenceAgentRuntime>,
    registry: &hakille_agents::AgentRegistry,
) -> Vec<AgentDto> {
    let mut out: Vec<AgentDto> = registry
        .list_agents()
        .iter()
        .map(|m| AgentDto {
            id: m.id.clone(),
            name: m.name.clone(),
            required_caps: m
                .required_capabilities
                .iter()
                .map(|c| format!("{c:?}"))
                .collect(),
            artifact_present: false,
            sha256: String::new(),
        })
        .collect();
    if let Some(runtime) = runtime {
        for agent in runtime.agents() {
            out.push(AgentDto {
                id: agent.name.clone(),
                name: agent.name.clone(),
                required_caps: Vec::new(),
                artifact_present: true,
                sha256: agent.sha256.clone(),
            });
        }
    }
    out
}

///// Enveloppe pour les messages diffusés sur le bus broadcast.
#[derive(Clone, Debug)]
pub struct BroadcastEnvelope {
    /// Identifiant de la session ayant émis la commande à l'origine de ce message.
    /// Si Some(id), le client avec ce session_id a déjà reçu une réponse unicast
    /// dédiée (`frame.id = request.id`) et ignorera ce broadcast pour éviter les doublons.
    pub originating_session: Option<u64>,
    pub message: KernelToUi,
}

impl BroadcastEnvelope {
    pub fn all(message: KernelToUi) -> Self {
        Self {
            originating_session: None,
            message,
        }
    }

    pub fn peer_only(originating_session: u64, message: KernelToUi) -> Self {
        Self {
            originating_session: Some(originating_session),
            message,
        }
    }
}

/// Requête entrante transmise depuis une tâche de session cliente vers le coordinateur.
pub struct ClientSessionRequest {
    pub session_id: u64,
    pub frame: Frame,
    pub reply_tx: mpsc::Sender<Frame>,
}

/// Sert une session cliente individuelle dans une tâche asynchrone dédiée (`tokio::spawn`).
///
/// Maintient la connexion ouverte, écoute les trames entrantes et achemine les événements
/// diffusés sur le bus broadcast sans bloquer les autres clients.
async fn serve_client_session<S>(
    session_id: u64,
    mut stream: S,
    token: String,
    mut bcast_rx: broadcast::Receiver<BroadcastEnvelope>,
    request_tx: mpsc::Sender<ClientSessionRequest>,
) where
    S: tokio::io::AsyncBufRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    if let Err(e) = transport::server_handshake(&mut stream, &token).await {
        eprintln!("IPC [session {session_id}]: handshake refusé — {e}");
        return;
    }
    println!("IPC [session {session_id}]: client connecté");

    let (reader, writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);
    let mut writer = BufWriter::new(writer);

    let (session_outbound_tx, mut session_outbound_rx) = mpsc::channel::<Frame>(256);

    let writer_task = tokio::spawn(async move {
        while let Some(frame) = session_outbound_rx.recv().await {
            if let Err(e) = transport::write_frame(&mut writer, &frame).await {
                eprintln!("IPC [session {session_id}]: écriture socket échouée — {e}");
                break;
            }
        }
    });

    // Relais broadcast dans une tâche dédiée : `transport::read_frame` n'est
    // PAS annulation-sûr (et sous Windows, annuler une lecture sur un named
    // pipe `tokio::io::split` peut fermer la session prématurément). En
    // découplant le bus broadcast de la boucle de lecture, aucune trame
    // entrante n'est jamais interrompue en cours de lecture.
    let bcast_outbound_tx = session_outbound_tx.clone();
    let bcast_task = tokio::spawn(async move {
        loop {
            match bcast_rx.recv().await {
                Ok(envelope) => {
                    if envelope.originating_session == Some(session_id) {
                        continue;
                    }
                    // Les messages de broadcast ont TOUJOURS id = 0 pour respecter le contrat mobile et HUD
                    let frame = Frame::reply(0, envelope.message);
                    if bcast_outbound_tx.send(frame).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    eprintln!(
                        "IPC [session {session_id}]: bus broadcast décalé de {skipped} message(s)"
                    );
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    });

    loop {
        match transport::read_frame(&mut reader).await {
            Ok(Some(frame)) => {
                let client_req = ClientSessionRequest {
                    session_id,
                    frame,
                    reply_tx: session_outbound_tx.clone(),
                };
                if request_tx.send(client_req).await.is_err() {
                    break;
                }
            }
            Ok(None) => {
                // EOF propre — client déconnecté
                break;
            }
            Err(e) => {
                eprintln!("IPC [session {session_id}]: trame illisible — {e}");
                break;
            }
        }
    }

    println!("IPC [session {session_id}]: client déconnecté");
    bcast_task.abort();
    drop(session_outbound_tx);
    writer_task.abort();
}

/// Traite une requête cliente au niveau du coordinateur avec accès aux états protégés du démon.
#[allow(clippy::too_many_arguments)]
async fn handle_client_request(
    client_req: ClientSessionRequest,
    orchestrator: &mut CognitiveOrchestrator,
    presence: &mut PresenceIndicator,
    cost_counter: &mut CostCounter,
    continuity_tracker: &mut ContinuityTracker,
    bounded_initiative: &mut BoundedInitiativeTracker,
    generality_tracker: &mut GeneralityTracker,
    audit_journal: &mut AuditJournal,
    authority: &hakille_core::CapabilityAuthority,
    memory_path: &Path,
    outbox: &mut Outbox,
    data_dir: &Path,
    agent_runtime: Option<
        &std::sync::Arc<std::sync::Mutex<crate::agent_runtime::ReferenceAgentRuntime>>,
    >,
    agent_registry: &hakille_agents::AgentRegistry,
    media_manager: Option<&std::sync::Arc<hakille_media::MediaJobManager>>,
    card_manager: &mut ActionCardManager,
    perception_state: &PerceptionControlState,
    broadcast_tx: &broadcast::Sender<BroadcastEnvelope>,
    home_hub: Option<&std::sync::Arc<hakille_home::SovereignHomeHub>>,
) {
    let id = client_req.frame.id;
    let session_id = client_req.session_id;
    let reply_tx = client_req.reply_tx;

    match client_req.frame.body {
        FrameBody::Request {
            message: UiToKernel::HudSubmit { query },
        } => {
            if query.trim().is_empty() {
                eprintln!("IPC [session {session_id}]: requête vide refusée (id={id})");
                let _ = reply_tx
                    .send(Frame::error(
                        id,
                        ErrorCode::BadFrame,
                        "requête vide refusée",
                    ))
                    .await;
                return;
            }
            if !perception_state
                .cognitive_inference_enabled
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                let _ = reply_tx
                    .send(Frame::reply(
                        id,
                        KernelToUi::Toast {
                            text: "inférence cognitive désactivée par contrôle de perception (Règle 8)"
                                .into(),
                        },
                    ))
                    .await;
                return;
            }

            // Notifie l'attente réelle avant le tour (id 0) : envoi direct sur reply_tx
            // du client émetteur (ordre déterministe garanti avant la réponse finale)
            // et diffusion aux autres pairs connectés.
            presence.set_state(DaemonPresenceState::Acting);
            let _ = reply_tx
                .send(Frame::reply(
                    0,
                    KernelToUi::Presence(ProtoPresenceState::Acting),
                ))
                .await;
            let _ = broadcast_tx.send(BroadcastEnvelope::peer_only(
                session_id,
                KernelToUi::Presence(ProtoPresenceState::Acting),
            ));

            let started = Instant::now();
            let (delta_tx, mut delta_rx) = tokio::sync::mpsc::channel::<String>(256);
            let _stream_guard = hakille_inference::install_token_stream_callback(Box::new(
                move |event: &hakille_inference::TokenEvent| {
                    if !event.text_delta.is_empty() {
                        let _ = delta_tx.try_send(event.text_delta.clone());
                    }
                },
            ));

            let session_reply_tx = reply_tx.clone();
            let turn_result = {
                let turn = async {
                    run_turn_core(
                        orchestrator,
                        presence,
                        cost_counter,
                        continuity_tracker,
                        bounded_initiative,
                        generality_tracker,
                        audit_journal,
                        authority,
                        memory_path,
                        &query,
                    )
                    .await
                    .map_err(|e| e.to_string())
                };
                tokio::pin!(turn);
                let mut delta_open = true;
                loop {
                    tokio::select! {
                        result = &mut turn => break result,
                        maybe_delta = delta_rx.recv(), if delta_open => {
                            let Some(delta) = maybe_delta else { delta_open = false; continue };
                            // STREAMING : Unicast EXCLUSIVEMENT au client émetteur avec id = 0
                            let frame = Frame::reply(0, KernelToUi::TokenDelta { text: delta });
                            if session_reply_tx.send(frame).await.is_err() {
                                eprintln!("IPC [session {session_id}]: delta non écrit (id={id})");
                                break Err("écriture du flux impossible".to_string());
                            }
                        }
                    }
                }
            };
            drop(_stream_guard);

            let mut card_proposed = false;
            match turn_result {
                Ok(outcome) => {
                    let latency_ms = started.elapsed().as_millis();
                    println!(
                        "IPC [session {session_id}]: requête servie (id={id}, {latency_ms}ms, coût {}¢)",
                        outcome.cost_cents
                    );

                    if (outcome.routing_used.starts_with("Plan/Dry-run")
                        || outcome.routing_used == "MediaCloudCard")
                        && let Some((preview, plan)) =
                            CognitiveOrchestrator::action_card_with_plan(&query)
                    {
                        let card = card_manager.allocate_preview(preview);
                        let proposed = audit_action_with_id(
                            audit_journal,
                            "ActionCardProposed",
                            format!(
                                "Carte #{} proposée : {} {} (plan sec, aucune exécution)",
                                card.id, card.verb, card.object
                            ),
                            serde_json::json!({
                                "card_id": card.id,
                                "verb": card.verb,
                                "object": card.object,
                                "reversible": card.reversible,
                            }),
                            true,
                        );
                        match proposed {
                            Ok(_) => {
                                card_manager.insert_card(card.clone(), plan);
                                card_proposed = true;
                                presence.set_state(DaemonPresenceState::DecisionRequired);
                                println!(
                                    "IPC: carte #{} proposée (id={id}, {} en attente)",
                                    card.id,
                                    card_manager.pending_count()
                                );
                                // Envoi direct sur reply_tx (garantit que ShowCard précède HudReply
                                // et tout StatusReply de barrière sur la session émettrice) + diffusion aux pairs.
                                let _ = reply_tx
                                    .send(Frame::reply(
                                        0,
                                        KernelToUi::Presence(ProtoPresenceState::DecisionRequired),
                                    ))
                                    .await;
                                let _ = reply_tx
                                    .send(Frame::reply(0, KernelToUi::ShowCard(card.clone())))
                                    .await;
                                let _ = broadcast_tx.send(BroadcastEnvelope::peer_only(
                                    session_id,
                                    KernelToUi::Presence(ProtoPresenceState::DecisionRequired),
                                ));
                                let _ = broadcast_tx.send(BroadcastEnvelope::peer_only(
                                    session_id,
                                    KernelToUi::ShowCard(card),
                                ));
                            }
                            Err(e) => {
                                eprintln!(
                                    "IPC: carte non présentée (id={id}) — audit indisponible: {e}"
                                );
                                let toast = KernelToUi::Toast {
                                    text: format!("carte non présentée — audit indisponible: {e}"),
                                };
                                let _ = reply_tx.send(Frame::reply(0, toast.clone())).await;
                                let _ = broadcast_tx
                                    .send(BroadcastEnvelope::peer_only(session_id, toast));
                            }
                        }
                    }

                    if !card_proposed {
                        presence.set_state(DaemonPresenceState::Idle);
                        let _ = reply_tx
                            .send(Frame::reply(
                                0,
                                KernelToUi::Presence(ProtoPresenceState::Idle),
                            ))
                            .await;
                        let _ = broadcast_tx.send(BroadcastEnvelope::peer_only(
                            session_id,
                            KernelToUi::Presence(ProtoPresenceState::Idle),
                        ));
                    }

                    let response = if outcome.non_empty {
                        let validation = match &outcome.persistence_warning {
                            Some(warning) => Some((format!("⚠ {warning}"), outcome.cost_cents)),
                            None => (outcome.cost_cents > 0)
                                .then(|| ("coût tour".into(), outcome.cost_cents)),
                        };
                        Frame::reply(
                            id,
                            KernelToUi::HudReply {
                                response: outcome.response,
                                validation,
                            },
                        )
                    } else {
                        eprintln!("IPC: réponse vide (id={id}) — aucun succès simulé");
                        Frame::reply(
                            id,
                            KernelToUi::Toast {
                                text: "réponse vide (EOS immédiat) — aucun succès simulé".into(),
                            },
                        )
                    };
                    let _ = reply_tx.send(response).await;
                }
                Err(err_str) => {
                    let text = match turn_error_hint(&err_str) {
                        Some(hint) => format!("{err_str} — {hint}"),
                        None => err_str,
                    };
                    eprintln!("IPC: tour échoué (id={id}) — {text}");
                    presence.set_state(DaemonPresenceState::Idle);
                    let _ = reply_tx
                        .send(Frame::reply(
                            0,
                            KernelToUi::Presence(ProtoPresenceState::Idle),
                        ))
                        .await;
                    let _ = broadcast_tx.send(BroadcastEnvelope::peer_only(
                        session_id,
                        KernelToUi::Presence(ProtoPresenceState::Idle),
                    ));
                    let _ = reply_tx
                        .send(Frame::reply(id, KernelToUi::Toast { text }))
                        .await;
                }
            }
        }
        FrameBody::Request {
            message: UiToKernel::Approve { card_id },
        } => {
            let reply = card_manager.decide(audit_journal, outbox, data_dir, card_id, true);
            match &reply {
                KernelToUi::CardDecisionAck {
                    audit_id, effect, ..
                } => {
                    if card_manager.is_empty() {
                        presence.set_state(DaemonPresenceState::Idle);
                    }
                    println!(
                        "IPC [session {session_id}]: carte #{card_id} approuvée — décision auditée [{audit_id}] — {}",
                        effect.as_deref().unwrap_or("aucun effet")
                    );
                    // Unicast pour le client décideur avec son request.id
                    let _ = reply_tx.send(Frame::reply(id, reply.clone())).await;
                    // Broadcast pour les pairs (Desktop HUD, etc.) avec id = 0 pour refermer la carte
                    let _ = broadcast_tx.send(BroadcastEnvelope::peer_only(session_id, reply));
                    if card_manager.is_empty() {
                        let _ = reply_tx
                            .send(Frame::reply(
                                0,
                                KernelToUi::Presence(ProtoPresenceState::Idle),
                            ))
                            .await;
                        let _ = broadcast_tx.send(BroadcastEnvelope::peer_only(
                            session_id,
                            KernelToUi::Presence(ProtoPresenceState::Idle),
                        ));
                    }
                }
                KernelToUi::Toast { text } => {
                    eprintln!(
                        "IPC [session {session_id}]: approbation carte #{card_id} refusée — {text}"
                    );
                    let _ = reply_tx.send(Frame::reply(id, reply)).await;
                }
                _ => {
                    let _ = reply_tx.send(Frame::reply(id, reply)).await;
                }
            }
        }
        FrameBody::Request {
            message: UiToKernel::Reject { card_id },
        } => {
            let reply = card_manager.decide(audit_journal, outbox, data_dir, card_id, false);
            match &reply {
                KernelToUi::CardDecisionAck { audit_id, .. } => {
                    if card_manager.is_empty() {
                        presence.set_state(DaemonPresenceState::Idle);
                    }
                    println!(
                        "IPC [session {session_id}]: carte #{card_id} refusée — décision auditée [{audit_id}] (aucun effet)"
                    );
                    // Unicast pour le client émetteur
                    let _ = reply_tx.send(Frame::reply(id, reply.clone())).await;
                    // Broadcast pour les pairs pour refermer la carte
                    let _ = broadcast_tx.send(BroadcastEnvelope::peer_only(session_id, reply));
                    if card_manager.is_empty() {
                        let _ = reply_tx
                            .send(Frame::reply(
                                0,
                                KernelToUi::Presence(ProtoPresenceState::Idle),
                            ))
                            .await;
                        let _ = broadcast_tx.send(BroadcastEnvelope::peer_only(
                            session_id,
                            KernelToUi::Presence(ProtoPresenceState::Idle),
                        ));
                    }
                }
                KernelToUi::Toast { text } => {
                    eprintln!(
                        "IPC [session {session_id}]: refus carte #{card_id} non enregistré — {text}"
                    );
                    let _ = reply_tx.send(Frame::reply(id, reply)).await;
                }
                _ => {
                    let _ = reply_tx.send(Frame::reply(id, reply)).await;
                }
            }
        }
        FrameBody::Request {
            message: UiToKernel::Edit { card_id },
        } => {
            eprintln!(
                "IPC [session {session_id}]: édition carte #{card_id} non supportée — aucune mutation, carte conservée"
            );
            let _ = reply_tx
                .send(Frame::reply(
                    id,
                    KernelToUi::Toast {
                        text: "modification de carte non supportée — aucune mutation ; validez ou refusez"
                            .into(),
                    },
                ))
                .await;
        }
        FrameBody::Request {
            message: UiToKernel::JournalRequest { limit },
        } => {
            let response = match audit_journal.all_entries() {
                Ok(entries) => {
                    let listed: Vec<JournalEntryDto> = entries
                        .iter()
                        .rev()
                        .take(limit.min(MAX_LIST_ROWS))
                        .map(journal_entry_dto)
                        .collect();
                    println!(
                        "IPC [session {session_id}]: journal servi (id={id}, {} entrées)",
                        listed.len()
                    );
                    Frame::reply(id, KernelToUi::JournalReply { entries: listed })
                }
                Err(e) => {
                    eprintln!("IPC [session {session_id}]: journal indisponible (id={id}) — {e}");
                    Frame::error(
                        id,
                        ErrorCode::Internal,
                        format!("journal indisponible: {e}"),
                    )
                }
            };
            let _ = reply_tx.send(response).await;
        }
        FrameBody::Request {
            message: UiToKernel::MemoryRequest { query, limit },
        } => {
            let response = match query {
                Some(q) if q.trim().is_empty() => {
                    eprintln!("IPC [session {session_id}]: rappel vide refusé (id={id})");
                    Frame::error(id, ErrorCode::BadFrame, "rappel vide refusé")
                }
                Some(q) => match orchestrator.recall_evidence(q.trim()) {
                    Ok(report) => {
                        let rows = report
                            .hits
                            .iter()
                            .take(limit.min(MAX_LIST_ROWS))
                            .filter_map(|hit| orchestrator.memory_graph.get(&hit.id))
                            .map(memory_row_dto)
                            .collect();
                        Frame::reply(id, KernelToUi::MemoryReply { rows })
                    }
                    Err(error) => Frame::reply(
                        id,
                        KernelToUi::Toast {
                            text: format!("Rappel impossible : {error}"),
                        },
                    ),
                },
                None => {
                    let rows = memory_view_rows(
                        &orchestrator.memory_graph,
                        &orchestrator.recall_policy.partition,
                        limit,
                    );
                    println!(
                        "IPC [session {session_id}]: mémoire servie (id={id}, {} lignes)",
                        rows.len()
                    );
                    Frame::reply(id, KernelToUi::MemoryReply { rows })
                }
            };
            let _ = reply_tx.send(response).await;
        }
        FrameBody::Request {
            message: UiToKernel::MemoryRemember { text },
        } => {
            if text.trim().is_empty() {
                eprintln!("IPC [session {session_id}]: mémoire vide refusée (id={id})");
                let _ = reply_tx
                    .send(Frame::error(
                        id,
                        ErrorCode::BadFrame,
                        "mémoire vide refusée",
                    ))
                    .await;
            } else {
                let trimmed = text.trim().to_string();
                if let Err(e) = audit_memory_event(
                    audit_journal,
                    "MemoryRemember",
                    format!("Demande de mémorisation via HUD: '{trimmed}'"),
                    serde_json::json!({ "text": trimmed, "origin": "ipc" }),
                ) {
                    eprintln!(
                        "IPC [session {session_id}]: audit mémoire impossible (id={id}) — {e}"
                    );
                    let _ = reply_tx
                        .send(Frame::reply(
                            id,
                            KernelToUi::Toast {
                                text: format!("mémorisation refusée — audit indisponible: {e}"),
                            },
                        ))
                        .await;
                } else {
                    let (subject, predicate, object) = remember_parts(&trimmed);
                    let result = orchestrator.remember(&subject, predicate, object);
                    let ack = |triplet_id: &hakille_core::TripletId,
                               persisted: &Result<(), String>| {
                        let id_text = triplet_id.0.to_string();
                        match persisted {
                            Ok(()) => {
                                println!(
                                    "IPC [session {session_id}]: mémoire enregistrée et persistée (id={id})"
                                );
                                KernelToUi::MemoryRememberAck { id: id_text }
                            }
                            Err(e) => {
                                eprintln!(
                                    "IPC [session {session_id}]: mémoire en RAM non persistée (id={id}) — {e}"
                                );
                                KernelToUi::Toast {
                                    text: format!(
                                        "fait [{id_text}] en mémoire volatile, persistance échouée: {e}"
                                    ),
                                }
                            }
                        }
                    };
                    let response = match result {
                        hakille_memory::InsertResult::Inserted(triplet_id) => {
                            let persisted = persist_memory_to(orchestrator, memory_path);
                            Frame::reply(id, ack(&triplet_id, &persisted))
                        }
                        hakille_memory::InsertResult::ContradictionResolved { new, .. } => {
                            let persisted = persist_memory_to(orchestrator, memory_path);
                            println!(
                                "IPC [session {session_id}]: contradiction résolue (id={id}, remplacement tracé)"
                            );
                            Frame::reply(id, ack(&new, &persisted))
                        }
                        hakille_memory::InsertResult::NeedsUserResolution {
                            existing,
                            incoming,
                        } => {
                            let persistence = persist_memory_to(orchestrator, memory_path);
                            eprintln!(
                                "IPC [session {session_id}]: contradiction — décision utilisateur requise (id={id})"
                            );
                            Frame::reply(
                                id,
                                KernelToUi::Toast {
                                    text: format!(
                                        "contradiction — décision requise : existant [{}] vs entrant [{}] ; rien écrasé{}",
                                        existing.id.0,
                                        incoming.id.0,
                                        persistence
                                            .err()
                                            .map(|e| format!(" ; sauvegarde échouée : {e}"))
                                            .unwrap_or_default()
                                    ),
                                },
                            )
                        }
                    };
                    let _ = reply_tx.send(response).await;
                }
            }
        }
        FrameBody::Request {
            message: UiToKernel::StatusRequest,
        } => {
            let model_label = if orchestrator.system1.has_inference_engine() {
                format!(
                    "Qwen 0.5B Q4_K_M (timeout {} ms)",
                    orchestrator.system1.llm_timeout_ms()
                )
            } else {
                format!(
                    "Qwen 0.5B Q4_K_M [non chargé] (timeout {} ms)",
                    orchestrator.system1.llm_timeout_ms()
                )
            };
            let status = StatusDto {
                model: model_label,
                cost: cost_counter.formatted_total(),
                state: format!("{:?}", presence.state()),
                memory_count: orchestrator.memory_graph.len() as u64,
            };
            println!("IPC [session {session_id}]: statut servi (id={id})");
            let _ = reply_tx
                .send(Frame::reply(id, KernelToUi::StatusReply { status }))
                .await;
        }
        FrameBody::Request {
            message: UiToKernel::AgentListRequest,
        } => {
            let agents = {
                let guard = agent_runtime.and_then(|r| r.lock().ok());
                agent_dtos(guard.as_deref(), agent_registry)
            };
            println!(
                "IPC [session {session_id}]: agents listés (id={id}, {} agent(s))",
                agents.len()
            );
            let _ = reply_tx
                .send(Frame::reply(id, KernelToUi::AgentListReply { agents }))
                .await;
        }
        FrameBody::Request {
            message: UiToKernel::AgentRunRequest { name, input_json },
        } => {
            let agent_name = name.clone();
            let input = input_json.clone();
            if let Err(e) = audit_action_with_id(
                audit_journal,
                "WasmAgentRunRequested",
                format!("Demande d'exécution d'agent WASM : {agent_name}"),
                serde_json::json!({
                    "agent": agent_name,
                    "session_id": session_id,
                    "input_bytes": input.len(),
                }),
                false,
            ) {
                let _ = reply_tx
                    .send(Frame::reply(
                        id,
                        KernelToUi::Toast {
                            text: format!("exécution d'agent refusée — audit indisponible: {e}"),
                        },
                    ))
                    .await;
                return;
            }
            let reply = match agent_runtime {
                Some(runtime) => {
                    let runtime = std::sync::Arc::clone(runtime);
                    let outcome = tokio::task::spawn_blocking(move || {
                        let mut guard = runtime
                            .lock()
                            .map_err(|_| "verrou du runtime d'agents empoisonné".to_string())?;
                        run_agent_request(&mut guard, &agent_name, &input)
                    })
                    .await;
                    match outcome {
                        Ok(Ok(KernelToUi::AgentRunReply {
                            name: agent,
                            output_json,
                            duration_ms,
                            input_sha256,
                            output_sha256,
                        })) => {
                            let audit = audit_action_with_id(
                                audit_journal,
                                "WasmAgentRunCompleted",
                                format!("Agent {agent} exécuté ({duration_ms} ms)"),
                                serde_json::json!({
                                    "agent": agent,
                                    "duration_ms": duration_ms,
                                    "input_sha256": input_sha256,
                                    "output_sha256": output_sha256,
                                }),
                                false,
                            );
                            match audit {
                                Ok(audit_id) => {
                                    println!(
                                        "IPC [session {session_id}]: agent {agent} exécuté ({duration_ms} ms) — audit [{audit_id}]"
                                    );
                                    KernelToUi::AgentRunReply {
                                        name: agent,
                                        output_json,
                                        duration_ms,
                                        input_sha256,
                                        output_sha256,
                                    }
                                }
                                Err(e) => {
                                    eprintln!(
                                        "IPC [session {session_id}]: agent {agent} exécuté mais audit indisponible: {e}"
                                    );
                                    KernelToUi::Toast {
                                        text: format!(
                                            "agent {agent} exécuté mais audit indisponible: {e} — résultat non accusé"
                                        ),
                                    }
                                }
                            }
                        }
                        Ok(Ok(other)) => other,
                        Ok(Err(e)) => {
                            let _ = audit_action_with_id(
                                audit_journal,
                                "WasmAgentRunFailed",
                                format!("Échec d'exécution de l'agent {name}"),
                                serde_json::json!({
                                    "agent": name,
                                    "error": e,
                                }),
                                false,
                            );
                            eprintln!(
                                "IPC [session {session_id}]: exécution d'agent échouée ({name}) — {e}"
                            );
                            KernelToUi::Toast {
                                text: format!("agent {name} : {e}"),
                            }
                        }
                        Err(e) => {
                            eprintln!(
                                "IPC [session {session_id}]: tâche d'exécution d'agent interrompue — {e}"
                            );
                            KernelToUi::Toast {
                                text: format!("exécution d'agent interrompue : {e}"),
                            }
                        }
                    }
                }
                None => {
                    eprintln!(
                        "IPC [session {session_id}]: exécution d'agent refusée (id={id}) — runtime indisponible"
                    );
                    KernelToUi::Toast {
                        text: "runtime d'agents indisponible — aucun agent exécuté".into(),
                    }
                }
            };
            let _ = reply_tx.send(Frame::reply(id, reply)).await;
        }
        FrameBody::Request {
            message:
                UiToKernel::PerceptionControl {
                    hardware_camera_enabled,
                    hardware_mic_enabled,
                    cognitive_inference_enabled,
                    active_channels_mask,
                },
        } => {
            let audit_id = match audit_action_with_id(
                audit_journal,
                "PerceptionControlUpdated",
                format!(
                    "Contrôle perception : caméra={}, micro={}, inférence={}, masque={:#010b}",
                    hardware_camera_enabled,
                    hardware_mic_enabled,
                    cognitive_inference_enabled,
                    active_channels_mask
                ),
                serde_json::json!({
                    "hardware_camera_enabled": hardware_camera_enabled,
                    "hardware_mic_enabled": hardware_mic_enabled,
                    "cognitive_inference_enabled": cognitive_inference_enabled,
                    "active_channels_mask": active_channels_mask,
                }),
                false,
            ) {
                Ok(audit_id) => audit_id,
                Err(e) => {
                    let _ = reply_tx
                        .send(Frame::reply(
                            id,
                            KernelToUi::Toast {
                                text: format!(
                                    "contrôle de perception refusé — audit indisponible: {e}"
                                ),
                            },
                        ))
                        .await;
                    return;
                }
            };

            perception_state
                .hardware_camera_enabled
                .store(hardware_camera_enabled, std::sync::atomic::Ordering::SeqCst);
            perception_state
                .hardware_mic_enabled
                .store(hardware_mic_enabled, std::sync::atomic::Ordering::SeqCst);
            perception_state.cognitive_inference_enabled.store(
                cognitive_inference_enabled,
                std::sync::atomic::Ordering::SeqCst,
            );
            perception_state
                .active_channels_mask
                .store(active_channels_mask, std::sync::atomic::Ordering::SeqCst);

            // Si coupure logicielle des flux de perception, diffusion de l'événement PrivacyGuardAlert
            if !hardware_camera_enabled || !hardware_mic_enabled {
                let device = match (!hardware_camera_enabled, !hardware_mic_enabled) {
                    (true, true) => "camera_and_microphone".to_string(),
                    (true, false) => "camera".to_string(),
                    (false, true) => "microphone".to_string(),
                    _ => "none".to_string(),
                };
                let guard_event = PrivacyGuardEvent {
                    trigger: PrivacyGuardTrigger::HardwareKillSwitchActivated {
                        device: device.clone(),
                    },
                    description: format!("Coupure des flux de perception activée pour : {device}"),
                    recommended_action:
                        "Flux de capteurs bloqués par le noyau (Règle 8 souveraineté)".to_string(),
                    timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
                };
                let _ = broadcast_tx.send(BroadcastEnvelope::all(KernelToUi::PrivacyGuardAlert(
                    guard_event,
                )));
            }

            println!(
                "IPC [session {session_id}]: PerceptionControl (id={id}, cam={hardware_camera_enabled}, mic={hardware_mic_enabled}) — audit [{audit_id}]"
            );
            let reply = KernelToUi::Toast {
                text: format!(
                    "Contrôle de perception mis à jour (caméra: {}, micro: {}, inférence: {})",
                    if hardware_camera_enabled { "on" } else { "off" },
                    if hardware_mic_enabled { "on" } else { "off" },
                    if cognitive_inference_enabled {
                        "on"
                    } else {
                        "off"
                    }
                ),
            };
            let _ = reply_tx.send(Frame::reply(id, reply)).await;
        }
        FrameBody::Request {
            message:
                UiToKernel::NotificationAction {
                    id: notif_id,
                    action_id,
                },
        } => {
            let audit_id = match audit_action_with_id(
                audit_journal,
                "NotificationActionTaken",
                format!("Action '{action_id}' exécutée sur la notification '{notif_id}'"),
                serde_json::json!({
                    "notification_id": notif_id,
                    "action_id": action_id,
                    "session_id": session_id,
                }),
                true,
            ) {
                Ok(audit_id) => audit_id,
                Err(e) => {
                    let _ = reply_tx
                        .send(Frame::reply(
                            id,
                            KernelToUi::Toast {
                                text: format!(
                                    "action de notification refusée — audit indisponible: {e}"
                                ),
                            },
                        ))
                        .await;
                    return;
                }
            };
            println!(
                "IPC [session {session_id}]: NotificationAction (id={id}, notif={notif_id}, action={action_id}) — audit [{audit_id}]"
            );

            // Diffusion de la fermeture de la notification à tous les clients UI
            let _ = broadcast_tx.send(BroadcastEnvelope::all(KernelToUi::NotificationDismissed {
                id: notif_id.clone(),
            }));

            let reply = KernelToUi::Toast {
                text: format!("Action '{action_id}' validée"),
            };
            let _ = reply_tx.send(Frame::reply(id, reply)).await;
        }
        FrameBody::Request {
            message: UiToKernel::NotificationDismiss { id: notif_id },
        } => {
            let audit_id = match audit_action_with_id(
                audit_journal,
                "NotificationDismissed",
                format!("Notification '{notif_id}' acquittée manuellement"),
                serde_json::json!({
                    "notification_id": notif_id,
                    "session_id": session_id,
                }),
                true,
            ) {
                Ok(audit_id) => audit_id,
                Err(e) => {
                    let _ = reply_tx
                        .send(Frame::reply(
                            id,
                            KernelToUi::Toast {
                                text: format!(
                                    "acquittement de notification refusé — audit indisponible: {e}"
                                ),
                            },
                        ))
                        .await;
                    return;
                }
            };
            println!(
                "IPC [session {session_id}]: NotificationDismiss (id={id}, notif={notif_id}) — audit [{audit_id}]"
            );

            // Diffusion de la révocation aux autres interfaces
            let _ = broadcast_tx.send(BroadcastEnvelope::all(KernelToUi::NotificationDismissed {
                id: notif_id.clone(),
            }));

            let reply = KernelToUi::Toast {
                text: "Notification masquée".into(),
            };
            let _ = reply_tx.send(Frame::reply(id, reply)).await;
        }
        FrameBody::Request {
            message: UiToKernel::MediaEnginesRequest,
        } => {
            let (engines, ffmpeg_available) = match media_manager {
                Some(mgr) => (
                    mgr.list_engines()
                        .iter()
                        .map(crate::media_cli::engine_descriptor_to_dto)
                        .collect(),
                    mgr.ffmpeg_available(),
                ),
                None => (Vec::new(), false),
            };
            println!(
                "IPC [session {session_id}]: moteurs média servis (id={id}, {} moteur(s), ffmpeg={ffmpeg_available})",
                engines.len()
            );
            let _ = reply_tx
                .send(Frame::reply(
                    id,
                    KernelToUi::MediaEnginesReply {
                        engines,
                        ffmpeg_available,
                    },
                ))
                .await;
        }
        FrameBody::Request {
            message: UiToKernel::MediaGalleryRequest { limit },
        } => {
            let reply = match media_manager {
                Some(mgr) => match mgr.store().list(limit.min(MAX_LIST_ROWS)) {
                    Ok(items) => KernelToUi::MediaGalleryReply {
                        artifacts: items
                            .iter()
                            .map(crate::media_cli::artifact_to_dto)
                            .collect(),
                    },
                    Err(e) => KernelToUi::Toast {
                        text: format!("galerie média illisible : {e}"),
                    },
                },
                None => KernelToUi::MediaGalleryReply {
                    artifacts: Vec::new(),
                },
            };
            let _ = reply_tx.send(Frame::reply(id, reply)).await;
        }
        FrameBody::Request {
            message: UiToKernel::HomeDevicesRequest,
        } => {
            let reply = match home_hub {
                Some(hub) => {
                    let (devices, energy, protocols) =
                        crate::home_cli::collect_home_snapshot(hub).await;
                    println!(
                        "IPC [session {session_id}]: domotique servie (id={id}, {} appareil(s), {} protocole(s))",
                        devices.len(),
                        protocols.len()
                    );
                    KernelToUi::HomeDevicesReply {
                        devices,
                        energy,
                        protocols,
                    }
                }
                None => KernelToUi::Toast {
                    text: "SovereignHomeHub indisponible dans cette session".into(),
                },
            };
            let _ = reply_tx.send(Frame::reply(id, reply)).await;
        }
        FrameBody::Request {
            message: UiToKernel::MediaJobCancel { job_id },
        } => {
            let text = match media_manager {
                Some(mgr) if mgr.cancel_job(&job_id) => {
                    format!("Annulation demandée pour le job média '{job_id}'")
                }
                Some(_) => format!("Aucun job média actif avec l'identifiant '{job_id}'"),
                None => "Moteur média indisponible sur cette session".into(),
            };
            let _ = reply_tx
                .send(Frame::reply(id, KernelToUi::Toast { text }))
                .await;
        }
        FrameBody::Request {
            message: UiToKernel::MediaGenerateRequest(req),
        } => {
            if !perception_state
                .cognitive_inference_enabled
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                let _ = reply_tx
                    .send(Frame::reply(
                        id,
                        KernelToUi::MediaJobFailed {
                            job_id: "perception-blocked".into(),
                            modality: req.modality,
                            error: "inférence cognitive désactivée par contrôle de perception (Règle 8)"
                                .into(),
                        },
                    ))
                    .await;
                return;
            }
            let Some(mgr) = media_manager else {
                let _ = reply_tx
                    .send(Frame::reply(
                        id,
                        KernelToUi::MediaJobFailed {
                            job_id: "unavailable".into(),
                            modality: req.modality,
                            error: "moteur génératif média non initialisé sur le démon".into(),
                        },
                    ))
                    .await;
                return;
            };
            if req.prompt.trim().is_empty() {
                let _ = reply_tx
                    .send(Frame::error(
                        id,
                        ErrorCode::BadFrame,
                        "prompt média vide refusé",
                    ))
                    .await;
                return;
            }
            if let Err(e) = audit_action_with_id(
                audit_journal,
                "MediaGenerationRequested",
                format!(
                    "Génération média IPC ({:?}, moteur={})",
                    req.modality, req.engine
                ),
                serde_json::json!({
                    "session_id": session_id,
                    "modality": format!("{:?}", req.modality),
                    "engine": req.engine,
                }),
                false,
            ) {
                let _ = reply_tx
                    .send(Frame::reply(
                        id,
                        KernelToUi::MediaJobFailed {
                            job_id: "audit-blocked".into(),
                            modality: req.modality,
                            error: format!("audit indisponible — génération refusée : {e}"),
                        },
                    ))
                    .await;
                return;
            }

            presence.set_state(DaemonPresenceState::Acting);
            let _ = broadcast_tx.send(BroadcastEnvelope::all(KernelToUi::Presence(
                ProtoPresenceState::Acting,
            )));

            let mut progress_rx = mgr.subscribe_progress();
            let session_reply_tx = reply_tx.clone();
            let gen_result = {
                let fut = crate::media_cli::execute_ipc_generate_request(mgr, &req);
                tokio::pin!(fut);
                let mut progress_open = true;
                loop {
                    tokio::select! {
                        res = &mut fut => break res,
                        ev = progress_rx.recv(), if progress_open => {
                            match ev {
                                Ok(p) => {
                                    let dto = crate::media_cli::progress_to_dto(&p);
                                    let _ = session_reply_tx
                                        .send(Frame::reply(0, KernelToUi::MediaJobProgress(dto)))
                                        .await;
                                }
                                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                                Err(broadcast::error::RecvError::Closed) => progress_open = false,
                            }
                        }
                    }
                }
            };

            presence.set_state(DaemonPresenceState::Idle);
            let _ = broadcast_tx.send(BroadcastEnvelope::all(KernelToUi::Presence(
                ProtoPresenceState::Idle,
            )));

            let reply = match gen_result {
                Ok(artifact) => {
                    let subject = format!("media:{}", artifact.id);
                    let object = format!(
                        "{:?} [{}] → {} (sha256:{})",
                        artifact.modality,
                        artifact.engine_kind.badge_fr(),
                        artifact.path.display(),
                        &artifact.sha256[..artifact.sha256.len().min(16)]
                    );
                    let _ = orchestrator.remember(&subject, "generated_media", &object);
                    let _ = persist_memory_to(orchestrator, memory_path);
                    let _ = audit_action_with_id(
                        audit_journal,
                        "MediaGenerationCompleted",
                        format!(
                            "Artefact média IPC généré ({:?}, {})",
                            artifact.modality, artifact.engine_id
                        ),
                        serde_json::json!({
                            "artifact_id": artifact.id.to_string(),
                            "modality": format!("{:?}", artifact.modality),
                            "engine_id": artifact.engine_id,
                            "engine_badge": artifact.engine_kind.badge_fr(),
                            "sha256": artifact.sha256,
                            "path": artifact.path.display().to_string(),
                            "provenance_path": artifact.provenance_path.display().to_string(),
                            "elapsed_ms": artifact.elapsed_ms,
                        }),
                        false,
                    );
                    println!(
                        "IPC [session {session_id}]: média généré (id={id}, {:?}, {}, {} ms)",
                        artifact.modality, artifact.engine_id, artifact.elapsed_ms
                    );
                    KernelToUi::MediaJobCompleted {
                        artifact: crate::media_cli::artifact_to_dto(&artifact),
                    }
                }
                Err(error) => {
                    let _ = audit_action_with_id(
                        audit_journal,
                        "MediaGenerationFailed",
                        format!("Échec génération média IPC ({:?})", req.modality),
                        serde_json::json!({
                            "modality": format!("{:?}", req.modality),
                            "error": error,
                        }),
                        false,
                    );
                    eprintln!(
                        "IPC [session {session_id}]: génération média échouée (id={id}) — {error}"
                    );
                    KernelToUi::MediaJobFailed {
                        job_id: format!("ipc-{id}"),
                        modality: req.modality,
                        error,
                    }
                }
            };
            let _ = reply_tx.send(Frame::reply(id, reply)).await;
        }
        FrameBody::Request {
            message: UiToKernel::HomeDiscoverRequest { protocol },
        } => {
            let reply = match home_hub {
                Some(hub) => {
                    let filter = match protocol.as_deref().map(str::trim).filter(|s| !s.is_empty())
                    {
                        Some(raw) => match hakille_home::ProtocolKind::from_str_loose(raw) {
                            Some(p) => Some(p),
                            None => {
                                let _ = reply_tx
                                    .send(Frame::reply(
                                        id,
                                        KernelToUi::Toast {
                                            text: format!("Protocole domotique inconnu : '{raw}'"),
                                        },
                                    ))
                                    .await;
                                return;
                            }
                        },
                        None => None,
                    };
                    match hub
                        .discover_lan(filter, std::time::Duration::from_millis(600))
                        .await
                    {
                        Ok(found) => {
                            let _ = audit_action_with_id(
                                audit_journal,
                                "HomeLanDiscovery",
                                format!(
                                    "Découverte LAN réelle terminée ({} service(s))",
                                    found.len()
                                ),
                                serde_json::json!({
                                    "protocol_filter": protocol,
                                    "discovered_count": found.len(),
                                }),
                                true,
                            );
                            let _ = broadcast_tx.send(BroadcastEnvelope::all(KernelToUi::Toast {
                                text: format!(
                                    "Découverte LAN : {} service(s) détecté(s) sur le réseau local",
                                    found.len()
                                ),
                            }));
                            let (devices, energy, protocols) =
                                crate::home_cli::collect_home_snapshot(hub).await;
                            KernelToUi::HomeDevicesReply {
                                devices,
                                energy,
                                protocols,
                            }
                        }
                        Err(e) => KernelToUi::Toast {
                            text: format!("Découverte LAN échouée : {e}"),
                        },
                    }
                }
                None => KernelToUi::Toast {
                    text: "SovereignHomeHub indisponible dans cette session".into(),
                },
            };
            let _ = reply_tx.send(Frame::reply(id, reply)).await;
        }
        FrameBody::Request {
            message:
                UiToKernel::HomeCommandRequest {
                    device_id,
                    action,
                    params_json,
                },
        } => {
            let reply = match home_hub {
                Some(hub) => {
                    let parsed =
                        hakille_home::HomeCommand::parse_action(&action, Some(&params_json));
                    match parsed {
                        Err(e) => KernelToUi::HomeCommandReply {
                            device_id,
                            executed_on_wire: false,
                            summary: e.to_string(),
                            receipt_sha256: None,
                        },
                        Ok(command) => match hub.preview_command(&device_id, &command).await {
                            Err(e) => KernelToUi::HomeCommandReply {
                                device_id,
                                executed_on_wire: false,
                                summary: e.to_string(),
                                receipt_sha256: None,
                            },
                            Ok(preview) => {
                                let hold_ms = crate::home_cli::extract_hold_ms(&params_json);
                                if preview.requires_600ms_hold && hold_ms.unwrap_or(0) < 600 {
                                    let (card_preview, plan) =
                                        crate::home_cli::build_tier3_action_card(
                                            &preview, &command,
                                        );
                                    let card = card_manager.allocate_preview(card_preview);
                                    if audit_action_with_id(
                                        audit_journal,
                                        "ActionCardProposed",
                                        format!(
                                            "Carte domotique Tier 3 #{} proposée : {} {}",
                                            card.id, card.verb, card.object
                                        ),
                                        serde_json::json!({
                                            "card_id": card.id,
                                            "device_id": device_id,
                                            "action": command.action_name(),
                                        }),
                                        true,
                                    )
                                    .is_ok()
                                    {
                                        card_manager.insert_card(card.clone(), plan);
                                        presence.set_state(DaemonPresenceState::DecisionRequired);
                                        let _ = broadcast_tx.send(BroadcastEnvelope::all(
                                            KernelToUi::Presence(
                                                ProtoPresenceState::DecisionRequired,
                                            ),
                                        ));
                                        let _ = broadcast_tx.send(BroadcastEnvelope::all(
                                            KernelToUi::ShowCard(card),
                                        ));
                                    }
                                    KernelToUi::HomeCommandReply {
                                        device_id,
                                        executed_on_wire: false,
                                        summary: "Confirmation physique 600 ms requise — carte d'action proposée".to_string(),
                                        receipt_sha256: None,
                                    }
                                } else {
                                    match crate::home_cli::execute_home_command_with_audit(
                                        hub,
                                        authority,
                                        audit_journal,
                                        &device_id,
                                        &command,
                                        hold_ms,
                                    )
                                    .await
                                    {
                                        Ok(receipt) => KernelToUi::HomeCommandReply {
                                            device_id,
                                            executed_on_wire: receipt.executed_on_wire,
                                            summary: receipt.summary,
                                            receipt_sha256: Some(receipt.frame_sha256),
                                        },
                                        Err(err) => KernelToUi::HomeCommandReply {
                                            device_id,
                                            executed_on_wire: false,
                                            summary: format!("Refus / hors ligne : {err}"),
                                            receipt_sha256: None,
                                        },
                                    }
                                }
                            }
                        },
                    }
                }
                None => KernelToUi::Toast {
                    text: "SovereignHomeHub indisponible dans cette session".into(),
                },
            };
            let _ = reply_tx.send(Frame::reply(id, reply)).await;
        }
        FrameBody::Request { .. } => {
            eprintln!(
                "IPC [session {session_id}]: message non supporté (id={id}) — refus explicite"
            );
            let _ = reply_tx
                .send(Frame::reply(
                    id,
                    KernelToUi::Toast {
                        text: "non supporté par le démon v1 — refus explicite".into(),
                    },
                ))
                .await;
        }
        FrameBody::Hello { .. }
        | FrameBody::Welcome { .. }
        | FrameBody::Reply { .. }
        | FrameBody::Error { .. } => {
            eprintln!("IPC [session {session_id}]: trame inattendue côté serveur (id={id})");
            let _ = reply_tx
                .send(Frame::error(
                    id,
                    ErrorCode::BadFrame,
                    "trame inattendue côté serveur",
                ))
                .await;
        }
    }
}

/// Exécute le serveur IPC multi-clients sur un `IpcListener` déjà configuré.
///
/// Permet l'arrêt propre via `shutdown_rx` pour les tests d'intégration ou le cycle de vie du démon.
#[allow(clippy::too_many_arguments)]
pub async fn run_serve_on_listener(
    listener: IpcListener,
    token: String,
    orchestrator: &mut CognitiveOrchestrator,
    presence: &mut PresenceIndicator,
    cost_counter: &mut CostCounter,
    continuity_tracker: &mut ContinuityTracker,
    bounded_initiative: &mut BoundedInitiativeTracker,
    generality_tracker: &mut GeneralityTracker,
    audit_journal: &mut AuditJournal,
    authority: &hakille_core::CapabilityAuthority,
    memory_path: &Path,
    outbox: &mut Outbox,
    data_dir: &Path,
    agent_runtime: Option<
        std::sync::Arc<std::sync::Mutex<crate::agent_runtime::ReferenceAgentRuntime>>,
    >,
    agent_registry: &hakille_agents::AgentRegistry,
    media_manager: Option<std::sync::Arc<hakille_media::MediaJobManager>>,
    home_hub: Option<std::sync::Arc<hakille_home::SovereignHomeHub>>,
    mut shutdown_rx: Option<tokio::sync::oneshot::Receiver<()>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (broadcast_tx, _) = broadcast::channel::<BroadcastEnvelope>(256);
    let (request_tx, mut request_rx) = mpsc::channel::<ClientSessionRequest>(128);

    let listener_token = token.clone();
    let listener_bcast = broadcast_tx.clone();
    let listener_req = request_tx.clone();
    let session_slots = std::sync::Arc::new(tokio::sync::Semaphore::new(32));

    // Boucle d'acceptation asynchrone non-bloquante avec tokio::spawn et plafond de sessions
    let accept_task = tokio::spawn(async move {
        let mut session_counter: u64 = 1;
        loop {
            match listener.accept().await {
                Ok(stream) => {
                    let Ok(permit) = session_slots.clone().try_acquire_owned() else {
                        eprintln!(
                            "IPC: plafond de 32 sessions clientes atteint — connexion refusée"
                        );
                        continue;
                    };
                    let session_id = session_counter;
                    session_counter = session_counter.saturating_add(1);
                    let session_token = listener_token.clone();
                    let session_bcast = listener_bcast.subscribe();
                    let session_req = listener_req.clone();
                    tokio::spawn(async move {
                        let _permit = permit;
                        serve_client_session(
                            session_id,
                            stream,
                            session_token,
                            session_bcast,
                            session_req,
                        )
                        .await;
                    });
                }
                Err(e) => {
                    eprintln!("IPC: accept échoué — {e}");
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
        }
    });

    let mut card_manager = ActionCardManager::new(1);
    let perception_state = PerceptionControlState::default();

    loop {
        if let Some(ref mut stop) = shutdown_rx {
            tokio::select! {
                _ = stop => {
                    break;
                }
                Some(req) = request_rx.recv() => {
                    handle_client_request(
                        req,
                        orchestrator,
                        presence,
                        cost_counter,
                        continuity_tracker,
                        bounded_initiative,
                        generality_tracker,
                        audit_journal,
                        authority,
                        memory_path,
                        outbox,
                        data_dir,
                        agent_runtime.as_ref(),
                        agent_registry,
                        media_manager.as_ref(),
                        &mut card_manager,
                        &perception_state,
                        &broadcast_tx,
                        home_hub.as_ref(),
                    )
                    .await;
                }
            }
        } else {
            match request_rx.recv().await {
                Some(req) => {
                    handle_client_request(
                        req,
                        orchestrator,
                        presence,
                        cost_counter,
                        continuity_tracker,
                        bounded_initiative,
                        generality_tracker,
                        audit_journal,
                        authority,
                        memory_path,
                        outbox,
                        data_dir,
                        agent_runtime.as_ref(),
                        agent_registry,
                        media_manager.as_ref(),
                        &mut card_manager,
                        &perception_state,
                        &broadcast_tx,
                        home_hub.as_ref(),
                    )
                    .await;
                }
                None => break,
            }
        }
    }

    accept_task.abort();
    Ok(())
}

/// Boucle serveur : jeton neuf écrit pour le HUD, écoute sur l'endpoint local,
/// acceptation asynchrone concurrente de multiples clients (Desktop HUD et Mobile Gateway).
#[allow(clippy::too_many_arguments)]
pub async fn run_serve(
    orchestrator: &mut CognitiveOrchestrator,
    presence: &mut PresenceIndicator,
    cost_counter: &mut CostCounter,
    continuity_tracker: &mut ContinuityTracker,
    bounded_initiative: &mut BoundedInitiativeTracker,
    generality_tracker: &mut GeneralityTracker,
    audit_journal: &mut AuditJournal,
    authority: &hakille_core::CapabilityAuthority,
    memory_path: &std::path::Path,
    outbox: &mut Outbox,
    data_dir: &std::path::Path,
    agent_runtime: Option<
        std::sync::Arc<std::sync::Mutex<crate::agent_runtime::ReferenceAgentRuntime>>,
    >,
    agent_registry: &hakille_agents::AgentRegistry,
    media_manager: Option<std::sync::Arc<hakille_media::MediaJobManager>>,
    home_hub: Option<std::sync::Arc<hakille_home::SovereignHomeHub>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = hakille_ipc::transport::default_endpoint();
    let token_path = hakille_ipc::transport::default_token_path();
    if let Some(parent) = token_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let listener = match hakille_ipc::transport::IpcListener::bind(&endpoint).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("IPC: bind impossible sur {endpoint} — {e} (aucun serveur ne démarre)");
            return Err(e.into());
        }
    };
    let token = hakille_ipc::transport::fresh_token_hex();
    hakille_ipc::transport::write_token_file(&token_path, &token)?;
    println!("IPC: endpoint {endpoint}");
    println!("IPC: token file {}", token_path.display());
    println!("IPC: prêt — écoute multi-clients active");

    run_serve_on_listener(
        listener,
        token,
        orchestrator,
        presence,
        cost_counter,
        continuity_tracker,
        bounded_initiative,
        generality_tracker,
        audit_journal,
        authority,
        memory_path,
        outbox,
        data_dir,
        agent_runtime,
        agent_registry,
        media_manager,
        home_hub,
        None,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_audit::entry::{ActionRecord, ReasoningTrace};
    use hakille_core::{ActorId, EntryId, NodeId, PredicateId, Timestamp, TripletId, TrustLevel};
    use hakille_memory::{ContextPartition, NodeValue, Triplet, TripletMetadata, TripletSource};

    /// Triplet réel inséré dans un graphe de test (objet texte, activation
    /// décalée pour tester l'ordre).
    fn triplet(object: &str, activated_offset_secs: i64) -> Triplet {
        let mut metadata = TripletMetadata::new(
            1.0,
            1.0,
            TripletSource::User,
            ContextPartition::Personal,
            TrustLevel::User,
            0.01,
        )
        .expect("métadonnées triplet valides");
        let activated =
            Timestamp(chrono::Utc::now() + chrono::Duration::seconds(activated_offset_secs));
        metadata.created_at = activated.clone();
        metadata.last_activated = activated;
        Triplet::new(
            TripletId(uuid::Uuid::now_v7()),
            NodeId(uuid::Uuid::now_v7()),
            PredicateId(uuid::Uuid::now_v7()),
            NodeValue::Text(object.to_string()),
            metadata,
        )
        .expect("triplet valide")
    }

    #[test]
    fn remember_parts_matches_repl_rules() {
        assert_eq!(
            remember_parts("Marie aime le jazz"),
            ("Marie".into(), "aime", "le jazz")
        );
        let note = remember_parts("jazz");
        assert_eq!(note.1, "contenu");
        assert_eq!(note.2, "jazz");
        assert_eq!(note.0, remember_parts("jazz").0);
        assert_ne!(note.0, remember_parts("café").0);
        assert_eq!(
            remember_parts("Réunion lundi 10h salle B"),
            ("Réunion".into(), "lundi", "10h salle B")
        );
    }

    #[test]
    fn memory_view_rows_orders_by_last_activation_and_limits() {
        let mut graph = hakille_memory::KnowledgeGraph::new();
        graph.insert(triplet("ancien", -120));
        graph.insert(triplet("milieu", -60));
        graph.insert(triplet("récent", -10));

        let rows = memory_view_rows(&graph, &ContextPartition::Personal, 2);
        assert_eq!(rows.len(), 2, "limit respecté");
        assert_eq!(rows[0].text, "récent", "plus récent d'abord");
        assert_eq!(rows[1].text, "milieu");
        assert!(!rows[0].id.is_empty(), "id réel du triplet");
        assert_eq!(rows[0].source, "User");
        assert!(rows[0].tags.contains(&"Personal".to_string()));

        assert!(
            memory_view_rows(&graph, &ContextPartition::Personal, 0).is_empty(),
            "limit 0 = vide"
        );
        assert_eq!(
            memory_view_rows(&graph, &ContextPartition::Personal, 99).len(),
            3
        );
        assert!(
            memory_view_rows(&graph, &ContextPartition::Professional, 99).is_empty(),
            "isolation de partition"
        );
    }

    #[test]
    fn recall_rows_reattaches_real_ids_never_invents() {
        let mut graph = hakille_memory::KnowledgeGraph::new();
        graph.insert(triplet("Marie aime le jazz", 0));

        let rows = recall_rows(&graph, &["Marie aime le jazz".to_string()], 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "Marie aime le jazz");
        assert!(!rows[0].id.is_empty(), "id réel retrouvé dans le graphe");

        let ghost = recall_rows(&graph, &["inconnu".to_string()], 10);
        assert_eq!(ghost[0].id, "", "jamais d'id inventé");
        assert_eq!(ghost[0].source, "recall");
    }

    #[test]
    fn agent_execution_returns_the_real_wasm_output() {
        // Preuve d'exécution RÉELLE : le binaire WASM tourne dans le bac à
        // sable et sa sortie JSON est rapportée telle quelle.
        let dir = std::env::temp_dir().join(format!("hakille_run_{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).expect("dossier de test");
        let mut runtime = crate::agent_runtime::ReferenceAgentRuntime::provision(&dir)
            .expect("runtime d'agents provisionné");

        let reply = run_agent_request(&mut runtime, "echo-agent", "{\"msg\":\"bonjour\"}")
            .expect("exécution réelle");
        let KernelToUi::AgentRunReply {
            name,
            output_json,
            duration_ms,
            input_sha256,
            output_sha256,
        } = reply
        else {
            panic!("réponse d'exécution attendue");
        };
        assert_eq!(name, "echo-agent");
        assert!(
            output_json.contains("bonjour"),
            "la sortie réelle contient l'entrée : {output_json}"
        );
        assert_eq!(input_sha256.len(), 64);
        assert_eq!(output_sha256.len(), 64);
        assert!(duration_ms < 60_000, "exécution bornée : {duration_ms} ms");

        // Un agent inconnu est REFUSÉ explicitement (jamais de sortie inventée).
        let err = run_agent_request(&mut runtime, "agent-mail", "{}")
            .expect_err("agent sans binaire refusé");
        assert!(err.contains("agent-mail"), "{err}");
    }

    #[test]
    fn agent_list_is_honest_between_real_and_declarative_agents() {
        use hakille_agents::{AgentManifest, AgentRegistry};
        use hakille_core::CapabilityScope;

        // Un manifeste DÉCLARATIF : jamais présenté comme exécutable.
        let mut registry = AgentRegistry::new();
        registry
            .register(AgentManifest {
                id: "agent-mail".into(),
                name: "Mail Agent".into(),
                version: "0.1.0".into(),
                description: "manifest déclaratif".into(),
                required_capabilities: vec![CapabilityScope::Network],
            })
            .expect("manifest enregistré");
        let without_runtime = agent_dtos(None, &registry);
        assert_eq!(without_runtime.len(), 1);
        assert!(!without_runtime[0].artifact_present);
        assert!(without_runtime[0].sha256.is_empty());
        assert_eq!(without_runtime[0].required_caps.len(), 1);

        // Avec un runtime réel : les agents de référence sont exécutables.
        let dir = std::env::temp_dir().join(format!("hakille_agents_{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).expect("dossier de test");
        let runtime = crate::agent_runtime::ReferenceAgentRuntime::provision(&dir)
            .expect("runtime d'agents provisionné");
        let with_runtime = agent_dtos(Some(&runtime), &registry);
        let real: Vec<&AgentDto> = with_runtime.iter().filter(|a| a.artifact_present).collect();
        assert_eq!(
            real.len(),
            runtime.agents().len(),
            "chaque agent chargé est exécutable, avec son SHA-256"
        );
        assert!(real.iter().all(|a| a.sha256.len() == 64));
        assert!(
            with_runtime
                .iter()
                .any(|a| a.id == "agent-mail" && !a.artifact_present),
            "le manifeste déclaratif reste marqué non exécutable"
        );
    }

    /// Plan d'exécution de test : une étape irréversible (même forme que le
    /// plan réel produit par le noyau).
    fn test_plan() -> hakille_kernel::planner::ExecutionPlan {
        use hakille_core::{CapabilityAction, CapabilityScope};
        use hakille_kernel::planner::{ExecutionPlan, PlannedStep};
        let mut plan = ExecutionPlan::new("card-preview", "transférer 25 EUR à Awa");
        plan.steps.push(PlannedStep {
            step_id: 1,
            description: "transférer 25 EUR à Awa".to_string(),
            required_scope: CapabilityScope::Wallet,
            required_action: CapabilityAction::Execute,
            is_irreversible: true,
            fallback_strategy: None,
        });
        plan.requires_human_confirmation = true;
        plan
    }

    #[test]
    fn approved_card_produces_a_real_durable_effect_and_audits_it() {
        use ed25519_dalek::SigningKey;

        let dir = std::env::temp_dir().join(format!("hakille_card_{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).expect("dossier de test");
        let db_path = dir.join("audit.redb");
        let journal = AuditJournal::new(
            db_path.to_str().expect("chemin utf8"),
            SigningKey::from_bytes(&[7u8; 32]),
        )
        .expect("journal de test");

        let mut cards: HashMap<u64, PendingCard> = HashMap::new();
        cards.insert(
            42,
            PendingCard {
                card: ActionCard {
                    id: 42,
                    verb: "Transférer".into(),
                    object: "25 EUR à Awa".into(),
                    reason: "demande explicite".into(),
                    cost: Money::from_cents(2500, "EUR"),
                    data_exit: DataExit::Local,
                    reversible: false,
                    provenance: Provenance::Hakille,
                },
                plan: test_plan(),
                proposed_at: Instant::now() - std::time::Duration::from_secs(1),
            },
        );
        let mut outbox = Outbox::new();

        // Régression : l'ancien code annonçait un accusé durable même si
        // l'écriture échouait. Un chemin occupé par un dossier force cet échec
        // indépendamment des droits root du runner.
        let blocked_path = dir.join("outbox.json");
        std::fs::create_dir(&blocked_path).unwrap();
        let denied = decide_card(&mut cards, &journal, &mut outbox, &dir, 42, true);
        assert!(matches!(denied, KernelToUi::Toast { .. }));
        assert!(cards.contains_key(&42), "la décision reste réessayable");
        assert!(outbox.is_empty(), "aucun ajout RAM non persisté");
        assert!(
            journal.all_entries().unwrap().is_empty(),
            "aucun audit de succès inventé"
        );
        std::fs::remove_dir(blocked_path).unwrap();

        let reply = decide_card(&mut cards, &journal, &mut outbox, &dir, 42, true);
        let KernelToUi::CardDecisionAck {
            approved,
            audit_id,
            effect,
            ..
        } = reply
        else {
            panic!("un accusé audité est attendu, pas un refus");
        };
        assert!(approved);
        assert!(!audit_id.is_empty(), "UUID d'audit réel");

        // EFFET RÉEL : l'action approuvée est en file, durable et idempotente.
        assert_eq!(outbox.len(), 1, "l'action approuvée est en file");
        let entry = &outbox.list()[0];
        assert_eq!(entry.verb, "Transférer");
        assert!(entry.dedup_key.contains("card#42"));
        assert!(
            entry.object.contains("plan card-preview"),
            "l'action en file PORTE le plan approuvé : {}",
            entry.object
        );
        assert!(
            entry.object.contains("transférer 25 EUR à Awa"),
            "l'étape du plan est dans l'action : {}",
            entry.object
        );
        assert!(
            effect
                .as_deref()
                .unwrap_or_default()
                .contains("mise en file"),
            "l'accusé rapporte l'effet réel : {effect:?}"
        );
        // Persistance : le fichier d'envoi existe et contient l'entrée.
        let outbox_file = dir.join("outbox.json");
        assert!(outbox_file.is_file(), "effet durable sur disque");
        let persisted = std::fs::read_to_string(&outbox_file).expect("lecture outbox");
        assert!(persisted.contains("Transférer"), "entrée persistée");

        // L'AUDIT décrit l'effet réel (pas un "aucun effet" mensonger).
        let entries = journal.all_entries().expect("entrées d'audit");
        let approved_entry = entries
            .iter()
            .find(|e| e.action.action_type == "ActionCardApproved")
            .expect("entrée d'approbation");
        assert_eq!(
            approved_entry.action.parameters.get("effect"),
            Some(&serde_json::json!("outbox_enqueued"))
        );
        assert_eq!(
            approved_entry.action.parameters.get("plan_steps"),
            Some(&serde_json::json!(1)),
            "l'audit porte le plan approuvé"
        );
        let audit_id_debug = format!("{:?}", approved_entry.id);
        assert!(
            audit_id_debug.contains(&audit_id),
            "l'accusé porte l'identifiant réel de l'entrée auditée : {audit_id_debug} vs {audit_id}"
        );

        // Refus : AUCUN effet, et l'audit le dit.
        cards.insert(
            43,
            PendingCard {
                card: ActionCard {
                    id: 43,
                    verb: "Supprimer".into(),
                    object: "/vfs/notes.txt".into(),
                    reason: "irréversible".into(),
                    cost: Money::from_cents(0, "EUR"),
                    data_exit: DataExit::Local,
                    reversible: false,
                    provenance: Provenance::Hakille,
                },
                plan: test_plan(),
                proposed_at: Instant::now(),
            },
        );
        let reply = decide_card(&mut cards, &journal, &mut outbox, &dir, 43, false);
        let KernelToUi::CardDecisionAck { effect, .. } = reply else {
            panic!("un accusé de refus est attendu");
        };
        assert!(effect.is_none(), "un refus ne produit aucun effet");
        assert_eq!(outbox.len(), 1, "la file n'a pas changé");
    }

    #[test]
    fn card_from_preview_maps_real_fields_and_never_cloud() {
        let card = card_from_preview(
            42,
            ActionCardPreview {
                verb: "Confirmer".into(),
                object: "Virement : 25 EUR".into(),
                reason: "irréversible".into(),
                cost_cents: 2500,
                currency: "EUR".into(),
                reversible: false,
                data_exit_local: true,
                provenance_label: "Hakille — plan sec".into(),
            },
        );
        assert_eq!(card.id, 42);
        assert_eq!(card.cost.display(), "25.00 EUR");
        assert!(matches!(card.data_exit, DataExit::Local));
        assert!(!card.reversible);
        assert!(matches!(card.provenance, Provenance::Hakille));
        assert!(card.reason.contains("irréversible"));
    }

    #[test]
    fn journal_entry_dto_maps_real_fields() {
        let id = EntryId(uuid::Uuid::now_v7());
        let entry = AuditEntry {
            id: id.clone(),
            timestamp: Timestamp(chrono::Utc::now()),
            actor: ActorId::Kernel,
            action: ActionRecord {
                action_type: "CognitiveTurn".to_string(),
                description: "Processed prompt".to_string(),
                parameters: serde_json::json!({}),
                reversible: false,
                reversed: false,
            },
            capability_used: None,
            reasoning_trace: ReasoningTrace {
                steps: vec![],
                model_used: None,
                duration_ms: 1,
            },
            graph_nodes_used: vec![],
            confidence: 0.9,
            cost: None,
            signature: vec![],
            prev_hash: vec![],
            token_hash: None,
        };
        let dto = journal_entry_dto(&entry);
        assert_eq!(dto.id, id.0.to_string());
        assert_eq!(dto.action, "CognitiveTurn");
        assert_eq!(dto.summary, "Processed prompt");
        assert!(dto.timestamp.contains('T'), "RFC 3339: {}", dto.timestamp);
    }

    #[tokio::test]
    async fn test_action_card_manager_lifecycle() {
        let mut mgr = ActionCardManager::new(1);
        assert!(mgr.is_empty());
        assert_eq!(mgr.pending_count(), 0);

        let preview = ActionCardPreview {
            verb: "Confirmer".into(),
            object: "Paiement 50 EUR".into(),
            reason: "irréversible".into(),
            cost_cents: 5000,
            currency: "EUR".into(),
            reversible: false,
            data_exit_local: true,
            provenance_label: "Hakille".into(),
        };
        let card = mgr.insert(preview, test_plan());
        assert_eq!(card.id, 1);
        assert_eq!(mgr.pending_count(), 1);
        assert!(!mgr.is_empty());
        assert_eq!(mgr.get(1).map(|p| p.card.id), Some(1));

        let card2 = ActionCard {
            id: 2,
            verb: "Exécuter".into(),
            object: "Test".into(),
            reason: "test".into(),
            cost: hakille_ui_protocol::Money {
                micros: 0,
                currency: "EUR".into(),
            },
            reversible: true,
            data_exit: hakille_ui_protocol::DataExit::Local,
            provenance: hakille_ui_protocol::Provenance::Hakille,
        };
        mgr.insert_card(card2, test_plan());
        assert_eq!(mgr.pending_count(), 2);
        assert_eq!(mgr.get(2).map(|p| p.card.id), Some(2));

        let dir = std::env::temp_dir().join(format!("hakille_card_mgr_{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let journal = AuditJournal::new(
            dir.join("audit.redb").to_str().unwrap(),
            ed25519_dalek::SigningKey::from_bytes(&[8u8; 32]),
        )
        .unwrap();
        let mut outbox = Outbox::new();

        // Approbation immédiate (< 600 ms) : refusée par le démon et la carte reste en attente.
        let premature = mgr.decide(&journal, &mut outbox, &dir, 1, true);
        assert!(matches!(premature, KernelToUi::Toast { .. }));
        assert_eq!(mgr.pending_count(), 2);

        tokio::time::sleep(std::time::Duration::from_millis(610)).await;

        let ack1 = mgr.decide(&journal, &mut outbox, &dir, 1, true);
        assert!(matches!(
            ack1,
            KernelToUi::CardDecisionAck {
                card_id: 1,
                approved: true,
                ..
            }
        ));
        let ack2 = mgr.decide(&journal, &mut outbox, &dir, 2, false);
        assert!(matches!(
            ack2,
            KernelToUi::CardDecisionAck {
                card_id: 2,
                approved: false,
                ..
            }
        ));
        assert!(mgr.is_empty());
        assert_eq!(mgr.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_multi_client_concurrent_coexistence_and_card_resolution() {
        use std::time::Duration;

        async fn connect_with_retry(endpoint: &str) -> std::io::Result<transport::ClientStream> {
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            loop {
                match transport::connect(endpoint).await {
                    Ok(stream) => return Ok(stream),
                    Err(err) if std::time::Instant::now() < deadline => {
                        let _ = err;
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                    Err(err) => return Err(err),
                }
            }
        }

        let id = uuid::Uuid::now_v7();
        let dir = std::env::temp_dir().join(format!("hakille_ipc_mc_{id}"));
        std::fs::create_dir_all(&dir).unwrap();
        #[cfg(unix)]
        let endpoint = format!("/tmp/hakille-ipc-mc-{id}.sock");
        #[cfg(windows)]
        let endpoint = format!(r"\\.\pipe\hakille-ipc-mc-{id}");
        let token = transport::fresh_token_hex();

        let listener = transport::IpcListener::bind(&endpoint).await.unwrap();

        let user_id = hakille_core::UserId(uuid::Uuid::now_v7());
        let fabric =
            hakille_kernel::CognitiveFabric::new(hakille_kernel::CognitiveManifest::default());
        let constitution = hakille_constitution::Constitution::default_v1();
        let mut orchestrator = CognitiveOrchestrator::new(fabric, user_id, constitution);
        let mut presence = PresenceIndicator::new();
        let mut cost_counter = CostCounter::new(10_000, "EUR");
        let mut continuity = ContinuityTracker::new();
        let mut initiative = BoundedInitiativeTracker::new();
        let mut generality = GeneralityTracker::new();
        let journal = AuditJournal::new(
            dir.join("audit.redb").to_str().unwrap(),
            ed25519_dalek::SigningKey::from_bytes(&[9u8; 32]),
        )
        .unwrap();
        let mut audit_journal = journal;
        let memory_path = dir.join("memory.json");
        let mut outbox = Outbox::new();
        let registry = hakille_agents::AgentRegistry::new();

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let server_token = token.clone();
        let server_dir = dir.clone();

        // 1. Démarrer le serveur IPC multi-clients sur tâche de fond
        let server_task = tokio::spawn(async move {
            run_serve_on_listener(
                listener,
                server_token,
                &mut orchestrator,
                &mut presence,
                &mut cost_counter,
                &mut continuity,
                &mut initiative,
                &mut generality,
                &mut audit_journal,
                &hakille_core::CapabilityAuthority::new(),
                &memory_path,
                &mut outbox,
                &server_dir,
                None,
                &registry,
                None,
                None,
                Some(shutdown_rx),
            )
            .await
            .unwrap();
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        // 2. Client 1 (Desktop HUD) connecte et handshake
        let mut desktop_stream = connect_with_retry(&endpoint)
            .await
            .expect("connect desktop");
        transport::client_handshake(&mut desktop_stream, &token)
            .await
            .expect("handshake desktop");
        let (desktop_reader, mut desktop_writer) = tokio::io::split(desktop_stream);
        let mut desktop_reader = BufReader::new(desktop_reader);

        // 3. Client 2 (Mobile Gateway) connecte simultanément et handshake sans blocage !
        let mut mobile_stream = connect_with_retry(&endpoint).await.expect("connect mobile");
        transport::client_handshake(&mut mobile_stream, &token)
            .await
            .expect("handshake mobile");
        let (mobile_reader, mut mobile_writer) = tokio::io::split(mobile_stream);
        let mut mobile_reader = BufReader::new(mobile_reader);

        // VÉRIFICATION 1 : Requêtes concurrentes StatusRequest simultanées
        let req_desktop = Frame::request(101, UiToKernel::StatusRequest);
        transport::write_frame(&mut desktop_writer, &req_desktop)
            .await
            .unwrap();

        let req_mobile = Frame::request(201, UiToKernel::StatusRequest);
        transport::write_frame(&mut mobile_writer, &req_mobile)
            .await
            .unwrap();

        // Desktop reçoit exactement son statut avec son request.id = 101
        let resp_desktop = transport::read_frame(&mut desktop_reader)
            .await
            .unwrap()
            .expect("desktop frame");
        assert_eq!(resp_desktop.id, 101, "Desktop reçoit sa propre réponse");
        assert!(matches!(
            resp_desktop.body,
            FrameBody::Reply {
                message: KernelToUi::StatusReply { .. }
            }
        ));

        // Mobile reçoit exactement son statut avec son request.id = 201 (aucune fuite du paquet Desktop)
        let resp_mobile = transport::read_frame(&mut mobile_reader)
            .await
            .unwrap()
            .expect("mobile frame");
        assert_eq!(
            resp_mobile.id, 201,
            "Mobile reçoit sa propre réponse sans collision d'ID"
        );
        assert!(matches!(
            resp_mobile.body,
            FrameBody::Reply {
                message: KernelToUi::StatusReply { .. }
            }
        ));

        // VÉRIFICATION 2 : Proposition d'une Action Card via HudSubmit avec plan sec
        let prompt_card = Frame::request(
            102,
            UiToKernel::HudSubmit {
                query: "payer 25 EUR à Awa".to_string(),
            },
        );
        transport::write_frame(&mut desktop_writer, &prompt_card)
            .await
            .unwrap();

        // Les deux clients observent l'événement de réflexion Acting diffusé sur le bus broadcast avec id = 0
        // Desktop reçoit sa réponse HudReply (id=102) et les événements broadcast (id=0)
        let mut desktop_frames = Vec::new();
        for _ in 0..4 {
            let f = transport::read_frame(&mut desktop_reader)
                .await
                .unwrap()
                .expect("desktop frame");
            desktop_frames.push(f);
        }

        // Vérification de la séparation stricte : exactement une réponse finale Unicast (id=102)
        let unicast_replies: Vec<&Frame> = desktop_frames.iter().filter(|f| f.id == 102).collect();
        assert_eq!(
            unicast_replies.len(),
            1,
            "Desktop reçoit sa réponse Unicast avec son id 102"
        );
        assert!(matches!(
            unicast_replies[0].body,
            FrameBody::Reply {
                message: KernelToUi::HudReply { .. }
            }
        ));

        // Vérification des événements broadcast reçus par Desktop (id=0)
        let broadcast_frames: Vec<&Frame> = desktop_frames.iter().filter(|f| f.id == 0).collect();
        assert_eq!(
            broadcast_frames.len(),
            3,
            "Desktop reçoit les 3 événements broadcast avec id 0"
        );
        assert!(broadcast_frames.iter().any(|f| matches!(
            &f.body,
            FrameBody::Reply {
                message: KernelToUi::Presence(ProtoPresenceState::Acting)
            }
        )));
        assert!(broadcast_frames.iter().any(|f| matches!(
            &f.body,
            FrameBody::Reply {
                message: KernelToUi::Presence(ProtoPresenceState::DecisionRequired)
            }
        )));
        let desktop_card_id = broadcast_frames
            .iter()
            .find_map(|f| match &f.body {
                FrameBody::Reply {
                    message: KernelToUi::ShowCard(card),
                } => Some(card.id),
                _ => None,
            })
            .expect("Desktop a reçu ShowCard");

        // Mobile (passerelle) ne reçoit STRICTEMENT QUE les 3 trames de broadcast avec id = 0 (zéro trame id 102)
        let bcast_m_acting = transport::read_frame(&mut mobile_reader)
            .await
            .unwrap()
            .expect("mobile acting");
        assert_eq!(bcast_m_acting.id, 0);
        assert!(matches!(
            bcast_m_acting.body,
            FrameBody::Reply {
                message: KernelToUi::Presence(ProtoPresenceState::Acting)
            }
        ));

        let bcast_m_dec = transport::read_frame(&mut mobile_reader)
            .await
            .unwrap()
            .expect("mobile decision required");
        assert_eq!(bcast_m_dec.id, 0);
        assert!(matches!(
            bcast_m_dec.body,
            FrameBody::Reply {
                message: KernelToUi::Presence(ProtoPresenceState::DecisionRequired)
            }
        ));

        let bcast_m_card = transport::read_frame(&mut mobile_reader)
            .await
            .unwrap()
            .expect("mobile show card");
        assert_eq!(bcast_m_card.id, 0);
        let card_id = match bcast_m_card.body {
            FrameBody::Reply {
                message: KernelToUi::ShowCard(card),
            } => {
                assert_eq!(card.verb, "Confirmer");
                assert_eq!(card.id, desktop_card_id);
                card.id
            }
            other => panic!("ShowCard attendu, reçu : {other:?}"),
        };

        // VÉRIFICATION 3 : Mobile respecte le maintien 600 ms puis approuve la carte -> Mobile reçoit Unicast avec son id 202, Desktop reçoit broadcast avec id 0
        tokio::time::sleep(Duration::from_millis(610)).await;
        let approve_req = Frame::request(202, UiToKernel::Approve { card_id });
        transport::write_frame(&mut mobile_writer, &approve_req)
            .await
            .unwrap();

        // Mobile reçoit son accusé Unicast direct avec id = 202
        let ack_mobile = transport::read_frame(&mut mobile_reader)
            .await
            .unwrap()
            .expect("mobile ack");
        assert_eq!(ack_mobile.id, 202);
        assert!(matches!(
            ack_mobile.body,
            FrameBody::Reply {
                message: KernelToUi::CardDecisionAck { approved: true, .. }
            }
        ));

        // Desktop reçoit l'accusé de résolution via le broadcast bus avec id = 0
        let ack_desktop = transport::read_frame(&mut desktop_reader)
            .await
            .unwrap()
            .expect("desktop ack broadcast");
        assert_eq!(ack_desktop.id, 0);
        assert!(matches!(
            ack_desktop.body,
            FrameBody::Reply {
                message: KernelToUi::CardDecisionAck { approved: true, .. }
            }
        ));

        // Tous deux reçoivent Presence(Idle) avec id = 0
        let idle_desktop = transport::read_frame(&mut desktop_reader)
            .await
            .unwrap()
            .expect("desktop idle");
        assert_eq!(idle_desktop.id, 0);
        assert!(matches!(
            idle_desktop.body,
            FrameBody::Reply {
                message: KernelToUi::Presence(ProtoPresenceState::Idle)
            }
        ));

        let idle_mobile = transport::read_frame(&mut mobile_reader)
            .await
            .unwrap()
            .expect("mobile idle");
        assert_eq!(idle_mobile.id, 0);
        assert!(matches!(
            idle_mobile.body,
            FrameBody::Reply {
                message: KernelToUi::Presence(ProtoPresenceState::Idle)
            }
        ));

        // VÉRIFICATION 4 : Résilience à la déconnexion abrupte
        drop(mobile_reader);
        drop(mobile_writer);

        tokio::time::sleep(Duration::from_millis(50)).await;

        // Desktop continue à fonctionner normalement
        let ping = Frame::request(103, UiToKernel::StatusRequest);
        transport::write_frame(&mut desktop_writer, &ping)
            .await
            .unwrap();
        let pong = transport::read_frame(&mut desktop_reader)
            .await
            .unwrap()
            .expect("desktop pong");
        assert_eq!(pong.id, 103);
        assert!(matches!(
            pong.body,
            FrameBody::Reply {
                message: KernelToUi::StatusReply { .. }
            }
        ));

        // Arrêt propre du serveur
        let _ = shutdown_tx.send(());
        let _ = server_task.await;
        #[cfg(unix)]
        let _ = std::fs::remove_file(&endpoint);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn media_ipc_engines_generate_and_gallery_roundtrip() {
        use hakille_constitution::rules::Constitution;
        use hakille_core::UserId;
        use hakille_kernel::fabric::{CognitiveFabric, CognitiveManifest};
        use hakille_ui_protocol::{MediaGenerateRequestDto, MediaModalityDto};

        let test_id = uuid::Uuid::new_v4();
        let dir = std::env::temp_dir().join(format!("hakille-ipc-media-test-{}", test_id));
        std::fs::create_dir_all(&dir).unwrap();
        #[cfg(not(windows))]
        let endpoint = dir.join("ipc.sock").to_string_lossy().to_string();
        #[cfg(windows)]
        let endpoint = format!(r"\\.\pipe\hakille-ipc-media-{}", test_id);
        let token = transport::fresh_token_hex();

        let listener = IpcListener::bind(&endpoint).await.unwrap();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&[11u8; 32]);
        let verifying_key = signing_key.verifying_key();
        let media_mgr =
            crate::media_cli::build_media_manager(&dir, &dir.join("models"), signing_key.clone())
                .unwrap();

        let fabric = CognitiveFabric::new(CognitiveManifest::default());
        let user_id = UserId(uuid::Uuid::new_v4());
        let constitution = Constitution::default_v1();
        let mut orchestrator = CognitiveOrchestrator::new(fabric, user_id, constitution);
        let mut presence = PresenceIndicator::new();
        let mut cost_counter = CostCounter::new(10_000, "EUR");
        let mut continuity = ContinuityTracker::new();
        let mut initiative = BoundedInitiativeTracker::new();
        let mut generality = GeneralityTracker::new();
        let mut audit_journal =
            AuditJournal::new(dir.join("audit.redb").to_str().unwrap(), signing_key).unwrap();
        let memory_path = dir.join("memory.json");
        let mut outbox = Outbox::new();
        let registry = hakille_agents::AgentRegistry::new();

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let server_token = token.clone();
        let server_dir = dir.clone();
        let server_task = tokio::spawn(async move {
            run_serve_on_listener(
                listener,
                server_token,
                &mut orchestrator,
                &mut presence,
                &mut cost_counter,
                &mut continuity,
                &mut initiative,
                &mut generality,
                &mut audit_journal,
                &hakille_core::CapabilityAuthority::new(),
                &memory_path,
                &mut outbox,
                &server_dir,
                None,
                &registry,
                Some(media_mgr),
                None,
                Some(shutdown_rx),
            )
            .await
            .unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(40)).await;

        let mut stream = transport::connect(&endpoint).await.unwrap();
        transport::client_handshake(&mut stream, &token)
            .await
            .unwrap();
        let (r, w) = tokio::io::split(stream);
        let mut reader = BufReader::new(r);
        let mut writer = BufWriter::new(w);

        // 1. Inventaire honnête des moteurs
        transport::write_frame(
            &mut writer,
            &Frame::request(1, UiToKernel::MediaEnginesRequest),
        )
        .await
        .unwrap();
        let f1 = transport::read_frame(&mut reader).await.unwrap().unwrap();
        assert_eq!(f1.id, 1);
        match f1.body {
            FrameBody::Reply {
                message: KernelToUi::MediaEnginesReply { engines, .. },
            } => {
                assert!(engines.len() >= 10, "inventaire complet attendu");
                assert!(engines.iter().any(|e| e.id == "procedural-dsp" && e.ready));
                assert!(
                    engines
                        .iter()
                        .any(|e| e.id == "sd-turbo-candle" && !e.ready)
                );
            }
            other => panic!("MediaEnginesReply attendu, reçu : {other:?}"),
        }

        // 2. Génération image locale via IPC + aperçu RGBA + provenance Ed25519
        let gen_req = MediaGenerateRequestDto {
            modality: MediaModalityDto::Image,
            prompt: "constellation boréale souveraine".into(),
            engine: "procedural-dsp".into(),
            width: Some(128),
            height: Some(128),
            steps: Some(8),
            seed: Some(77),
            duration_secs: None,
            fps: None,
            num_frames: None,
            format: Some("png".into()),
            mode: None,
        };
        transport::write_frame(
            &mut writer,
            &Frame::request(2, UiToKernel::MediaGenerateRequest(gen_req)),
        )
        .await
        .unwrap();

        let path_str = loop {
            let f = transport::read_frame(&mut reader).await.unwrap().unwrap();
            if f.id == 2 {
                match f.body {
                    FrameBody::Reply {
                        message: KernelToUi::MediaJobCompleted { artifact },
                    } => {
                        assert!(artifact.provenance_signed);
                        assert!(artifact.preview.is_some());
                        assert_eq!(artifact.engine_id, "procedural-dsp");
                        break artifact.path;
                    }
                    other => panic!("MediaJobCompleted attendu, reçu : {other:?}"),
                }
            }
        };
        let report = hakille_media::verify_artifact_provenance(
            std::path::Path::new(&path_str),
            Some(&verifying_key),
        )
        .expect("vérification Ed25519 + PNG iTXt");
        assert!(report.embedded_png_chunk_checked);

        // 3. Galerie locale
        transport::write_frame(
            &mut writer,
            &Frame::request(3, UiToKernel::MediaGalleryRequest { limit: 10 }),
        )
        .await
        .unwrap();
        loop {
            let f = transport::read_frame(&mut reader).await.unwrap().unwrap();
            if f.id == 3 {
                match f.body {
                    FrameBody::Reply {
                        message: KernelToUi::MediaGalleryReply { artifacts },
                    } => {
                        assert_eq!(artifacts.len(), 1);
                        assert_eq!(artifacts[0].path, path_str);
                        break;
                    }
                    other => panic!("MediaGalleryReply attendu, reçu : {other:?}"),
                }
            }
        }

        let _ = shutdown_tx.send(());
        let _ = server_task.await;
        #[cfg(unix)]
        let _ = std::fs::remove_file(&endpoint);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn home_ipc_devices_and_tier3_card_plus_real_mqtt_dispatch() {
        use hakille_home::protocols::mqtt::{MqttPacket, read_mqtt_packet};
        use hakille_home::{DeviceCategory, HomeDevice, ProtocolKind, RoomZone, SovereignHomeHub};
        use std::sync::Arc;
        use std::time::Duration;
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        let id = uuid::Uuid::now_v7();
        let dir = std::env::temp_dir().join(format!("hakille_home_ipc_{id}"));
        std::fs::create_dir_all(&dir).unwrap();
        #[cfg(unix)]
        let endpoint = format!("/tmp/hakille-home-ipc-{id}.sock");
        #[cfg(windows)]
        let endpoint = format!(r"\\.\pipe\hakille-home-ipc-{id}");
        let token = transport::fresh_token_hex();
        let listener = transport::IpcListener::bind(&endpoint).await.unwrap();

        // Broker MQTT loopback réel
        let mqtt_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mqtt_addr = mqtt_listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = mqtt_listener.accept().await.unwrap();
            let _ = read_mqtt_packet(&mut sock).await.unwrap();
            sock.write_all(
                &MqttPacket::ConnAck {
                    session_present: false,
                    reason_code: 0,
                }
                .to_bytes(),
            )
            .await
            .unwrap();
            if let MqttPacket::Publish { packet_id, .. } =
                read_mqtt_packet(&mut sock).await.unwrap()
            {
                sock.write_all(
                    &MqttPacket::PubAck {
                        packet_id: packet_id.unwrap(),
                    }
                    .to_bytes(),
                )
                .await
                .unwrap();
            }
        });

        let home_hub = Arc::new(SovereignHomeHub::open(dir.join("home.redb")).await.unwrap());
        home_hub
            .upsert_room(RoomZone::new("salon", "Salon", 0, [0.0, 1.4, -3.0]))
            .await
            .unwrap();
        home_hub
            .register_device(HomeDevice::new(
                "plug-salon",
                "Prise Salon",
                "salon",
                ProtocolKind::Mqtt,
                DeviceCategory::Switch,
                format!("mqtt://{mqtt_addr}"),
            ))
            .await
            .unwrap();
        home_hub
            .register_device(HomeDevice::new(
                "door-front",
                "Porte Entrée",
                "salon",
                ProtocolKind::Matter,
                DeviceCategory::DoorLock,
                "127.0.0.1:5540",
            ))
            .await
            .unwrap();

        let user_id = hakille_core::UserId(uuid::Uuid::now_v7());
        let fabric =
            hakille_kernel::CognitiveFabric::new(hakille_kernel::CognitiveManifest::default());
        let constitution = hakille_constitution::Constitution::default_v1();
        let mut orchestrator = CognitiveOrchestrator::new(fabric, user_id, constitution);
        let mut presence = PresenceIndicator::new();
        let mut cost_counter = CostCounter::new(10_000, "EUR");
        let mut continuity = ContinuityTracker::new();
        let mut initiative = BoundedInitiativeTracker::new();
        let mut generality = GeneralityTracker::new();
        let mut audit_journal = AuditJournal::new(
            dir.join("audit.redb").to_str().unwrap(),
            ed25519_dalek::SigningKey::from_bytes(&[11u8; 32]),
        )
        .unwrap();
        let authority = hakille_core::CapabilityAuthority::new();
        let memory_path = dir.join("memory.json");
        let mut outbox = Outbox::new();
        let registry = hakille_agents::AgentRegistry::new();

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let server_token = token.clone();
        let server_dir = dir.clone();
        let hub_srv = Arc::clone(&home_hub);
        let server_task = tokio::spawn(async move {
            run_serve_on_listener(
                listener,
                server_token,
                &mut orchestrator,
                &mut presence,
                &mut cost_counter,
                &mut continuity,
                &mut initiative,
                &mut generality,
                &mut audit_journal,
                &authority,
                &memory_path,
                &mut outbox,
                &server_dir,
                None,
                &registry,
                None,
                Some(hub_srv),
                Some(shutdown_rx),
            )
            .await
            .unwrap();
        });

        tokio::time::sleep(Duration::from_millis(40)).await;
        let mut stream = transport::connect(&endpoint).await.unwrap();
        transport::client_handshake(&mut stream, &token)
            .await
            .unwrap();
        let (reader, mut writer) = tokio::io::split(stream);
        let mut reader = BufReader::new(reader);

        // 1. HomeDevicesRequest retourne les 2 appareils et 19 protocoles
        transport::write_frame(
            &mut writer,
            &Frame::request(1, UiToKernel::HomeDevicesRequest),
        )
        .await
        .unwrap();
        let f1 = transport::read_frame(&mut reader).await.unwrap().unwrap();
        assert_eq!(f1.id, 1);
        match f1.body {
            FrameBody::Reply {
                message:
                    KernelToUi::HomeDevicesReply {
                        devices, protocols, ..
                    },
            } => {
                assert_eq!(devices.len(), 2);
                assert_eq!(protocols.len(), 19);
            }
            other => panic!("HomeDevicesReply attendu, reçu {other:?}"),
        }

        // 2. Commande Tier 3 (unlock) SANS maintien 600 ms -> émet ShowCard et NE s'exécute PAS
        transport::write_frame(
            &mut writer,
            &Frame::request(
                2,
                UiToKernel::HomeCommandRequest {
                    device_id: "door-front".into(),
                    action: "unlock".into(),
                    params_json: "{}".into(),
                },
            ),
        )
        .await
        .unwrap();
        let mut saw_card = false;
        let mut saw_reply = false;
        for _ in 0..3 {
            let f = transport::read_frame(&mut reader).await.unwrap().unwrap();
            if f.id == 2 {
                if let FrameBody::Reply {
                    message:
                        KernelToUi::HomeCommandReply {
                            executed_on_wire,
                            summary,
                            ..
                        },
                } = f.body
                {
                    assert!(!executed_on_wire);
                    assert!(summary.contains("600 ms"));
                    saw_reply = true;
                }
            } else if let FrameBody::Reply {
                message: KernelToUi::ShowCard(card),
            } = f.body
            {
                assert!(card.verb.contains("600 ms"));
                saw_card = true;
            }
        }
        assert!(saw_card && saw_reply);

        // 3. Commande MQTT réelle Tier 1 sur le courtier loopback
        transport::write_frame(
            &mut writer,
            &Frame::request(
                3,
                UiToKernel::HomeCommandRequest {
                    device_id: "plug-salon".into(),
                    action: "on".into(),
                    params_json: "{}".into(),
                },
            ),
        )
        .await
        .unwrap();
        let f3 = transport::read_frame(&mut reader).await.unwrap().unwrap();
        assert_eq!(f3.id, 3);
        match f3.body {
            FrameBody::Reply {
                message:
                    KernelToUi::HomeCommandReply {
                        executed_on_wire,
                        receipt_sha256,
                        ..
                    },
            } => {
                assert!(executed_on_wire);
                assert_eq!(receipt_sha256.as_deref().map(str::len), Some(64));
            }
            other => panic!("HomeCommandReply MQTT attendu, reçu {other:?}"),
        }

        let _ = shutdown_tx.send(());
        let _ = server_task.await;
        #[cfg(unix)]
        let _ = std::fs::remove_file(&endpoint);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
