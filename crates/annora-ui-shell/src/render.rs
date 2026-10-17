//! Fenêtre Slint réelle. Nécessite feature `render`.
//!
//! Event-loop sur le thread main (exigence Slint/winit) ; la logique métier
//! tourne sur worker et pousse via `invoke_from_event_loop`.
//!
//! Chaque callback Slint est branché sur `crate::actions` (headless, testé
//! sans fenêtre) : soit il exécute localement (kill_all via PowersView,
//! aperçu sec dry-run, filtres), soit il retourne `Err` explicite tracé +
//! toast `nécessite daemon`. Plus aucun `TODO` silencieux : le shell rend
//! et route, le daemon décide et agit (spec : shell ne décide ni n'agit).

#[cfg(feature = "render")]
use crate::AnnoraWindow;
#[cfg(feature = "render")]
use crate::bridge::{ShellPrefs, ShellSnapshot};
#[cfg(feature = "render")]
use crate::{JournalRow, MemoryRow, OnboardingView, PowersView, SettingsView};
#[cfg(feature = "render")]
use annora_design::{ActionCardView, WhySheet};
#[cfg(feature = "render")]
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

#[cfg(feature = "render")]
fn str_model(lines: &[String]) -> ModelRc<SharedString> {
    let v: Vec<SharedString> = lines.iter().map(SharedString::from).collect();
    ModelRc::new(VecModel::from_slice(&v))
}

#[cfg(feature = "render")]
pub fn run_window(initial: ShellSnapshot) -> Result<(), String> {
    use crate::actions::{DaemonLink, destination_selected};
    use std::cell::RefCell;
    use std::rc::Rc;

    let win = AnnoraWindow::new().map_err(|e| e.to_string())?;
    apply_snapshot(&win, &initial);
    apply_prefs(&win, &ShellPrefs::default(), initial.presence_pulse);

    // Shell seul (démo/galerie) : pas de daemon → les actions noyau
    // retournent `Err(nécessite daemon)` explicite + toast. Le shell
    // connecté au daemon passerait `DaemonLink::Connected`.
    const LINK: DaemonLink = DaemonLink::Detached;

    // État démo partagé pour les actions locales (kill_all exécute vraiment).
    let powers_demo = Rc::new(RefCell::new(PowersView::from_manifest(
        &["Camera".to_string()],
        &["FullSystem".to_string()],
    )));

    let win_weak = win.as_weak();
    win.on_presence_tapped(move || {
        let out = crate::actions::presence_tapped();
        tracing::info!(log = %out.log, "presence tapped");
        if let Some(w) = win_weak.upgrade() {
            w.set_hud_response(out.toast.into());
        }
    });
    let win_weak = win.as_weak();
    win.on_hud_submitted(move |q| {
        let qs = q.to_string();
        match crate::actions::hud_submitted(&qs, LINK) {
            Ok(out) => {
                tracing::info!(log = %out.log, "hud submitted");
                if let Some(w) = win_weak.upgrade() {
                    w.set_hud_response(out.toast.into());
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "hud submitted → nécessite daemon");
                if let Some(w) = win_weak.upgrade() {
                    w.set_hud_response(e.toast().into());
                }
            }
        }
    });
    let win_weak = win.as_weak();
    win.on_hud_validated(move || match crate::actions::hud_validated(LINK) {
        Ok(out) => {
            tracing::info!(log = %out.log, "hud validation confirmed");
            if let Some(w) = win_weak.upgrade() {
                w.set_hud_validation(false);
                w.set_hud_response(out.toast.into());
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "hud validation → nécessite daemon");
            if let Some(w) = win_weak.upgrade() {
                w.set_hud_validation(false);
                w.set_hud_response(e.toast().into());
            }
        }
    });
    let win_weak = win.as_weak();
    win.on_hud_cancelled(move || {
        let out = crate::actions::hud_cancelled();
        tracing::info!(log = %out.log, "hud validation cancelled");
        if let Some(w) = win_weak.upgrade() {
            w.set_hud_validation(false);
            w.set_hud_response(out.toast.into());
        }
    });

    // Journal revert → via AuditJournal côté daemon, sinon Err explicite.
    let win_weak = win.as_weak();
    win.on_journal_revert(move |i| {
        // Lignes démo visibles (mêmes IDs que feed_demo) pour valider
        // bornes + can_revert sans fenêtre ; la réversion réelle reste daemon.
        let rows = vec![
            JournalRow {
                id: "démo-1".into(),
                time: "2026-10-15T09:12:00Z".into(),
                actor: "System".into(),
                action: "Récap déjeuner préparé".into(),
                reason: "Demande explicite".into(),
                reason_full: "Demande explicite".into(),
                graph_nodes: vec![],
                confidence_pct: 92,
                cost: "0,00 EUR".into(),
                verified: true,
                can_revert: true,
            },
            JournalRow {
                id: "démo-2".into(),
                time: "2026-10-15T08:03:00Z".into(),
                actor: "System".into(),
                action: "Veille mails nuit".into(),
                reason: "Silence préservé".into(),
                reason_full: "Silence préservé".into(),
                graph_nodes: vec![],
                confidence_pct: 88,
                cost: "0,00 EUR".into(),
                verified: true,
                can_revert: false,
            },
        ];
        match crate::actions::journal_revert(&rows, i, LINK) {
            Ok(out) => {
                tracing::info!(log = %out.log, index = i, "journal revert exécuté");
                if let Some(w) = win_weak.upgrade() {
                    w.set_hud_response(out.toast.into());
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, index = i, "journal revert refusé");
                if let Some(w) = win_weak.upgrade() {
                    w.set_hud_response(e.toast().into());
                }
            }
        }
    });
    let win_weak = win.as_weak();
    let powers_for_toggle = powers_demo.clone();
    win.on_power_toggled(move |i| {
        let view = powers_for_toggle.borrow();
        match crate::actions::power_toggled(&view, i, LINK) {
            Ok(out) => {
                tracing::info!(log = %out.log, index = i, "power toggled");
                if let Some(w) = win_weak.upgrade() {
                    w.set_hud_response(out.toast.into());
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, index = i, "power toggled refusé");
                if let Some(w) = win_weak.upgrade() {
                    w.set_hud_response(e.toast().into());
                }
            }
        }
    });
    let win_weak = win.as_weak();
    win.on_powers_kill_all(move || {
        // Exécute vraiment via PowersView (local), puis reflète l'état.
        let mut view =
            PowersView::from_manifest(&["Camera".to_string()], &["FullSystem".to_string()]);
        let out = crate::actions::powers_kill_all(&mut view);
        tracing::info!(log = %out.log, armed = view.kill_all_armed, "powers kill-all exécuté");
        if let Some(w) = win_weak.upgrade() {
            w.set_hud_response(out.toast.into());
        }
    });
    // Carte d'action → dry-run orchestrateur si daemon, sinon Err explicite.
    let win_weak = win.as_weak();
    win.on_card_approve(move || match crate::actions::card_approve(LINK) {
        Ok(out) => {
            tracing::info!(log = %out.log, "card approve dry-run");
            if let Some(w) = win_weak.upgrade() {
                w.set_hud_validation(false);
                w.set_hud_response(out.toast.into());
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "card approve → nécessite daemon");
            if let Some(w) = win_weak.upgrade() {
                w.set_hud_response(e.toast().into());
            }
        }
    });
    let win_weak = win.as_weak();
    win.on_card_edit(move || match crate::actions::card_edit(LINK) {
        Ok(out) => {
            tracing::info!(log = %out.log, "card edit");
            if let Some(w) = win_weak.upgrade() {
                w.set_hud_response(out.toast.into());
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "card edit → nécessite daemon");
            if let Some(w) = win_weak.upgrade() {
                w.set_hud_response(e.toast().into());
            }
        }
    });
    let win_weak = win.as_weak();
    win.on_card_reject(move || match crate::actions::card_reject(LINK) {
        Ok(out) => {
            tracing::info!(log = %out.log, "card reject");
            if let Some(w) = win_weak.upgrade() {
                w.set_hud_validation(false);
                w.set_hud_response(out.toast.into());
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "card reject → nécessite daemon");
            if let Some(w) = win_weak.upgrade() {
                w.set_hud_response(e.toast().into());
            }
        }
    });
    let win_weak = win.as_weak();
    win.on_fact_disputed(move |s| {
        let fact = s.to_string();
        match crate::actions::fact_disputed(&fact, LINK) {
            Ok(out) => {
                tracing::info!(log = %out.log, "fact disputed");
                if let Some(w) = win_weak.upgrade() {
                    w.set_hud_response(out.toast.into());
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "fact disputed → nécessite daemon");
                if let Some(w) = win_weak.upgrade() {
                    w.set_hud_response(e.toast().into());
                }
            }
        }
    });
    let win_weak = win.as_weak();
    win.on_memory_select_partition(move |i| {
        let parts = vec!["Personnel".to_string(), "Professionnel".to_string()];
        match crate::actions::memory_select_partition(&parts, i) {
            Ok(out) => {
                tracing::info!(log = %out.log, index = i, "memory partition selected");
                if let Some(w) = win_weak.upgrade() {
                    w.set_memory_selected(i);
                    w.set_hud_response(out.toast.into());
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, index = i, "memory partition invalide");
                if let Some(w) = win_weak.upgrade() {
                    w.set_hud_response(e.toast().into());
                }
            }
        }
    });
    let win_weak = win.as_weak();
    win.on_memory_forget(move |s| {
        let row = s.to_string();
        match crate::actions::memory_forget(&row, LINK) {
            Ok(out) => {
                tracing::info!(log = %out.log, "memory forget exécuté");
                if let Some(w) = win_weak.upgrade() {
                    w.set_hud_response(out.toast.into());
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "memory forget → nécessite daemon");
                if let Some(w) = win_weak.upgrade() {
                    w.set_hud_response(e.toast().into());
                }
            }
        }
    });
    let win_weak = win.as_weak();
    win.on_settings_toggled(move |i| {
        // 4 lignes réglages démo (fournisseurs/voix/canaux/langues) : bornes réelles.
        match crate::actions::settings_toggled(4, i, LINK) {
            Ok(out) => {
                tracing::info!(log = %out.log, index = i, "settings toggled");
                if let Some(w) = win_weak.upgrade() {
                    w.set_hud_response(out.toast.into());
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, index = i, "settings toggled refusé");
                if let Some(w) = win_weak.upgrade() {
                    w.set_hud_response(e.toast().into());
                }
            }
        }
    });
    let win_weak = win.as_weak();
    win.on_onboarding_acknowledge(move || {
        let out = crate::actions::onboarding_acknowledge();
        tracing::info!(log = %out.log, "onboarding acknowledged");
        if let Some(w) = win_weak.upgrade() {
            w.set_onboarding_ready(true);
            w.set_hud_response(out.toast.into());
        }
    });
    let win_weak = win.as_weak();
    win.on_outbox_revalidate(
        move |i| match crate::actions::outbox_revalidate(1, i, LINK) {
            Ok(out) => {
                tracing::info!(log = %out.log, index = i, "outbox revalidate");
                if let Some(w) = win_weak.upgrade() {
                    w.set_hud_response(out.toast.into());
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, index = i, "outbox revalidate → nécessite daemon");
                if let Some(w) = win_weak.upgrade() {
                    w.set_hud_response(e.toast().into());
                }
            }
        },
    );
    let win_weak = win.as_weak();
    win.on_outbox_flush(move || match crate::actions::outbox_flush(LINK) {
        Ok(out) => {
            tracing::info!(log = %out.log, "outbox flush");
            if let Some(w) = win_weak.upgrade() {
                w.set_hud_response(out.toast.into());
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "outbox flush → nécessite daemon");
            if let Some(w) = win_weak.upgrade() {
                w.set_hud_response(e.toast().into());
            }
        }
    });
    let win_weak = win.as_weak();
    win.on_destination_selected(move |i| match destination_selected(i) {
        Ok(out) => {
            tracing::info!(log = %out.log, destination = i, "destination selected");
            if let Some(w) = win_weak.upgrade() {
                w.set_destination(i);
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, destination = i, "destination invalide");
            if let Some(w) = win_weak.upgrade() {
                w.set_hud_response(e.toast().into());
            }
        }
    });

    feed_demo(&win);

    win.run().map_err(|e| e.to_string())
}

/// Démo : au moins une carte + journal (galerie §28, shell-demo).
/// Données statiques d'exemple, jamais d'exécution.
#[cfg(feature = "render")]
fn feed_demo(win: &AnnoraWindow) {
    let card = ActionCardView {
        verb: "Envoyer".into(),
        object: "ce récap à Awa".into(),
        reason: "Tu as demandé un récap du déjeuner.".into(),
        why_trace: "graphe : déjeuner → Awa → récap".into(),
        cost_label: "0,00 EUR".into(),
        data: annora_design::DataPastille::Local,
        reversibility: annora_design::Reversibility::Undoable { undo_seconds: 30 },
        needs_hold: false,
        provenance_label: "Akora".into(),
        is_external: false,
    };
    apply_card(win, &card);

    let journal = vec![
        JournalRow {
            id: "démo-1".into(),
            time: "2026-10-15T09:12:00Z".into(),
            actor: "System".into(),
            action: "Récap déjeuner préparé".into(),
            reason: "Demande explicite".into(),
            reason_full: "Demande explicite".into(),
            graph_nodes: vec![],
            confidence_pct: 92,
            cost: "0,00 EUR".into(),
            verified: true,
            can_revert: true,
        },
        JournalRow {
            id: "démo-2".into(),
            time: "2026-10-15T08:03:00Z".into(),
            actor: "System".into(),
            action: "Veille mails nuit".into(),
            reason: "Silence préservé".into(),
            reason_full: "Silence préservé".into(),
            graph_nodes: vec![],
            confidence_pct: 88,
            cost: "0,00 EUR".into(),
            verified: true,
            can_revert: false,
        },
    ];
    let powers = PowersView::from_manifest(&["Camera".to_string()], &["FullSystem".to_string()]);
    apply_lists(win, &journal, &powers);

    let why = WhySheet {
        sentence: "J'ai préparé ce récap parce que tu l'as demandé.".into(),
        facts: vec![annora_design::FactRef {
            label: "Déjeuner avec Awa".into(),
            kind: "événement".into(),
            date: "2026-10-15".into(),
            source: "calendrier".into(),
            freshness: "frais".into(),
        }],
        plan: "Envoyer le récap puis tracer au journal.".into(),
        powers: vec!["Partager (bac à sable)".into()],
        confidence_word: "haute".into(),
        confidence_value: 0.92,
        alternatives: vec!["Ne rien envoyer (écarté : demande explicite)".into()],
        model_line: "modèle local".into(),
        cost_label: "0,00 EUR".into(),
        data_line: "Local".into(),
        would_change: "Si la demande n'était pas explicite.".into(),
    };
    apply_why(win, &why);

    let memory = vec![MemoryRow {
        id: "m1".into(),
        text: "Awa préfère être tutoyée".into(),
        weight: 0.9,
        confidence_pct: 90,
        source: "User".into(),
        partition: "Personnel".into(),
    }];
    apply_memory(
        win,
        &memory,
        &["Personnel".to_string(), "Professionnel".to_string()],
        0,
        "1 partition affichée — aucune contradiction.",
    );

    apply_settings(win, &SettingsView::default());

    let mut onboarding = annora_ui::OnboardingManager::new();
    onboarding.acknowledge_limits();
    apply_onboarding(win, &OnboardingView::from_manager(&onboarding));

    apply_outbox(
        win,
        &["Envoi 25,00 EUR — en attente de revalidation".to_string()],
    );
}

#[cfg(feature = "render")]
pub fn apply_snapshot(win: &AnnoraWindow, s: &ShellSnapshot) {
    win.set_presence_state(s.presence_state as i32);
    win.set_presence_pulse(s.presence_pulse);
    win.set_presence_attention(s.presence_attention);
    win.set_hud_query(s.hud_query.clone().into());
    win.set_hud_response(s.hud_response.clone().into());
    win.set_hud_streaming(s.hud_streaming);
    win.set_hud_validation(s.hud_validation);
    win.set_hud_validation_text(s.hud_validation_text.clone().into());
    win.set_hud_validation_cost(s.hud_validation_cost.clone().into());
    win.set_cost_label(format!("{:.2} EUR", s.cost_total_cents as f64 / 100.0).into());
    win.set_cost_level(s.cost_level as i32);
    win.set_silence_label(format!("{} / {}", s.silence_remaining, s.silence_quota).into());
    win.set_high_contrast(s.high_contrast);
    win.set_light_mode(s.light_mode);
    win.set_presence_a11y_label(s.presence_a11y_label.clone().into());
}

/// Préférences : destination + pulse quantifié (mouvement réduit).
#[cfg(feature = "render")]
pub fn apply_prefs(win: &AnnoraWindow, prefs: &ShellPrefs, pulse: f32) {
    win.set_destination(prefs.destination.min(3) as i32);
    win.set_presence_pulse(prefs.quantized_pulse(pulse));
}

/// Carte d'action → ActionCardScreen. Lève `hud-validation` : l'overlay
/// carte est piloté par ce drapeau (spec : une seule décision à la fois).
#[cfg(feature = "render")]
pub fn apply_card(win: &AnnoraWindow, card: &ActionCardView) {
    let pastilles = card.pastilles();
    win.set_card_verb(card.verb.clone().into());
    win.set_card_object(card.object.clone().into());
    win.set_card_reason(card.reason.clone().into());
    win.set_card_why_trace(card.why_trace.clone().into());
    win.set_card_cost(pastilles[0].clone().into());
    win.set_card_data(pastilles[1].clone().into());
    win.set_card_reversibility(pastilles[2].clone().into());
    win.set_card_provenance(card.provenance_label.clone().into());
    win.set_card_external(card.is_external);
    win.set_card_needs_hold(card.needs_hold);
    win.set_card_hold_progress(if card.needs_hold { 0.0 } else { 1.0 });
    win.set_hud_validation(true);
    win.set_hud_validation_text(format!("{} {}", card.verb, card.object).into());
    win.set_hud_validation_cost(card.cost_label.clone().into());
}

/// Feuille Pourquoi → WhyScreen (mots en surface, valeur jointe).
#[cfg(feature = "render")]
pub fn apply_why(win: &AnnoraWindow, why: &WhySheet) {
    win.set_why_sentence(why.sentence.clone().into());
    let facts: Vec<String> = why
        .facts
        .iter()
        .map(|f| {
            format!(
                "{} — {} · {} · {} (fraîcheur : {})",
                f.label, f.kind, f.date, f.source, f.freshness
            )
        })
        .collect();
    win.set_why_facts(str_model(&facts));
    win.set_why_plan(why.plan.clone().into());
    win.set_why_powers(str_model(&why.powers));
    win.set_why_confidence(
        format!(
            "{} — {:.0} %",
            why.confidence_word,
            why.confidence_value * 100.0
        )
        .into(),
    );
    win.set_why_alternatives(str_model(&why.alternatives));
    win.set_why_model_line(why.model_line.clone().into());
    win.set_why_cost_line(why.cost_label.clone().into());
    win.set_why_data_line(why.data_line.clone().into());
    win.set_why_would_change(why.would_change.clone().into());
}

/// Mémoire → MemoryScreen. `selected` = index de `partitions`.
#[cfg(feature = "render")]
pub fn apply_memory(
    win: &AnnoraWindow,
    rows: &[MemoryRow],
    partitions: &[String],
    selected: usize,
    contra: &str,
) {
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "{} — {} · {} · {} (confiance {} %)",
                r.text,
                r.source,
                r.partition,
                r.freshness(),
                r.confidence_pct
            )
        })
        .collect();
    win.set_memory_rows(str_model(&lines));
    win.set_memory_partitions(str_model(partitions));
    win.set_memory_selected(selected as i32);
    win.set_memory_contra(SharedString::from(contra));
}

/// Réglages → SettingsScreen. `states[i]` (1/0) est parallèle à `rows`
/// (voir mapping en commentaire : local/1er/fr/simulation → 1).
#[cfg(feature = "render")]
pub fn apply_settings(win: &AnnoraWindow, s: &SettingsView) {
    let mut rows: Vec<String> = Vec::new();
    let mut states: Vec<i32> = Vec::new();
    for p in &s.providers {
        rows.push(format!("Fournisseur : {} [{}]", p.name, p.data_label));
        states.push(i32::from(p.is_local));
    }
    for (i, v) in s.voices.iter().enumerate() {
        rows.push(format!("Voix : {v}"));
        states.push(i32::from(i == 0));
    }
    for c in &s.channels {
        rows.push(format!("Canal : {c}"));
        states.push(1);
    }
    for l in &s.languages {
        rows.push(format!("Langue : {l}"));
        states.push(i32::from(l == "fr"));
    }
    rows.push(format!("Accès : {}", s.accessibility_summary));
    states.push(1);
    rows.push(format!("Sauvegarde : {}", s.backup_label));
    states.push(1);
    for (i, h) in s.household_profiles.iter().enumerate() {
        rows.push(format!("Foyer : {h}"));
        states.push(i32::from(i == 0));
    }
    rows.push(format!(
        "Simulation : {}",
        if s.simulation_mode {
            "activée (prévisualisation, rien ne s'exécute)"
        } else {
            "désactivée"
        }
    ));
    states.push(i32::from(s.simulation_mode));
    win.set_settings_rows(str_model(&rows));
    win.set_settings_states(ModelRc::new(VecModel::from_slice(&states)));
}

/// Onboarding → OnboardingScreen. L'overlay se ferme quand `ready`.
#[cfg(feature = "render")]
pub fn apply_onboarding(win: &AnnoraWindow, v: &OnboardingView) {
    let limits: Vec<String> = v
        .limits
        .iter()
        .map(|(title, desc, non_neg)| {
            format!(
                "{title} — {desc}{}",
                if *non_neg { " (non négociable)" } else { "" }
            )
        })
        .collect();
    win.set_onboarding_limits(str_model(&limits));
    win.set_onboarding_caps(str_model(&v.capabilities));
    win.set_onboarding_ready(v.ready);
}

/// Outbox → OutboxScreen (lignes déjà mises en forme par l'appelant).
#[cfg(feature = "render")]
pub fn apply_outbox(win: &AnnoraWindow, rows: &[String]) {
    win.set_outbox_rows(str_model(rows));
}

/// Remplit les panneaux Journal + Pouvoirs (frise 20 dernières, jauges).
#[cfg(feature = "render")]
pub fn apply_lists(win: &AnnoraWindow, journal: &[JournalRow], powers: &PowersView) {
    let jlines: Vec<SharedString> = journal
        .iter()
        .take(20)
        .map(|r| {
            format!(
                "[{}] {} — conf {}% {}{}",
                r.time,
                r.action,
                r.confidence_pct,
                if r.verified {
                    "[signé]"
                } else {
                    "[signature douteuse]"
                },
                if r.can_revert {
                    " — Annuler: Entrée"
                } else {
                    ""
                },
            )
            .into()
        })
        .collect();
    win.set_journal_lines(ModelRc::new(VecModel::from_slice(&jlines)));
    let plines: Vec<SharedString> = powers
        .rows
        .iter()
        .map(|p| {
            format!(
                "{} ({}) : {} — jauge {} [{}]{}",
                p.group,
                p.scope,
                if p.enabled { "activé" } else { "coupé" },
                p.gauge_state(),
                p.quota_label,
                if p.policy_locked {
                    " — verrouillé par politique"
                } else {
                    ""
                },
            )
            .into()
        })
        .collect();
    win.set_powers_lines(ModelRc::new(VecModel::from_slice(&plines)));
}
