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
use crate::renderer::{
    CONSULT_HEIGHT, ConsultLine, ConsultLineKind, ConsultView, card_scene, consult_scene, hud_scene,
};
use crate::runtime::FrameBudget;
use crate::scene::{Color, DrawCmd, Scene};
use hakille_ui::{CostCounter, EphemeralHud, HudState, PresenceIndicator, SilenceBudgetTracker};
use hakille_ui_model::snapshot::{EVAPORATE_MS, HUD_HEIGHT, HUD_WIDTH};
use hakille_ui_model::{KernelEvent, StableSnapshot, UiEvent};
use hakille_ui_protocol::{
    ActionCard, AgentDto, JournalEntryDto, KernelToUi, MemoryRowDto, StatusDto,
};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Longueur maximale de la requête HUD (octets UTF-8).
pub const MAX_QUERY_BYTES: usize = 512;

/// Seuil au-delà duquel une réponse est rendue dans le panneau étendu :
/// une seule ligne du HUD (≈ 100 caractères) ne suffit plus.
pub const EXPAND_RESPONSE_CHARS: usize = 120;

/// Nombre de lignes demandées lors d'une consultation (le serveur plafonne).
pub const CONSULT_LIMIT: usize = 50;

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
    /// Ouvre le panneau de consultation du journal (F1).
    ShowJournal,
    /// Ouvre le panneau mémoire (F2).
    ShowMemory,
    /// Ouvre le panneau statut (F3).
    ShowStatus,
    /// Ouvre le panneau des agents (F4) : liste des agents réellement
    /// exécutables vs manifests déclaratifs.
    ShowAgents,
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
    Submit {
        query: String,
    },
    /// Redimensionne la fenêtre logique (HUD 64 px ou panneau 360 px).
    ResizeWindow {
        width: u32,
        height: u32,
    },
    /// Consultation additive v1 : dernières entrées du journal.
    RequestJournal {
        limit: usize,
    },
    /// Consultation additive v1 : vue mémoire ou rappel.
    RequestMemory {
        query: Option<String>,
        limit: usize,
    },
    /// Statut minimal du démon.
    RequestStatus,
    /// Liste des agents exposés par le démon (additif v1).
    RequestAgents,
    /// Exécution d'un agent WASM réel (additif v1) : entrée JSON → sortie
    /// JSON. Le démon refuse tout agent sans binaire vérifié.
    RunAgent {
        name: String,
        input_json: String,
    },
    /// Décision explicite sur la carte en attente (jamais un effet local :
    /// le démon revérifie et audite avant tout accusé).
    ApproveCard {
        card_id: u64,
    },
    RejectCard {
        card_id: u64,
    },
    /// Édition demandée : le démon peut la refuser explicitement.
    EditCard {
        card_id: u64,
    },
    /// Quitte la boucle d'événements.
    Quit,
}

/// Panneau de consultation affiché sous le HUD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    None,
    Response,
    Journal,
    Memory,
    Status,
    /// Liste des agents (exécutables vs déclaratifs).
    Agents,
    Card,
}

/// Action de demande de donnée pour un panneau de consultation.
///
/// SOURCE UNIQUE : utilisée à l'ouverture du panneau ET au rejeu après
/// connexion (une demande perdue laisserait un panneau vide).
pub fn panel_request_action(kind: PanelKind) -> HudAction {
    match kind {
        PanelKind::Journal => HudAction::RequestJournal {
            limit: CONSULT_LIMIT,
        },
        PanelKind::Memory => HudAction::RequestMemory {
            query: None,
            limit: CONSULT_LIMIT,
        },
        PanelKind::Status => HudAction::RequestStatus,
        PanelKind::Agents => HudAction::RequestAgents,
        PanelKind::Card | PanelKind::Response | PanelKind::None => HudAction::Redraw,
    }
}

fn map_presence(state: hakille_ui_protocol::PresenceState) -> hakille_ui::PresenceState {
    match state {
        hakille_ui_protocol::PresenceState::Idle => hakille_ui::PresenceState::Idle,
        hakille_ui_protocol::PresenceState::Listening => hakille_ui::PresenceState::Listening,
        hakille_ui_protocol::PresenceState::Acting => hakille_ui::PresenceState::Acting,
        hakille_ui_protocol::PresenceState::Speaking => hakille_ui::PresenceState::Speaking,
        hakille_ui_protocol::PresenceState::DecisionRequired => {
            hakille_ui::PresenceState::DecisionRequired
        }
        hakille_ui_protocol::PresenceState::Urgent => hakille_ui::PresenceState::Urgent,
    }
}

/// Compose la réponse et son éventuel coût sans jamais afficher « 0¢ »
/// quand le coût est nul (une alerte de persistance n'est pas un coût).
fn compose_reply(response: String, validation: Option<(String, u64)>) -> String {
    match validation {
        Some((description, cost_cents)) if cost_cents > 0 => {
            format!("{response}  —  {description}: {cost_cents}¢")
        }
        Some((description, _)) => format!("{response}  —  {description}"),
        None => response,
    }
}

fn response_is_long(text: &str) -> bool {
    text.chars().count() > EXPAND_RESPONSE_CHARS || text.contains('\n')
}

/// Message d'attente affiché pendant qu'un tour est en vol.
///
/// Honnête : le temps écoulé est réel (mesuré, pas estimé) et la cause
/// habituelle de l'attente (décodage Qwen CPU-local, premier tour = chargement
/// du modèle) est explicitée plutôt que laissée dans le vide.
pub fn waiting_text(elapsed_secs: u64) -> String {
    format!(
        "En attente de réponse du démon… {elapsed_secs}s (Qwen CPU local ; 1er tour = chargement du modèle)"
    )
}

/// Ligne auxiliaire sous la requête : réponse du noyau ou refus honnête.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuxNotice {
    pub text: String,
    /// `true` ssi refus/erreur (rendu en rouge, jamais en braise).
    pub error: bool,
}

/// Cible d'un clic souris dans la fenêtre (coordonnées logiques de scène).
///
/// Machine pure : le desktop ne fait qu'exécuter la cible renvoyée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerTarget {
    /// Zone neutre : aucune action.
    None,
    /// Zone de déplacement de la fenêtre.
    Drag,
    /// Bouton « Valider » de la carte d'action.
    CardValidate,
    /// Bouton « Modifier » de la carte d'action.
    CardEdit,
    /// Bouton « Refuser » de la carte d'action.
    CardReject,
}

/// Machine à états du shell HUD desktop.
pub struct HudApp {
    pub visible: bool,
    pub hud: EphemeralHud,
    pub presence: PresenceIndicator,
    pub cost: CostCounter,
    pub silence: SilenceBudgetTracker,
    pub pending_actions: usize,
    /// Carte d'action issue du démon, en attente de décision. Une seule à la
    /// fois (spec §8) ; elle n'est jamais déduite d'un texte local.
    pending_card: Option<ActionCard>,
    pub bridge: KernelBridge<RealDaemonBridge>,
    /// Refus honnête à afficher (plus prioritaire que `hud_response`).
    pub notice: Option<String>,
    pub budget: FrameBudget,
    pub reduced_motion: bool,
    evaporate_remaining_ms: u32,
    started: Instant,
    /// Requête réellement envoyée au démon, réponse pas encore reçue (IPC).
    pending_query: Option<String>,
    /// Instant de début de l'attente d'une réponse (temps écoulé affiché).
    waiting_since: Option<Instant>,
    /// Dernier temps d'attente affiché (évite un redraw par frame).
    waiting_shown_secs: u64,
    /// Texte reçu en streaming (deltas de tokens) pendant le tour en cours.
    streamed_text: String,
    /// Réponse/notice arrivée fenêtre masquée : à afficher à la prochaine
    /// ouverture, jamais de frame en veille entre-temps.
    unseen_reply: bool,
    /// Dernières entrées de journal reçues (consultation additive v1).
    journal_entries: Vec<JournalEntryDto>,
    /// Dernières lignes mémoire reçues (vue ou rappel).
    memory_rows: Vec<MemoryRowDto>,
    /// Dernier statut reçu du démon.
    last_status: Option<StatusDto>,
    /// Panneau dont la donnée n'a pas pu être demandée (démon absent) : la
    /// demande est REJOUÉE à la connexion, sinon le panneau resterait vide
    /// pour toujours.
    pending_panel_request: Option<PanelKind>,
    /// Agents listés par le démon (additif v1) : `artifact_present` distingue
    /// un agent réellement exécutable d'un manifeste déclaratif.
    agents: Vec<AgentDto>,
    /// Dernière exécution d'agent rapportée par le démon (nom, sortie, durée).
    last_agent_run: Option<(String, String, u64)>,
    /// Dernier identifiant de fait mémorisé (accusé `MemoryRemember`).
    last_remember_id: Option<String>,
    /// Confirmation de consultation (non-erreur) affichée sous la requête.
    data_notice: Option<String>,
    /// Requêtes saisies alors que l'IPC n'était pas encore connecté : elles
    /// partent dans l'ordre à la connexion, jamais perdues silencieusement.
    queued_queries: VecDeque<String>,
    /// Panneau actif sous le HUD (`None` : HUD compact 720×64).
    panel: PanelKind,
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
            pending_card: None,
            bridge: KernelBridge::new(bridge),
            notice: None,
            budget,
            reduced_motion,
            evaporate_remaining_ms: 0,
            started: Instant::now(),
            pending_query: None,
            waiting_since: None,
            waiting_shown_secs: 0,
            streamed_text: String::new(),
            unseen_reply: false,
            journal_entries: Vec::new(),
            memory_rows: Vec::new(),
            last_status: None,
            pending_panel_request: None,
            agents: Vec::new(),
            last_agent_run: None,
            last_remember_id: None,
            data_notice: None,
            queued_queries: VecDeque::new(),
            panel: PanelKind::None,
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

    /// Panneau actif sous le HUD.
    pub fn panel(&self) -> PanelKind {
        self.panel
    }

    /// Hauteur logique de la scène courante (64 compact, 360 étendu).
    pub fn scene_height(&self) -> u32 {
        if self.panel != PanelKind::None {
            CONSULT_HEIGHT
        } else {
            HUD_HEIGHT
        }
    }

    /// Cible d'un clic (coordonnées logiques de scène, origine en haut-gauche).
    ///
    /// Règles :
    /// - une carte ouverte expose ses trois boutons (clic réel) et n'est
    ///   **jamais** une zone de déplacement (on ne bouge pas la fenêtre
    ///   pendant une décision) ;
    /// - le panneau de consultation n'est déplaçable que par son en-tête ;
    /// - le HUD compact est intégralement une poignée de déplacement.
    pub fn pointer_target(&self, x: f32, y: f32) -> PointerTarget {
        let panel_top = HUD_HEIGHT as f32 + crate::renderer::LAYOUT_GRID;
        let theme_dark = !self.snapshot().light_mode;
        if self.pending_card.is_some() {
            let buttons = crate::renderer::card_buttons(theme_dark);
            if buttons.validate.contains(x, y) {
                return PointerTarget::CardValidate;
            }
            if buttons.edit.contains(x, y) {
                return PointerTarget::CardEdit;
            }
            if buttons.reject.contains(x, y) {
                return PointerTarget::CardReject;
            }
            // En-tête au-dessus de la carte : seule zone déplaçable.
            if y >= panel_top && y < crate::renderer::card_rect().y {
                return PointerTarget::Drag;
            }
            return PointerTarget::None;
        }
        if self.panel != PanelKind::None {
            // En-tête du panneau de consultation (avant le contenu).
            if y >= panel_top && y < panel_top + 84.0 {
                return PointerTarget::Drag;
            }
            return PointerTarget::None;
        }
        if y >= 0.0 && y < HUD_HEIGHT as f32 {
            PointerTarget::Drag
        } else {
            PointerTarget::None
        }
    }

    /// Carte d'action réelle en attente, si le démon en a proposé une.
    pub fn pending_card(&self) -> Option<&ActionCard> {
        self.pending_card.as_ref()
    }

    /// Scène HUD : compacte, ou étendue quand un panneau est ouvert.
    pub fn scene(&self) -> Scene {
        let snap = self.snapshot();
        let dark = !snap.light_mode;
        if let Some(card) = &self.pending_card {
            return card_scene(&snap, card, self.data_notice.as_deref(), dark);
        }
        if self.panel != PanelKind::None {
            return consult_scene(&snap, &self.consult_view(), dark);
        }
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

    /// Contenu texte du panneau actif. Aucune donnée inventée : une liste
    /// vide dit explicitement qu'il faut interroger le démon.
    pub fn consult_view(&self) -> ConsultView {
        let footer =
            "Échap fermer  ·  F1 Journal  ·  F2 Mémoire  ·  F3 Statut  ·  F4 Agents".to_string();
        match self.panel {
            PanelKind::Journal => {
                let mut lines = Vec::new();
                if self.journal_entries.is_empty() {
                    lines.push(ConsultLine::new(
                        "Aucune entrée reçue. F1 interroge le démon ; sans connexion, rien n'est inventé.",
                        ConsultLineKind::Muted,
                    ));
                } else {
                    for entry in self.journal_entries.iter().take(12) {
                        let stamp: String = entry.timestamp.chars().take(16).collect();
                        lines.push(ConsultLine::new(
                            format!("{}  ·  {}", stamp.replace('T', " "), entry.action),
                            ConsultLineKind::Strong,
                        ));
                        lines.push(ConsultLine::new(
                            entry.summary.clone(),
                            ConsultLineKind::Normal,
                        ));
                        lines.push(ConsultLine::new(String::new(), ConsultLineKind::Muted));
                    }
                }
                ConsultView {
                    title: "Journal d'audit".into(),
                    subtitle: format!(
                        "{} entrée(s) — chaîne signée, plus récentes d'abord",
                        self.journal_entries.len()
                    ),
                    lines,
                    footer,
                }
            }
            PanelKind::Memory => {
                let mut lines = Vec::new();
                if self.memory_rows.is_empty() {
                    lines.push(ConsultLine::new(
                        "Aucune ligne reçue. F2 interroge le démon ; la mémoire n'est pas simulée ici.",
                        ConsultLineKind::Muted,
                    ));
                } else {
                    for row in self.memory_rows.iter().take(12) {
                        lines.push(ConsultLine::new(row.text.clone(), ConsultLineKind::Normal));
                        let short_id: String = row.id.chars().take(8).collect();
                        let tags = if row.tags.is_empty() {
                            "—".to_string()
                        } else {
                            row.tags.join(" · ")
                        };
                        lines.push(ConsultLine::new(
                            format!("{short_id}  ·  {tags}  ·  {}", row.source),
                            ConsultLineKind::Muted,
                        ));
                        lines.push(ConsultLine::new(String::new(), ConsultLineKind::Muted));
                    }
                }
                ConsultView {
                    title: "Mémoire souveraine".into(),
                    subtitle: format!("{} ligne(s) reçue(s)", self.memory_rows.len()),
                    lines,
                    footer,
                }
            }
            PanelKind::Status => {
                let lines = match &self.last_status {
                    Some(status) => vec![
                        ConsultLine::new(
                            format!("Modèle : {}", status.model),
                            ConsultLineKind::Strong,
                        ),
                        ConsultLine::new(
                            format!("État : {}", status.state),
                            ConsultLineKind::Normal,
                        ),
                        ConsultLine::new(
                            format!("Coût : {}", status.cost),
                            ConsultLineKind::Normal,
                        ),
                        ConsultLine::new(
                            format!("Mémoire : {} triplet(s)", status.memory_count),
                            ConsultLineKind::Normal,
                        ),
                    ],
                    None => vec![ConsultLine::new(
                        "Aucun statut reçu. F3 interroge le démon ; aucune valeur n'est inventée.",
                        ConsultLineKind::Muted,
                    )],
                };
                ConsultView {
                    title: "Statut du démon".into(),
                    subtitle: "Modèle, coût et mémoire réellement rapportés".into(),
                    lines,
                    footer,
                }
            }
            PanelKind::Card => {
                let lines = match &self.pending_card {
                    Some(card) => vec![
                        ConsultLine::new(
                            format!("{} {}", card.verb, card.object),
                            ConsultLineKind::Strong,
                        ),
                        ConsultLine::new(card.reason.clone(), ConsultLineKind::Normal),
                        ConsultLine::new(
                            format!(
                                "Coût : {} · Données : {:?} · {}",
                                card.cost.display(),
                                card.data_exit,
                                if card.reversible {
                                    "annulable"
                                } else {
                                    "définitif"
                                }
                            ),
                            ConsultLineKind::Muted,
                        ),
                    ],
                    None => vec![ConsultLine::new(
                        "Aucune carte en attente.",
                        ConsultLineKind::Muted,
                    )],
                };
                ConsultView {
                    title: "Décision requise".into(),
                    subtitle: "Carte issue du démon — aucune exécution sans décision auditée"
                        .into(),
                    lines,
                    footer,
                }
            }
            PanelKind::Agents => {
                let mut lines: Vec<ConsultLine> = Vec::new();
                for agent in &self.agents {
                    let etat = if agent.artifact_present {
                        "exécutable (WASM vérifié)".to_string()
                    } else {
                        "déclaratif (aucun binaire)".to_string()
                    };
                    let sha = if agent.sha256.is_empty() {
                        String::new()
                    } else {
                        format!(" [sha256 {}…]", &agent.sha256[..agent.sha256.len().min(12)])
                    };
                    lines.push(ConsultLine::new(
                        format!("{} — {etat}{sha}", agent.name),
                        if agent.artifact_present {
                            ConsultLineKind::Strong
                        } else {
                            ConsultLineKind::Muted
                        },
                    ));
                }
                if lines.is_empty() {
                    lines.push(ConsultLine::new(
                        "Aucun agent listé par le démon.",
                        ConsultLineKind::Muted,
                    ));
                }
                if let Some((name, output, ms)) = &self.last_agent_run {
                    lines.push(ConsultLine::new(
                        format!("Dernière exécution : {name} ({ms} ms) → {output}"),
                        ConsultLineKind::Normal,
                    ));
                }
                ConsultView {
                    title: "Agents".into(),
                    subtitle: "Exécutables (WASM réel) vs manifests déclaratifs".into(),
                    lines,
                    footer,
                }
            }
            PanelKind::Response | PanelKind::None => {
                let aux = self.aux_notice();
                let lines = match &aux {
                    Some(aux) => vec![ConsultLine::new(
                        aux.text.clone(),
                        if aux.error {
                            ConsultLineKind::Error
                        } else {
                            ConsultLineKind::Normal
                        },
                    )],
                    None => vec![ConsultLine::new(
                        "Aucune réponse disponible.",
                        ConsultLineKind::Muted,
                    )],
                };
                ConsultView {
                    title: "Réponse du démon".into(),
                    subtitle: if self.pending_query.is_some() {
                        "Requête en cours…".into()
                    } else {
                        "Dernière réponse reçue".into()
                    },
                    lines,
                    footer,
                }
            }
        }
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
        // Une carte ouverte est un état de décision : la frappe ne doit ni
        // modifier la requête ni envoyer un tour par accident.
        if self.pending_card.is_some()
            && matches!(
                &input,
                HudInput::Char(_) | HudInput::ImeCommit(_) | HudInput::Backspace | HudInput::Enter
            )
        {
            return Vec::new();
        }
        // Un tour est EN VOL (réponse attendue du démon) : la frappe ne doit
        // pas réécrire la requête. Sans ce garde-fou, une lettre tapée pendant
        // l'attente remettait le HUD en mode saisie (`open_prompt`) et
        // pouvait déclencher un SECOND tour concurrent.
        if self.pending_query.is_some()
            && matches!(
                &input,
                HudInput::Char(_) | HudInput::ImeCommit(_) | HudInput::Backspace | HudInput::Enter
            )
        {
            return Vec::new();
        }
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
                        } else if let Some(rest) = q.strip_prefix(":agent ") {
                            // Commande LOCALE : exécution d'un agent WASM réel
                            // via le démon (`:agent <nom> <json>`). Le HUD
                            // n'invente rien : il transmet, le démon décide.
                            let mut parts = rest.splitn(2, ' ');
                            let name = parts.next().unwrap_or_default().trim().to_string();
                            let input_json = parts.next().unwrap_or("{}").trim().to_string();
                            if name.is_empty() {
                                self.notice = Some("usage : :agent <nom> <json>".into());
                                vec![HudAction::Redraw]
                            } else {
                                self.hud.open_prompt();
                                vec![HudAction::RunAgent {
                                    name,
                                    input_json: if input_json.is_empty() {
                                        "{}".to_string()
                                    } else {
                                        input_json
                                    },
                                }]
                            }
                        } else {
                            vec![HudAction::Submit { query: q }]
                        }
                    }
                    _ => Vec::new(),
                }
            }
            HudInput::Escape => self.escape(),
            HudInput::ShowJournal => self.open_panel(PanelKind::Journal),
            HudInput::ShowMemory => self.open_panel(PanelKind::Memory),
            HudInput::ShowStatus => self.open_panel(PanelKind::Status),
            HudInput::ShowAgents => self.open_panel(PanelKind::Agents),
            HudInput::Tick => self.tick(),
            HudInput::FocusQuery => {
                if self.visible {
                    vec![HudAction::Redraw]
                } else {
                    self.show();
                    let mut actions = vec![HudAction::ShowWindow];
                    if self.panel != PanelKind::None {
                        actions.push(self.resize_action(true));
                    }
                    actions.push(HudAction::Redraw);
                    actions
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
                let text = compose_reply(response, validation);
                self.display_turn(query, text, false)
            }
            Ok(Some(KernelEvent::Toast { text })) => self.display_turn(query, text, false),
            Ok(Some(other)) => {
                let msg = format!("réponse noyau inattendue: {other:?}");
                self.display_turn(query, msg, true)
            }
            Ok(None) => Vec::new(),
            Err(e) => {
                let msg = e.to_string();
                self.display_turn(query, msg, true)
            }
        }
    }

    /// Requête réellement envoyée au démon (IPC) : attente affichée, réponse
    /// à venir par `on_kernel_reply` — la machine reste pure.
    pub fn on_submit_sent(&mut self, query: String) -> Vec<HudAction> {
        let was_expanded = self.panel != PanelKind::None;
        self.pending_query = Some(query.clone());
        self.waiting_since = Some(Instant::now());
        self.waiting_shown_secs = 0;
        self.streamed_text.clear();
        self.notice = None;
        self.data_notice = None;
        self.panel = PanelKind::None;
        self.hud.set_streaming(query, waiting_text(0), false);
        if !self.visible {
            return Vec::new();
        }
        let mut actions = Vec::new();
        if was_expanded {
            actions.push(self.resize_action(false));
        }
        actions.push(HudAction::Redraw);
        actions
    }

    /// Requête saisie avant le handshake IPC : mise en file, jamais perdue.
    /// Elle partira automatiquement à `on_ipc_connected`.
    pub fn on_query_queued(&mut self, query: String) -> Vec<HudAction> {
        self.queued_queries.push_back(query.clone());
        self.notice = None;
        self.data_notice = None;
        self.panel = PanelKind::None;
        self.hud.set_streaming(
            query,
            format!(
                "Démon non connecté — requête en file ({}). Elle partira à la connexion.",
                self.queued_queries.len()
            ),
            false,
        );
        if self.visible {
            vec![HudAction::Redraw]
        } else {
            Vec::new()
        }
    }

    /// Handshake IPC (re)établi : envoie la plus ancienne requête en attente.
    pub fn on_ipc_connected(&mut self) -> Vec<HudAction> {
        match self.queued_queries.pop_front() {
            Some(query) => vec![HudAction::Submit { query }],
            None => match self.pending_panel_request.take() {
                // La donnée du panneau était demandée AVANT la connexion :
                // on la redemande (sinon le panneau resterait vide).
                Some(kind) => vec![panel_request_action(kind)],
                None => Vec::new(),
            },
        }
    }

    /// Demande de panneau impossible (démon absent) : le HUD le signale ET
    /// garde la demande pour la rejouer à la prochaine connexion.
    pub fn on_panel_request_failed(&mut self, kind: PanelKind) -> Vec<HudAction> {
        self.pending_panel_request = Some(kind);
        // Refus explicite (erreur) : l'utilisateur doit voir que la donnée
        // n'a PAS été inventée.
        self.notice = Some("démon non connecté — données réelles du démon requises".to_string());
        if self.visible {
            vec![HudAction::Redraw]
        } else {
            self.unseen_reply = true;
            Vec::new()
        }
    }

    /// Consultation demandée alors que l'IPC n'est pas disponible : refus
    /// honnête, le panneau reste ouvert avec son contenu éventuellement vide.
    pub fn on_ipc_unavailable(&mut self, what: &str) -> Vec<HudAction> {
        self.notice = Some(format!("démon non connecté — {what} impossible"));
        if self.visible {
            vec![HudAction::Redraw]
        } else {
            self.unseen_reply = true;
            Vec::new()
        }
    }

    /// Carte réelle proposée par le démon : une seule à la fois. Elle force
    /// l'état `DecisionRequired`, ouvre le panneau 720×360 et ne produit aucun
    /// effet avant une décision auditée côté démon.
    pub fn show_card(&mut self, card: ActionCard) -> Vec<HudAction> {
        let was_visible = self.visible;
        if self.pending_card.is_some() {
            // Le protocole ne permet qu'une carte ouverte ; on conserve la
            // plus récente et on rend l'ancienne explicitement inaccessible.
            tracing::warn!(
                card_id = card.id,
                "nouvelle carte : la précédente est remplacée"
            );
        }
        self.pending_card = Some(card);
        self.pending_actions = 1;
        self.presence
            .set_state(hakille_ui::PresenceState::DecisionRequired);
        self.notice = None;
        self.data_notice = None;
        self.pending_query = None;
        self.panel = PanelKind::Card;
        if !self.visible {
            self.visible = true;
            self.hud.open_prompt();
        }
        let mut actions = Vec::new();
        if !was_visible {
            actions.push(HudAction::ShowWindow);
        }
        actions.push(self.resize_action(true));
        actions.push(HudAction::Redraw);
        actions
    }

    /// Décision envoyée au démon : la carte reste visible jusqu'à l'accusé
    /// d'audit. Aucun effet local n'est simulé.
    pub fn on_card_decision_sent(&mut self, card_id: u64, approved: bool) -> Vec<HudAction> {
        if self.pending_card.as_ref().map(|c| c.id) != Some(card_id) {
            return Vec::new();
        }
        self.presence.set_state(hakille_ui::PresenceState::Acting);
        self.data_notice = Some(if approved {
            "validation envoyée — en attente de l'audit du démon…".to_string()
        } else {
            "refus envoyé — en attente de l'audit du démon…".to_string()
        });
        if self.visible {
            vec![HudAction::Redraw]
        } else {
            Vec::new()
        }
    }

    /// Édition envoyée : aucune mutation locale ; le démon peut refuser.
    pub fn on_card_edit_sent(&mut self, card_id: u64) -> Vec<HudAction> {
        if self.pending_card.as_ref().map(|c| c.id) != Some(card_id) {
            return Vec::new();
        }
        self.data_notice =
            Some("demande de modification envoyée — le démon peut la refuser".to_string());
        if self.visible {
            vec![HudAction::Redraw]
        } else {
            Vec::new()
        }
    }

    /// Accusé signé du démon : la carte est retirée uniquement après l'audit.
    /// Le texte rappelle qu'aucun effet métier n'a été exécuté.
    pub fn on_card_decision_ack(
        &mut self,
        card_id: u64,
        approved: bool,
        audit_id: &str,
        effect: Option<&str>,
    ) -> Vec<HudAction> {
        if self.pending_card.as_ref().map(|c| c.id) != Some(card_id) {
            return self.notice_actions(format!(
                "accusé de carte #{card_id} ignoré — aucune carte correspondante"
            ));
        }
        self.pending_card = None;
        self.pending_actions = 0;
        self.presence.set_state(hakille_ui::PresenceState::Idle);
        let short: String = audit_id.chars().take(8).collect();
        // L'effet est celui RÉELLEMENT produit par le démon (mis en file
        // durable, ou aucun effet si refus). Le HUD ne l'invente jamais.
        let effect_text = match effect {
            Some(e) => format!(" ; effet : {e}"),
            None => " ; aucun effet".to_string(),
        };
        self.data_notice = Some(format!(
            "carte #{card_id} {} — décision auditée [{short}]{effect_text}",
            if approved { "approuvée" } else { "refusée" }
        ));
        let was_expanded = self.panel != PanelKind::None;
        self.panel = PanelKind::None;
        if self.visible {
            let mut actions = Vec::new();
            if was_expanded {
                actions.push(self.resize_action(false));
            }
            actions.push(HudAction::Redraw);
            actions
        } else {
            self.unseen_reply = true;
            Vec::new()
        }
    }

    /// Appui trop court sur une carte irréversible : aucun envoi, aucun effet.
    pub fn on_card_hold_too_short(&mut self) -> Vec<HudAction> {
        self.data_notice =
            Some("appui trop court — maintenir 600 ms pour valider, ou refuser".to_string());
        if self.visible {
            vec![HudAction::Redraw]
        } else {
            Vec::new()
        }
    }

    /// Réponse asynchrone du démon — même logique que les branches `Ok` de
    /// `submit_query`. Fenêtre masquée : mémorisée (`unseen_reply`), zéro
    /// frame.
    pub fn on_kernel_reply(&mut self, msg: KernelToUi) -> Vec<HudAction> {
        match msg {
            KernelToUi::Presence(state) => {
                self.presence.set_state(map_presence(state));
                if self.visible {
                    vec![HudAction::Redraw]
                } else {
                    Vec::new()
                }
            }
            KernelToUi::TokenDelta { text } => {
                // STREAMING : le texte apparaît au fil de l'eau. La réponse
                // finale (HudReply) remplacera ce texte (c'est la référence).
                self.streamed_text.push_str(&text);
                let streamed = self.streamed_text.clone();
                if let Some(query) = self.pending_query.clone() {
                    self.hud.set_streaming(query, streamed, false);
                }
                if self.visible {
                    vec![HudAction::Redraw]
                } else {
                    Vec::new()
                }
            }
            KernelToUi::ShowCard(card) => self.show_card(card),
            KernelToUi::CardDecisionAck {
                card_id,
                approved,
                audit_id,
                effect,
            } => self.on_card_decision_ack(card_id, approved, &audit_id, effect.as_deref()),
            KernelToUi::JournalReply { entries } => {
                let count = entries.len();
                self.journal_entries = entries;
                self.data_reply_actions(PanelKind::Journal, format!("journal : {count} entrée(s)"))
            }
            KernelToUi::MemoryReply { rows } => {
                let count = rows.len();
                self.memory_rows = rows;
                self.data_reply_actions(PanelKind::Memory, format!("mémoire : {count} ligne(s)"))
            }
            KernelToUi::MemoryRememberAck { id } => {
                self.last_remember_id = Some(id.clone());
                self.notice_actions(format!("mémoire enregistrée [{id}]"))
            }
            KernelToUi::StatusReply { status } => {
                let label = format!("statut : {} — {}", status.model, status.cost);
                self.last_status = Some(status);
                self.data_reply_actions(PanelKind::Status, label)
            }
            KernelToUi::AgentListReply { agents } => {
                // Liste HONNÊTE : on affiche ce que le démon a réellement
                // (exécutable vs déclaratif). Aucune capability inventée.
                let count = agents.len();
                let real = agents.iter().filter(|a| a.artifact_present).count();
                self.agents = agents;
                self.data_reply_actions(
                    PanelKind::Agents,
                    format!("agents : {count} listé(s), {real} réellement exécutable(s)"),
                )
            }
            KernelToUi::AgentRunReply {
                name,
                output_json,
                duration_ms,
                input_sha256,
                output_sha256,
            } => {
                // Sortie RÉELLE rapportée par le démon (jamais simulée ici).
                self.last_agent_run = Some((name.clone(), output_json.clone(), duration_ms));
                self.data_notice = Some(format!(
                    "agent {name} — {duration_ms} ms — sortie {output_json} (in {input_sha256} / out {output_sha256})"
                ));
                if self.visible {
                    vec![HudAction::Redraw]
                } else {
                    self.unseen_reply = true;
                    Vec::new()
                }
            }
            msg => self.on_turn_reply(msg),
        }
    }

    /// Réponse d'un tour HUD (HudReply/Toast) — affichage compact ou étendu.
    fn on_turn_reply(&mut self, msg: KernelToUi) -> Vec<HudAction> {
        let query = self.pending_query.take().unwrap_or_default();
        match msg {
            KernelToUi::HudReply {
                response,
                validation,
            } => {
                let text = compose_reply(response, validation);
                self.display_turn(query, text, false)
            }
            KernelToUi::Toast { text } => self.display_turn(query, text, false),
            other => {
                let msg = format!("réponse noyau inattendue: {other:?}");
                self.display_turn(query, msg, true)
            }
        }
    }

    /// Affiche un tour finalisé. Une réponse longue (ou multi-ligne) ouvre
    /// automatiquement le panneau 720×360 ; une réponse courte referme un
    /// panneau précédent pour revenir au HUD 720×64.
    fn display_turn(&mut self, query: String, text: String, error: bool) -> Vec<HudAction> {
        let long = response_is_long(&text);
        // Le tour est terminé : plus de chrono d'attente, plus de flux.
        self.waiting_since = None;
        self.streamed_text.clear();
        if error {
            self.notice = Some(text.clone());
        } else {
            self.notice = None;
        }
        self.hud.set_streaming(query, text, true);
        self.presence.set_state(hakille_ui::PresenceState::Idle);
        let was_expanded = self.panel != PanelKind::None;
        self.panel = if long {
            PanelKind::Response
        } else {
            PanelKind::None
        };
        let mut actions = Vec::new();
        if self.visible {
            if long || was_expanded {
                actions.push(self.resize_action(long));
            }
            actions.push(HudAction::Redraw);
        } else {
            self.unseen_reply = true;
        }
        if let Some(next) = self.queued_queries.pop_front() {
            actions.push(HudAction::Submit { query: next });
        }
        actions
    }

    /// Consultation : ouvre le panneau, confirmation non-erreur, resize + redraw.
    fn data_reply_actions(&mut self, kind: PanelKind, notice: String) -> Vec<HudAction> {
        self.data_notice = Some(notice);
        self.panel = kind;
        if self.visible {
            vec![self.resize_action(true), HudAction::Redraw]
        } else {
            self.unseen_reply = true;
            Vec::new()
        }
    }

    /// Accusé/confirmation sans changement de panneau.
    fn notice_actions(&mut self, notice: String) -> Vec<HudAction> {
        self.data_notice = Some(notice);
        if self.visible {
            vec![HudAction::Redraw]
        } else {
            self.unseen_reply = true;
            Vec::new()
        }
    }

    /// Ouvre (ou referme si déjà actif) un panneau de consultation et émet la
    /// requête IPC correspondante. Sans connexion, l'hôte appellera
    /// `on_ipc_unavailable` — le panneau reste honnêtement vide.
    fn open_panel(&mut self, kind: PanelKind) -> Vec<HudAction> {
        if self.panel == kind {
            return self.close_panel();
        }
        self.panel = kind;
        self.data_notice = None;
        let mut actions = Vec::new();
        if !self.visible {
            self.visible = true;
            self.hud.open_prompt();
            actions.push(HudAction::ShowWindow);
        }
        actions.push(self.resize_action(true));
        actions.push(panel_request_action(kind));
        actions.push(HudAction::Redraw);
        actions
    }

    fn close_panel(&mut self) -> Vec<HudAction> {
        self.panel = PanelKind::None;
        if self.visible {
            vec![self.resize_action(false), HudAction::Redraw]
        } else {
            Vec::new()
        }
    }

    fn resize_action(&self, expanded: bool) -> HudAction {
        HudAction::ResizeWindow {
            width: HUD_WIDTH,
            height: if expanded { CONSULT_HEIGHT } else { HUD_HEIGHT },
        }
    }

    /// Déconnexion IPC : la requête en vol est annulée (le démon n'a pas
    /// d'annulation, aucune réponse ne viendra plus).
    pub fn on_kernel_disconnected(&mut self) -> Vec<HudAction> {
        let had_card = self.pending_card.take().is_some();
        // Plus aucune réponse ne viendra : le chrono d'attente s'arrête.
        self.waiting_since = None;
        if had_card {
            self.pending_actions = 0;
            self.panel = PanelKind::None;
        }
        let Some(query) = self.pending_query.take() else {
            if had_card {
                self.notice =
                    Some("démon déconnecté — carte annulée, aucune décision possible".to_string());
                self.presence.set_state(hakille_ui::PresenceState::Idle);
                return if self.visible {
                    vec![self.resize_action(false), HudAction::Redraw]
                } else {
                    self.unseen_reply = true;
                    Vec::new()
                };
            }
            return Vec::new();
        };
        let cancel = if had_card {
            "démon déconnecté — requête et carte annulées"
        } else {
            "démon déconnecté — requête annulée"
        };
        self.notice = Some(cancel.to_string());
        // État final : sinon l'animation d'attente tournerait sans fin alors
        // qu'aucune réponse ne viendra (aucune frame fantôme).
        self.hud.set_streaming(query, cancel.to_string(), true);
        self.presence.set_state(hakille_ui::PresenceState::Idle);
        if self.visible {
            let mut actions = Vec::new();
            if had_card {
                actions.push(self.resize_action(false));
            }
            actions.push(HudAction::Redraw);
            actions
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
            let mut actions = vec![HudAction::ShowWindow];
            if self.panel != PanelKind::None {
                actions.push(self.resize_action(true));
            }
            actions.push(HudAction::Redraw);
            if self.needs_animation() {
                actions.push(HudAction::RequestFrameIn(self.frame_interval()));
            }
            actions
        }
    }

    fn show(&mut self) {
        self.visible = true;
        self.evaporate_remaining_ms = 0;
        if self.pending_card.is_some() {
            // Une carte masquée par Échap reste la vue active à la
            // réouverture : jamais de HUD compact qui cache la décision.
            self.panel = PanelKind::Card;
        }
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
        self.panel = PanelKind::None;
        self.evaporate_remaining_ms = 0;
        self.hud.dismiss();
    }

    fn escape(&mut self) -> Vec<HudAction> {
        if !self.visible {
            return Vec::new();
        }
        // Une carte reste en attente : Échap masque le HUD sans refuser ni
        // approuver. La carte revient à la réouverture, avec son id intact.
        if self.pending_card.is_some() {
            self.hide_now();
            return vec![HudAction::HideWindow];
        }
        // Premier Échap : referme le panneau étendu et revient au HUD compact.
        // Le second applique la sémantique existante (évaporation/veille).
        if self.panel != PanelKind::None {
            return self.close_panel();
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
        // Temps d'attente : une seule mise à jour par seconde, le texte
        // reste stable entre deux ticks (pas de redraw par frame).
        let elapsed_update = self.refresh_waiting_text();
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
        let mut actions = if self.budget.poll(Instant::now(), self.needs_animation()) {
            vec![
                HudAction::Redraw,
                HudAction::RequestFrameIn(self.frame_interval()),
            ]
        } else {
            Vec::new()
        };
        if elapsed_update && !actions.contains(&HudAction::Redraw) {
            actions.push(HudAction::Redraw);
        }
        actions
    }

    /// Met à jour le texte d'attente (temps réel écoulé). `true` si le texte
    /// a changé et qu'un redraw est donc nécessaire.
    fn refresh_waiting_text(&mut self) -> bool {
        let Some(start) = self.waiting_since else {
            return false;
        };
        let secs = start.elapsed().as_secs();
        if secs == self.waiting_shown_secs {
            return false;
        }
        self.waiting_shown_secs = secs;
        let Some(query) = self.pending_query.clone() else {
            return false;
        };
        self.hud.set_streaming(query, waiting_text(secs), false);
        true
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
    fn agent_command_is_sent_to_the_daemon_not_to_the_llm() {
        let mut app = shown(false);
        type_text(&mut app, ":agent echo-agent {\"msg\":\"bonjour\"}");
        let actions = app.handle(HudInput::Enter);
        assert_eq!(
            actions,
            vec![HudAction::RunAgent {
                name: "echo-agent".to_string(),
                input_json: "{\"msg\":\"bonjour\"}".to_string(),
            }],
            "une commande agent ne part JAMAIS dans le LLM"
        );
        // Le champ est vidé : la commande ne reste pas dans le HUD.
        assert_eq!(app.snapshot().hud_query, "");
    }

    #[test]
    fn agent_list_reply_distinguishes_real_and_declarative_agents() {
        let mut app = shown(false);
        let actions = app.on_kernel_reply(KernelToUi::AgentListReply {
            agents: vec![
                AgentDto {
                    id: "echo-agent".into(),
                    name: "echo-agent".into(),
                    required_caps: vec![],
                    artifact_present: true,
                    sha256: "a".repeat(64),
                },
                AgentDto {
                    id: "agent-mail".into(),
                    name: "Mail Agent".into(),
                    required_caps: vec!["Network".into()],
                    artifact_present: false,
                    sha256: String::new(),
                },
            ],
        });
        assert!(actions.contains(&HudAction::ResizeWindow {
            width: 720,
            height: 360
        }));
        let view = app.consult_view();
        let texte: String = view.lines.iter().map(|l| l.text.clone()).collect();
        assert!(texte.contains("echo-agent"), "{texte}");
        assert!(texte.contains("exécutable"), "{texte}");
        assert!(texte.contains("déclaratif"), "{texte}");
    }

    #[test]
    fn agent_run_reply_shows_the_real_output() {
        let mut app = shown(false);
        let actions = app.on_kernel_reply(KernelToUi::AgentRunReply {
            name: "echo-agent".into(),
            output_json: "{\"echo\":\"bonjour\"}".into(),
            duration_ms: 7,
            input_sha256: "b".repeat(64),
            output_sha256: "c".repeat(64),
        });
        assert!(actions.contains(&HudAction::Redraw));
        let notice = app.aux_notice().expect("sortie d'agent affichée");
        assert!(notice.text.contains("echo-agent"), "{}", notice.text);
        assert!(
            notice.text.contains("{\"echo\":\"bonjour\"}"),
            "{}",
            notice.text
        );
    }

    #[test]
    fn panel_request_failed_is_replayed_on_connection() {
        // Un panneau demandé AVANT la connexion ne doit pas rester vide
        // pour toujours : la demande est rejouée à la connexion.
        let mut app = shown(false);
        let actions = app.handle(HudInput::ShowMemory);
        assert!(actions.contains(&HudAction::RequestMemory {
            query: None,
            limit: CONSULT_LIMIT,
        }));
        // Le démon est absent : la demande échoue, le HUD le signale.
        let actions = app.on_panel_request_failed(PanelKind::Memory);
        assert!(actions.contains(&HudAction::Redraw));
        let aux = app.aux_notice().expect("refus affiché");
        assert!(aux.error, "le refus doit être visible");
        assert!(aux.text.contains("non connecté"), "{}", aux.text);
        // À la connexion, la demande est rejouée.
        let actions = app.on_ipc_connected();
        assert!(
            actions.contains(&HudAction::RequestMemory {
                query: None,
                limit: CONSULT_LIMIT,
            }),
            "la demande doit être rejouée : {actions:?}"
        );
        // Une seconde connexion ne rejoue rien (pas de boucle).
        assert!(app.on_ipc_connected().is_empty());
    }

    #[test]
    fn streamed_tokens_are_accumulated_then_replaced_by_the_final_reply() {
        let mut app = shown(false);
        app.on_submit_sent("salut".into());
        // Deltas : le texte apparaît au fil de l'eau.
        for delta in ["Bon", "jour", " à tous"] {
            let actions = app.on_kernel_reply(KernelToUi::TokenDelta {
                text: delta.to_string(),
            });
            assert!(actions.contains(&HudAction::Redraw));
        }
        let snap = app.snapshot();
        assert_eq!(snap.hud_response, "Bonjour à tous");
        assert!(
            snap.hud_streaming,
            "la réponse reste en cours (streaming) : {}",
            snap.hud_response
        );
        // La réponse FINALE remplace le texte cumulé (c'est la référence).
        let actions = app.on_kernel_reply(KernelToUi::HudReply {
            response: "Bonjour à tous, comment vas-tu ?".into(),
            validation: None,
        });
        assert!(actions.contains(&HudAction::Redraw));
        let snap = app.snapshot();
        assert_eq!(snap.hud_response, "Bonjour à tous, comment vas-tu ?");
        assert!(!snap.hud_streaming, "la réponse finale n'est plus un flux");
    }

    #[test]
    fn close_quits() {
        let mut app = app(false, false);
        assert_eq!(app.handle(HudInput::Close), vec![HudAction::Quit]);
    }

    #[test]
    fn typing_during_a_pending_turn_never_starts_a_second_turn() {
        let mut app = shown(false);
        app.on_submit_sent("combien".into());
        // Une frappe pendant l'attente ne doit ni réécrire la requête ni
        // remettre le HUD en mode saisie (source du double tour concurrent).
        assert!(app.handle(HudInput::Char('x')).is_empty());
        assert!(app.handle(HudInput::Backspace).is_empty());
        assert!(app.handle(HudInput::Enter).is_empty());
        let snap = app.snapshot();
        assert_eq!(snap.hud_query, "combien");
        assert!(
            snap.hud_response
                .starts_with("En attente de réponse du démon…"),
            "l'attente reste affichée : {}",
            snap.hud_response
        );
    }

    #[test]
    fn waiting_text_is_explicit_about_the_real_wait() {
        let text = waiting_text(42);
        assert!(text.contains("42s"), "temps écoulé réel : {text}");
        assert!(
            text.contains("Qwen CPU local"),
            "la cause de l'attente est annoncée : {text}"
        );
    }

    #[test]
    fn reply_clears_the_waiting_chrono() {
        let mut app = shown(false);
        app.on_submit_sent("statut".into());
        assert!(app.snapshot().hud_response.contains("En attente"));
        app.on_kernel_reply(KernelToUi::HudReply {
            response: "ok".into(),
            validation: None,
        });
        let snap = app.snapshot();
        assert_eq!(snap.hud_response, "ok");
        assert!(!snap.hud_response.contains("En attente"));
    }

    #[test]
    fn pointer_target_exposes_card_buttons_and_drag_zones() {
        let mut app = shown(false);
        // HUD compact : toute la barre est une poignée de déplacement.
        assert_eq!(app.pointer_target(300.0, 30.0), PointerTarget::Drag);
        assert_eq!(app.pointer_target(300.0, 200.0), PointerTarget::None);

        // Carte ouverte : les trois boutons sont cliquables, le corps non.
        app.on_kernel_reply(KernelToUi::ShowCard(test_card(7)));
        let buttons = crate::renderer::card_buttons(true);
        assert_eq!(
            app.pointer_target(buttons.validate.x + 2.0, buttons.validate.y + 2.0),
            PointerTarget::CardValidate
        );
        assert_eq!(
            app.pointer_target(buttons.edit.x + 2.0, buttons.edit.y + 2.0),
            PointerTarget::CardEdit
        );
        assert_eq!(
            app.pointer_target(buttons.reject.x + 2.0, buttons.reject.y + 2.0),
            PointerTarget::CardReject
        );
        // On ne bouge pas la fenêtre pendant une décision.
        assert_eq!(app.pointer_target(300.0, 300.0), PointerTarget::None);
        // L'en-tête de la carte reste une zone de déplacement.
        assert_eq!(app.pointer_target(300.0, 80.0), PointerTarget::Drag);
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
    fn presence_reacts_without_resolving_pending_query() {
        let mut app = shown(false);
        app.on_submit_sent("statut".into());
        let actions = app.on_kernel_reply(KernelToUi::Presence(PresenceState::Acting));
        assert_eq!(actions, vec![HudAction::Redraw]);
        assert_eq!(app.presence.state(), hakille_ui::PresenceState::Acting);
        assert!(
            app.snapshot()
                .hud_response
                .starts_with("En attente de réponse du démon…"),
            "la requête reste en vol après une présence : {}",
            app.snapshot().hud_response
        );
        assert!(app.needs_animation(), "présence Acting animée");
        let actions = app.on_kernel_disconnected();
        assert_eq!(actions, vec![HudAction::Redraw]);
        assert_eq!(app.presence.state(), hakille_ui::PresenceState::Idle);
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
        assert_eq!(
            actions,
            vec![
                HudAction::ResizeWindow {
                    width: 720,
                    height: 360
                },
                HudAction::Redraw
            ]
        );
        assert_eq!(app.panel(), PanelKind::Journal);
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
        assert_eq!(
            actions,
            vec![
                HudAction::ResizeWindow {
                    width: 720,
                    height: 360
                },
                HudAction::Redraw
            ]
        );
        assert_eq!(app.panel(), PanelKind::Status);
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
        assert_eq!(
            actions,
            vec![
                HudAction::ResizeWindow {
                    width: 720,
                    height: 360
                },
                HudAction::Redraw
            ]
        );
        assert!(
            app.snapshot()
                .hud_response
                .starts_with("En attente de réponse du démon…"),
            "la consultation ne doit pas clore la requête en vol : {}",
            app.snapshot().hud_response
        );
        let actions = app.on_kernel_disconnected();
        assert_eq!(actions, vec![HudAction::Redraw]);
    }

    #[test]
    fn long_response_opens_360_panel_and_escape_closes_it() {
        let mut app = shown(false);
        app.on_submit_sent("raconte".into());
        let long = "ligne très longue ".repeat(20);
        let actions = app.on_kernel_reply(KernelToUi::HudReply {
            response: long,
            validation: None,
        });
        assert!(actions.contains(&HudAction::ResizeWindow {
            width: 720,
            height: 360
        }));
        assert!(actions.contains(&HudAction::Redraw));
        assert_eq!(app.panel(), PanelKind::Response);
        assert_eq!(app.scene().height, CONSULT_HEIGHT);

        let actions = app.handle(HudInput::Escape);
        assert_eq!(
            actions,
            vec![
                HudAction::ResizeWindow {
                    width: 720,
                    height: 64
                },
                HudAction::Redraw
            ]
        );
        assert_eq!(app.panel(), PanelKind::None);
        assert_eq!(app.scene().height, HUD_HEIGHT);
    }

    #[test]
    fn short_turn_after_expanded_response_returns_to_compact_hud() {
        let mut app = shown(false);
        let _ = app.on_submit_sent("long".into());
        let _ = app.on_kernel_reply(KernelToUi::HudReply {
            response: "x".repeat(EXPAND_RESPONSE_CHARS + 1),
            validation: None,
        });
        assert_eq!(app.panel(), PanelKind::Response);
        let actions = app.on_submit_sent("court".into());
        assert_eq!(
            actions,
            vec![
                HudAction::ResizeWindow {
                    width: 720,
                    height: 64
                },
                HudAction::Redraw
            ]
        );
        assert_eq!(app.panel(), PanelKind::None);
    }

    #[test]
    fn f_keys_open_consultation_panels_and_emit_real_requests() {
        let mut app = shown(false);

        let actions = app.handle(HudInput::ShowJournal);
        assert!(actions.contains(&HudAction::ResizeWindow {
            width: 720,
            height: 360
        }));
        assert!(actions.contains(&HudAction::RequestJournal {
            limit: CONSULT_LIMIT
        }));
        assert_eq!(app.panel(), PanelKind::Journal);
        assert!(
            app.consult_view()
                .lines
                .iter()
                .any(|line| line.text.contains("Aucune entrée reçue"))
        );

        let actions = app.handle(HudInput::ShowMemory);
        assert!(actions.contains(&HudAction::RequestMemory {
            query: None,
            limit: CONSULT_LIMIT
        }));
        assert_eq!(app.panel(), PanelKind::Memory);

        let actions = app.handle(HudInput::ShowStatus);
        assert!(actions.contains(&HudAction::RequestStatus));
        assert_eq!(app.panel(), PanelKind::Status);
        assert!(
            app.consult_view()
                .lines
                .iter()
                .any(|line| line.text.contains("Aucun statut reçu"))
        );

        // Re-appuyer sur F3 referme le panneau et restaure le HUD compact.
        let actions = app.handle(HudInput::ShowStatus);
        assert_eq!(
            actions,
            vec![
                HudAction::ResizeWindow {
                    width: 720,
                    height: 64
                },
                HudAction::Redraw
            ]
        );
        assert_eq!(app.panel(), PanelKind::None);
    }

    #[test]
    fn queries_before_handshake_are_queued_then_sent_in_order() {
        let mut app = shown(false);
        let actions = app.on_query_queued("premier".into());
        assert_eq!(actions, vec![HudAction::Redraw]);
        assert!(app.snapshot().hud_response.contains("file (1)"));
        app.on_query_queued("second".into());
        assert!(app.snapshot().hud_response.contains("file (2)"));

        assert_eq!(
            app.on_ipc_connected(),
            vec![HudAction::Submit {
                query: "premier".into()
            }]
        );
        assert_eq!(
            app.on_ipc_connected(),
            vec![HudAction::Submit {
                query: "second".into()
            }]
        );
        assert!(app.on_ipc_connected().is_empty());
    }

    #[test]
    fn ipc_unavailable_is_an_honest_error_not_an_empty_success() {
        let mut app = shown(false);
        let actions = app.on_ipc_unavailable("journal");
        assert_eq!(actions, vec![HudAction::Redraw]);
        let aux = app.aux_notice().expect("refus IPC affiché");
        assert!(aux.error);
        assert!(aux.text.contains("démon non connecté"));
    }

    fn test_card(id: u64) -> ActionCard {
        ActionCard {
            id,
            verb: "Confirmer".into(),
            object: "Virement : 25 EUR".into(),
            reason: "Action irréversible détectée".into(),
            cost: hakille_ui_protocol::Money::from_cents(2500, "EUR"),
            data_exit: hakille_ui_protocol::DataExit::Local,
            reversible: false,
            provenance: hakille_ui_protocol::Provenance::Hakille,
        }
    }

    #[test]
    fn show_card_expands_blocks_typing_and_escape_only_hides_it() {
        let mut app = shown(false);
        let actions = app.on_kernel_reply(KernelToUi::ShowCard(test_card(7)));
        assert!(actions.contains(&HudAction::ResizeWindow {
            width: 720,
            height: 360
        }));
        assert_eq!(app.panel(), PanelKind::Card);
        assert_eq!(app.pending_card().map(|c| c.id), Some(7));
        assert_eq!(app.pending_actions, 1);
        assert_eq!(app.snapshot().presence_state, 4);
        assert_eq!(app.scene().height, CONSULT_HEIGHT);
        assert!(
            app.handle(HudInput::Char('x')).is_empty(),
            "une carte ouverte ne doit pas modifier la requête"
        );

        // Échap masque mais ne refuse pas : la carte reste en attente.
        assert_eq!(app.handle(HudInput::Escape), vec![HudAction::HideWindow]);
        assert!(!app.visible);
        assert_eq!(app.pending_card().map(|c| c.id), Some(7));

        let actions = app.handle(HudInput::HotkeyToggle);
        assert!(actions.contains(&HudAction::ShowWindow));
        assert!(actions.contains(&HudAction::ResizeWindow {
            width: 720,
            height: 360
        }));
        assert_eq!(app.scene().height, CONSULT_HEIGHT);
    }

    #[test]
    fn card_ack_is_the_only_way_to_clear_the_card_and_restores_compact_hud() {
        let mut app = shown(false);
        app.on_kernel_reply(KernelToUi::ShowCard(test_card(9)));
        let actions = app.on_card_decision_sent(9, true);
        assert!(actions.contains(&HudAction::Redraw));
        assert_eq!(app.snapshot().presence_state, 2, "Acting pendant l'audit");
        assert_eq!(app.pending_card().map(|c| c.id), Some(9));

        let actions = app.on_card_decision_ack(
            9,
            true,
            "018f0000-aaaa-bbbb-cccc-000000000009",
            Some("action mise en file (clé card#9)"),
        );
        assert!(actions.contains(&HudAction::ResizeWindow {
            width: 720,
            height: 64
        }));
        assert!(app.pending_card().is_none());
        assert_eq!(app.pending_actions, 0);
        assert_eq!(app.panel(), PanelKind::None);
        assert_eq!(app.scene().height, HUD_HEIGHT);
        let notice = app.aux_notice().expect("accusé affiché");
        assert!(notice.text.contains("décision auditée"));
        assert!(
            notice.text.contains("action mise en file"),
            "l'effet réel est affiché tel quel : {}",
            notice.text
        );
    }

    #[test]
    fn unmatched_card_ack_never_clears_the_pending_card() {
        let mut app = shown(false);
        app.on_kernel_reply(KernelToUi::ShowCard(test_card(11)));
        let actions = app.on_card_decision_ack(12, true, "uuid", None);
        assert!(actions.contains(&HudAction::Redraw));
        assert_eq!(app.pending_card().map(|c| c.id), Some(11));
        let notice = app.aux_notice().expect("accusé ignoré signalé");
        assert!(notice.text.contains("ignoré"));
    }

    #[test]
    fn disconnect_cancels_pending_card_without_pretending_a_decision() {
        let mut app = shown(false);
        app.on_kernel_reply(KernelToUi::ShowCard(test_card(13)));
        let actions = app.on_kernel_disconnected();
        assert!(actions.contains(&HudAction::ResizeWindow {
            width: 720,
            height: 64
        }));
        assert!(app.pending_card().is_none());
        assert_eq!(app.pending_actions, 0);
        assert_eq!(app.panel(), PanelKind::None);
        let notice = app.aux_notice().expect("carte annulée signalée");
        assert!(notice.text.contains("carte annulée"));
    }

    #[test]
    fn hidden_long_reply_expands_on_next_show() {
        let mut app = shown(false);
        app.on_submit_sent("raconte".into());
        app.handle(HudInput::HotkeyToggle);
        let actions = app.on_kernel_reply(KernelToUi::HudReply {
            response: "x".repeat(EXPAND_RESPONSE_CHARS + 1),
            validation: None,
        });
        assert!(actions.is_empty(), "masqué : aucune frame");
        assert_eq!(app.panel(), PanelKind::Response);

        let actions = app.handle(HudInput::HotkeyToggle);
        assert!(actions.contains(&HudAction::ShowWindow));
        assert!(actions.contains(&HudAction::ResizeWindow {
            width: 720,
            height: 360
        }));
        assert_eq!(app.scene().height, CONSULT_HEIGHT);
    }
}
