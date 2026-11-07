//! MockKernel: In-memory protocol broker simulating kernel message exchange.
//!
//! Provides bidirectional communication simulation (UiToKernel and KernelToUi),
//! fail-closed policy verification, outbox queues, journal event recording,
//! and card decision handling.

use hakille_ui_protocol::{
    ActionCard, AgentDto, JournalEntryDto, KernelToUi, MemoryRowDto, StatusDto, UiMessage,
    UiToKernel,
};
use std::collections::VecDeque;

/// Policy for mock kernel authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockAuthorityPolicy {
    /// Kernel is detached / disconnected: fail-closed on all sensitive requests.
    Detached,
    /// Kernel is connected and authorized.
    Authorized,
}

/// In-memory mock kernel broker.
pub struct MockKernel {
    pub policy: MockAuthorityPolicy,
    pub sent_to_kernel: Vec<UiToKernel>,
    pub pending_to_ui: VecDeque<KernelToUi>,
    pub journal_entries: Vec<JournalEntryDto>,
    pub memory_rows: Vec<MemoryRowDto>,
    pub staged_cards: Vec<ActionCard>,
    pub next_audit_id: u64,
}

impl Default for MockKernel {
    fn default() -> Self {
        Self::new(MockAuthorityPolicy::Authorized)
    }
}

impl MockKernel {
    pub fn new(policy: MockAuthorityPolicy) -> Self {
        Self {
            policy,
            sent_to_kernel: Vec::new(),
            pending_to_ui: VecDeque::new(),
            journal_entries: Vec::new(),
            memory_rows: Vec::new(),
            staged_cards: Vec::new(),
            next_audit_id: 1,
        }
    }

    pub fn detached() -> Self {
        Self::new(MockAuthorityPolicy::Detached)
    }

    pub fn authorized() -> Self {
        Self::new(MockAuthorityPolicy::Authorized)
    }

    /// Queue a message to be dispatched to the UI.
    pub fn enqueue_reply(&mut self, msg: KernelToUi) {
        self.pending_to_ui.push_back(msg);
    }

    /// Stage an Action Card for validation.
    pub fn stage_card(&mut self, card: ActionCard) {
        self.staged_cards.push(card.clone());
        self.enqueue_reply(KernelToUi::ShowCard(card));
    }

    /// Process a UiToKernel message, simulating kernel response.
    pub fn handle_ui_message(&mut self, msg: UiMessage) -> Result<Option<KernelToUi>, String> {
        msg.check().map_err(|e| e.to_string())?;
        self.sent_to_kernel.push(msg.inner.clone());

        if self.policy == MockAuthorityPolicy::Detached {
            match &msg.inner {
                UiToKernel::Approve { .. }
                | UiToKernel::Edit { .. }
                | UiToKernel::Reject { .. }
                | UiToKernel::HudSubmit { .. }
                | UiToKernel::SettingsToggle { .. }
                | UiToKernel::MemoryForget { .. }
                | UiToKernel::OutboxFlush
                | UiToKernel::JournalRequest { .. }
                | UiToKernel::MemoryRequest { .. }
                | UiToKernel::MemoryRemember { .. }
                | UiToKernel::StatusRequest
                | UiToKernel::AgentListRequest
                | UiToKernel::AgentRunRequest { .. } => {
                    return Err(
                        "daemon requis: opération refusée en mode détaché (fail-closed)".into(),
                    );
                }
                UiToKernel::Undo { journal_id } => {
                    return Ok(Some(KernelToUi::JournalAppended { id: *journal_id }));
                }
                UiToKernel::RevokeAll => {
                    return Ok(Some(KernelToUi::JournalAppended { id: 0 }));
                }
            }
        }

        // Mode Authorized
        match msg.inner {
            UiToKernel::HudSubmit { query } => {
                let response = format!("Réponse synthétisée pour : {query}");
                Ok(Some(KernelToUi::HudReply {
                    response,
                    validation: None,
                }))
            }
            UiToKernel::Approve { card_id } => {
                let audit_id = self.next_audit_id;
                self.next_audit_id += 1;
                self.journal_entries.push(JournalEntryDto {
                    id: audit_id.to_string(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    action: "ActionApprove".into(),
                    summary: format!("Action #{card_id} approuvée"),
                });
                Ok(Some(KernelToUi::CardDecisionAck {
                    card_id,
                    approved: true,
                    audit_id: audit_id.to_string(),
                    effect: Some("Appliqué avec succès".into()),
                }))
            }
            UiToKernel::Reject { card_id } => {
                let audit_id = self.next_audit_id;
                self.next_audit_id += 1;
                Ok(Some(KernelToUi::CardDecisionAck {
                    card_id,
                    approved: false,
                    audit_id: audit_id.to_string(),
                    effect: Some("Rejeté par l'utilisateur".into()),
                }))
            }
            UiToKernel::Edit { card_id } => Ok(Some(KernelToUi::Toast {
                text: format!("Édition demandée pour la carte #{card_id}"),
            })),
            UiToKernel::Undo { journal_id } => {
                let j_id_str = journal_id.to_string();
                if let Some(entry) = self.journal_entries.iter_mut().find(|e| e.id == j_id_str) {
                    entry.summary = format!("{} (annulée)", entry.summary);
                }
                Ok(Some(KernelToUi::Toast {
                    text: format!("Action #{journal_id} annulée"),
                }))
            }
            UiToKernel::JournalRequest { limit } => {
                let count = self.journal_entries.len().min(limit);
                let entries = self.journal_entries[..count].to_vec();
                Ok(Some(KernelToUi::JournalReply { entries }))
            }
            UiToKernel::MemoryRequest { query: _, limit } => {
                let count = self.memory_rows.len().min(limit);
                let rows = self.memory_rows[..count].to_vec();
                Ok(Some(KernelToUi::MemoryReply { rows }))
            }
            UiToKernel::MemoryRemember { text } => {
                let id = format!("mem-{}", self.next_audit_id);
                self.next_audit_id += 1;
                self.memory_rows.push(MemoryRowDto {
                    id: id.clone(),
                    text,
                    tags: vec!["sovereign".into(), "user".into()],
                    source: "User".into(),
                });
                Ok(Some(KernelToUi::MemoryRememberAck { id }))
            }
            UiToKernel::MemoryForget { row_id } => {
                self.memory_rows.retain(|r| r.id != row_id);
                Ok(Some(KernelToUi::Toast {
                    text: format!("Fait mémoriel {row_id} effacé"),
                }))
            }
            UiToKernel::StatusRequest => Ok(Some(KernelToUi::StatusReply {
                status: StatusDto {
                    model: "Qwen2.5-0.5B-Instruct-GGUF".into(),
                    cost: "0.00 EUR".into(),
                    state: "Idle".into(),
                    memory_count: self.memory_rows.len() as u64,
                },
            })),
            UiToKernel::AgentListRequest => Ok(Some(KernelToUi::AgentListReply {
                agents: vec![
                    AgentDto {
                        id: "agent-mail".into(),
                        name: "mail".into(),
                        required_caps: vec!["mail:read".into()],
                        artifact_present: true,
                        sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                            .into(),
                    },
                    AgentDto {
                        id: "agent-calendar".into(),
                        name: "calendar".into(),
                        required_caps: vec!["calendar:read".into()],
                        artifact_present: true,
                        sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                            .into(),
                    },
                    AgentDto {
                        id: "agent-code".into(),
                        name: "code".into(),
                        required_caps: vec!["fs:write".into()],
                        artifact_present: true,
                        sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                            .into(),
                    },
                ],
            })),
            UiToKernel::AgentRunRequest {
                name,
                input_json: _,
            } => Ok(Some(KernelToUi::AgentRunReply {
                name,
                output_json: "{\"status\": \"ok\", \"items\": 0}".into(),
                duration_ms: 12,
                input_sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                    .into(),
                output_sha256: "f5a5fd42d16a20302798ef6ed309979b43003d2320d9f0e8ea9831a92759fb4b"
                    .into(),
            })),
            UiToKernel::SettingsToggle { index } => Ok(Some(KernelToUi::Toast {
                text: format!("Paramètre #{index} basculé"),
            })),
            UiToKernel::RevokeAll => {
                self.journal_entries.clear();
                Ok(Some(KernelToUi::Toast {
                    text: "Toutes les autorisations ont été révoquées".into(),
                }))
            }
            UiToKernel::OutboxFlush => Ok(Some(KernelToUi::Toast {
                text: "Boîte d'envoi vidée".into(),
            })),
        }
    }

    /// Assert that a specific message kind was received by the kernel.
    pub fn assert_received_matching<F>(&self, predicate: F)
    where
        F: Fn(&UiToKernel) -> bool,
    {
        assert!(
            self.sent_to_kernel.iter().any(predicate),
            "Expected matching UiToKernel message was not received! Received: {:?}",
            self.sent_to_kernel
        );
    }

    /// Assert that an approval for a specific card was received.
    pub fn assert_received_approval(&self, expected_card_id: u64) {
        self.assert_received_matching(|msg| match msg {
            UiToKernel::Approve { card_id } => *card_id == expected_card_id,
            _ => false,
        });
    }

    /// Assert that a rejection for a specific card was received.
    pub fn assert_received_rejection(&self, expected_card_id: u64) {
        self.assert_received_matching(|msg| match msg {
            UiToKernel::Reject { card_id } => *card_id == expected_card_id,
            _ => false,
        });
    }

    /// Assert that no sensitive operations were executed.
    pub fn assert_no_sensitive_dispatched(&self) {
        for msg in &self.sent_to_kernel {
            assert!(
                !matches!(
                    msg,
                    UiToKernel::Approve { .. }
                        | UiToKernel::MemoryForget { .. }
                        | UiToKernel::RevokeAll
                ),
                "Unexpected sensitive message dispatched: {:?}",
                msg
            );
        }
    }

    /// Assert journal entry committed with specific id.
    pub fn assert_journal_entry_committed(&self, id: &str) {
        assert!(
            self.journal_entries.iter().any(|e| e.id == id),
            "Journal entry #{} not found in committed journal!",
            id
        );
    }
}
