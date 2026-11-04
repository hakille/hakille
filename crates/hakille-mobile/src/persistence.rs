//! Checkpoint minimal v2 : brouillon et métadonnées d'incertitude uniquement.
//! L'hôte doit le publier durablement AVANT d'émettre une requête retournée.
use crate::core::{Core, MAX_DRAFT_BYTES, MAX_RECOVERY_OPERATIONS, text};
use crate::types::*;
use std::collections::BTreeMap;

pub(crate) const MAX_CHECKPOINT_BYTES: usize = 64 * 1024;
const RECOVERY_NOTICE: &str = "Une opération précédente, éventuellement sur un autre hub, reste incertaine. Consulter le journal avant de prendre connaissance de l'avertissement ; aucun renvoi automatique.";
const ACK_NOTICE: &str = "Avertissement déjà lu ; l'effet de cette opération reste inconnu, sans annulation ni confirmation déduite.";

impl Core {
    pub(crate) fn saved_draft(&self) -> SavedDraftV2 {
        SavedDraftV2 {
            schema_version: 2,
            draft: self.snapshot.draft.clone(),
            next_request_id: self.next_id,
            uncertain_operations: self.snapshot.uncertain_operations.clone(),
            acknowledged_operations: self.snapshot.acknowledged_operations.clone(),
        }
    }

    pub(crate) fn restore_saved(&mut self, saved: SavedDraft) -> Result<(), String> {
        if self.snapshot.pending.is_some() {
            return Err("Restauration refusée pendant une requête".into());
        }
        let mut candidate = self.clone();
        match saved {
            SavedDraft::V1(draft) => {
                if draft.schema_version != 1 {
                    return Err("Version de brouillon incompatible".into());
                }
                text(&draft.draft, MAX_DRAFT_BYTES, true)?;
                // Un ancien brouillon ne peut effacer les incertitudes déjà
                // connues de cette instance ni remettre son compteur à zéro.
                candidate.snapshot.draft = draft.draft;
            }
            SavedDraft::V2(saved) => {
                validate_saved(&saved)?;
                candidate.snapshot.draft = saved.draft;
                candidate.next_id = candidate.next_id.max(saved.next_request_id);
                for archive in saved.acknowledged_operations {
                    candidate.merge_archive(archive)?;
                }
                for operation in saved.uncertain_operations {
                    candidate.merge_uncertain(operation)?;
                }
                if candidate.snapshot.uncertain_operations.len() > MAX_RECOVERY_OPERATIONS {
                    return Err("Trop d'opérations incertaines après fusion du checkpoint".into());
                }
                candidate
                    .snapshot
                    .uncertain_operations
                    .sort_by_key(|op| op.request_id);
                candidate
                    .snapshot
                    .acknowledged_operations
                    .sort_by_key(|op| op.request_id);
                let excess = candidate
                    .snapshot
                    .acknowledged_operations
                    .len()
                    .saturating_sub(MAX_RECOVERY_OPERATIONS);
                candidate.snapshot.acknowledged_operations.drain(..excess);
                // Reconstituer un avertissement lisible, jamais une demande
                // pending ou une trame à renvoyer au hub.
                for archive in candidate.snapshot.acknowledged_operations.clone() {
                    candidate.ensure_recovery_outcome(archive.request_id, archive.kind, true)?;
                }
                for operation in candidate.snapshot.uncertain_operations.clone() {
                    candidate.ensure_recovery_outcome(
                        operation.request_id,
                        operation.kind,
                        false,
                    )?;
                }
            }
        }
        candidate.cancel_hold();
        candidate.invalidate_voice();
        candidate.snapshot.microphone_enabled = false;
        candidate.refresh_recovery();
        if candidate.snapshot.recovery_required {
            candidate.snapshot.connection = ConnectionState::Unknown;
            candidate.snapshot.gateway_session = None;
            candidate.snapshot.cards.clear();
            if !candidate
                .snapshot
                .notifications
                .iter()
                .any(|notice| notice == RECOVERY_NOTICE)
            {
                candidate
                    .snapshot
                    .notifications
                    .push(RECOVERY_NOTICE.into());
                if candidate.snapshot.notifications.len() > 16 {
                    candidate.snapshot.notifications.remove(0);
                }
            }
        }
        candidate.trim_history_to_budget()?;
        *self = candidate;
        Ok(())
    }

    fn reject_contradictory_outcome(
        &self,
        request_id: u64,
        kind: MutationKind,
    ) -> Result<(), String> {
        if self.snapshot.outcomes.iter().any(|outcome| {
            outcome.request_id == request_id
                && (outcome.kind != kind.label() || outcome.state != "unknown")
        }) {
            return Err("Le checkpoint contredit un résultat déjà reçu dans cette session".into());
        }
        Ok(())
    }

    fn merge_archive(&mut self, archive: AcknowledgedOperation) -> Result<(), String> {
        if self
            .snapshot
            .uncertain_operations
            .iter()
            .any(|op| op.request_id == archive.request_id)
        {
            // Importer un fichier n'est pas une action utilisateur permettant
            // de masquer une incertitude actuellement connue.
            return Err("L'archive importée contredit une opération encore incertaine".into());
        }
        self.reject_contradictory_outcome(archive.request_id, archive.kind)?;
        if let Some(existing) = self
            .snapshot
            .acknowledged_operations
            .iter()
            .find(|op| op.request_id == archive.request_id)
        {
            if existing.kind != archive.kind {
                return Err("Identifiant d'opération ambigu".into());
            }
            return Ok(());
        }
        self.snapshot.acknowledged_operations.push(archive);
        Ok(())
    }

    fn merge_uncertain(&mut self, mut operation: UncertainOperation) -> Result<(), String> {
        // Une archive déjà connue ne se réarme pas lorsque le même ancien
        // checkpoint est restauré une seconde fois.
        if let Some(archive) = self
            .snapshot
            .acknowledged_operations
            .iter()
            .find(|op| op.request_id == operation.request_id)
        {
            if archive.kind != operation.kind {
                return Err("Identifiant d'opération ambigu".into());
            }
            return Ok(());
        }
        self.reject_contradictory_outcome(operation.request_id, operation.kind)?;
        operation.phase = OperationPhase::Unknown;
        if let Some(existing) = self
            .snapshot
            .uncertain_operations
            .iter_mut()
            .find(|op| op.request_id == operation.request_id)
        {
            if existing.kind != operation.kind {
                return Err("Identifiant d'opération ambigu".into());
            }
            existing.phase = OperationPhase::Unknown;
        } else {
            self.snapshot.uncertain_operations.push(operation);
        }
        Ok(())
    }

    fn ensure_recovery_outcome(
        &mut self,
        request_id: u64,
        kind: MutationKind,
        acknowledged: bool,
    ) -> Result<(), String> {
        self.reject_contradictory_outcome(request_id, kind)?;
        if let Some(outcome) = self
            .snapshot
            .outcomes
            .iter_mut()
            .find(|outcome| outcome.request_id == request_id)
        {
            // L'état reste unknown, y compris après prise de connaissance.
            outcome.acknowledged |= acknowledged;
            return Ok(());
        }
        self.snapshot.outcomes.push(RequestOutcome {
            request_id,
            kind: kind.label().into(),
            state: "unknown".into(),
            message: if acknowledged {
                ACK_NOTICE
            } else {
                RECOVERY_NOTICE
            }
            .into(),
            result: None,
            acknowledged,
        });
        if self.snapshot.outcomes.len() > 32 {
            self.snapshot.outcomes.remove(0);
            self.snapshot.history_trimmed = true;
        }
        Ok(())
    }
}

fn validate_saved(saved: &SavedDraftV2) -> Result<(), String> {
    if saved.schema_version != 2 {
        return Err("Version de checkpoint incompatible".into());
    }
    text(&saved.draft, MAX_DRAFT_BYTES, true)?;
    if saved.next_request_id == 0 {
        return Err("Compteur de requête nul interdit".into());
    }
    if saved.uncertain_operations.len() > MAX_RECOVERY_OPERATIONS
        || saved.acknowledged_operations.len() > MAX_RECOVERY_OPERATIONS
    {
        return Err("Checkpoint supérieur à 32 incertitudes ou 32 archives".into());
    }
    let mut ids = BTreeMap::new();
    for (id, kind) in saved
        .uncertain_operations
        .iter()
        .map(|op| (op.request_id, op.kind))
        .chain(
            saved
                .acknowledged_operations
                .iter()
                .map(|op| (op.request_id, op.kind)),
        )
    {
        if id == 0 || id == u64::MAX || id >= saved.next_request_id {
            return Err("Identifiant d'opération réservé ou compteur incohérent".into());
        }
        if ids.insert(id, kind).is_some() {
            return Err("Identifiant d'opération dupliqué dans le checkpoint".into());
        }
    }
    Ok(())
}
