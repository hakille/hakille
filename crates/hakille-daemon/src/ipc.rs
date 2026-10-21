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

use std::time::Instant;

use hakille_audit::entry::AuditEntry;
use hakille_audit::journal::AuditJournal;
use hakille_ipc::{ErrorCode, Frame, FrameBody};
use hakille_kernel::orchestrator::CognitiveOrchestrator;
use hakille_metrics::{BoundedInitiativeTracker, ContinuityTracker, GeneralityTracker};
use hakille_ui::{CostCounter, MemoryViewerPanel, PresenceIndicator};
use hakille_ui_protocol::{JournalEntryDto, KernelToUi, MemoryRowDto, StatusDto, UiToKernel};

use crate::{audit_memory_event, persist_memory_to, run_turn_core, turn_error_hint};

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
    audit_journal: &AuditJournal,
    memory_path: &std::path::Path,
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
    audit_journal: &AuditJournal,
    memory_path: &std::path::Path,
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
        if is_turn {
            // Le tour est terminé (succès ou échec) : la présence revient à
            // l'état de veille avant la réponse finale.
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
