//! Pont daemon — adaptateur du noyau réel, fail-closed par défaut.
//!
//! Contrat (plan refonte §3 règle 2) : le renderer ne produit que des
//! `UiEvent` → `UiMessage` ; le noyau revérifie chaque autorisation.
//! `AuthorityPolicy::Detached` refuse tout effet sensible, tracé et honnête.
//! `AuthorityPolicy::LocalAuthorized` simule le retour du noyau branché :
//! chaque action acceptée produit un identifiant de journal (l'effet réel
//! appartient au daemon — jamais au renderer).
//!
//! Le daemon (crate `hakille-daemon`) implémente ce même contrat au-dessus de
//! `CognitiveOrchestrator::process_user_turn_with_authorization` et
//! `build_plan_card_authorized` ; il ne partage ici qu'un trait sans clé,
//! sans réseau et sans capability.

use crate::kernel_bridge::{BridgeError, KernelStub};
use hakille_ui_protocol::{KernelToUi, UiMessage, UiToKernel};
use std::collections::HashMap;

/// Politique d'autorisation effective du noyau.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityPolicy {
    /// Aucun noyau branché : refus fail-closed de tout effet sensible.
    Detached,
    /// Session locale autorisée : flux acceptés, chaque effet journalisé.
    LocalAuthorized,
}

/// Contrat du daemon — implémentable au-dessus du vrai noyau.
pub trait DaemonKernel: KernelStub {
    fn policy(&self) -> AuthorityPolicy;
    /// Trace locale des effets (miroir de l'AuditJournal du daemon).
    fn traces(&self) -> &[(u64, String)];
}

/// Pont daemon complet — garde les cartes en attente et journalise.
pub struct RealDaemonBridge {
    pub policy: AuthorityPolicy,
    journal: Vec<(u64, String)>,
    next_id: u64,
    pending_cards: HashMap<u64, String>,
}

impl RealDaemonBridge {
    pub fn detached() -> Self {
        Self {
            policy: AuthorityPolicy::Detached,
            journal: Vec::new(),
            next_id: 1,
            pending_cards: HashMap::new(),
        }
    }

    pub fn local_authorized() -> Self {
        Self {
            policy: AuthorityPolicy::LocalAuthorized,
            journal: Vec::new(),
            next_id: 1,
            pending_cards: HashMap::new(),
        }
    }

    /// Met une carte en attente (noyau → UI, jamais l'inverse).
    pub fn stage_card(&mut self, card_id: u64, label: impl Into<String>) {
        self.pending_cards.insert(card_id, label.into());
    }

    fn commit(&mut self, label: impl Into<String>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.journal.push((id, label.into()));
        id
    }

    fn require_daemon<T>(&self, what: &str) -> Result<T, BridgeError> {
        Err(BridgeError::DaemonRequired(format!(
            "daemon non branché — {what}"
        )))
    }
}

impl KernelStub for RealDaemonBridge {
    fn submit(&mut self, msg: UiMessage) -> Result<KernelToUi, BridgeError> {
        msg.check().map_err(BridgeError::Protocol)?;
        if self.policy == AuthorityPolicy::Detached {
            return match &msg.inner {
                UiToKernel::Undo { journal_id } => {
                    Ok(KernelToUi::JournalAppended { id: *journal_id })
                }
                UiToKernel::RevokeAll => Ok(KernelToUi::JournalAppended { id: 0 }),
                UiToKernel::Approve { .. }
                | UiToKernel::Edit { .. }
                | UiToKernel::Reject { .. } => self.require_daemon("carte en attente"),
                UiToKernel::HudSubmit { .. } => self.require_daemon("requête HUD en attente"),
                UiToKernel::SettingsToggle { .. } => self.require_daemon("réglage non persisté"),
                UiToKernel::MemoryForget { .. } => self.require_daemon("reçu crypto requis"),
                UiToKernel::OutboxFlush => self.require_daemon("quotas requis"),
                // Extension additive v1 (consultation) : daemon réel requis.
                UiToKernel::JournalRequest { .. }
                | UiToKernel::MemoryRequest { .. }
                | UiToKernel::MemoryRemember { .. }
                | UiToKernel::StatusRequest => {
                    self.require_daemon("données réelles du démon requises")
                }
                // Extension additive v1 (agents) : sans démon, aucun agent
                // réel n'est disponible — refus explicite.
                UiToKernel::AgentListRequest | UiToKernel::AgentRunRequest { .. } => {
                    self.require_daemon("runtime d'agents réel requis")
                }
            };
        }
        // LocalAuthorized : le noyau (simulé ici) revérifie puis journalise.
        match &msg.inner {
            UiToKernel::Approve { card_id } => {
                let label = self.pending_cards.remove(card_id);
                let id = self.commit(format!("approuvé carte #{card_id}"));
                tracing::info!(
                    card_id,
                    label = label.as_deref().unwrap_or(""),
                    id,
                    "carte approuvée"
                );
                Ok(KernelToUi::JournalAppended { id })
            }
            UiToKernel::Edit { card_id } => {
                let id = self.commit(format!("modifiée carte #{card_id}"));
                Ok(KernelToUi::JournalAppended { id })
            }
            UiToKernel::Reject { card_id } => {
                self.pending_cards.remove(card_id);
                let id = self.commit(format!("refusée carte #{card_id}"));
                Ok(KernelToUi::JournalAppended { id })
            }
            UiToKernel::Undo { journal_id } => {
                let id = self.commit(format!("annulée entrée #{journal_id}"));
                Ok(KernelToUi::JournalAppended { id })
            }
            UiToKernel::RevokeAll => {
                let id = self.commit("révocation globale des pouvoirs");
                Ok(KernelToUi::JournalAppended { id })
            }
            UiToKernel::HudSubmit { query } => {
                self.commit(format!("hud: {query}"));
                Ok(KernelToUi::HudReply {
                    response: "Reçu — routage System1".into(),
                    validation: None,
                })
            }
            UiToKernel::SettingsToggle { index } => {
                let id = self.commit(format!("réglage #{index} basculé"));
                Ok(KernelToUi::JournalAppended { id })
            }
            UiToKernel::MemoryForget { row_id } => {
                let id = self.commit(format!("mémoire oubliée {row_id}"));
                Ok(KernelToUi::JournalAppended { id })
            }
            UiToKernel::OutboxFlush => {
                let id = self.commit("outbox vidée (quotas vérifiés)");
                Ok(KernelToUi::JournalAppended { id })
            }
            // Extension additive v1 (consultation) : le pont simulé ne peut
            // pas répondre sans les données réelles — refus explicite, jamais
            // de liste inventée.
            UiToKernel::JournalRequest { .. }
            | UiToKernel::MemoryRequest { .. }
            | UiToKernel::MemoryRemember { .. }
            | UiToKernel::StatusRequest => Ok(KernelToUi::Toast {
                text: "données réelles requises — démon non branché (pont simulé)".into(),
            }),
            // Extension additive v1 (agents) : le pont simulé n'exécute AUCUN
            // agent — refus explicite (jamais de sortie inventée).
            UiToKernel::AgentListRequest | UiToKernel::AgentRunRequest { .. } => {
                Ok(KernelToUi::Toast {
                    text: "runtime d'agents réel requis — démon non branché (pont simulé)".into(),
                })
            }
        }
    }
}

impl DaemonKernel for RealDaemonBridge {
    fn policy(&self) -> AuthorityPolicy {
        self.policy
    }
    fn traces(&self) -> &[(u64, String)] {
        &self.journal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel_bridge::KernelBridge;
    use hakille_ui_model::UiEvent;

    #[test]
    fn detached_refuses_all_sensitive_and_traces_none() {
        let mut b = KernelBridge::new(RealDaemonBridge::detached());
        for ev in [
            UiEvent::CardApprove { card_id: 1 },
            UiEvent::HudSubmitted {
                query: "test".into(),
            },
            UiEvent::SettingsToggled { index: 0 },
            UiEvent::MemoryForget { row_id: "m".into() },
            UiEvent::OutboxFlush,
        ] {
            assert!(b.dispatch(ev).is_err(), "Detached doit refuser sans noyau");
        }
        assert!(b.kernel.traces().is_empty());
    }

    #[test]
    fn authorized_full_flow_request_simulate_confirm_journal_undo() {
        let mut bridge = KernelBridge::new(RealDaemonBridge::local_authorized());
        bridge.kernel.stage_card(7, "Envoi 25.00 EUR à Awa");
        // 1. Demande : le noyau accepte et journalise (id produit côté noyau).
        let reply = bridge
            .dispatch(UiEvent::CardApprove { card_id: 7 })
            .unwrap()
            .unwrap();
        match reply {
            hakille_ui_model::KernelEvent::JournalAppended { id } => assert_eq!(id, 1),
            other => panic!("attendu journal, obtenu {other:?}"),
        }
        // 2. HUD : réponse honnête, pas de validation surprise.
        let hud = bridge
            .dispatch(UiEvent::HudSubmitted {
                query: "résume".into(),
            })
            .unwrap()
            .unwrap();
        assert!(matches!(
            hud,
            hakille_ui_model::KernelEvent::HudReply { .. }
        ));
        // 3. Journal puis annulation : nouvelle entrée signée côté noyau.
        let undo = bridge
            .dispatch(UiEvent::JournalRevert { index: 1 })
            .unwrap()
            .unwrap();
        assert!(matches!(
            undo,
            hakille_ui_model::KernelEvent::JournalAppended { id: 3 }
        ));
        // 4. Trace locale = miroir du journal noyau, ordonnée.
        let traces = bridge.kernel.traces();
        assert_eq!(traces.len(), 3);
        assert_eq!(traces[0].1, "approuvé carte #7");
        assert_eq!(traces[2].1, "annulée entrée #1");
    }

    #[test]
    fn approve_is_only_a_request_never_an_effect() {
        // Même autorisé, Approve ne produit qu'un identifiant de journal :
        // aucun champ d'effet, aucune capability, aucune durée d'appui.
        let mut bridge = KernelBridge::new(RealDaemonBridge::local_authorized());
        let msg = UiMessage::new(UiToKernel::Approve { card_id: 9 });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("capability") && !json.contains("hold"));
        let reply = bridge.kernel.submit(msg).unwrap();
        assert!(matches!(reply, KernelToUi::JournalAppended { .. }));
    }

    #[test]
    fn local_events_stay_local_even_when_authorized() {
        let mut bridge = KernelBridge::new(RealDaemonBridge::local_authorized());
        assert!(bridge.dispatch(UiEvent::PresenceTapped).unwrap().is_none());
        assert!(
            bridge
                .dispatch(UiEvent::OnboardingAcknowledge)
                .unwrap()
                .is_none()
        );
        assert!(bridge.kernel.traces().is_empty());
    }
}
