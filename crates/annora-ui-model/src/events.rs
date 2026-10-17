//! Événements typés UI ↔ noyau — remplace les 18 callbacks Slint de démo.
//!
//! Chaque interaction produit un `UiEvent`. Le noyau re-valide chaque
//! autorisation ; aucun bouton ne produit d'effet système direct.
//! Conversion stricte vers `annora_ui_protocol::{UiToKernel, UiMessage}`.

use annora_ui_protocol::{KernelToUi, UiMessage, UiToKernel};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Les 18 interactions issues de `annora.slint` + écrans secondaires,
/// désormais exhaustives, testées et versionnées.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiEvent {
    PresenceTapped,
    HudSubmitted { query: String },
    HudValidated,
    HudCancelled,
    JournalRevert { index: usize },
    PowerToggled { index: usize },
    PowersKillAll,
    CardApprove { card_id: u64 },
    CardEdit { card_id: u64 },
    CardReject { card_id: u64 },
    FactDisputed { fact: String },
    MemorySelectPartition { index: usize },
    MemoryForget { row_id: String },
    SettingsToggled { index: usize },
    OnboardingAcknowledge,
    OutboxRevalidate { index: usize },
    OutboxFlush,
    DestinationSelected { index: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KernelEvent {
    Presence(annora_ui_protocol::PresenceState),
    ShowCard(annora_ui_protocol::ActionCard),
    JournalAppended {
        id: u64,
    },
    HudReply {
        response: String,
        validation: Option<(String, u64)>,
    },
    Toast {
        text: String,
    },
}

impl From<KernelToUi> for KernelEvent {
    fn from(v: KernelToUi) -> Self {
        match v {
            KernelToUi::Presence(p) => Self::Presence(p),
            KernelToUi::ShowCard(c) => Self::ShowCard(c),
            KernelToUi::JournalAppended { id } => Self::JournalAppended { id },
            KernelToUi::HudReply {
                response,
                validation,
            } => Self::HudReply {
                response,
                validation,
            },
            KernelToUi::Toast { text } => Self::Toast { text },
        }
    }
}

impl From<KernelEvent> for KernelToUi {
    fn from(v: KernelEvent) -> Self {
        match v {
            KernelEvent::Presence(p) => Self::Presence(p),
            KernelEvent::ShowCard(c) => Self::ShowCard(c),
            KernelEvent::JournalAppended { id } => Self::JournalAppended { id },
            KernelEvent::HudReply {
                response,
                validation,
            } => Self::HudReply {
                response,
                validation,
            },
            KernelEvent::Toast { text } => Self::Toast { text },
        }
    }
}

#[derive(Debug, Error)]
pub enum UiEventError {
    #[error("requête vide")]
    EmptyQuery,
    #[error("entrée vide")]
    EmptyEntry,
    #[error("index invalide: {0}")]
    InvalidIndex(usize),
    #[error("nécessite le daemon: {0}")]
    DaemonRequired(String),
    #[error("annulation interdite")]
    RevertNotAllowed,
}

impl UiEvent {
    /// Validation locale pure (sans effet système). Les effets réels sont
    /// décidés par le noyau après `into_protocol`.
    pub fn validate(&self) -> Result<(), UiEventError> {
        match self {
            Self::HudSubmitted { query } if query.trim().is_empty() => {
                Err(UiEventError::EmptyQuery)
            }
            Self::FactDisputed { fact } if fact.trim().is_empty() => Err(UiEventError::EmptyEntry),
            Self::DestinationSelected { index } if *index > 3 => {
                Err(UiEventError::InvalidIndex(*index as usize))
            }
            _ => Ok(()),
        }
    }

    /// Conversion vers le protocole versionné v1 quand c'est une action sensible.
    /// Les événements purement locaux (navigation, cancel) retournent `None`.
    pub fn into_protocol(self) -> Option<UiToKernel> {
        match self {
            Self::CardApprove { card_id } => Some(UiToKernel::Approve { card_id }),
            Self::CardEdit { card_id } => Some(UiToKernel::Edit { card_id }),
            Self::CardReject { card_id } => Some(UiToKernel::Reject { card_id }),
            Self::JournalRevert { index } => Some(UiToKernel::Undo {
                journal_id: index as u64,
            }),
            Self::PowersKillAll => Some(UiToKernel::RevokeAll),
            Self::HudSubmitted { query } => Some(UiToKernel::HudSubmit { query }),
            Self::SettingsToggled { index } => Some(UiToKernel::SettingsToggle { index }),
            Self::MemoryForget { row_id } => Some(UiToKernel::MemoryForget { row_id }),
            Self::OutboxFlush => Some(UiToKernel::OutboxFlush),
            _ => None,
        }
    }

    /// Enveloppe versionnée prête à sérialiser (fil natif / UniFFI / WASM).
    pub fn into_message(self) -> Option<UiMessage> {
        self.into_protocol().map(UiMessage::new)
    }

    /// Libellé unique dérivé de la même donnée que la carte visuelle,
    /// le label AccessKit, le raccourci et la trace d'audit (§3 règle 3).
    pub fn audit_label(&self) -> String {
        match self {
            Self::PresenceTapped => "présence touchée → journal".into(),
            Self::HudSubmitted { query } => format!("hud soumis: {query}"),
            Self::HudValidated => "hud validé".into(),
            Self::HudCancelled => "hud annulé (aucun effet)".into(),
            Self::JournalRevert { index } => format!("journal annulation #{index}"),
            Self::PowerToggled { index } => format!("pouvoir #{index} basculé"),
            Self::PowersKillAll => "kill-all pouvoirs".into(),
            Self::CardApprove { card_id } => format!("carte {card_id} approuvée (demande)"),
            Self::CardEdit { card_id } => format!("carte {card_id} modifiée (retour skill)"),
            Self::CardReject { card_id } => format!("carte {card_id} refusée"),
            Self::FactDisputed { fact } => format!("fait contesté: {fact}"),
            Self::MemorySelectPartition { index } => format!("mémoire partition {index}"),
            Self::MemoryForget { row_id } => format!("mémoire oubli {row_id}"),
            Self::SettingsToggled { index } => format!("réglage #{index} basculé"),
            Self::OnboardingAcknowledge => "onboarding acquitté".into(),
            Self::OutboxRevalidate { index } => format!("outbox revalidation #{index}"),
            Self::OutboxFlush => "outbox flush groupé".into(),
            Self::DestinationSelected { index } => format!("destination {index}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_18_events_have_audit_labels() {
        let evts = vec![
            UiEvent::PresenceTapped,
            UiEvent::HudSubmitted { query: "x".into() },
            UiEvent::HudValidated,
            UiEvent::HudCancelled,
            UiEvent::JournalRevert { index: 0 },
            UiEvent::PowerToggled { index: 0 },
            UiEvent::PowersKillAll,
            UiEvent::CardApprove { card_id: 1 },
            UiEvent::CardEdit { card_id: 1 },
            UiEvent::CardReject { card_id: 1 },
            UiEvent::FactDisputed { fact: "f".into() },
            UiEvent::MemorySelectPartition { index: 0 },
            UiEvent::MemoryForget { row_id: "r".into() },
            UiEvent::SettingsToggled { index: 0 },
            UiEvent::OnboardingAcknowledge,
            UiEvent::OutboxRevalidate { index: 0 },
            UiEvent::OutboxFlush,
            UiEvent::DestinationSelected { index: 0 },
        ];
        assert_eq!(evts.len(), 18);
        for e in &evts {
            assert!(!e.audit_label().is_empty());
            let _ = e.clone().into_message();
        }
    }

    #[test]
    fn validation_rejects_empty_and_bad_destination() {
        assert!(
            UiEvent::HudSubmitted { query: "  ".into() }
                .validate()
                .is_err()
        );
        assert!(
            UiEvent::DestinationSelected { index: 9 }
                .validate()
                .is_err()
        );
        assert!(
            UiEvent::HudSubmitted {
                query: "bonjour".into()
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn sensitive_events_map_to_protocol_v1() {
        let m = UiEvent::CardApprove { card_id: 3 }.into_message().unwrap();
        assert!(m.check().is_ok());
        assert_eq!(m.inner, UiToKernel::Approve { card_id: 3 });
        // Local pur → None (aucun effet système).
        assert!(UiEvent::HudCancelled.into_message().is_none());
        assert!(
            UiEvent::DestinationSelected { index: 1 }
                .into_message()
                .is_none()
        );
    }
}
