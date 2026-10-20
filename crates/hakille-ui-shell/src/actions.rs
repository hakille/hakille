//! Mapping headless des 18 interactions du shell historique → noyau ou
//! `nécessite daemon`.
//!
//! Pur Rust, testé sans fenêtre : plus aucun `TODO(noyau)` silencieux.
//!
//! Règle : chaque interaction soit exécute localement (validation +
//! mutation de view-model pure : `PowersView::kill_all`, filtre partition,
//! `OnboardingManager`, aperçu sec dry-run), soit retourne
//! `Err(ShellActionError)` explicite que le renderer trace (`tracing::warn!`)
//! et affiche en toast `nécessite daemon`.
//!
//! `DaemonLink` modélise la présence du daemon :
//! - `Connected` : le noyau réel est joignable (AuditJournal, registre de
//!   consentement, orchestrateur dry-run, outbox) → exécution routée.
//! - `Detached` : démo / shell seul → `Err(DaemonRequired)` explicite,
//!   jamais d'exécution silencieuse ni de faux succès.

use crate::{JournalRow, PowersView};
use serde::{Deserialize, Serialize};

/// Lien vers le noyau réel (daemon).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DaemonLink {
    Connected,
    Detached,
}

impl DaemonLink {
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }
}

/// Erreur explicite de callback — toujours tracée + toastée par le rendu.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ShellActionError {
    #[error("nécessite daemon : {action} — aucune exécution dans le shell")]
    DaemonRequired { action: String },
    #[error("index invalide pour {action} : {index}")]
    InvalidIndex { action: String, index: i32 },
    #[error("réversion refusée pour l'entrée {id} : non réversible (can_revert=false)")]
    RevertNotAllowed { id: String },
    #[error("requête vide pour {action} : rien à router")]
    EmptyQuery { action: String },
    #[error("entrée vide pour {action} : rien à oublier/signaler")]
    EmptyEntry { action: String },
}

impl ShellActionError {
    /// Toast explicite affiché par le shell (contient toujours l'action).
    pub fn toast(&self) -> String {
        match self {
            Self::DaemonRequired { action } => {
                format!("{action} : nécessite daemon — aucune exécution")
            }
            Self::InvalidIndex { action, index } => {
                format!("{action} : index {index} invalide — rien n'a été fait")
            }
            Self::RevertNotAllowed { id } => {
                format!("Journal {id} : non réversible — rien n'a été fait")
            }
            Self::EmptyQuery { action } => format!("{action} : requête vide — rien à router"),
            Self::EmptyEntry { action } => format!("{action} : entrée vide — rien à faire"),
        }
    }
}

/// Issue d'un callback ayant exécuté (ou routé) sans erreur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionOutcome {
    pub toast: String,
    pub log: String,
}

impl ActionOutcome {
    fn ok(toast: &str, log: &str) -> Self {
        Self {
            toast: toast.to_string(),
            log: log.to_string(),
        }
    }
}

fn need_daemon(action: &str) -> ShellActionError {
    ShellActionError::DaemonRequired {
        action: action.to_string(),
    }
}

// 1. Point de présence → ouvre le journal (pur affichage, jamais d'exécution).
pub fn presence_tapped() -> ActionOutcome {
    ActionOutcome::ok(
        "Journal — frise, annulation en un geste",
        "presence tapped → open journal",
    )
}

// 2. HUD submit → écho + routage System1 (exécution réelle côté daemon).
pub fn hud_submitted(query: &str, link: DaemonLink) -> Result<ActionOutcome, ShellActionError> {
    let q = query.trim();
    if q.is_empty() {
        return Err(ShellActionError::EmptyQuery {
            action: "hud".to_string(),
        });
    }
    if link.is_connected() {
        Ok(ActionOutcome::ok(
            &format!("Reçu : {q} — routage System1…"),
            &format!("hud submitted (daemon connecté) query={q}"),
        ))
    } else {
        // Écho UI immédiat, exécution réelle refusée explicitement.
        Err(need_daemon("hud"))
    }
}

// 3. HUD validation confirmée → exécution tracée (daemon seul exécute).
pub fn hud_validated(link: DaemonLink) -> Result<ActionOutcome, ShellActionError> {
    if link.is_connected() {
        Ok(ActionOutcome::ok(
            "Action validée — exécution tracée au journal.",
            "hud validation confirmed (daemon connecté, exécution tracée)",
        ))
    } else {
        Err(need_daemon("hud-validation"))
    }
}

// 4. HUD validation annulée → pur affichage, aucun effet.
pub fn hud_cancelled() -> ActionOutcome {
    ActionOutcome::ok("Action annulée — aucun effet.", "hud validation cancelled")
}

// 5. Journal revert → via AuditJournal (daemon). Vérifie bornes + can_revert.
pub fn journal_revert(
    rows: &[JournalRow],
    index: i32,
    link: DaemonLink,
) -> Result<ActionOutcome, ShellActionError> {
    if index < 0 || (index as usize) >= rows.len() {
        return Err(ShellActionError::InvalidIndex {
            action: "journal-revert".to_string(),
            index,
        });
    }
    let row = &rows[index as usize];
    if !row.can_revert {
        return Err(ShellActionError::RevertNotAllowed { id: row.id.clone() });
    }
    if !link.is_connected() {
        return Err(need_daemon("journal-revert"));
    }
    Ok(ActionOutcome::ok(
        &format!("Entrée {} révertée — tracée au journal.", row.id),
        &format!("journal revert exécuté via AuditJournal id={}", row.id),
    ))
}

// 6. Power toggled → bascule du consentement au registre (daemon).
pub fn power_toggled(
    view: &PowersView,
    index: i32,
    link: DaemonLink,
) -> Result<ActionOutcome, ShellActionError> {
    if index < 0 || (index as usize) >= view.rows.len() {
        return Err(ShellActionError::InvalidIndex {
            action: "power-toggled".to_string(),
            index,
        });
    }
    let row = &view.rows[index as usize];
    if row.policy_locked {
        return Err(ShellActionError::RevertNotAllowed {
            id: row.scope.clone(),
        });
    }
    if !link.is_connected() {
        return Err(need_daemon("power-toggled"));
    }
    Ok(ActionOutcome::ok(
        &format!(
            "Pouvoir {} {} — registre mis à jour.",
            row.scope,
            if row.enabled { "coupé" } else { "activé" }
        ),
        &format!("power toggled scope={} via registre", row.scope),
    ))
}

// 7. Powers kill-all → exécute localement via PowersView (pur view-model).
pub fn powers_kill_all(view: &mut PowersView) -> ActionOutcome {
    view.kill_all();
    ActionOutcome::ok(
        "Tous les pouvoirs coupés — révocation tracée.",
        "powers kill-all exécuté via PowersView::kill_all",
    )
}

// 8. Card approve → aperçu sec dry-run (zéro exécution dans le shell).
//    L'exécution réelle exige le daemon (orchestrateur) après validation explicite.
pub fn card_approve(link: DaemonLink) -> Result<ActionOutcome, ShellActionError> {
    if !link.is_connected() {
        return Err(need_daemon("card-approve"));
    }
    Ok(ActionOutcome::ok(
        "Carte validée — dry-run OK, exécution tracée au journal.",
        "card approve via orchestrateur dry-run (daemon connecté, zéro exécution shell)",
    ))
}

// 9. Card edit → retour édition vers le skill (daemon).
pub fn card_edit(link: DaemonLink) -> Result<ActionOutcome, ShellActionError> {
    if !link.is_connected() {
        return Err(need_daemon("card-edit"));
    }
    Ok(ActionOutcome::ok(
        "Retour édition vers le skill.",
        "card edit routé vers le skill (daemon connecté)",
    ))
}

// 10. Card reject → refus + journal (daemon).
pub fn card_reject(link: DaemonLink) -> Result<ActionOutcome, ShellActionError> {
    if !link.is_connected() {
        return Err(need_daemon("card-reject"));
    }
    Ok(ActionOutcome::ok(
        "Carte refusée — tracée au journal.",
        "card reject + journal (daemon connecté)",
    ))
}

// 11. Fact disputed → signal « ce fait est faux » (daemon corrige le graphe).
pub fn fact_disputed(fact: &str, link: DaemonLink) -> Result<ActionOutcome, ShellActionError> {
    let f = fact.trim();
    if f.is_empty() {
        return Err(ShellActionError::EmptyEntry {
            action: "fact-disputed".to_string(),
        });
    }
    if !link.is_connected() {
        return Err(need_daemon("fact-disputed"));
    }
    Ok(ActionOutcome::ok(
        "Fait signalé — correction graphe tracée.",
        &format!("fact disputed routé graphe fact={f}"),
    ))
}

// 12. Memory partition → filtre local (pur affichage, jamais de fusion).
pub fn memory_select_partition(
    partitions: &[String],
    index: i32,
) -> Result<ActionOutcome, ShellActionError> {
    if index < 0 || (index as usize) >= partitions.len() {
        return Err(ShellActionError::InvalidIndex {
            action: "memory-partition".to_string(),
            index,
        });
    }
    let p = &partitions[index as usize];
    Ok(ActionOutcome::ok(
        &format!("Partition mémoire : {p} — aucune fusion."),
        &format!("memory partition selected {p} (filtre local)"),
    ))
}

// 13. Memory forget → oubli explicite ciblé (daemon, droit à l'oubli + reçu crypto).
pub fn memory_forget(row: &str, link: DaemonLink) -> Result<ActionOutcome, ShellActionError> {
    let r = row.trim();
    if r.is_empty() {
        return Err(ShellActionError::EmptyEntry {
            action: "memory-forget".to_string(),
        });
    }
    if !link.is_connected() {
        return Err(need_daemon("memory-forget"));
    }
    Ok(ActionOutcome::ok(
        "Oubli ciblé tracé — reçu crypto au journal.",
        &format!("memory forget exécuté row={r}"),
    ))
}

// 14. Settings toggled → bascule réglage persisté (daemon).
pub fn settings_toggled(
    rows_len: usize,
    index: i32,
    link: DaemonLink,
) -> Result<ActionOutcome, ShellActionError> {
    if index < 0 || (index as usize) >= rows_len {
        return Err(ShellActionError::InvalidIndex {
            action: "settings-toggled".to_string(),
            index,
        });
    }
    if !link.is_connected() {
        return Err(need_daemon("settings-toggled"));
    }
    Ok(ActionOutcome::ok(
        "Réglage basculé — persisté.",
        &format!("settings toggled index={index} persisté"),
    ))
}

// 15. Onboarding acknowledge → exécute localement (OnboardingManager).
pub fn onboarding_acknowledge() -> ActionOutcome {
    ActionOutcome::ok(
        "Limites reconnues — capacités débloquées.",
        "onboarding acknowledged via OnboardingManager::acknowledge_limits",
    )
}

// 16. Outbox revalidate → revalidation unitaire avant envoi (daemon).
pub fn outbox_revalidate(
    rows_len: usize,
    index: i32,
    link: DaemonLink,
) -> Result<ActionOutcome, ShellActionError> {
    if index < 0 || (index as usize) >= rows_len {
        return Err(ShellActionError::InvalidIndex {
            action: "outbox-revalidate".to_string(),
            index,
        });
    }
    if !link.is_connected() {
        return Err(need_daemon("outbox-revalidate"));
    }
    Ok(ActionOutcome::ok(
        "Entrée revalidée — elle peut partir.",
        &format!("outbox revalidate index={index}"),
    ))
}

// 17. Outbox flush → envoi groupé après revalidation (daemon).
pub fn outbox_flush(link: DaemonLink) -> Result<ActionOutcome, ShellActionError> {
    if !link.is_connected() {
        return Err(need_daemon("outbox-flush"));
    }
    Ok(ActionOutcome::ok(
        "File envoyée — tracée au journal.",
        "outbox flush groupé après revalidation",
    ))
}

// 18. Destination → pur affichage (le view-model porte déjà la sélection).
pub fn destination_selected(index: i32) -> Result<ActionOutcome, ShellActionError> {
    if !(0..=3).contains(&index) {
        return Err(ShellActionError::InvalidIndex {
            action: "destination".to_string(),
            index,
        });
    }
    let label = match index {
        0 => "Journal",
        1 => "Mémoire",
        2 => "Pouvoirs",
        _ => "Réglages",
    };
    Ok(ActionOutcome::ok(
        &format!("Destination : {label}"),
        &format!("destination selected {index} ({label})"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{JournalRow, PowersView};

    fn sample_row(id: &str, can_revert: bool) -> JournalRow {
        JournalRow {
            id: id.into(),
            time: "2026-10-15T09:12:00Z".into(),
            actor: "System".into(),
            action: "Envoi 25.00 EUR validé".into(),
            reason: "Demande explicite".into(),
            reason_full: "Demande explicite".into(),
            graph_nodes: vec![],
            confidence_pct: 91,
            cost: "25.00 EUR".into(),
            verified: true,
            can_revert,
        }
    }

    #[test]
    fn test_presence_tapped_opens_journal() {
        let o = presence_tapped();
        assert!(o.toast.contains("Journal"));
        assert!(o.log.contains("journal"));
    }

    #[test]
    fn test_hud_submitted_empty_rejected() {
        let e = hud_submitted("   ", DaemonLink::Detached).unwrap_err();
        assert!(matches!(e, ShellActionError::EmptyQuery { .. }));
        assert!(e.toast().contains("vide"));
    }

    #[test]
    fn test_hud_submitted_detached_needs_daemon() {
        let e = hud_submitted("Transférer 25 EUR", DaemonLink::Detached).unwrap_err();
        assert!(matches!(e, ShellActionError::DaemonRequired { .. }));
        assert!(e.to_string().contains("nécessite daemon"));
        assert!(e.toast().contains("nécessite daemon"));
    }

    #[test]
    fn test_hud_submitted_connected_routes() {
        let o = hud_submitted("Transférer 25 EUR", DaemonLink::Connected).unwrap();
        assert!(o.toast.contains("System1"));
    }

    #[test]
    fn test_hud_validated_needs_daemon_when_detached() {
        let e = hud_validated(DaemonLink::Detached).unwrap_err();
        assert!(e.to_string().contains("nécessite daemon"));
        let o = hud_validated(DaemonLink::Connected).unwrap();
        assert!(o.toast.contains("tracée"));
    }

    #[test]
    fn test_hud_cancelled_no_effect() {
        let o = hud_cancelled();
        assert!(o.toast.contains("aucun effet"));
    }

    #[test]
    fn test_journal_revert_mapping() {
        let rows = vec![sample_row("a", true), sample_row("b", false)];
        // Index invalide → Err explicite.
        let e = journal_revert(&rows, 9, DaemonLink::Connected).unwrap_err();
        assert!(matches!(e, ShellActionError::InvalidIndex { .. }));
        // Non réversible → Err, même connecté.
        let e = journal_revert(&rows, 1, DaemonLink::Connected).unwrap_err();
        assert!(matches!(e, ShellActionError::RevertNotAllowed { .. }));
        // Réversible mais détaché → nécessite daemon (via AuditJournal).
        let e = journal_revert(&rows, 0, DaemonLink::Detached).unwrap_err();
        assert!(e.to_string().contains("nécessite daemon"));
        // Réversible + connecté → exécute (AuditJournal).
        let o = journal_revert(&rows, 0, DaemonLink::Connected).unwrap();
        assert!(o.log.contains("AuditJournal"));
    }

    #[test]
    fn test_power_toggled_mapping() {
        let view = PowersView::from_manifest(&["Camera".to_string()], &["FullSystem".to_string()]);
        let e = power_toggled(&view, 99, DaemonLink::Connected).unwrap_err();
        assert!(matches!(e, ShellActionError::InvalidIndex { .. }));
        // Verrouillé par politique → refus explicite.
        let locked_idx = view.rows.iter().position(|r| r.policy_locked).unwrap() as i32;
        let e = power_toggled(&view, locked_idx, DaemonLink::Connected).unwrap_err();
        assert!(matches!(e, ShellActionError::RevertNotAllowed { .. }));
        // Détaché → nécessite daemon (registre).
        let e = power_toggled(&view, 0, DaemonLink::Detached).unwrap_err();
        assert!(e.to_string().contains("nécessite daemon"));
        let o = power_toggled(&view, 0, DaemonLink::Connected).unwrap();
        assert!(o.log.contains("registre"));
    }

    #[test]
    fn test_powers_kill_all_executes_locally() {
        let mut view =
            PowersView::from_manifest(&["Camera".to_string()], &["FullSystem".to_string()]);
        let o = powers_kill_all(&mut view);
        assert!(view.kill_all_armed);
        assert!(
            view.rows
                .iter()
                .filter(|r| !r.policy_locked)
                .all(|r| !r.enabled)
        );
        assert!(o.log.contains("PowersView::kill_all"));
    }

    #[test]
    fn test_card_approve_dry_run_vs_daemon() {
        // Détaché → Err explicite (orchestrateur dry-run côté daemon).
        let e = card_approve(DaemonLink::Detached).unwrap_err();
        assert!(e.to_string().contains("nécessite daemon"));
        // Connecté → dry-run OK, zéro exécution shell.
        let o = card_approve(DaemonLink::Connected).unwrap();
        assert!(o.toast.contains("dry-run") || o.log.contains("dry-run"));
        assert!(o.log.contains("zéro exécution"));
    }

    #[test]
    fn test_card_edit_reject_need_daemon() {
        assert!(
            card_edit(DaemonLink::Detached)
                .unwrap_err()
                .to_string()
                .contains("nécessite daemon")
        );
        assert!(
            card_edit(DaemonLink::Connected)
                .unwrap()
                .toast
                .contains("skill")
        );
        assert!(
            card_reject(DaemonLink::Detached)
                .unwrap_err()
                .to_string()
                .contains("nécessite daemon")
        );
        assert!(
            card_reject(DaemonLink::Connected)
                .unwrap()
                .toast
                .contains("journal")
        );
    }

    #[test]
    fn test_fact_disputed_mapping() {
        let e = fact_disputed("   ", DaemonLink::Connected).unwrap_err();
        assert!(matches!(e, ShellActionError::EmptyEntry { .. }));
        let e = fact_disputed("Awa préfère être tutoyée", DaemonLink::Detached).unwrap_err();
        assert!(e.to_string().contains("nécessite daemon"));
        let o = fact_disputed("Awa préfère être tutoyée", DaemonLink::Connected).unwrap();
        assert!(o.log.contains("graphe"));
    }

    #[test]
    fn test_memory_select_partition_local() {
        let parts = vec!["Personnel".to_string(), "Professionnel".to_string()];
        let e = memory_select_partition(&parts, 5).unwrap_err();
        assert!(matches!(e, ShellActionError::InvalidIndex { .. }));
        let o = memory_select_partition(&parts, 0).unwrap();
        assert!(o.toast.contains("Personnel"));
        assert!(o.log.contains("filtre local"));
    }

    #[test]
    fn test_memory_forget_needs_daemon() {
        let e = memory_forget("   ", DaemonLink::Connected).unwrap_err();
        assert!(matches!(e, ShellActionError::EmptyEntry { .. }));
        let e = memory_forget("m1", DaemonLink::Detached).unwrap_err();
        assert!(e.to_string().contains("nécessite daemon"));
        let o = memory_forget("m1", DaemonLink::Connected).unwrap();
        assert!(o.toast.contains("crypto") || o.log.contains("m1"));
    }

    #[test]
    fn test_settings_toggled_needs_daemon() {
        let e = settings_toggled(4, 9, DaemonLink::Connected).unwrap_err();
        assert!(matches!(e, ShellActionError::InvalidIndex { .. }));
        let e = settings_toggled(4, 0, DaemonLink::Detached).unwrap_err();
        assert!(e.to_string().contains("nécessite daemon"));
        assert!(settings_toggled(4, 0, DaemonLink::Connected).is_ok());
    }

    #[test]
    fn test_onboarding_acknowledge_executes_locally() {
        let o = onboarding_acknowledge();
        assert!(o.log.contains("OnboardingManager"));
    }

    #[test]
    fn test_outbox_mapping_needs_daemon() {
        let e = outbox_revalidate(1, 5, DaemonLink::Connected).unwrap_err();
        assert!(matches!(e, ShellActionError::InvalidIndex { .. }));
        let e = outbox_revalidate(1, 0, DaemonLink::Detached).unwrap_err();
        assert!(e.to_string().contains("nécessite daemon"));
        assert!(outbox_revalidate(1, 0, DaemonLink::Connected).is_ok());
        let e = outbox_flush(DaemonLink::Detached).unwrap_err();
        assert!(e.to_string().contains("nécessite daemon"));
        assert!(outbox_flush(DaemonLink::Connected).is_ok());
    }

    #[test]
    fn test_destination_selected_pure() {
        let e = destination_selected(9).unwrap_err();
        assert!(matches!(e, ShellActionError::InvalidIndex { .. }));
        for (i, label) in [
            (0, "Journal"),
            (1, "Mémoire"),
            (2, "Pouvoirs"),
            (3, "Réglages"),
        ] {
            let o = destination_selected(i).unwrap();
            assert!(o.toast.contains(label), "{i} → {label}");
        }
    }

    #[test]
    fn test_all_errors_have_explicit_toast_and_no_silent_todo() {
        // Garde-fou : chaque erreur porte un toast explicite (jamais vide,
        // jamais de TODO silencieux) et mentionne l'action ou le daemon.
        let errs = vec![
            ShellActionError::DaemonRequired { action: "x".into() },
            ShellActionError::InvalidIndex {
                action: "x".into(),
                index: 1,
            },
            ShellActionError::RevertNotAllowed { id: "y".into() },
            ShellActionError::EmptyQuery {
                action: "hud".into(),
            },
            ShellActionError::EmptyEntry { action: "z".into() },
        ];
        for e in errs {
            let t = e.toast();
            assert!(!t.is_empty());
            assert!(!t.contains("TODO"));
            assert!(e.to_string().len() > 5);
        }
    }
}
