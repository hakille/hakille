//! Machine à états HUD — pure, testable headless, sans winit.
//!
//! Entrées [`HudInput`] → transitions → [`HudAction`] exécutées par l'hôte
//! winit (fenêtre, redraw, minuterie). Invariant S1/S5 : fenêtre cachée
//! ⇒ jamais de `Redraw` ni de `RequestFrameIn` (0 frame en veille).
//!
//! La soumission passe par `KernelBridge<RealDaemonBridge>` : en mode
//! `Detached`, le refus est affiché tel quel — jamais de succès feint.

use crate::daemon_bridge::RealDaemonBridge;
use crate::kernel_bridge::KernelBridge;
use crate::renderer::hud_scene;
use crate::runtime::FrameBudget;
use crate::scene::{Color, DrawCmd, Scene};
use hakille_ui::{CostCounter, EphemeralHud, HudState, PresenceIndicator, SilenceBudgetTracker};
use hakille_ui_model::snapshot::EVAPORATE_MS;
use hakille_ui_model::{KernelEvent, StableSnapshot, UiEvent};
use hakille_ui_protocol::{JournalEntryDto, KernelToUi, MemoryRowDto, StatusDto};
use std::time::{Duration, Instant};

/// Longueur maximale de la requête HUD (octets UTF-8).
pub const MAX_QUERY_BYTES: usize = 512;

/// Entrées de la machine (clavier, IME, hotkey, horloge, AccessKit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HudInput {
    /// Appui sur le hotkey global : bascule la visibilité.
    HotkeyToggle,
    /// Caractère saisi (hors caractères de contrôle).
    Char(char),
    /// IME : texte commité d'un coup.
    ImeCommit(String),
    Backspace,
    Enter,
    Escape,
    /// Frame due (minuterie d'animation — voir `FrameBudget`).
    Tick,
    /// Demande de focus sur le champ requête (AccessKit focus/click).
    FocusQuery,
    Close,
}

/// Effets demandés à l'hôte winit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HudAction {
    /// Rend la fenêtre visible et lui donne le focus clavier.
    ShowWindow,
    /// Masque la fenêtre → retour veille (0 frame).
    HideWindow,
    /// Un frame doit être dessiné (`request_redraw`).
    Redraw,
    /// Prochaine frame d'animation au plus tôt dans cette durée.
    RequestFrameIn(Duration),
    /// Soumission de la requête au pont noyau (l'hôte appelle `submit_query`).
    Submit { query: String },
    /// Quitte la boucle d'événements.
    Quit,
}

/// Ligne auxiliaire sous la requête : réponse du noyau ou refus honnête.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuxNotice {
    pub text: String,
    /// `true` ssi refus/erreur (rendu en rouge, jamais en braise).
    pub error: bool,
}

/// Machine à états du shell HUD desktop.
pub struct HudApp {
    pub visible: bool,
    pub hud: EphemeralHud,
    pub presence: PresenceIndicator,
    pub cost: CostCounter,
    pub silence: SilenceBudgetTracker,
    pub pending_actions: usize,
    pub bridge: KernelBridge<RealDaemonBridge>,
    /// Refus honnête à afficher (plus prioritaire que `hud_response`).
    pub notice: Option<String>,
    pub budget: FrameBudget,
    pub reduced_motion: bool,
    evaporate_remaining_ms: u32,
    started: Instant,
    /// Requête réellement envoyée au démon, réponse pas encore reçue (IPC).
    pending_query: Option<String>,
    /// Réponse/notice arrivée fenêtre masquée : à afficher à la prochaine
    /// ouverture, jamais de frame en veille entre-temps.
    unseen_reply: bool,
    /// Dernières entrées de journal reçues (consultation additive v1).
    journal_entries: Vec<JournalEntryDto>,
    /// Dernières lignes mémoire reçues (vue ou rappel).
    memory_rows: Vec<MemoryRowDto>,
    /// Dernier statut reçu du démon.
    last_status: Option<StatusDto>,
    /// Dernier identifiant de fait mémorisé (accusé `MemoryRemember`).
    last_remember_id: Option<String>,
    /// Confirmation de consultation (non-erreur) affichée sous la requête.
    data_notice: Option<String>,
}

impl HudApp {
    /// `authorized` : pont `LocalAuthorized` simulé ; sinon `Detached`
    /// (refus fail-closed). `visible` : `--show` ou repli sans hotkey.
    pub fn new(authorized: bool, visible: bool, budget: FrameBudget, reduced_motion: bool) -> Self {
        let bridge = if authorized {
            RealDaemonBridge::local_authorized()
        } else {
            RealDaemonBridge::detached()
        };
        let mut hud = EphemeralHud::new();
        if visible {
            hud.open_prompt();
        }
        Self {
            visible,
            hud,
            presence: PresenceIndicator::new(),
            cost: CostCounter::new(500, "EUR"),
            silence: SilenceBudgetTracker::new(3),
            pending_actions: 0,
            bridge: KernelBridge::new(bridge),
            notice: None,
            budget,
            reduced_motion,
            evaporate_remaining_ms: 0,
            started: Instant::now(),
            pending_query: None,
            unseen_reply: false,
            journal_entries: Vec::new(),
            memory_rows: Vec::new(),
            last_status: None,
            last_remember_id: None,
            data_notice: None,
        }
    }

    /// Snapshot immuable consommé par le renderer et l'arbre AccessKit.
    pub fn snapshot(&self) -> StableSnapshot {
        StableSnapshot::from_states(
            &self.presence,
            &self.hud,
            &self.cost,
            &self.silence,
            self.started.elapsed().as_millis() as u64,
            self.pending_actions,
        )
    }

    /// Réponse ou refus à afficher sous la requête.
    pub fn aux_notice(&self) -> Option<AuxNotice> {
        if let Some(n) = &self.notice {
            return Some(AuxNotice {
                text: n.clone(),
                error: true,
            });
        }
        if let Some(n) = &self.data_notice {
            return Some(AuxNotice {
                text: n.clone(),
                error: false,
            });
        }
        let snap = self.snapshot();
        if !snap.hud_response.is_empty() {
            return Some(AuxNotice {
                text: snap.hud_response,
                error: false,
            });
        }
        None
    }

    /// Scène HUD : `hud_scene` + ligne auxiliaire (réponse/refus) si présente.
    pub fn scene(&self) -> Scene {
        let snap = self.snapshot();
        let dark = !snap.light_mode;
        let mut scene = hud_scene(&snap, dark);
        if let Some(aux) = self.aux_notice() {
            // La requête remonte d'une ligne pour laisser la place à la réponse.
            for cmd in scene.cmds.iter_mut() {
                if let DrawCmd::GlyphRun { y, size, .. } = cmd
                    && (*y - 40.0).abs() < 0.01
                {
                    *y = 26.0;
                    *size = 14.0;
                }
            }
            let color = if aux.error {
                Color::from_hex(0xFF5A5A)
            } else {
                Color::from_hex(if dark { 0xA3A9B6 } else { 0x4A5060 })
            };
            scene.text(56.0, 56.0, 12.0, color, aux.text);
        }
        scene
    }

    /// Dernières entrées de journal reçues (`JournalRequest`).
    pub fn journal_entries(&self) -> &[JournalEntryDto] {
        &self.journal_entries
    }

    /// Dernières lignes mémoire reçues (vue ou rappel).
    pub fn memory_rows(&self) -> &[MemoryRowDto] {
        &self.memory_rows
    }

    /// Dernier statut reçu du démon (`StatusRequest`).
    pub fn last_status(&self) -> Option<&StatusDto> {
        self.last_status.as_ref()
    }

    /// Dernier identifiant de fait mémorisé (accusé `MemoryRemember`).
    pub fn last_remember_id(&self) -> Option<&str> {
        self.last_remember_id.as_deref()
    }

    /// Intervalle minimal entre frames animées (FrameBudget existant).
    pub fn frame_interval(&self) -> Duration {
        self.budget.min_frame_interval
    }

    /// `true` ssi un frame animé est encore nécessaire (visible uniquement).
    pub fn needs_animation(&self) -> bool {
        if !self.visible {
            return false;
        }
        if self.evaporate_remaining_ms > 0 {
            return true;
        }
        let snap = self.snapshot();
        if snap.hud_streaming || snap.hud_validation {
            return true;
        }
        !self.reduced_motion && snap.presence_pulse > 0.01 && snap.presence_state != 0
    }

    /// Traite une entrée et renvoie les effets pour l'hôte.
    pub fn handle(&mut self, input: HudInput) -> Vec<HudAction> {
        match input {
            HudInput::HotkeyToggle => self.toggle_visibility(),
            HudInput::Char(c) => {
                if !self.visible || c.is_control() {
                    return Vec::new();
                }
                self.ensure_prompt();
                if self.query_len() + c.len_utf8() <= MAX_QUERY_BYTES {
                    self.hud.insert_char(c);
                    self.notice = None;
                    self.data_notice = None;
                }
                vec![HudAction::Redraw]
            }
            HudInput::ImeCommit(text) => {
                if !self.visible {
                    return Vec::new();
                }
                self.ensure_prompt();
                for c in text.chars() {
                    if self.query_len() + c.len_utf8() > MAX_QUERY_BYTES {
                        break;
                    }
                    if !c.is_control() {
                        self.hud.insert_char(c);
                    }
                }
                self.notice = None;
                self.data_notice = None;
                vec![HudAction::Redraw]
            }
            HudInput::Backspace => {
                if !self.visible {
                    return Vec::new();
                }
                self.ensure_prompt();
                self.hud.backspace();
                self.notice = None;
                self.data_notice = None;
                vec![HudAction::Redraw]
            }
            HudInput::Enter => {
                if !self.visible {
                    return Vec::new();
                }
                match self.hud.state() {
                    HudState::Prompting { query, .. } => {
                        let q = query.trim().to_string();
                        if q.is_empty() {
                            Vec::new()
                        } else {
                            vec![HudAction::Submit { query: q }]
                        }
                    }
                    _ => Vec::new(),
                }
            }
            HudInput::Escape => self.escape(),
            HudInput::Tick => self.tick(),
            HudInput::FocusQuery => {
                if self.visible {
                    vec![HudAction::Redraw]
                } else {
                    self.show();
                    vec![HudAction::ShowWindow, HudAction::Redraw]
                }
            }
            HudInput::Close => vec![HudAction::Quit],
        }
    }

    /// Exécute la soumission (Enter) : pont → réponse ou refus affiché.
    pub fn submit_query(&mut self, query: String) -> Vec<HudAction> {
        let result = self.bridge.dispatch(UiEvent::HudSubmitted {
            query: query.clone(),
        });
        match result {
            Ok(Some(KernelEvent::HudReply {
                response,
                validation,
            })) => {
                self.notice = None;
                match validation {
                    Some((description, cost_cents)) => {
                        // La réponse ne doit jamais être perdue : coût en
                        // suffixe honnête (aucune action à approuver pour un
                        // tour déjà exécuté — pas de carte trompeuse).
                        self.hud.set_streaming(
                            query,
                            format!("{response}  —  {description}: {cost_cents}¢"),
                            true,
                        );
                    }
                    None => self.hud.set_streaming(query, response, true),
                }
                vec![HudAction::Redraw]
            }
            Ok(Some(KernelEvent::Toast { text })) => {
                self.notice = None;
                self.hud.set_streaming(query, text, true);
                vec![HudAction::Redraw]
            }
            Ok(Some(other)) => {
                let msg = format!("réponse noyau inattendue: {other:?}");
                self.notice = Some(msg.clone());
                self.hud.set_streaming(query, msg, true);
                vec![HudAction::Redraw]
            }
            Ok(None) => Vec::new(),
            Err(e) => {
                let msg = e.to_string();
                self.notice = Some(msg.clone());
                self.hud.set_streaming(query, msg, true);
                vec![HudAction::Redraw]
            }
        }
    }

    /// Requête réellement envoyée au démon (IPC) : attente affichée, réponse
    /// à venir par `on_kernel_reply` — la machine reste pure.
    pub fn on_submit_sent(&mut self, query: String) -> Vec<HudAction> {
        self.pending_query = Some(query.clone());
        self.notice = None;
        self.data_notice = None;
        self.hud
            .set_streaming(query, "En attente de réponse du démon…", false);
        vec![HudAction::Redraw]
    }

    /// Réponse asynchrone du démon — même logique que les branches `Ok` de
    /// `submit_query`. Fenêtre masquée : mémorisée (`unseen_reply`), zéro
    /// frame (`Presence` reste un no-op pur).
    pub fn on_kernel_reply(&mut self, msg: KernelToUi) -> Vec<HudAction> {
        if matches!(msg, KernelToUi::Presence(_)) {
            return Vec::new();
        }
        // Consultation additive v1 : stockée telle quelle, sans toucher à la
        // requête HUD en vol (`pending_query`).
        match msg {
            KernelToUi::JournalReply { entries } => {
                let count = entries.len();
                self.journal_entries = entries;
                self.data_reply_actions(format!("journal : {count} entrée(s)"))
            }
            KernelToUi::MemoryReply { rows } => {
                let count = rows.len();
                self.memory_rows = rows;
                self.data_reply_actions(format!("mémoire : {count} ligne(s)"))
            }
            KernelToUi::MemoryRememberAck { id } => {
                self.last_remember_id = Some(id.clone());
                self.data_reply_actions(format!("mémoire enregistrée [{id}]"))
            }
            KernelToUi::StatusReply { status } => {
                let label = format!("statut : {} — {}", status.model, status.cost);
                self.last_status = Some(status);
                self.data_reply_actions(label)
            }
            msg => self.on_turn_reply(msg),
        }
    }

    /// Réponse d'un tour HUD (HudReply/Toast) — chemin historique inchangé.
    fn on_turn_reply(&mut self, msg: KernelToUi) -> Vec<HudAction> {
        let query = self.pending_query.take().unwrap_or_default();
        match msg {
            KernelToUi::HudReply {
                response,
                validation,
            } => {
                self.notice = None;
                match validation {
                    Some((description, cost_cents)) => {
                        // La réponse ne doit jamais être perdue : coût en
                        // suffixe honnête (aucune action à approuver pour un
                        // tour déjà exécuté — pas de carte trompeuse).
                        self.hud.set_streaming(
                            query,
                            format!("{response}  —  {description}: {cost_cents}¢"),
                            true,
                        );
                    }
                    None => self.hud.set_streaming(query, response, true),
                }
            }
            KernelToUi::Toast { text } => {
                self.notice = None;
                self.hud.set_streaming(query, text, true);
            }
            other => {
                let msg = format!("réponse noyau inattendue: {other:?}");
                self.notice = Some(msg.clone());
                self.hud.set_streaming(query, msg, true);
            }
        }
        if self.visible {
            vec![HudAction::Redraw]
        } else {
            self.unseen_reply = true;
            Vec::new()
        }
    }

    /// Consultation : confirmation affichable (non-erreur), redraw si visible.
    fn data_reply_actions(&mut self, notice: String) -> Vec<HudAction> {
        self.data_notice = Some(notice);
        if self.visible {
            vec![HudAction::Redraw]
        } else {
            Vec::new()
        }
    }

    /// Déconnexion IPC : la requête en vol est annulée (le démon n'a pas
    /// d'annulation, aucune réponse ne viendra plus).
    pub fn on_kernel_disconnected(&mut self) -> Vec<HudAction> {
        let Some(query) = self.pending_query.take() else {
            return Vec::new();
        };
        let cancel = "démon déconnecté — requête annulée";
        self.notice = Some(cancel.to_string());
        // État final : sinon l'animation d'attente tournerait sans fin alors
        // qu'aucune réponse ne viendra (aucune frame fantôme).
        self.hud.set_streaming(query, cancel.to_string(), true);
        if self.visible {
            vec![HudAction::Redraw]
        } else {
            self.unseen_reply = true;
            Vec::new()
        }
    }

    fn toggle_visibility(&mut self) -> Vec<HudAction> {
        if self.visible {
            self.hide_now();
            vec![HudAction::HideWindow]
        } else {
            self.show();
            let mut actions = vec![HudAction::ShowWindow, HudAction::Redraw];
            if self.needs_animation() {
                actions.push(HudAction::RequestFrameIn(self.frame_interval()));
            }
            actions
        }
    }

    fn show(&mut self) {
        self.visible = true;
        self.evaporate_remaining_ms = 0;
        if self.unseen_reply {
            // Réponse/notice arrivée masqué : `open_prompt` effacerait le
            // streaming mémorisé → on réaffiche tel quel ; l'utilisateur
            // reprend la main au premier caractère (`ensure_prompt`).
            self.unseen_reply = false;
            return;
        }
        self.notice = None;
        self.hud.open_prompt();
    }

    fn hide_now(&mut self) {
        self.visible = false;
        self.notice = None;
        self.data_notice = None;
        self.evaporate_remaining_ms = 0;
        self.hud.dismiss();
    }

    fn escape(&mut self) -> Vec<HudAction> {
        if !self.visible {
            return Vec::new();
        }
        let has_text = match self.hud.state() {
            HudState::Prompting { query, .. } => !query.is_empty(),
            HudState::StreamingResponse { .. }
            | HudState::ValidationRequired { .. }
            | HudState::Evaporating { .. } => true,
            HudState::Hidden => false,
        };
        if !has_text {
            self.hide_now();
            return vec![HudAction::HideWindow];
        }
        self.evaporate_remaining_ms = EVAPORATE_MS;
        vec![
            HudAction::Redraw,
            HudAction::RequestFrameIn(self.frame_interval()),
        ]
    }

    fn tick(&mut self) -> Vec<HudAction> {
        if !self.visible {
            return Vec::new();
        }
        if self.evaporate_remaining_ms > 0 {
            let step = self.frame_interval().as_millis().max(1) as u32;
            self.evaporate_remaining_ms = self.evaporate_remaining_ms.saturating_sub(step);
            if self.evaporate_remaining_ms == 0 {
                self.hide_now();
                return vec![HudAction::HideWindow];
            }
            return vec![
                HudAction::Redraw,
                HudAction::RequestFrameIn(self.frame_interval()),
            ];
        }
        if self.budget.poll(Instant::now(), self.needs_animation()) {
            vec![
                HudAction::Redraw,
                HudAction::RequestFrameIn(self.frame_interval()),
            ]
        } else {
            Vec::new()
        }
    }

    fn ensure_prompt(&mut self) {
        if !matches!(self.hud.state(), HudState::Prompting { .. }) {
            self.hud.open_prompt();
            self.evaporate_remaining_ms = 0;
        }
    }

    fn query_len(&self) -> usize {
        match self.hud.state() {
            HudState::Prompting { query, .. } => query.len(),
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_bridge::DaemonKernel;
    use hakille_ui_protocol::PresenceState;

    fn app(authorized: bool, visible: bool) -> HudApp {
        HudApp::new(authorized, visible, FrameBudget::battery_saver(), true)
    }

    fn shown(authorized: bool) -> HudApp {
        let mut app = app(authorized, false);
        let actions = app.handle(HudInput::HotkeyToggle);
        assert!(actions.contains(&HudAction::ShowWindow));
        assert!(app.visible);
        app
    }

    fn type_text(app: &mut HudApp, text: &str) {
        for c in text.chars() {
            app.handle(HudInput::Char(c));
        }
    }

    fn submit(app: &mut HudApp) -> Vec<HudAction> {
        let actions = app.handle(HudInput::Enter);
        let query = actions
            .iter()
            .find_map(|a| match a {
                HudAction::Submit { query } => Some(query.clone()),
                _ => None,
            })
            .expect("Enter doit produire HudAction::Submit");
        app.submit_query(query)
    }

    #[test]
    fn veille_zero_redraw_hidden() {
        let mut app = app(false, false);
        assert!(!app.needs_animation());
        for _ in 0..8 {
            assert!(app.handle(HudInput::Tick).is_empty());
        }
        assert!(app.handle(HudInput::Char('a')).is_empty());
        assert!(app.handle(HudInput::Backspace).is_empty());
        assert!(app.handle(HudInput::Enter).is_empty());
        assert!(app.handle(HudInput::Escape).is_empty());
        assert_eq!(app.snapshot().hud_query, "");
    }

    #[test]
    fn hotkey_toggle_shows_then_hides() {
        let mut app = app(false, false);
        let actions = app.handle(HudInput::HotkeyToggle);
        assert!(app.visible);
        assert!(actions.contains(&HudAction::ShowWindow));
        assert!(actions.contains(&HudAction::Redraw));
        let actions = app.handle(HudInput::HotkeyToggle);
        assert!(!app.visible);
        assert_eq!(actions, vec![HudAction::HideWindow]);
        assert!(app.handle(HudInput::Tick).is_empty());
    }

    #[test]
    fn typing_and_backspace_update_query() {
        let mut app = shown(false);
        type_text(&mut app, "héll");
        app.handle(HudInput::ImeCommit("世界".into()));
        assert_eq!(app.snapshot().hud_query, "héll世界");
        assert!(app.handle(HudInput::Backspace).contains(&HudAction::Redraw));
        assert_eq!(app.snapshot().hud_query, "héll世");
        for _ in 0..16 {
            app.handle(HudInput::Backspace);
        }
        assert_eq!(app.snapshot().hud_query, "");
        assert_eq!(app.handle(HudInput::Backspace).len(), 1);
    }

    #[test]
    fn query_length_capped() {
        let mut app = shown(false);
        for _ in 0..MAX_QUERY_BYTES + 32 {
            app.handle(HudInput::Char('x'));
        }
        assert_eq!(app.snapshot().hud_query.len(), MAX_QUERY_BYTES);
        app.handle(HudInput::ImeCommit("y".repeat(64)));
        assert_eq!(app.snapshot().hud_query.len(), MAX_QUERY_BYTES);
        // Multioctet au bord : jamais de dépassement (octets, pas caractères).
        app.handle(HudInput::Backspace);
        app.handle(HudInput::Char('é'));
        assert_eq!(app.snapshot().hud_query.len(), MAX_QUERY_BYTES - 1);
        app.handle(HudInput::Char('x'));
        app.handle(HudInput::ImeCommit("é".repeat(8)));
        assert!(app.snapshot().hud_query.len() <= MAX_QUERY_BYTES);
    }

    #[test]
    fn escape_empty_hides_immediately() {
        let mut app = shown(false);
        let actions = app.handle(HudInput::Escape);
        assert_eq!(actions, vec![HudAction::HideWindow]);
        assert!(!app.visible);
        assert!(app.handle(HudInput::Tick).is_empty());
    }

    #[test]
    fn escape_with_query_evaporates_then_hides() {
        let mut app = shown(false);
        type_text(&mut app, "brouillon");
        let actions = app.handle(HudInput::Escape);
        assert!(app.visible && app.needs_animation());
        assert!(actions.contains(&HudAction::Redraw));
        assert!(matches!(actions.last(), Some(HudAction::RequestFrameIn(_))));
        let mut hidden = false;
        for _ in 0..1_000 {
            let actions = app.handle(HudInput::Tick);
            if actions.contains(&HudAction::HideWindow) {
                hidden = true;
                break;
            }
            assert!(
                actions.contains(&HudAction::Redraw),
                "chaque tick d'évaporation doit redessiner"
            );
        }
        assert!(hidden && !app.visible);
        assert!(app.handle(HudInput::Tick).is_empty());
    }

    #[test]
    fn enter_detached_refusal_is_displayed() {
        let mut app = shown(false);
        type_text(&mut app, "résume ma journée");
        let actions = submit(&mut app);
        assert!(actions.contains(&HudAction::Redraw));
        let snap = app.snapshot();
        assert_eq!(snap.hud_query, "résume ma journée");
        assert!(
            snap.hud_response.contains("daemon non branché"),
            "refus honnête attendu, obtenu: {}",
            snap.hud_response
        );
        let aux = app.aux_notice().expect("ligne de refus affichée");
        assert!(aux.error);
        assert!(aux.text.contains("daemon non branché"));
        assert!(app.bridge.kernel.traces().is_empty());
        let scene = app.scene();
        assert!(scene.cmds.iter().any(|c| matches!(
            c,
            DrawCmd::GlyphRun { text, .. } if text.contains("daemon non branché")
        )));
    }

    #[test]
    fn enter_authorized_reply_is_displayed_and_traced() {
        let mut app = shown(true);
        type_text(&mut app, "résume");
        let actions = submit(&mut app);
        assert!(actions.contains(&HudAction::Redraw));
        let snap = app.snapshot();
        assert!(snap.hud_response.contains("Reçu"), "{}", snap.hud_response);
        let aux = app.aux_notice().expect("réponse affichée");
        assert!(!aux.error);
        assert_eq!(app.bridge.kernel.traces().len(), 1);
    }

    #[test]
    fn show_flag_starts_visible() {
        let app = app(false, true);
        assert!(app.visible);
        assert!(matches!(app.hud.state(), HudState::Prompting { .. }));
    }

    #[test]
    fn focus_query_shows_window() {
        let mut app = app(false, false);
        let actions = app.handle(HudInput::FocusQuery);
        assert!(app.visible);
        assert!(actions.contains(&HudAction::ShowWindow));
        assert!(actions.contains(&HudAction::Redraw));
    }

    #[test]
    fn close_quits() {
        let mut app = app(false, false);
        assert_eq!(app.handle(HudInput::Close), vec![HudAction::Quit]);
    }

    #[test]
    fn static_visible_state_is_quiescent() {
        let mut app = shown(false);
        type_text(&mut app, "x");
        submit(&mut app);
        // Réponse finale statique : aucun tick ne doit produire de frame.
        assert!(!app.needs_animation());
        for _ in 0..20 {
            assert!(app.handle(HudInput::Tick).is_empty());
        }
    }

    #[test]
    fn reply_after_submit_sent_shows_response_and_clears_pending() {
        let mut app = shown(false);
        let actions = app.on_submit_sent("statut".into());
        assert!(actions.contains(&HudAction::Redraw));
        let snap = app.snapshot();
        assert_eq!(snap.hud_query, "statut");
        assert!(snap.hud_response.contains("En attente"));
        assert!(snap.hud_streaming, "attente = streaming non final");
        let actions = app.on_kernel_reply(KernelToUi::HudReply {
            response: "pong".into(),
            validation: None,
        });
        assert_eq!(actions, vec![HudAction::Redraw]);
        let snap = app.snapshot();
        assert_eq!(snap.hud_query, "statut");
        assert_eq!(snap.hud_response, "pong");
        assert!(!snap.hud_streaming, "réponse finale : plus d'animation");
        assert!(!app.needs_animation());
        let aux = app.aux_notice().expect("réponse affichée");
        assert!(!aux.error);
        assert_eq!(aux.text, "pong");
        assert!(app.on_kernel_disconnected().is_empty(), "plus rien en vol");
    }

    #[test]
    fn validation_reply_keeps_response_and_cost_visible() {
        let mut app = shown(false);
        let _ = app.on_submit_sent("combien".into());
        let actions = app.on_kernel_reply(KernelToUi::HudReply {
            response: "42".into(),
            validation: Some(("coût tour".into(), 12)),
        });
        assert_eq!(actions, vec![HudAction::Redraw]);
        let snap = app.snapshot();
        assert!(
            snap.hud_response.contains("42"),
            "la réponse réelle ne doit jamais être perdue (got: {})",
            snap.hud_response
        );
        assert!(
            snap.hud_response.contains("12"),
            "le coût est affiché honnêtement (got: {})",
            snap.hud_response
        );
        assert!(!app.needs_animation(), "état final : plus d'animation");
    }

    #[test]
    fn reply_while_hidden_is_shown_on_next_open() {
        let mut app = shown(false);
        app.on_submit_sent("statut".into());
        // Masquer n'annule pas la requête en vol (le démon n'a pas d'annulation).
        let actions = app.handle(HudInput::HotkeyToggle);
        assert_eq!(actions, vec![HudAction::HideWindow]);
        let actions = app.on_kernel_reply(KernelToUi::HudReply {
            response: "pong".into(),
            validation: None,
        });
        assert!(actions.is_empty(), "réponse reçue masqué : 0 frame");
        assert!(!app.needs_animation());
        for _ in 0..8 {
            assert!(app.handle(HudInput::Tick).is_empty());
        }
        // Réouverture : la réponse arrivée masqué n'est pas perdue.
        let actions = app.handle(HudInput::HotkeyToggle);
        assert!(actions.contains(&HudAction::ShowWindow));
        assert!(actions.contains(&HudAction::Redraw));
        let snap = app.snapshot();
        assert_eq!(snap.hud_query, "statut");
        assert_eq!(snap.hud_response, "pong");
        assert!(app.scene().cmds.iter().any(|c| matches!(
            c,
            DrawCmd::GlyphRun { text, .. } if text.contains("pong")
        )));
    }

    #[test]
    fn disconnect_with_pending_shows_error_notice() {
        let mut app = shown(false);
        app.on_submit_sent("statut".into());
        let actions = app.on_kernel_disconnected();
        assert_eq!(actions, vec![HudAction::Redraw]);
        let aux = app.aux_notice().expect("notice de déconnexion affichée");
        assert!(aux.error);
        assert!(aux.text.contains("démon déconnecté"));
        assert_eq!(
            app.snapshot().hud_response,
            "démon déconnecté — requête annulée"
        );
        assert!(!app.needs_animation(), "attente annulée : plus d'animation");
        assert!(
            app.on_kernel_disconnected().is_empty(),
            "sans requête : no-op"
        );
    }

    #[test]
    fn presence_reply_is_noop_no_redraw() {
        let mut app = shown(false);
        app.on_submit_sent("statut".into());
        let actions = app.on_kernel_reply(KernelToUi::Presence(PresenceState::Listening));
        assert!(actions.is_empty(), "présence : zéro frame");
        assert_eq!(
            app.snapshot().hud_response,
            "En attente de réponse du démon…",
            "la requête reste en vol après une présence"
        );
        assert!(app.needs_animation(), "attente encore animée");
        let actions = app.on_kernel_disconnected();
        assert_eq!(actions, vec![HudAction::Redraw]);
    }

    #[test]
    fn journal_reply_is_stored_and_noticed() {
        let mut app = shown(false);
        let actions = app.on_kernel_reply(KernelToUi::JournalReply {
            entries: vec![JournalEntryDto {
                id: "e-1".into(),
                timestamp: "2026-10-19T09:00:00+00:00".into(),
                action: "CognitiveTurn".into(),
                summary: "tour servi".into(),
            }],
        });
        assert_eq!(actions, vec![HudAction::Redraw]);
        assert_eq!(app.journal_entries().len(), 1);
        assert_eq!(app.journal_entries()[0].id, "e-1");
        assert_eq!(app.journal_entries()[0].action, "CognitiveTurn");
        let aux = app.aux_notice().expect("confirmation journal affichée");
        assert!(!aux.error, "consultation : notice non-erreur");
        assert!(aux.text.contains("journal"));
    }

    #[test]
    fn memory_reply_is_stored_even_while_hidden() {
        let mut app = app(false, false);
        let actions = app.on_kernel_reply(KernelToUi::MemoryReply {
            rows: vec![MemoryRowDto {
                id: "018f0000-0000-7000-8000-000000000002".into(),
                text: "Marie aime le jazz".into(),
                tags: vec!["Personal".into(), "User".into()],
                source: "User".into(),
            }],
        });
        assert!(actions.is_empty(), "fenêtre masquée : 0 frame");
        assert_eq!(app.memory_rows().len(), 1);
        assert_eq!(app.memory_rows()[0].text, "Marie aime le jazz");
        let aux = app.aux_notice().expect("lignes mémoire accessibles");
        assert!(!aux.error);
        assert!(aux.text.contains("mémoire"));
    }

    #[test]
    fn remember_ack_stores_id() {
        let mut app = shown(false);
        let actions = app.on_kernel_reply(KernelToUi::MemoryRememberAck {
            id: "018f0000-0000-7000-8000-000000000003".into(),
        });
        assert_eq!(actions, vec![HudAction::Redraw]);
        assert_eq!(
            app.last_remember_id(),
            Some("018f0000-0000-7000-8000-000000000003")
        );
        assert!(app.aux_notice().is_some_and(|n| !n.error));
    }

    #[test]
    fn status_reply_stores_status() {
        let mut app = shown(false);
        let actions = app.on_kernel_reply(KernelToUi::StatusReply {
            status: StatusDto {
                model: "Qwen 0.5B Q4_K_M (timeout 30000 ms)".into(),
                cost: "0.00 EUR".into(),
                state: "Idle".into(),
                memory_count: 3,
            },
        });
        assert_eq!(actions, vec![HudAction::Redraw]);
        let status = app.last_status().expect("statut stocké");
        assert_eq!(status.state, "Idle");
        assert_eq!(status.memory_count, 3);
        assert!(app.aux_notice().is_some_and(|n| !n.error));
    }

    #[test]
    fn data_reply_does_not_resolve_pending_query() {
        let mut app = shown(false);
        app.on_submit_sent("statut".into());
        let actions = app.on_kernel_reply(KernelToUi::JournalReply { entries: vec![] });
        assert_eq!(actions, vec![HudAction::Redraw]);
        assert_eq!(
            app.snapshot().hud_response,
            "En attente de réponse du démon…",
            "la consultation ne doit pas clore la requête en vol"
        );
        let actions = app.on_kernel_disconnected();
        assert_eq!(actions, vec![HudAction::Redraw]);
    }
}
