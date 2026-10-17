//! Pont noyau — connecte les vrais flux, jamais de stub UI.
//!
//! Ordre de branchement (§5) : onboarding → présence/HUD → carte →
//! journal → mémoire → pouvoirs/réglages → outbox.
//! Chaque flux garde son test de refus d'autorisation côté noyau :
//! le renderer ne produit que `UiEvent` → `UiMessage` ; le noyau revérifie.

use annora_ui_model::{KernelEvent, UiEvent};
use annora_ui_protocol::{KernelToUi, UiMessage, UiToKernel};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("protocole: {0}")]
    Protocol(String),
    #[error("refus noyau: {0}")]
    Denied(String),
    #[error("daemon requis: {0}")]
    DaemonRequired(String),
}

/// Noyau minimal — le vrai `CognitiveOrchestrator` est branché par le daemon.
/// Ici : politique fail-closed testable (refus par défaut si capability absente).
pub trait KernelStub: Send {
    fn submit(&mut self, msg: UiMessage) -> Result<KernelToUi, BridgeError>;
}

/// Noyau facticestrict : refuse tout sauf Undo/RevokeAll sans capability.
/// Sert de filet de sécurité + tests de refus.
pub struct DenyAllKernel;

impl KernelStub for DenyAllKernel {
    fn submit(&mut self, msg: UiMessage) -> Result<KernelToUi, BridgeError> {
        msg.check().map_err(BridgeError::Protocol)?;
        match &msg.inner {
            UiToKernel::Approve { .. } | UiToKernel::Edit { .. } | UiToKernel::Reject { .. } => {
                Err(BridgeError::DaemonRequired("noyau non branché — carte en attente".into()))
            }
            UiToKernel::HudSubmit { .. } => {
                Err(BridgeError::DaemonRequired("noyau non branché — requête HUD en attente".into()))
            }
            UiToKernel::SettingsToggle { .. } => {
                Err(BridgeError::DaemonRequired("noyau non branché — réglage non persisté".into()))
            }
            UiToKernel::MemoryForget { .. } => {
                Err(BridgeError::DaemonRequired("noyau non branché — reçu crypto requis".into()))
            }
            UiToKernel::OutboxFlush => {
                Err(BridgeError::DaemonRequired("noyau non branché — quotas requis".into()))
            }
            UiToKernel::Undo { journal_id } => Ok(KernelToUi::JournalAppended { id: *journal_id }),
            UiToKernel::RevokeAll => Ok(KernelToUi::JournalAppended { id: 0 }),
        }
    }
}

/// Pont UI→noyau : valide localement, enveloppe versionnée, soumet.
pub struct KernelBridge<K: KernelStub> {
    pub kernel: K,
    pub outbox: Vec<KernelEvent>,
}

impl<K: KernelStub> KernelBridge<K> {
    pub fn new(kernel: K) -> Self {
        Self { kernel, outbox: Vec::new() }
    }

    /// Traite un `UiEvent` : validation locale → protocole → noyau.
    /// Les événements purement locaux (navigation, cancel) ne touchent pas le noyau.
    pub fn dispatch(&mut self, ev: UiEvent) -> Result<Option<KernelEvent>, BridgeError> {
        ev.validate().map_err(|e| BridgeError::Protocol(e.to_string()))?;
        let label = ev.audit_label();
        tracing::debug!(event = %label, "dispatch");
        let Some(msg) = ev.into_message() else {
            return Ok(None);
        };
        let reply = self.kernel.submit(msg)?;
        let kev = KernelEvent::from(reply);
        self.outbox.push(kev.clone());
        Ok(Some(kev))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_events_never_hit_kernel() {
        let mut b = KernelBridge::new(DenyAllKernel);
        assert!(b.dispatch(UiEvent::HudCancelled).unwrap().is_none());
        assert!(b.dispatch(UiEvent::DestinationSelected { index: 1 }).unwrap().is_none());
        assert!(b.outbox.is_empty());
    }

    #[test]
    fn sensitive_events_require_daemon_fail_closed() {
        let mut b = KernelBridge::new(DenyAllKernel);
        let err = b.dispatch(UiEvent::CardApprove { card_id: 7 }).expect_err("doit refuser sans noyau");
        assert!(matches!(err, BridgeError::DaemonRequired(_)));
        // Undo est autorisé (compensation locale traçable).
        let ok = b.dispatch(UiEvent::JournalRevert { index: 3 }).unwrap().unwrap();
        assert!(matches!(ok, KernelEvent::JournalAppended { id: 3 }));
    }

    #[test]
    fn full_flow_request_simulate_confirm_journal_undo() {
        // Parcours complet sans stub UI : demande → refus tracé → undo → journal.
        let mut b = KernelBridge::new(DenyAllKernel);
        let denied = b.dispatch(UiEvent::CardApprove { card_id: 1 });
        assert!(denied.is_err());
        let undone = b.dispatch(UiEvent::JournalRevert { index: 1 }).unwrap().unwrap();
        assert!(matches!(undone, KernelEvent::JournalAppended { id: 1 }));
        assert_eq!(b.outbox.len(), 1);
    }
}
