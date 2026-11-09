//! Machine d'état bornée, sans réseau ni accès aux credentials de l'hôte.
use crate::types::*;
use hakille_ipc::{Frame, FrameBody};
use hakille_ui_protocol::{KernelToUi, PROTOCOL_VERSION, UiToKernel};
use std::time::{Duration, Instant};

pub const MAX_JSON_BYTES: usize = 1024 * 1024;
pub const MAX_DRAFT_BYTES: usize = 8192;
pub const MAX_RECOVERY_OPERATIONS: usize = 32;
const MAX_SNAPSHOT_BYTES: usize = 512 * 1024;
const MAX_TEXT_BYTES: usize = 64 * 1024;
const HOLD_MS: u64 = 600;
const VOICE_ARM_TTL: Duration = Duration::from_secs(120);

#[derive(Clone)]
pub(crate) struct Core {
    pub snapshot: MobileSnapshot,
    pub(crate) next_id: u64,
    inflight: Option<UiToKernel>,
    hold_started: Option<Instant>,
    voice_started_at: Option<Instant>,
    saved_owner_cranks: Option<Vec<(String, u8)>>,
}
impl Default for Core {
    fn default() -> Self {
        Self {
            snapshot: MobileSnapshot::default(),
            next_id: 1,
            inflight: None,
            hold_started: None,
            voice_started_at: None,
            saved_owner_cranks: None,
        }
    }
}

impl Core {
    fn is_guest_active(&self) -> bool {
        self.snapshot
            .guest_pass
            .as_ref()
            .is_some_and(|g| !g.expired)
    }

    fn restore_owner_cranks(&mut self) {
        if let Some(saved) = self.saved_owner_cranks.take() {
            for (domain, crank) in saved {
                if let Some(d) = self
                    .snapshot
                    .domain_autonomies
                    .iter_mut()
                    .find(|d| d.domain == domain)
                {
                    d.crank = crank;
                    d.crank_label = crank_label(crank).to_string();
                }
            }
        }
    }

    pub fn dispatch(
        &mut self,
        action: MobileAction,
        now: Instant,
    ) -> Result<Option<Frame>, String> {
        // Une intention manuelle invalide la capture/lecture vocale en cours.
        // Un callback ancien ne doit toutefois pas annuler un nouveau contexte.
        if !matches!(
            &action,
            MobileAction::BeginVoice {}
                | MobileAction::CancelVoice {}
                | MobileAction::SubmitVoice { .. }
                | MobileAction::SetForeground { active: true }
                | MobileAction::FailBeforeSend { .. }
        ) {
            self.invalidate_voice();
        }
        match action {
            MobileAction::BeginVoice {} => {
                self.voice_available()?;
                if self.next_id >= u64::MAX - 1 {
                    return Err("Identifiants insuffisants pour un tour vocal".into());
                }
                self.invalidate_voice();
                self.snapshot.voice_context_id = Some(self.next_id);
                self.next_id += 1; // La vérification réserve aussi l'identifiant de requête.
                self.voice_started_at = Some(now);
                return Ok(None);
            }
            MobileAction::CancelVoice {} => {
                let pending_voice = self.snapshot.voice_request_id.is_some()
                    && self.snapshot.voice_request_id
                        == self
                            .snapshot
                            .pending
                            .as_ref()
                            .map(|pending| pending.request_id);
                self.invalidate_voice();
                if pending_voice {
                    self.interrupt("Tour vocal interrompu : résultat en vol non confirmé");
                }
                return Ok(None);
            }
            MobileAction::SetHub { hub_id } => {
                text(&hub_id, 2048, false)?;
                if hub_id.contains(['@', '?', '#']) {
                    return Err(
                        "Identifiant de hub sans credentials, query ni fragment requis".into(),
                    );
                }
                if self.snapshot.hub_id.as_deref() != Some(&hub_id) {
                    self.interrupt("Hub changé : aucune requête n'est rejouée");
                    self.restore_owner_cranks();
                    let draft = self.snapshot.draft.clone();
                    let foreground = self.snapshot.foreground;
                    let outcomes = self.snapshot.outcomes.clone();
                    let uncertain_operations = self.snapshot.uncertain_operations.clone();
                    let acknowledged_operations = self.snapshot.acknowledged_operations.clone();
                    let recovery_required = self.snapshot.recovery_required;
                    let domain_autonomies = self.snapshot.domain_autonomies.clone();
                    let tombstones = self.snapshot.tombstones.clone();
                    let erasure_receipts = self.snapshot.erasure_receipts.clone();
                    self.snapshot = MobileSnapshot {
                        hub_id: Some(hub_id),
                        draft,
                        foreground,
                        outcomes,
                        uncertain_operations,
                        acknowledged_operations,
                        recovery_required,
                        domain_autonomies,
                        tombstones,
                        erasure_receipts,
                        ..Default::default()
                    };
                    if recovery_required {
                        self.snapshot.connection = ConnectionState::Unknown;
                    }
                    self.cancel_hold();
                }
                return Ok(None);
            }
            MobileAction::SetDraft { text: value } => {
                text(&value, MAX_DRAFT_BYTES, true)?;
                self.cancel_hold();
                self.snapshot.draft = value;
                return Ok(None);
            }
            MobileAction::SetForeground { active } => {
                if !active {
                    self.detach_active_flight();
                    self.cancel_hold();
                    self.snapshot.microphone_enabled = false;
                }
                self.snapshot.foreground = active;
                return Ok(None);
            }
            MobileAction::DetachInFlight { request_id } => {
                if let Some(op) = self
                    .snapshot
                    .uncertain_operations
                    .iter_mut()
                    .find(|op| op.request_id == request_id)
                    && op.phase == OperationPhase::InFlight
                {
                    op.phase = OperationPhase::InFlightDetached;
                }
                self.refresh_recovery();
                return Ok(None);
            }
            MobileAction::SetMicrophone { enabled } => {
                self.snapshot.microphone_enabled = false;
                if enabled {
                    return Err(
                        "Capture microphone native non raccordée ; aucune écoute démarrée".into(),
                    );
                }
                return Ok(None);
            }
            MobileAction::CancelApproval {} => {
                self.cancel_hold();
                return Ok(None);
            }
            MobileAction::FailBeforeSend {
                request_id,
                message,
            } => {
                text(&message, 4096, false)?;
                if self.snapshot.pending.as_ref().map(|p| p.request_id) != Some(request_id) {
                    return Err("Échec avant envoi ancien ou sans requête correspondante".into());
                }
                self.complete("failed", format!("Non envoyée : {message}"), None);
                self.cancel_hold();
                return Ok(None);
            }
            MobileAction::AcknowledgeUnknown { request_id } => {
                if !self.snapshot.foreground {
                    return Err("Application inactive : prise de connaissance refusée".into());
                }
                self.acknowledge_unknown(request_id)?;
                return Ok(None);
            }
            MobileAction::CloseWhy {} => {
                self.snapshot.why_sheet = None;
                return Ok(None);
            }
            MobileAction::DismissPrivacyGuard {} => {
                self.snapshot.privacy_guard = None;
                self.snapshot.privacy_veil_active = false;
                return Ok(None);
            }
            MobileAction::StartGuestPass {
                name,
                power,
                duration_secs,
            } => {
                text(&name, 128, false)?;
                text(&power, 128, false)?;
                let dur = duration_secs.unwrap_or(3600);
                let dto = GuestPassDto::new(name.trim(), power.trim(), dur);
                if !dto.expired {
                    if self.saved_owner_cranks.is_none() {
                        self.saved_owner_cranks = Some(
                            self.snapshot
                                .domain_autonomies
                                .iter()
                                .map(|d| (d.domain.clone(), d.crank))
                                .collect(),
                        );
                    }
                    for d in &mut self.snapshot.domain_autonomies {
                        d.crank = 0;
                        d.crank_label = DomainAutonomyDto::label_for_crank(0).to_string();
                    }
                } else {
                    self.restore_owner_cranks();
                }
                self.snapshot.guest_pass = Some(dto);
                return Ok(None);
            }
            MobileAction::TickGuestPass { elapsed_secs } => {
                let Some(pass) = &mut self.snapshot.guest_pass else {
                    return Err("Aucun pass invité actif".into());
                };
                pass.tick(elapsed_secs);
                let expired = pass.expired;
                if expired {
                    self.restore_owner_cranks();
                }
                return Ok(None);
            }
            MobileAction::RevokeGuestPass {} => {
                self.restore_owner_cranks();
                self.snapshot.guest_pass = None;
                return Ok(None);
            }
            MobileAction::RequestWhy {
                target_id,
                target_type,
            } => {
                text(&target_id, 256, false)?;
                text(&target_type, 64, false)?;
                let why = if target_type == "card" {
                    if let Some(card) = self
                        .snapshot
                        .cards
                        .iter()
                        .find(|c| c.id.to_string() == target_id)
                    {
                        WhySheetDto {
                            target_id: target_id.clone(),
                            sentence: format!("Action proposée : {} {}", card.verb, card.object),
                            facts: vec![],
                            plan: card.reason.clone(),
                            powers: vec!["Intervention locale".to_string()],
                            confidence_word: "haute".to_string(),
                            confidence_value: 0.95,
                            alternatives: vec![
                                "Refuser l'action".to_string(),
                                "Modifier les paramètres".to_string(),
                            ],
                            model_line: "Qwen 2.5 0.5B Local CPU".to_string(),
                            cost_label: card.cost.display(),
                            data_line: match &card.data_exit {
                                hakille_ui_protocol::DataExit::Local => {
                                    "Local uniquement".to_string()
                                }
                                hakille_ui_protocol::DataExit::Cloud { provider } => {
                                    format!("Cloud ({provider})")
                                }
                            },
                            would_change: "L'action sera exécutée ou mise en file selon le mode."
                                .to_string(),
                        }
                    } else {
                        WhySheetDto::missing(&target_id, "Trace non conservée pour cette entrée")
                    }
                } else if target_type == "journal" {
                    if let Some(entry) = self.snapshot.journal.iter().find(|e| e.id == target_id) {
                        WhySheetDto {
                            target_id: target_id.clone(),
                            sentence: format!("{}: {}", entry.action, entry.summary),
                            facts: vec![],
                            plan: format!("Exécution validée à {}", entry.timestamp),
                            powers: vec!["Audit Ed25519".to_string()],
                            confidence_word: "haute".to_string(),
                            confidence_value: 1.0,
                            alternatives: vec!["Annuler l'action (Undo)".to_string()],
                            model_line: "Qwen 2.5 0.5B Local CPU".to_string(),
                            cost_label: "0.00 EUR".to_string(),
                            data_line: "Local uniquement".to_string(),
                            would_change: "Révocation dans le journal d'audit".to_string(),
                        }
                    } else {
                        WhySheetDto::missing(&target_id, "Trace non conservée pour cette entrée")
                    }
                } else {
                    WhySheetDto::missing(&target_id, "Trace non conservée pour cette entrée")
                };
                self.snapshot.why_sheet = Some(why);
                return Ok(None);
            }
            MobileAction::SetAutonomyCrank {
                domain,
                crank,
                max_cost_cents,
                max_actions,
                days_valid,
            } => {
                if self.is_guest_active() {
                    return Err("Action interdite en mode invité éphémère".into());
                }
                text(&domain, 64, false)?;
                if crank > 4 {
                    return Err("Cran d'autonomie invalide (0 à 4 attendu)".into());
                }
                if let Some(d) = self
                    .snapshot
                    .domain_autonomies
                    .iter_mut()
                    .find(|d| d.domain.eq_ignore_ascii_case(&domain))
                {
                    d.crank = crank;
                    d.crank_label = crank_label(crank).to_string();
                    if crank >= 3 {
                        d.envelope = Some(EnvelopeDto {
                            max_cost_cents: max_cost_cents.unwrap_or(1000),
                            max_actions: max_actions.unwrap_or(50),
                            days_valid: days_valid.unwrap_or(90),
                        });
                    } else {
                        d.envelope = None;
                    }
                    d.promotion_offerable = (crank == 1 || crank == 2) && d.validated_count >= 20;
                } else {
                    let mut d = DomainAutonomyDto::new(&domain);
                    d.crank = crank;
                    d.crank_label = crank_label(crank).to_string();
                    if crank >= 3 {
                        d.envelope = Some(EnvelopeDto {
                            max_cost_cents: max_cost_cents.unwrap_or(1000),
                            max_actions: max_actions.unwrap_or(50),
                            days_valid: days_valid.unwrap_or(90),
                        });
                    }
                    self.snapshot.domain_autonomies.push(d);
                }
                return Ok(None);
            }
            _ => {}
        }
        if !self.snapshot.foreground {
            return Err("Application inactive : demande refusée".into());
        }
        if self.snapshot.hub_id.is_none() {
            return Err("Aucun hub configuré".into());
        }
        if self.snapshot.pending.is_some() {
            return Err("Une requête est déjà en cours".into());
        }

        let voice_submission = matches!(&action, MobileAction::SubmitVoice { .. });
        let (request, kind, mutating) = match action {
            MobileAction::SubmitVoice {
                context_id,
                text: value,
            } => {
                if self.snapshot.voice_context_id != Some(context_id)
                    || self.snapshot.voice_request_id.is_some()
                {
                    return Err("Contexte vocal ancien, absent ou déjà soumis".into());
                }
                if !self.voice_started_at.is_some_and(|start| {
                    now.checked_duration_since(start)
                        .is_some_and(|elapsed| elapsed < VOICE_ARM_TTL)
                }) {
                    self.invalidate_voice();
                    return Err("Tour vocal expiré : armer une nouvelle capture".into());
                }
                self.voice_available()?;
                text(&value, MAX_DRAFT_BYTES, false)?;
                (UiToKernel::HudSubmit { query: value }, "submit", true)
            }
            MobileAction::Submit {} => {
                text(&self.snapshot.draft, MAX_DRAFT_BYTES, false)?;
                (
                    UiToKernel::HudSubmit {
                        query: self.snapshot.draft.clone(),
                    },
                    "submit",
                    true,
                )
            }
            MobileAction::RefreshStatus {} => (UiToKernel::StatusRequest, "refresh_status", false),
            MobileAction::LoadMemory { query, limit } => {
                rows(limit)?;
                if let Some(query) = &query {
                    text(query, MAX_DRAFT_BYTES, false)?;
                }
                (
                    UiToKernel::MemoryRequest { query, limit },
                    "load_memory",
                    false,
                )
            }
            MobileAction::Remember { text: value } => {
                text(&value, MAX_DRAFT_BYTES, false)?;
                (UiToKernel::MemoryRemember { text: value }, "remember", true)
            }
            MobileAction::LoadJournal { limit } => {
                rows(limit)?;
                (UiToKernel::JournalRequest { limit }, "load_journal", false)
            }
            MobileAction::LoadAgents {} => (UiToKernel::AgentListRequest, "load_agents", false),
            MobileAction::RunAgent { name, input_json } => {
                text(&name, 128, false)?;
                text(&input_json, MAX_DRAFT_BYTES, false)?;
                serde_json::from_str::<serde_json::Value>(&input_json)
                    .map_err(|_| "Entrée agent JSON invalide")?;
                let Some(agent) = self
                    .snapshot
                    .agents
                    .iter()
                    .find(|agent| agent.name == name && agent.artifact_present)
                else {
                    return Err(
                        "Agent non confirmé exécutable par le hub ; actualiser sa liste".into(),
                    );
                };
                if self.is_guest_active() && !agent.required_caps.is_empty() {
                    return Err(
                        "Mode invité : seuls les agents sans capacité hôte sont autorisés".into(),
                    );
                }
                (
                    UiToKernel::AgentRunRequest { name, input_json },
                    "run_agent",
                    true,
                )
            }
            MobileAction::BeginApproval { card_id } => {
                self.card_available(card_id)?;
                self.snapshot.approval = Some(ApprovalState {
                    card_id,
                    required_hold_ms: HOLD_MS,
                });
                self.hold_started = Some(now);
                return Ok(None);
            }
            MobileAction::Approve { card_id } => {
                self.card_available(card_id)?;
                if self.snapshot.approval.as_ref().map(|a| a.card_id) != Some(card_id)
                    || !self.hold_started.is_some_and(|start| {
                        now.saturating_duration_since(start) >= Duration::from_millis(HOLD_MS)
                    })
                {
                    self.cancel_hold();
                    return Err(
                        "Maintien continu de 600 ms requis pour approuver cette carte".into(),
                    );
                }
                (UiToKernel::Approve { card_id }, "approve", true)
            }
            MobileAction::Reject { card_id } => {
                self.card_available(card_id)?;
                (UiToKernel::Reject { card_id }, "reject", true)
            }
            MobileAction::GhostActionAccept { id } => {
                text(&id, 256, false)?;
                (
                    UiToKernel::GhostActionAccept { id },
                    "ghost_action_accept",
                    false,
                )
            }
            MobileAction::GhostActionDismiss { id } => {
                text(&id, 256, false)?;
                (
                    UiToKernel::GhostActionDismiss { id },
                    "ghost_action_dismiss",
                    false,
                )
            }
            MobileAction::Undo { journal_id } => {
                if self.is_guest_active() {
                    return Err("Action interdite en mode invité éphémère".into());
                }
                if journal_id == 0 {
                    return Err("Identifiant de journal invalide".into());
                }
                (UiToKernel::Undo { journal_id }, "undo", true)
            }
            MobileAction::ForgetMemory { row_id } => {
                if self.is_guest_active() {
                    return Err("Action interdite en mode invité éphémère".into());
                }
                text(&row_id, 256, false)?;
                (UiToKernel::MemoryForget { row_id }, "forget_memory", true)
            }
            _ => return Err("Action mobile non prise en charge".into()),
        };
        if mutating && self.snapshot.connection == ConnectionState::Unknown {
            return Err("Résultat précédent inconnu : actualiser le statut avant toute nouvelle mutation ; aucun rejeu automatique".into());
        }
        if mutating && self.snapshot.recovery_required {
            return Err("Opération précédente incertaine : consulter le journal puis prendre explicitement connaissance de l'avertissement avant une nouvelle mutation".into());
        }
        if mutating && self.snapshot.uncertain_operations.len() >= MAX_RECOVERY_OPERATIONS {
            return Err("Trop d'opérations incertaines conservées ; prendre connaissance des avertissements avant de continuer".into());
        }
        if self.next_id == u64::MAX {
            return Err("Identifiants de requête épuisés ; identifiant barrière réservé".into());
        }
        let durable_kind = if mutating {
            Some(MutationKind::from_kind(kind).ok_or("Type de mutation non persistable")?)
        } else {
            None
        };
        let id = self.next_id;
        let frame = Frame::request(id, request.clone());
        if serde_json::to_vec(&frame)
            .map_err(|_| "Sérialisation impossible")?
            .len()
            > MAX_JSON_BYTES
        {
            return Err("Requête supérieure à 1 Mio".into());
        }
        self.next_id = self.next_id.checked_add(1).ok_or("Identifiants épuisés")?;
        self.cancel_hold();
        match &request {
            UiToKernel::HudSubmit { query } => {
                if voice_submission {
                    self.snapshot.draft = query.clone();
                    self.snapshot.voice_request_id = Some(id);
                    self.voice_started_at = None;
                }
                self.push_chat("user", query.clone());
                self.snapshot.streaming = false;
                // Le brouillon n'est pas effacé avant l'accusé : aucun texte perdu
                // si l'application s'arrête ou si le résultat réseau reste inconnu.
            }
            UiToKernel::GhostActionAccept { .. } | UiToKernel::GhostActionDismiss { .. } => {
                self.snapshot.ghost_action = None;
            }
            UiToKernel::Undo { journal_id } => {
                let matched_domain = self
                    .snapshot
                    .journal
                    .iter()
                    .find(|e| e.id == journal_id.to_string())
                    .and_then(|entry| {
                        self.snapshot
                            .domain_autonomies
                            .iter()
                            .find(|d| {
                                entry
                                    .action
                                    .to_lowercase()
                                    .contains(&d.domain.to_lowercase())
                                    || entry
                                        .summary
                                        .to_lowercase()
                                        .contains(&d.domain.to_lowercase())
                            })
                            .map(|d| d.domain.clone())
                    });
                if let Some(domain_name) = matched_domain {
                    if let Some(d) = self
                        .snapshot
                        .domain_autonomies
                        .iter_mut()
                        .find(|d| d.domain == domain_name)
                    {
                        d.record_reverted();
                    }
                } else {
                    for d in self.snapshot.domain_autonomies.iter_mut() {
                        d.record_reverted();
                    }
                }
            }
            UiToKernel::MemoryForget { row_id } => {
                if !self.snapshot.tombstones.contains(row_id) {
                    self.snapshot.tombstones.push(row_id.clone());
                }
                self.snapshot.memory.retain(|row| &row.id != row_id);
                let receipt_id = format!("receipt-{id}");
                self.snapshot.erasure_receipts.push(ErasureReceiptDto {
                    receipt_id: receipt_id.clone(),
                    target_id: row_id.clone(),
                    timestamp_str: format!("t-{id}"),
                    reason: "Droit à l'oubli / Rectification mémoire".to_string(),
                    verification_token: format!("sha256-receipt-{receipt_id}"),
                });
            }
            _ => {}
        }
        self.snapshot.pending = Some(PendingRequest {
            request_id: id,
            kind: kind.into(),
            mutating,
        });
        if let Some(operation_kind) = durable_kind {
            self.snapshot.uncertain_operations.push(UncertainOperation {
                request_id: id,
                kind: operation_kind,
                phase: OperationPhase::InFlight,
            });
        }
        if self.snapshot.connection == ConnectionState::Disconnected {
            self.snapshot.connection = ConnectionState::Connecting;
        }
        self.inflight = Some(request);
        Ok(Some(frame))
    }

    fn voice_available(&self) -> Result<(), String> {
        if !self.snapshot.foreground
            || self.snapshot.hub_id.is_none()
            || self.snapshot.connection != ConnectionState::Connected
            || self.snapshot.gateway_session.is_none()
            || self.snapshot.pending.is_some()
            || !self.snapshot.cards.is_empty()
            || self.snapshot.approval.is_some()
            || self.snapshot.recovery_required
        {
            return Err("Tour vocal indisponible : confirmer la connexion, terminer la demande et traiter les cartes ou incertitudes".into());
        }
        if !self.snapshot.draft.is_empty() {
            return Err("Un brouillon existe : l'envoyer ou l'effacer avant un tour vocal".into());
        }
        Ok(())
    }

    pub(crate) fn invalidate_voice(&mut self) {
        self.voice_started_at = None;
        self.snapshot.voice_context_id = None;
        self.snapshot.voice_request_id = None;
        self.snapshot.voice_reply = None;
    }

    fn card_available(&self, card_id: u64) -> Result<(), String> {
        if self.snapshot.recovery_required {
            return Err("Une opération antérieure reste incertaine : consulter son avertissement avant une décision de carte".into());
        }
        if self.snapshot.connection != ConnectionState::Connected
            || self.snapshot.gateway_session.is_none()
        {
            return Err("Carte inutilisable sans session de hub confirmée".into());
        }
        if !self.snapshot.cards.iter().any(|card| card.id == card_id) {
            return Err("Carte absente ou périmée".into());
        }
        Ok(())
    }

    pub fn receive(&mut self, request_id: u64, reply: GatewayResponse) -> Result<(), String> {
        let was_unknown = self.snapshot.connection == ConnectionState::Unknown;
        let pending = self
            .snapshot
            .pending
            .clone()
            .filter(|p| p.request_id == request_id)
            .ok_or("Réponse ancienne ou sans requête correspondante")?;
        let session = reply.session_id.as_deref();
        if let Some(session) = session {
            text(session, 256, false)?;
            if !session
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            {
                return Err("Identifiant de session gateway invalide pour un en-tête HTTP".into());
            }
        }
        let changed = self
            .snapshot
            .gateway_session
            .as_deref()
            .zip(session)
            .is_some_and(|(a, b)| a != b);
        if changed {
            self.clear_hub_views();
            self.snapshot.gateway_session = reply.session_id.clone();
            self.cancel_hold();
            if pending.mutating && reply.error.as_ref().is_none_or(|error| error.issue_unknown) {
                self.finish_unknown(
                    "Session du hub changée pendant une mutation : résultat inconnu",
                );
                return Err(
                    "Session du hub changée ; cartes invalidées et aucune mutation rejouée".into(),
                );
            }
        }
        if let Some(error) = reply.error {
            if !reply.frames.is_empty() {
                return Err("Enveloppe d'erreur contenant des réponses ambiguës".into());
            }
            text(&error.code, 128, false)?;
            text(&error.message, 4096, false)?;
            self.cancel_hold();
            self.snapshot.cards.clear();
            if error.issue_unknown {
                self.finish_unknown("Transport interrompu après envoi : résultat non confirmé");
            } else {
                self.complete(
                    "failed",
                    format!("{} : {}", error.code, error.message),
                    None,
                );
                self.snapshot.connection = if was_unknown {
                    ConnectionState::Unknown
                } else if session.is_some() {
                    ConnectionState::Connected
                } else {
                    ConnectionState::Disconnected
                };
            }
            if !error.issue_unknown {
                self.snapshot.gateway_session = reply.session_id;
            }
            if error.code == "stale_session" {
                // La prochaine demande de statut doit pouvoir repartir sans
                // l'en-tête périmé ; une erreur n'appaire pas la nouvelle session.
                self.snapshot.gateway_session = None;
                self.snapshot.connection = ConnectionState::Unknown;
            }
            self.bound_error_state();
            return Err(error.message);
        }
        if session.is_none() || reply.frames.is_empty() || reply.frames.len() > 256 {
            return Err("Réponse gateway sans session ou batch hors limites".into());
        }
        let inflight = self.inflight.clone().ok_or("Requête interne absente")?;
        let mut terminal = false;
        for frame in &reply.frames {
            if frame.version != PROTOCOL_VERSION || (frame.id != 0 && frame.id != request_id) {
                return Err("Version ou corrélation de réponse invalide".into());
            }
            match &frame.body {
                FrameBody::Reply { message } => {
                    validate_message(message)?;
                    if terminal && matches!(message, KernelToUi::TokenDelta { .. }) {
                        return Err("Delta de texte reçu après la réponse finale".into());
                    }
                    if frame.id == request_id && terminal_matches(&inflight, message) {
                        if terminal {
                            return Err("Réponse terminale dupliquée".into());
                        }
                        terminal = true;
                    } else if !matches!(
                        message,
                        KernelToUi::Presence(_)
                            | KernelToUi::TokenDelta { .. }
                            | KernelToUi::ShowCard(_)
                            | KernelToUi::JournalAppended { .. }
                            | KernelToUi::Toast { .. }
                            | KernelToUi::FatigueAlert(_)
                            | KernelToUi::PrivacyGuardAlert(_)
                            | KernelToUi::GhostActionSuggest(_)
                            | KernelToUi::GhostActionDismissed { .. }
                    ) {
                        return Err("Type de réponse incompatible avec la requête".into());
                    }
                }
                FrameBody::Error { message, .. } if frame.id == request_id => {
                    text(message, 4096, false)?;
                    if terminal {
                        return Err("Réponse terminale dupliquée".into());
                    }
                    terminal = true;
                }
                _ => return Err("Trame gateway inattendue (seules Reply/Error admises)".into()),
            }
        }
        if !terminal {
            return Err("Aucune réponse terminale corrélée dans le batch".into());
        }
        // Application transactionnelle du batch : une réponse terminale ne
        // masque pas les cartes de suivi qui arrivent après elle.
        let mut candidate = self.clone();
        candidate.snapshot.gateway_session = reply.session_id;
        candidate.snapshot.connection =
            if was_unknown && !matches!(inflight, UiToKernel::StatusRequest) {
                ConnectionState::Unknown
            } else {
                ConnectionState::Connected
            };
        for frame in reply.frames {
            match frame.body {
                FrameBody::Reply { message } => {
                    let final_reply =
                        frame.id == request_id && terminal_matches(&inflight, &message);
                    candidate.apply_message(&message, &inflight)?;
                    if final_reply {
                        if let KernelToUi::HudReply { response, .. } = &message
                            && candidate.snapshot.voice_request_id == Some(request_id)
                            && let Some(context_id) = candidate.snapshot.voice_context_id
                            && !response.trim().is_empty()
                            && response.len() <= MAX_DRAFT_BYTES
                        {
                            candidate.snapshot.voice_reply = Some(VoiceReply {
                                context_id,
                                request_id,
                                text: response.clone(),
                            });
                        }
                        if let KernelToUi::Toast { text } = &message {
                            // Un texte libre ne constitue pas un accusé d'effet.
                            // Une mutation sans ACK typé reste inconnue.
                            let state = if pending.mutating {
                                "unknown"
                            } else {
                                "failed"
                            };
                            candidate.complete(state, text.clone(), Some(message));
                            if pending.mutating || was_unknown {
                                candidate.snapshot.connection = ConnectionState::Unknown;
                                candidate.snapshot.cards.clear();
                                candidate.cancel_hold();
                            }
                        } else {
                            candidate.complete(
                                "confirmed",
                                "Réponse du hub reçue ; voir son effet réel".into(),
                                Some(message),
                            );
                        }
                    }
                }
                FrameBody::Error { code, message } => {
                    let uncertain =
                        pending.mutating && matches!(code, hakille_ipc::ErrorCode::Internal);
                    candidate.complete(
                        if uncertain { "unknown" } else { "failed" },
                        format!("{code:?} : {message}"),
                        None,
                    );
                    if uncertain || was_unknown {
                        candidate.snapshot.connection = ConnectionState::Unknown;
                        candidate.snapshot.cards.clear();
                        candidate.cancel_hold();
                    }
                }
                _ => unreachable!("prévalidation du batch"),
            }
        }
        if candidate.snapshot.connection == ConnectionState::Unknown {
            candidate.snapshot.cards.clear();
            candidate.cancel_hold();
        }
        candidate.trim_history_to_budget()?;
        *self = candidate;
        Ok(())
    }

    pub fn receive_stream_frame(&mut self, request_id: u64, frame: Frame) -> Result<(), String> {
        let was_unknown = self.snapshot.connection == ConnectionState::Unknown;
        if self.snapshot.pending.as_ref().map(|p| p.request_id) != Some(request_id) {
            return Err("Trame de streaming sans requête active correspondante".into());
        }
        if frame.version != PROTOCOL_VERSION || (frame.id != 0 && frame.id != request_id) {
            return Err("Version ou corrélation de trame de streaming invalide".into());
        }
        let inflight = self.inflight.clone().ok_or("Requête interne absente")?;
        match frame.body {
            FrameBody::Reply { message } => {
                validate_message(&message)?;
                match &message {
                    KernelToUi::TokenDelta { text: delta } => {
                        if !matches!(inflight, UiToKernel::HudSubmit { .. }) {
                            return Err("Delta de texte sans demande de dialogue".into());
                        }
                        if !self.snapshot.streaming {
                            self.push_chat("assistant", String::new());
                            self.snapshot.streaming = true;
                        }
                        let entry = self
                            .snapshot
                            .chat
                            .last_mut()
                            .ok_or("Réponse de dialogue absente")?;
                        if entry.text.len().saturating_add(delta.len()) > MAX_TEXT_BYTES {
                            return Err("Réponse de dialogue trop longue".into());
                        }
                        entry.text.push_str(delta);
                    }
                    KernelToUi::HudReply { response, .. } if frame.id == request_id => {
                        if self.snapshot.streaming {
                            if let Some(last) = self.snapshot.chat.last_mut() {
                                last.text = response.clone();
                            }
                        } else {
                            self.push_chat("assistant", response.clone());
                        }
                        self.snapshot.streaming = false;
                        if let UiToKernel::HudSubmit { query } = &inflight
                            && self.snapshot.draft == *query
                        {
                            self.snapshot.draft.clear();
                        }
                        if self.snapshot.voice_request_id == Some(request_id)
                            && let Some(context_id) = self.snapshot.voice_context_id
                            && !response.trim().is_empty()
                            && response.len() <= MAX_DRAFT_BYTES
                        {
                            self.snapshot.voice_reply = Some(VoiceReply {
                                context_id,
                                request_id,
                                text: response.clone(),
                            });
                        }
                        self.complete(
                            "confirmed",
                            "Réponse du hub reçue ; voir son effet réel".into(),
                            Some(message),
                        );
                    }
                    _ => {
                        let final_reply =
                            frame.id == request_id && terminal_matches(&inflight, &message);
                        self.apply_message(&message, &inflight)?;
                        if final_reply {
                            self.complete(
                                "confirmed",
                                "Réponse du hub reçue ; voir son effet réel".into(),
                                Some(message),
                            );
                        }
                    }
                }
            }
            FrameBody::Error { code, message } if frame.id == request_id => {
                let pending_mutating = self.snapshot.pending.as_ref().is_some_and(|p| p.mutating);
                let uncertain =
                    pending_mutating && matches!(code, hakille_ipc::ErrorCode::Internal);
                self.complete(
                    if uncertain { "unknown" } else { "failed" },
                    format!("{code:?} : {message}"),
                    None,
                );
                if uncertain || was_unknown {
                    self.snapshot.connection = ConnectionState::Unknown;
                    self.snapshot.cards.clear();
                    self.snapshot.ghost_action = None;
                    self.cancel_hold();
                }
            }
            _ => return Err("Corps de trame de streaming inattendu".into()),
        }
        self.trim_history_to_budget()?;
        Ok(())
    }

    fn apply_message(&mut self, message: &KernelToUi, request: &UiToKernel) -> Result<(), String> {
        match message {
            KernelToUi::Presence(presence) => self.snapshot.presence = Some(presence.clone()),
            KernelToUi::ShowCard(card) => {
                self.invalidate_voice();
                if let Some(existing) = self.snapshot.cards.iter_mut().find(|c| c.id == card.id) {
                    if existing != card {
                        *existing = card.clone();
                        self.cancel_hold();
                    }
                } else {
                    if self.snapshot.cards.len() >= 32 {
                        return Err("Trop de cartes en attente".into());
                    }
                    self.snapshot.cards.push(card.clone());
                }
            }
            KernelToUi::TokenDelta { text: delta } => {
                if !matches!(request, UiToKernel::HudSubmit { .. }) {
                    return Err("Delta de texte sans demande de dialogue".into());
                }
                if !self.snapshot.streaming {
                    self.push_chat("assistant", String::new());
                    self.snapshot.streaming = true;
                }
                let entry = self
                    .snapshot
                    .chat
                    .last_mut()
                    .ok_or("Réponse de dialogue absente")?;
                if entry.text.len().saturating_add(delta.len()) > MAX_TEXT_BYTES {
                    return Err("Réponse de dialogue trop longue".into());
                }
                entry.text.push_str(delta);
            }
            KernelToUi::HudReply { response, .. } => {
                if self.snapshot.streaming {
                    if let Some(last) = self.snapshot.chat.last_mut() {
                        last.text = response.clone();
                    }
                } else {
                    self.push_chat("assistant", response.clone());
                }
                self.snapshot.streaming = false;
                if let UiToKernel::HudSubmit { query } = request
                    && self.snapshot.draft == *query
                {
                    self.snapshot.draft.clear();
                }
            }
            KernelToUi::JournalReply { entries } => self.snapshot.journal = entries.clone(),
            KernelToUi::MemoryReply { rows } => {
                self.snapshot.memory = rows
                    .iter()
                    .filter(|r| !self.snapshot.tombstones.contains(&r.id))
                    .cloned()
                    .collect();
            }
            KernelToUi::StatusReply { status } => self.snapshot.status = Some(status.clone()),
            KernelToUi::AgentListReply { agents } => self.snapshot.agents = agents.clone(),
            KernelToUi::CardDecisionAck { card_id, .. } => {
                self.snapshot.cards.retain(|card| card.id != *card_id);
                self.cancel_hold();
            }
            KernelToUi::Toast { text } => {
                self.snapshot.notifications.push(text.clone());
                if self.snapshot.notifications.len() > 16 {
                    self.snapshot.notifications.remove(0);
                    self.snapshot.history_trimmed = true;
                }
            }
            KernelToUi::JournalAppended { id } => self.snapshot.last_journal_entry_id = Some(*id),
            KernelToUi::Notification(dto) => {
                let text = format!("{}: {}", dto.title, dto.body);
                self.snapshot.notifications.push(text);
                if self.snapshot.notifications.len() > 16 {
                    self.snapshot.notifications.remove(0);
                    self.snapshot.history_trimmed = true;
                }
            }
            KernelToUi::NotificationDismissed { .. } => {}
            KernelToUi::FatigueAlert(alert) => {
                let text = format!("Conseil bien-être : {}", alert.recommendation);
                self.snapshot.notifications.push(text);
                if self.snapshot.notifications.len() > 16 {
                    self.snapshot.notifications.remove(0);
                    self.snapshot.history_trimmed = true;
                }
            }
            KernelToUi::GhostActionSuggest(dto) => {
                self.snapshot.ghost_action = Some(dto.clone());
            }
            KernelToUi::GhostActionDismissed { id } => {
                if self
                    .snapshot
                    .ghost_action
                    .as_ref()
                    .is_some_and(|g| g.id == *id)
                {
                    self.snapshot.ghost_action = None;
                }
            }
            KernelToUi::AgentRunReply {
                name,
                output_json,
                duration_ms,
                input_sha256,
                output_sha256,
            } => {
                self.snapshot.last_agent_run = Some(AgentRunOutcomeDto {
                    name: name.clone(),
                    output_json: output_json.clone(),
                    duration_ms: *duration_ms,
                    input_sha256: input_sha256.clone(),
                    output_sha256: output_sha256.clone(),
                });
            }
            KernelToUi::PrivacyGuardAlert(alert) => match &alert.trigger {
                hakille_ui_protocol::PrivacyGuardTrigger::ShoulderSurfingDetected {
                    people_count,
                } => {
                    if *people_count >= 2 {
                        self.snapshot.privacy_veil_active = true;
                        self.snapshot.privacy_guard = Some(alert.clone());
                    } else {
                        self.snapshot.privacy_veil_active = false;
                        self.snapshot.privacy_guard = None;
                    }
                }
                hakille_ui_protocol::PrivacyGuardTrigger::RawDataExfiltrationBlocked { .. }
                | hakille_ui_protocol::PrivacyGuardTrigger::HardwareKillSwitchActivated {
                    ..
                }
                | hakille_ui_protocol::PrivacyGuardTrigger::EphemeralTtlPurged => {
                    self.snapshot.privacy_guard = Some(alert.clone());
                }
            },
            KernelToUi::MemoryRememberAck { .. }
            | KernelToUi::UserContext(_)
            | KernelToUi::EmotionUpdate(_)
            | KernelToUi::AuraRingUpdate(_)
            | KernelToUi::MicroGestureRecognized(_) => {}
        }
        Ok(())
    }

    pub fn fail(&mut self, request_id: u64, message: &str) -> Result<(), String> {
        text(message, 4096, false)?;
        if self.snapshot.pending.as_ref().map(|p| p.request_id) != Some(request_id) {
            return Err("Échec ancien ou sans requête correspondante".into());
        }
        self.finish_unknown(message);
        Ok(())
    }

    pub fn interrupt(&mut self, reason: &str) {
        self.invalidate_voice();
        if self.snapshot.pending.is_some() {
            self.finish_unknown(reason);
        }
    }

    pub fn detach_active_flight(&mut self) {
        self.invalidate_voice();
        if let Some(pending) = &self.snapshot.pending
            && pending.mutating
            && let Some(op) = self
                .snapshot
                .uncertain_operations
                .iter_mut()
                .find(|op| op.request_id == pending.request_id)
            && op.phase == OperationPhase::InFlight
        {
            op.phase = OperationPhase::InFlightDetached;
        }
        self.refresh_recovery();
    }

    fn finish_unknown(&mut self, message: &str) {
        let state = if self.snapshot.pending.as_ref().is_some_and(|p| p.mutating) {
            "unknown"
        } else {
            "failed"
        };
        self.complete(state, message.into(), None);
        self.snapshot.connection = ConnectionState::Unknown;
        self.snapshot.gateway_session = None;
        self.snapshot.cards.clear();
        self.snapshot.ghost_action = None;
        self.cancel_hold();
        self.bound_error_state();
    }

    fn bound_error_state(&mut self) {
        if self.trim_history_to_budget().is_err() {
            // En cas de saturation, préserver le brouillon et le résultat
            // incertain plutôt qu'un cache de consultation devenu inaccessible.
            self.clear_hub_views();
            let _ = self.trim_history_to_budget();
        }
    }

    fn complete(&mut self, state: &str, message: String, result: Option<KernelToUi>) {
        if state != "confirmed" {
            self.invalidate_voice();
        }
        // Les listes consultées restent intégralement dans leurs champs de
        // snapshot ; ne pas les dupliquer dans chaque résultat historique.
        let result = result.filter(|reply| {
            !matches!(
                reply,
                KernelToUi::MemoryReply { .. }
                    | KernelToUi::JournalReply { .. }
                    | KernelToUi::AgentListReply { .. }
                    | KernelToUi::StatusReply { .. }
            )
        });
        if let Some(pending) = self.snapshot.pending.take() {
            if pending.mutating {
                if state == "unknown" {
                    if let Some(operation) = self
                        .snapshot
                        .uncertain_operations
                        .iter_mut()
                        .find(|operation| operation.request_id == pending.request_id)
                    {
                        operation.phase = OperationPhase::Unknown;
                    }
                } else {
                    self.snapshot
                        .uncertain_operations
                        .retain(|operation| operation.request_id != pending.request_id);
                }
                self.refresh_recovery();
            }
            self.snapshot.outcomes.push(RequestOutcome {
                request_id: pending.request_id,
                kind: pending.kind,
                state: state.into(),
                message,
                result,
                acknowledged: false,
            });
            if self.snapshot.outcomes.len() > 32 {
                self.snapshot.outcomes.remove(0);
                self.snapshot.history_trimmed = true;
            }
        }
        self.inflight = None;
        self.snapshot.streaming = false;
    }
    fn clear_hub_views(&mut self) {
        self.invalidate_voice();
        self.snapshot.cards.clear();
        self.snapshot.ghost_action = None;
        self.snapshot.status = None;
        self.snapshot.memory.clear();
        self.snapshot.journal.clear();
        self.snapshot.agents.clear();
        self.snapshot.presence = None;
        self.snapshot.last_journal_entry_id = None;
        self.snapshot.notifications.clear();
    }
    pub(crate) fn cancel_hold(&mut self) {
        self.hold_started = None;
        self.snapshot.approval = None;
    }
    pub(crate) fn refresh_recovery(&mut self) {
        self.snapshot.recovery_required = self
            .snapshot
            .uncertain_operations
            .iter()
            .any(|operation| operation.phase == OperationPhase::Unknown);
    }

    fn acknowledge_unknown(&mut self, request_id: u64) -> Result<(), String> {
        let Some(index) = self
            .snapshot
            .uncertain_operations
            .iter()
            .position(|operation| operation.request_id == request_id)
        else {
            if self
                .snapshot
                .acknowledged_operations
                .iter()
                .any(|operation| operation.request_id == request_id)
            {
                return Ok(());
            }
            return Err("Aucune opération incertaine correspondante".into());
        };
        if self.snapshot.uncertain_operations[index].phase != OperationPhase::Unknown {
            return Err(
                "Opération encore en cours : aucune prise de connaissance anticipée".into(),
            );
        }
        self.cancel_hold();
        let operation = self.snapshot.uncertain_operations.remove(index);
        self.snapshot
            .acknowledged_operations
            .push(AcknowledgedOperation {
                request_id,
                kind: operation.kind,
            });
        if self.snapshot.acknowledged_operations.len() > MAX_RECOVERY_OPERATIONS {
            self.snapshot.acknowledged_operations.remove(0);
        }
        let message = "Avertissement lu explicitement ; l'effet de l'opération reste inconnu. Aucune annulation ni confirmation envoyée au hub.";
        if let Some(outcome) = self
            .snapshot
            .outcomes
            .iter_mut()
            .find(|outcome| outcome.request_id == request_id && outcome.state == "unknown")
        {
            outcome.acknowledged = true;
            outcome.message = message.into();
        } else {
            self.snapshot.outcomes.push(RequestOutcome {
                request_id,
                kind: operation.kind.label().into(),
                state: "unknown".into(),
                message: message.into(),
                result: None,
                acknowledged: true,
            });
            if self.snapshot.outcomes.len() > 32 {
                self.snapshot.outcomes.remove(0);
            }
        }
        self.refresh_recovery();
        Ok(())
    }
    fn push_chat(&mut self, role: &str, text: String) {
        self.snapshot.chat.push(ChatEntry {
            role: role.into(),
            text,
        });
        if self.snapshot.chat.len() > 64 {
            self.snapshot.chat.remove(0);
            self.snapshot.history_trimmed = true;
        }
    }
    pub fn trim_history_to_budget(&mut self) -> Result<(), String> {
        while self.check_snapshot_budget().is_err() {
            if !self.snapshot.notifications.is_empty() {
                self.snapshot.notifications.remove(0);
            } else if self.snapshot.outcomes.len() > 1 {
                self.snapshot.outcomes.remove(0);
            } else if self.snapshot.chat.len() > 2 {
                self.snapshot.chat.remove(0);
            } else {
                return Err("Réponse supérieure au budget mobile de 512 Kio".into());
            }
            self.snapshot.history_trimmed = true;
        }
        Ok(())
    }
    pub fn check_snapshot_budget(&self) -> Result<(), String> {
        if serde_json::to_vec(&self.snapshot)
            .map_err(|_| "Snapshot non sérialisable")?
            .len()
            > MAX_SNAPSHOT_BYTES
        {
            return Err(
                "État mobile supérieur à 512 Kio ; réponse refusée sans tronquer les données"
                    .into(),
            );
        }
        Ok(())
    }
}

pub(crate) fn text(value: &str, max: usize, empty: bool) -> Result<(), String> {
    if value.len() > max || value.contains('\0') || (!empty && value.trim().is_empty()) {
        return Err(format!(
            "Texte invalide (limite {max} octets, NUL interdit)"
        ));
    }
    Ok(())
}
fn rows(limit: usize) -> Result<(), String> {
    if !(1..=200).contains(&limit) {
        return Err("Limite de liste attendue entre 1 et 200".into());
    }
    Ok(())
}
fn terminal_matches(request: &UiToKernel, message: &KernelToUi) -> bool {
    if matches!(message, KernelToUi::Toast { .. }) {
        return true;
    }
    match (request, message) {
        (UiToKernel::HudSubmit { .. }, KernelToUi::HudReply { .. })
        | (UiToKernel::StatusRequest, KernelToUi::StatusReply { .. })
        | (UiToKernel::MemoryRequest { .. }, KernelToUi::MemoryReply { .. })
        | (UiToKernel::MemoryRemember { .. }, KernelToUi::MemoryRememberAck { .. })
        | (UiToKernel::JournalRequest { .. }, KernelToUi::JournalReply { .. })
        | (UiToKernel::AgentListRequest, KernelToUi::AgentListReply { .. }) => true,
        (
            UiToKernel::AgentRunRequest { name, .. },
            KernelToUi::AgentRunReply { name: reply, .. },
        ) => name == reply,
        (
            UiToKernel::Approve { card_id },
            KernelToUi::CardDecisionAck {
                card_id: reply,
                approved: true,
                ..
            },
        )
        | (
            UiToKernel::Reject { card_id },
            KernelToUi::CardDecisionAck {
                card_id: reply,
                approved: false,
                ..
            },
        ) => card_id == reply,
        (
            UiToKernel::GhostActionAccept { .. },
            KernelToUi::HudReply { .. }
            | KernelToUi::JournalAppended { .. }
            | KernelToUi::CardDecisionAck { .. },
        ) => true,
        (
            UiToKernel::GhostActionDismiss { id },
            KernelToUi::GhostActionDismissed { id: reply_id },
        ) => id == reply_id,
        (UiToKernel::Undo { journal_id }, KernelToUi::JournalAppended { id }) => journal_id == id,
        (UiToKernel::Undo { .. }, KernelToUi::Toast { .. }) => true,
        (UiToKernel::MemoryForget { .. }, KernelToUi::Toast { .. }) => true,
        (UiToKernel::MemoryForget { .. }, KernelToUi::HudReply { .. }) => true,
        (UiToKernel::MemoryForget { .. }, KernelToUi::JournalAppended { .. }) => true,
        _ => false,
    }
}

fn validate_message(message: &KernelToUi) -> Result<(), String> {
    match message {
        KernelToUi::ShowCard(card) => {
            for value in [&card.verb, &card.object, &card.reason] {
                text(value, 8192, false)?;
            }
            text(&card.cost.currency, 16, false)?;
            if card.cost.micros < 0 {
                return Err("Coût de carte négatif".into());
            }
        }
        KernelToUi::HudReply { response, .. } | KernelToUi::TokenDelta { text: response } => {
            text(response, MAX_TEXT_BYTES, true)?
        }
        KernelToUi::Toast { text: message } => text(message, 4096, false)?,
        KernelToUi::JournalReply { entries } => {
            if entries.len() > 200 {
                return Err("Journal supérieur à 200 lignes".into());
            }
            for entry in entries {
                for value in [&entry.id, &entry.timestamp, &entry.action, &entry.summary] {
                    text(value, 8192, true)?;
                }
            }
        }
        KernelToUi::MemoryReply { rows } => {
            if rows.len() > 200 {
                return Err("Mémoire supérieure à 200 lignes".into());
            }
            for row in rows {
                for value in [&row.id, &row.text, &row.source] {
                    text(value, 8192, true)?;
                }
                if row.tags.len() > 32 {
                    return Err("Trop d'étiquettes mémoire".into());
                }
                for tag in &row.tags {
                    text(tag, 256, false)?;
                }
            }
        }
        KernelToUi::AgentListReply { agents } => {
            if agents.len() > 200 {
                return Err("Liste agents supérieure à 200 lignes".into());
            }
            for agent in agents {
                for value in [&agent.id, &agent.name, &agent.sha256] {
                    text(value, 256, true)?;
                }
                if agent.required_caps.len() > 64 {
                    return Err("Trop de scopes déclarés".into());
                }
                for cap in &agent.required_caps {
                    text(cap, 256, false)?;
                }
            }
        }
        KernelToUi::StatusReply { status } => {
            for value in [&status.model, &status.cost, &status.state] {
                text(value, 8192, true)?;
            }
        }
        KernelToUi::MemoryRememberAck { id } => text(id, 256, false)?,
        KernelToUi::AgentRunReply {
            name,
            output_json,
            input_sha256,
            output_sha256,
            ..
        } => {
            text(name, 128, false)?;
            text(output_json, MAX_TEXT_BYTES, false)?;
            text(input_sha256, 128, false)?;
            text(output_sha256, 128, false)?;
            serde_json::from_str::<serde_json::Value>(output_json)
                .map_err(|_| "Sortie agent JSON invalide")?;
        }
        KernelToUi::CardDecisionAck {
            audit_id, effect, ..
        } => {
            text(audit_id, 256, false)?;
            if let Some(effect) = effect {
                text(effect, 4096, false)?;
            }
        }
        KernelToUi::GhostActionSuggest(ghost) => {
            text(&ghost.id, 256, false)?;
            text(&ghost.label, 8192, false)?;
            if !(0.0..=1.0).contains(&ghost.focal_u)
                || !(0.0..=1.0).contains(&ghost.focal_v)
                || !(0.0..=1.0).contains(&ghost.confidence)
            {
                return Err("Coordonnées ou confiance de Ghost Action hors limites".into());
            }
        }
        KernelToUi::GhostActionDismissed { id } => {
            text(id, 256, false)?;
        }
        KernelToUi::FatigueAlert(alert) => {
            text(&alert.recommendation, 8192, false)?;
        }
        KernelToUi::PrivacyGuardAlert(alert) => {
            text(&alert.description, 8192, false)?;
            text(&alert.recommended_action, 8192, false)?;
            match &alert.trigger {
                hakille_ui_protocol::PrivacyGuardTrigger::RawDataExfiltrationBlocked {
                    blocked_scope,
                } => text(blocked_scope, 256, false)?,
                hakille_ui_protocol::PrivacyGuardTrigger::HardwareKillSwitchActivated {
                    device,
                } => text(device, 256, false)?,
                hakille_ui_protocol::PrivacyGuardTrigger::ShoulderSurfingDetected { .. }
                | hakille_ui_protocol::PrivacyGuardTrigger::EphemeralTtlPurged => {}
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    #[test]
    fn request_id_never_uses_reserved_barrier_or_wraps() {
        let mut core = Core::default();
        core.dispatch(
            MobileAction::SetHub {
                hub_id: "hub".into(),
            },
            Instant::now(),
        )
        .unwrap();
        core.next_id = u64::MAX - 1;
        let frame = core
            .dispatch(MobileAction::RefreshStatus {}, Instant::now())
            .unwrap()
            .unwrap();
        assert_eq!(frame.id, u64::MAX - 1);
        core.fail(frame.id, "échec réseau").unwrap();
        assert!(
            core.dispatch(MobileAction::RefreshStatus {}, Instant::now())
                .is_err()
        );
        assert_eq!(core.next_id, u64::MAX);
        assert!(core.snapshot.pending.is_none());
    }
}
