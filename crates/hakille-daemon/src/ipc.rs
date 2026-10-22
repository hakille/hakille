//! Mode `--serve` : serveur IPC local (pipe nommé Windows / socket Unix)
//! exposant le contrat `hakille-ipc` au HUD desktop, sans REPL.
//!
//! Un seul client à la fois, requêtes servies séquentiellement. Aucun succès
//! simulé : chaque requête reçoit une Reply ou une Error/Toast explicite, et
//! toute erreur réelle du tour est propagée telle quelle au HUD.
//!
//! Extension additive v1 : le HUD peut consulter le journal d'audit, la vue
//! mémoire / le recall, mémoriser un fait et lire un statut minimal — toutes
//! les données proviennent des API réelles (journal, graphe, orchestrateurs,
//! compteurs), jamais d'une liste inventée.

use std::collections::HashMap;
use std::time::Instant;

use hakille_audit::entry::AuditEntry;
use hakille_audit::journal::AuditJournal;
use hakille_ipc::{ErrorCode, Frame, FrameBody};
use hakille_kernel::orchestrator::{ActionCardPreview, CognitiveOrchestrator};
use hakille_metrics::{BoundedInitiativeTracker, ContinuityTracker, GeneralityTracker};
use hakille_sync::{Outbox, OutboxEntry};
use hakille_ui::{CostCounter, MemoryViewerPanel, PresenceIndicator};
use hakille_ui_protocol::{
    ActionCard, AgentDto, DataExit, JournalEntryDto, KernelToUi, MemoryRowDto, Money, Provenance,
    StatusDto, UiToKernel,
};

use crate::{
    audit_action_with_id, audit_memory_event, persist_memory_to, run_turn_core, save_outbox,
    turn_error_hint,
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
fn remember_parts(text: &str) -> (&str, &str, &str) {
    let parts: Vec<&str> = text.splitn(3, ' ').collect();
    if parts.len() == 3 {
        (parts[0], parts[1], parts[2])
    } else {
        ("Fait", "est", text)
    }
}

/// Vue mémoire : `limit` dernières lignes (dernière activation décroissante ;
/// le graphe est un index, pas une file — l'ordre est explicite et testé).
fn memory_view_rows(graph: &hakille_memory::KnowledgeGraph, limit: usize) -> Vec<MemoryRowDto> {
    let mut triplets: Vec<&hakille_memory::Triplet> = graph.all_triplets();
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
    pending_cards: &mut HashMap<u64, ActionCard>,
    audit_journal: &AuditJournal,
    outbox: &mut Outbox,
    data_dir: &std::path::Path,
    card_id: u64,
    approved: bool,
) -> KernelToUi {
    let Some(card) = pending_cards.get(&card_id).cloned() else {
        return KernelToUi::Toast {
            text: format!("carte #{card_id} inconnue ou déjà traitée — aucune décision"),
        };
    };
    let action_type = if approved {
        "ActionCardApproved"
    } else {
        "ActionCardRejected"
    };
    let verb = if approved { "approuvée" } else { "refusée" };

    // Effet réel, calculé AVANT l'audit : l'entrée de journal doit décrire ce
    // qui a réellement eu lieu, pas une intention.
    let (effect_kind, effect_detail) = if approved {
        let dedup_key = format!(
            "card#{}|{}|{}",
            card.id,
            card.verb.to_lowercase(),
            card.object.to_lowercase()
        );
        let entry = OutboxEntry::new(&card.verb, &card.object, &dedup_key);
        let added = outbox.enqueue(entry);
        if added {
            // Persistance crash-safe : l'effet doit survivre au redémarrage.
            save_outbox(data_dir, outbox);
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
            // L'audit a échoué : on ANNULE l'effet mis en file (la décision
            // n'est pas validée, donc rien ne doit être durable).
            if approved {
                let dedup_key = format!(
                    "card#{}|{}|{}",
                    card.id,
                    card.verb.to_lowercase(),
                    card.object.to_lowercase()
                );
                outbox.remove(&dedup_key);
                save_outbox(data_dir, outbox);
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

/// Boucle serveur : jeton neuf écrit pour le HUD, écoute sur l'endpoint local,
/// un client à la fois. Ne retourne jamais en marche normale (sauf erreur de
/// bind : message explicite + `Err`, jamais de serveur silencieux).
#[allow(clippy::too_many_arguments)]
pub async fn run_serve(
    orchestrator: &mut CognitiveOrchestrator,
    presence: &mut PresenceIndicator,
    cost_counter: &mut CostCounter,
    continuity_tracker: &mut ContinuityTracker,
    bounded_initiative: &mut BoundedInitiativeTracker,
    generality_tracker: &mut GeneralityTracker,
    audit_journal: &mut AuditJournal,
    memory_path: &std::path::Path,
    outbox: &mut Outbox,
    data_dir: &std::path::Path,
    agent_runtime: Option<
        std::sync::Arc<std::sync::Mutex<crate::agent_runtime::ReferenceAgentRuntime>>,
    >,
    agent_registry: &hakille_agents::AgentRegistry,
) -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = hakille_ipc::transport::default_endpoint();
    let token_path = hakille_ipc::transport::default_token_path();
    let token = hakille_ipc::transport::fresh_token_hex();
    hakille_ipc::transport::write_token_file(&token_path, &token)?;
    let listener = match hakille_ipc::transport::IpcListener::bind(&endpoint).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("IPC: bind impossible sur {endpoint} — {e} (aucun serveur ne démarre)");
            return Err(e.into());
        }
    };
    println!("IPC: endpoint {endpoint}");
    println!("IPC: token file {}", token_path.display());
    println!("IPC: prêt — en attente du HUD (hakille-desktop)");

    // Les cartes en attente sont liées à la session du HUD ; l'identifiant
    // reste monotone entre reconnexions pour ne jamais réutiliser un id.
    let mut next_card_id: u64 = 1;
    loop {
        let mut stream = match listener.accept().await {
            Ok(stream) => stream,
            Err(e) => {
                eprintln!("IPC: accept échoué — {e}");
                // Anti-boucle-chaude : un échec persistant (création
                // d'instance pipe) ne doit pas saturer le CPU ni les logs.
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                continue;
            }
        };
        if let Err(e) = hakille_ipc::transport::server_handshake(&mut stream, &token).await {
            eprintln!("IPC: handshake refusé — {e}");
            continue;
        }
        println!("IPC: client connecté");
        let mut pending_cards: HashMap<u64, ActionCard> = HashMap::new();
        serve_client(
            &mut stream,
            orchestrator,
            presence,
            cost_counter,
            continuity_tracker,
            bounded_initiative,
            generality_tracker,
            audit_journal,
            memory_path,
            outbox,
            data_dir,
            agent_runtime.as_ref(),
            agent_registry,
            &mut pending_cards,
            &mut next_card_id,
        )
        .await;
        println!("IPC: client déconnecté — en attente du prochain HUD");
    }
}

/// Sert un client connecté : lit les trames une à une, répond toujours
/// (Reply, Toast ou Error), et rend la main à l'accept à la déconnexion.
#[allow(clippy::too_many_arguments)]
async fn serve_client<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    stream: &mut S,
    orchestrator: &mut CognitiveOrchestrator,
    presence: &mut PresenceIndicator,
    cost_counter: &mut CostCounter,
    continuity_tracker: &mut ContinuityTracker,
    bounded_initiative: &mut BoundedInitiativeTracker,
    generality_tracker: &mut GeneralityTracker,
    audit_journal: &mut AuditJournal,
    memory_path: &std::path::Path,
    outbox: &mut Outbox,
    data_dir: &std::path::Path,
    agent_runtime: Option<
        &std::sync::Arc<std::sync::Mutex<crate::agent_runtime::ReferenceAgentRuntime>>,
    >,
    agent_registry: &hakille_agents::AgentRegistry,
    pending_cards: &mut HashMap<u64, ActionCard>,
    next_card_id: &mut u64,
) {
    loop {
        let frame = match hakille_ipc::transport::read_frame(stream).await {
            Ok(Some(frame)) => frame,
            Ok(None) => return,
            Err(e) => {
                eprintln!("IPC: trame illisible — {e}");
                return;
            }
        };
        let id = frame.id;
        let mut followups: Vec<Frame> = Vec::new();
        let is_turn = matches!(
            &frame.body,
            FrameBody::Request {
                message: UiToKernel::HudSubmit { .. }
            }
        );
        if is_turn {
            // Notifie l'attente réelle avant le tour (id 0 : hors requête).
            // Le client filtre `Presence` et ne résout jamais la requête en
            // vol avec cette trame.
            let acting = Frame::reply(
                0,
                KernelToUi::Presence(hakille_ui_protocol::PresenceState::Acting),
            );
            if let Err(e) = hakille_ipc::transport::write_frame(stream, &acting).await {
                eprintln!("IPC: présence Acting non écrite (id={id}) — {e}");
                return;
            }
        }
        let response = match frame.body {
            FrameBody::Request {
                message: UiToKernel::HudSubmit { query },
            } => {
                if query.trim().is_empty() {
                    eprintln!("IPC: requête vide refusée (id={id})");
                    Frame::error(id, ErrorCode::BadFrame, "requête vide refusée")
                } else {
                    let started = Instant::now();
                    match run_turn_core(
                        orchestrator,
                        presence,
                        cost_counter,
                        continuity_tracker,
                        bounded_initiative,
                        generality_tracker,
                        audit_journal,
                        memory_path,
                        &query,
                    )
                    .await
                    {
                        Ok(outcome) => {
                            let latency_ms = started.elapsed().as_millis();
                            println!(
                                "IPC: requête servie (id={id}, {latency_ms}ms, coût {}¢)",
                                outcome.cost_cents
                            );
                            // Le noyau a détecté une action irréversible et
                            // produit un plan sec. On expose une carte réelle
                            // au HUD, après audit de la proposition.
                            if outcome.routing_used.starts_with("Plan/Dry-run")
                                && let Some(preview) =
                                    CognitiveOrchestrator::action_card_preview(&query)
                            {
                                let card = card_from_preview(*next_card_id, preview);
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
                                        *next_card_id = (*next_card_id).saturating_add(1);
                                        pending_cards.insert(card.id, card.clone());
                                        // La présence daemon reflète elle aussi
                                        // l'attente de décision, pas seulement
                                        // l'état local du HUD.
                                        presence
                                            .set_state(hakille_ui::PresenceState::DecisionRequired);
                                        println!(
                                            "IPC: carte #{} proposée (id={id}, 1 en attente)",
                                            card.id
                                        );
                                        followups.push(Frame::reply(
                                            0,
                                            KernelToUi::Presence(
                                                hakille_ui_protocol::PresenceState::DecisionRequired,
                                            ),
                                        ));
                                        followups.push(Frame::reply(0, KernelToUi::ShowCard(card)));
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "IPC: carte non présentée (id={id}) — audit indisponible: {e}"
                                        );
                                        followups.push(Frame::reply(
                                            0,
                                            KernelToUi::Toast {
                                                text: format!(
                                                    "carte non présentée — audit indisponible: {e}"
                                                ),
                                            },
                                        ));
                                    }
                                }
                            }
                            if outcome.non_empty {
                                let validation = match &outcome.persistence_warning {
                                    Some(warning) => {
                                        Some((format!("⚠ {warning}"), outcome.cost_cents))
                                    }
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
                                // Miroir honnête du REPL : jamais de réponse
                                // vide silencieuse.
                                eprintln!("IPC: réponse vide (id={id}) — aucun succès simulé");
                                Frame::reply(
                                    id,
                                    KernelToUi::Toast {
                                        text: "réponse vide (EOS immédiat) — aucun succès simulé"
                                            .into(),
                                    },
                                )
                            }
                        }
                        Err(e) => {
                            let text = match turn_error_hint(&e.to_string()) {
                                Some(hint) => format!("{e} — {hint}"),
                                None => e.to_string(),
                            };
                            eprintln!("IPC: tour échoué (id={id}) — {text}");
                            Frame::reply(id, KernelToUi::Toast { text })
                        }
                    }
                }
            }
            FrameBody::Request {
                message: UiToKernel::Approve { card_id },
            } => {
                let reply = decide_card(
                    pending_cards,
                    audit_journal,
                    outbox,
                    data_dir,
                    card_id,
                    true,
                );
                match &reply {
                    KernelToUi::CardDecisionAck {
                        audit_id, effect, ..
                    } => {
                        presence.set_state(hakille_ui::PresenceState::Idle);
                        println!(
                            "IPC: carte #{card_id} approuvée — décision auditée [{audit_id}] — {}",
                            effect.as_deref().unwrap_or("aucun effet")
                        );
                    }
                    KernelToUi::Toast { text } => {
                        eprintln!("IPC: approbation carte #{card_id} refusée — {text}");
                    }
                    _ => {}
                }
                Frame::reply(id, reply)
            }
            FrameBody::Request {
                message: UiToKernel::Reject { card_id },
            } => {
                let reply = decide_card(
                    pending_cards,
                    audit_journal,
                    outbox,
                    data_dir,
                    card_id,
                    false,
                );
                match &reply {
                    KernelToUi::CardDecisionAck { audit_id, .. } => {
                        presence.set_state(hakille_ui::PresenceState::Idle);
                        println!(
                            "IPC: carte #{card_id} refusée — décision auditée [{audit_id}] (aucun effet)"
                        );
                    }
                    KernelToUi::Toast { text } => {
                        eprintln!("IPC: refus carte #{card_id} non enregistré — {text}");
                    }
                    _ => {}
                }
                Frame::reply(id, reply)
            }
            FrameBody::Request {
                message: UiToKernel::Edit { card_id },
            } => {
                eprintln!(
                    "IPC: édition carte #{card_id} non supportée — aucune mutation, carte conservée"
                );
                Frame::reply(
                    id,
                    KernelToUi::Toast {
                        text: "modification de carte non supportée — aucune mutation ; validez ou refusez"
                            .into(),
                    },
                )
            }
            FrameBody::Request {
                message: UiToKernel::JournalRequest { limit },
            } => match audit_journal.all_entries() {
                Ok(entries) => {
                    let listed: Vec<JournalEntryDto> = entries
                        .iter()
                        .rev()
                        .take(limit.min(MAX_LIST_ROWS))
                        .map(journal_entry_dto)
                        .collect();
                    println!("IPC: journal servi (id={id}, {} entrées)", listed.len());
                    Frame::reply(id, KernelToUi::JournalReply { entries: listed })
                }
                Err(e) => {
                    eprintln!("IPC: journal indisponible (id={id}) — {e}");
                    Frame::error(
                        id,
                        ErrorCode::Internal,
                        format!("journal indisponible: {e}"),
                    )
                }
            },
            FrameBody::Request {
                message: UiToKernel::MemoryRequest { query, limit },
            } => match query {
                Some(q) if q.trim().is_empty() => {
                    eprintln!("IPC: rappel vide refusé (id={id})");
                    Frame::error(id, ErrorCode::BadFrame, "rappel vide refusé")
                }
                Some(q) => {
                    let recalled = orchestrator.recall(q.trim());
                    let rows = recall_rows(&orchestrator.memory_graph, &recalled, limit);
                    println!("IPC: rappel servi (id={id}, {} lignes)", rows.len());
                    Frame::reply(id, KernelToUi::MemoryReply { rows })
                }
                None => {
                    let rows = memory_view_rows(&orchestrator.memory_graph, limit);
                    println!("IPC: mémoire servie (id={id}, {} lignes)", rows.len());
                    Frame::reply(id, KernelToUi::MemoryReply { rows })
                }
            },
            FrameBody::Request {
                message: UiToKernel::MemoryRemember { text },
            } => {
                if text.trim().is_empty() {
                    eprintln!("IPC: mémoire vide refusée (id={id})");
                    Frame::error(id, ErrorCode::BadFrame, "mémoire vide refusée")
                } else {
                    let trimmed = text.trim().to_string();
                    // Mutation auditée AVANT application : si le journal est
                    // indisponible, rien n'est mémorisé.
                    if let Err(e) = audit_memory_event(
                        audit_journal,
                        "MemoryRemember",
                        format!("Demande de mémorisation via HUD: '{trimmed}'"),
                        serde_json::json!({ "text": trimmed, "origin": "ipc" }),
                    ) {
                        eprintln!("IPC: audit mémoire impossible (id={id}) — {e}");
                        Frame::reply(
                            id,
                            KernelToUi::Toast {
                                text: format!("mémorisation refusée — audit indisponible: {e}"),
                            },
                        )
                    } else {
                        let (subject, predicate, object) = remember_parts(&trimmed);
                        let result = orchestrator.remember(subject, predicate, object);
                        let ack =
                            |triplet_id: &hakille_core::TripletId,
                             persisted: &Result<(), String>| {
                                let id_text = triplet_id.0.to_string();
                                match persisted {
                                    Ok(()) => {
                                        println!("IPC: mémoire enregistrée et persistée (id={id})");
                                        KernelToUi::MemoryRememberAck { id: id_text }
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "IPC: mémoire en RAM non persistée (id={id}) — {e}"
                                        );
                                        KernelToUi::Toast {
                                            text: format!(
                                                "fait [{id_text}] en mémoire volatile, persistance échouée: {e}"
                                            ),
                                        }
                                    }
                                }
                            };
                        match result {
                            hakille_memory::InsertResult::Inserted(triplet_id) => {
                                let persisted = persist_memory_to(orchestrator, memory_path);
                                Frame::reply(id, ack(&triplet_id, &persisted))
                            }
                            hakille_memory::InsertResult::ContradictionResolved { new, .. } => {
                                let persisted = persist_memory_to(orchestrator, memory_path);
                                println!(
                                    "IPC: contradiction résolue (id={id}, remplacement tracé)"
                                );
                                Frame::reply(id, ack(&new, &persisted))
                            }
                            hakille_memory::InsertResult::NeedsUserResolution {
                                existing,
                                incoming,
                            } => {
                                // Rien n'a été écrasé : le HUD reçoit les deux ids
                                // et décide (même contrat que le REPL).
                                eprintln!(
                                    "IPC: contradiction — décision utilisateur requise (id={id})"
                                );
                                Frame::reply(
                                    id,
                                    KernelToUi::Toast {
                                        text: format!(
                                            "contradiction — décision requise : existant [{}] vs entrant [{}] ; rien écrasé",
                                            existing.id.0, incoming.id.0
                                        ),
                                    },
                                )
                            }
                        }
                    }
                }
            }
            FrameBody::Request {
                message: UiToKernel::StatusRequest,
            } => {
                let status = StatusDto {
                    model: format!(
                        "Qwen 0.5B Q4_K_M (timeout {} ms)",
                        orchestrator.system1.llm_timeout_ms()
                    ),
                    cost: cost_counter.formatted_total(),
                    state: format!("{:?}", presence.state()),
                    memory_count: orchestrator.memory_graph.len() as u64,
                };
                println!("IPC: statut servi (id={id})");
                Frame::reply(id, KernelToUi::StatusReply { status })
            }
            FrameBody::Request {
                message: UiToKernel::AgentListRequest,
            } => {
                // Liste HONNÊTE : `artifact_present` distingue un agent
                // réellement exécutable d'un manifeste déclaratif.
                let guard = agent_runtime.as_ref().and_then(|r| r.lock().ok());
                let agents = agent_dtos(guard.as_deref(), agent_registry);
                drop(guard);
                println!("IPC: agents listés (id={id}, {} agent(s))", agents.len());
                Frame::reply(id, KernelToUi::AgentListReply { agents })
            }
            FrameBody::Request {
                message: UiToKernel::AgentRunRequest { name, input_json },
            } => {
                // Exécution RÉELLE dans le bac à sable (fuel/timeout/mémoire
                // bornés). Le runtime est bloquant (Wasmtime) : on le sort du
                // runtime async pour ne pas figer le serveur IPC.
                let agent_name = name.clone();
                let input = input_json.clone();
                let reply = match agent_runtime.as_ref() {
                    Some(runtime) => {
                        // Le runtime est bloquant (Wasmtime) : on le sort du
                        // runtime async pour ne pas figer le serveur IPC.
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
                                // Audit : on journalise le RÉSULTAT (empreintes,
                                // jamais la charge utile complète).
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
                                            "IPC: agent {agent} exécuté ({duration_ms} ms) — audit [{audit_id}]"
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
                                        // Audit indisponible : l'exécution a eu
                                        // lieu mais n'est pas prouvable → on le dit.
                                        eprintln!(
                                            "IPC: agent {agent} exécuté mais audit indisponible: {e}"
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
                                eprintln!("IPC: exécution d'agent échouée ({name}) — {e}");
                                KernelToUi::Toast {
                                    text: format!("agent {name} : {e}"),
                                }
                            }
                            Err(e) => {
                                eprintln!("IPC: tâche d'exécution d'agent interrompue — {e}");
                                KernelToUi::Toast {
                                    text: format!("exécution d'agent interrompue : {e}"),
                                }
                            }
                        }
                    }
                    None => {
                        eprintln!(
                            "IPC: exécution d'agent refusée (id={id}) — runtime indisponible"
                        );
                        KernelToUi::Toast {
                            text: "runtime d'agents indisponible — aucun agent exécuté".into(),
                        }
                    }
                };
                Frame::reply(id, reply)
            }
            FrameBody::Request { .. } => {
                eprintln!("IPC: message non supporté (id={id}) — refus explicite");
                Frame::reply(
                    id,
                    KernelToUi::Toast {
                        text: "non supporté par le démon v1 — refus explicite".into(),
                    },
                )
            }
            FrameBody::Hello { .. }
            | FrameBody::Welcome { .. }
            | FrameBody::Reply { .. }
            | FrameBody::Error { .. } => {
                eprintln!("IPC: trame inattendue côté serveur (id={id})");
                Frame::error(id, ErrorCode::BadFrame, "trame inattendue côté serveur")
            }
        };
        let card_shown = followups.iter().any(|frame| {
            matches!(
                &frame.body,
                FrameBody::Reply {
                    message: KernelToUi::ShowCard(_),
                    ..
                }
            )
        });
        if is_turn && !card_shown {
            // Le tour est terminé (succès ou échec) : la présence revient à
            // l'état de veille avant la réponse finale. Une carte conserve au
            // contraire l'état DecisionRequired.
            let idle = Frame::reply(
                0,
                KernelToUi::Presence(hakille_ui_protocol::PresenceState::Idle),
            );
            if let Err(e) = hakille_ipc::transport::write_frame(stream, &idle).await {
                eprintln!("IPC: présence Idle non écrite (id={id}) — {e}");
                return;
            }
        }
        if let Err(e) = hakille_ipc::transport::write_frame(stream, &response).await {
            eprintln!("IPC: écriture échouée (id={id}) — {e}; retour à l'écoute");
            return;
        }
        for followup in followups {
            if let Err(e) = hakille_ipc::transport::write_frame(stream, &followup).await {
                eprintln!("IPC: trame de suivi non écrite (id={id}) — {e}; retour à l'écoute");
                return;
            }
        }
    }
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
            ("Marie", "aime", "le jazz")
        );
        assert_eq!(remember_parts("jazz"), ("Fait", "est", "jazz"));
        assert_eq!(
            remember_parts("Réunion lundi 10h salle B"),
            ("Réunion", "lundi", "10h salle B")
        );
    }

    #[test]
    fn memory_view_rows_orders_by_last_activation_and_limits() {
        let mut graph = hakille_memory::KnowledgeGraph::new();
        graph.insert(triplet("ancien", -120));
        graph.insert(triplet("milieu", -60));
        graph.insert(triplet("récent", -10));

        let rows = memory_view_rows(&graph, 2);
        assert_eq!(rows.len(), 2, "limit respecté");
        assert_eq!(rows[0].text, "récent", "plus récent d'abord");
        assert_eq!(rows[1].text, "milieu");
        assert!(!rows[0].id.is_empty(), "id réel du triplet");
        assert_eq!(rows[0].source, "User");
        assert!(rows[0].tags.contains(&"Personal".to_string()));

        assert!(memory_view_rows(&graph, 0).is_empty(), "limit 0 = vide");
        assert_eq!(memory_view_rows(&graph, 99).len(), 3);
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

        let mut cards: HashMap<u64, ActionCard> = HashMap::new();
        cards.insert(
            42,
            ActionCard {
                id: 42,
                verb: "Transférer".into(),
                object: "25 EUR à Awa".into(),
                reason: "demande explicite".into(),
                cost: Money::from_cents(2500, "EUR"),
                data_exit: DataExit::Local,
                reversible: false,
                provenance: Provenance::Hakille,
            },
        );
        let mut outbox = Outbox::new();

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
        let audit_id_debug = format!("{:?}", approved_entry.id);
        assert!(
            audit_id_debug.contains(&audit_id),
            "l'accusé porte l'identifiant réel de l'entrée auditée : {audit_id_debug} vs {audit_id}"
        );

        // Refus : AUCUN effet, et l'audit le dit.
        cards.insert(
            43,
            ActionCard {
                id: 43,
                verb: "Supprimer".into(),
                object: "/vfs/notes.txt".into(),
                reason: "irréversible".into(),
                cost: Money::from_cents(0, "EUR"),
                data_exit: DataExit::Local,
                reversible: false,
                provenance: Provenance::Hakille,
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
}
