//! AccessKit — même hiérarchie que l'affichage, testée avec lecteurs réels.
//!
//! S3 est bloquant : NVDA / VoiceOver / Orca doivent parcourir Carte +
//! Journal au clavier. Tout contrôle interactif a nom + rôle + action.

use accesskit::{Action, Node, NodeId, Role, Tree, TreeId, TreeUpdate};
use hakille_ui_model::{JournalRow, PowerRow, StableSnapshot};

fn id(n: u64) -> NodeId {
    NodeId(n)
}

fn node(role: Role, label: &str, actions: &[Action], children: Vec<NodeId>) -> Node {
    let mut n = Node::new(role);
    if !label.is_empty() {
        n.set_label(label);
        if role == Role::Label {
            n.set_value(label);
        }
    }
    for a in actions {
        n.add_action(*a);
    }
    if !children.is_empty() {
        n.set_children(children);
    }
    n
}

/// Arbre complet depuis snapshot + journal + pouvoirs.
pub fn build_shell_tree(
    snap: &StableSnapshot,
    journal: &[JournalRow],
    powers: &[PowerRow],
) -> TreeUpdate {
    let mut nodes: Vec<(NodeId, Node)> = Vec::new();
    nodes.push((
        id(0),
        node(
            Role::Window,
            "Hakille",
            &[],
            vec![id(1), id(2), id(3), id(4)],
        ),
    ));
    nodes.push((
        id(1),
        node(
            Role::Button,
            &snap.presence_a11y_label,
            &[Action::Focus, Action::Click],
            vec![],
        ),
    ));

    let mut hud_kids = vec![id(20)];
    let mut prompt = Node::new(Role::TextInput);
    prompt.set_label("Commande Hakille — Entrée pour envoyer, Échap pour fermer");
    prompt.set_value(snap.hud_query.clone());
    prompt.add_action(Action::Focus);
    nodes.push((id(20), prompt));
    if !snap.hud_response.is_empty() {
        let mut resp = Node::new(Role::Label);
        resp.set_value(snap.hud_response.clone());
        nodes.push((id(21), resp));
        hud_kids.push(id(21));
    }
    if snap.hud_validation {
        nodes.push((
            id(22),
            node(
                Role::Button,
                &format!(
                    "Valider : {} — {}",
                    snap.hud_validation_text, snap.hud_validation_cost
                ),
                &[Action::Focus, Action::Click],
                vec![],
            ),
        ));
        hud_kids.push(id(22));
    }
    nodes.push((
        id(2),
        node(
            Role::GenericContainer,
            "Ligne de commande Hakille",
            &[],
            hud_kids,
        ),
    ));

    let mut kids = Vec::new();
    for (i, row) in journal.iter().take(50).enumerate() {
        let nid = id(100 + i as u64);
        nodes.push((
            nid,
            node(
                Role::ListItem,
                &format!(
                    "{} — {} — {}{}",
                    row.time,
                    row.action,
                    if row.verified {
                        "signé, "
                    } else {
                        "signature douteuse, "
                    },
                    if row.can_revert {
                        "annulable"
                    } else {
                        "définitif"
                    }
                ),
                &[],
                vec![],
            ),
        ));
        kids.push(nid);
    }
    nodes.push((
        id(3),
        node(
            Role::List,
            "Journal d'audit — frise, annulation en un geste",
            &[],
            kids,
        ),
    ));

    let mut pkids = Vec::new();
    for (i, p) in powers.iter().enumerate() {
        let nid = id(200 + i as u64);
        let mut acts = vec![Action::Focus];
        if !p.policy_locked {
            acts.push(Action::Click);
        }
        nodes.push((
            nid,
            node(
                Role::Button,
                &format!(
                    "Pouvoir {} ({}) : {} {}",
                    p.group,
                    p.scope,
                    if p.enabled { "activé" } else { "coupé" },
                    if p.policy_locked {
                        "— verrouillé par politique"
                    } else {
                        ""
                    }
                ),
                &acts,
                vec![],
            ),
        ));
        pkids.push(nid);
    }
    nodes.push((id(4), node(Role::GenericContainer, "Pouvoirs", &[], pkids)));

    TreeUpdate {
        nodes,
        tree: Some(Tree::new(id(0))),
        tree_id: TreeId::ROOT,
        focus: id(1),
    }
}

/// Identifiants réservés hors cartes (40..42), listes et panneau (500).
pub const VOICE_TOGGLE_NODE: NodeId = NodeId(60);
pub const VOICE_STATUS_NODE: NodeId = NodeId(61);

pub fn append_voice_controls(
    update: &mut TreeUpdate,
    state: &crate::app::VoiceState,
    enabled: bool,
) {
    let mut button = node(
        Role::Button,
        if enabled {
            "Arrêter le microphone — F8"
        } else {
            "Activer le microphone — F8"
        },
        &[Action::Focus, Action::Click],
        vec![],
    );
    button.set_description(state.description());
    let rect = crate::renderer::voice_button_rect();
    button.set_bounds(accesskit::Rect::new(
        rect.x as f64,
        rect.y as f64,
        (rect.x + rect.w) as f64,
        (rect.y + rect.h) as f64,
    ));
    let mut status = node(Role::Label, &state.description(), &[], vec![]);
    status.set_live(accesskit::Live::Polite);
    update.nodes.push((VOICE_TOGGLE_NODE, button));
    update.nodes.push((VOICE_STATUS_NODE, status));
    if let Some((_, root)) = update.nodes.iter_mut().find(|(nid, _)| *nid == id(0)) {
        let mut children = root.children().to_vec();
        children.extend([VOICE_TOGGLE_NODE, VOICE_STATUS_NODE]);
        root.set_children(children);
    }
}

/// Panneau consulté : tout le texte reçu reste lisible par le lecteur
/// d'écran, même lorsque la fenêtre ne montre qu'une page.
pub const CONSULT_PANEL_NODE: NodeId = NodeId(500);

pub fn append_consult_panel(update: &mut TreeUpdate, view: &crate::renderer::ConsultView) {
    let mut children = Vec::new();
    for (index, line) in view.lines.iter().enumerate() {
        if line.text.is_empty() {
            continue;
        }
        let nid = id(1000 + index as u64);
        update
            .nodes
            .push((nid, node(Role::Label, &line.text, &[], vec![])));
        children.push(nid);
    }
    update.nodes.push((
        CONSULT_PANEL_NODE,
        node(
            Role::GenericContainer,
            &format!("{} — {}", view.title, view.subtitle),
            &[Action::ScrollUp, Action::ScrollDown],
            children,
        ),
    ));
    if let Some((_, root)) = update.nodes.iter_mut().find(|(nid, _)| *nid == id(0)) {
        let mut children = root.children().to_vec();
        children.push(CONSULT_PANEL_NODE);
        root.set_children(children);
    }
}

/// Nœuds interactifs sans label (doit être vide — gate S3).
pub fn audit_interactive_labels(update: &TreeUpdate) -> Vec<NodeId> {
    update
        .nodes
        .iter()
        .filter(|(_, n)| {
            matches!(n.role(), Role::Button | Role::TextInput)
                && n.label().map(|l| l.is_empty()).unwrap_or(true)
        })
        .map(|(i, _)| *i)
        .collect()
}

#[derive(Debug, Clone)]
pub struct ActionCardA11y {
    pub verb: String,
    pub object: String,
    pub reason: String,
    pub why_trace: String,
    pub cost_label: String,
    pub data_label: String,
    pub reversibility_label: String,
    pub needs_hold: bool,
    pub provenance_label: String,
    pub is_external: bool,
}

/// Arbre carte d'action : verbe+objet, raison+Pourquoi, 3 pastilles
/// (coût, données, réversibilité), Valider/Modifier/Refuser.
pub fn build_action_card_tree(card: &ActionCardA11y) -> TreeUpdate {
    let mut nodes: Vec<(NodeId, Node)> = Vec::new();
    let text = |value: String| {
        let mut n = Node::new(Role::Label);
        n.set_value(value);
        n
    };
    nodes.push((
        id(0),
        node(
            Role::GenericContainer,
            &format!("Carte d'action — {}", card.provenance_label),
            &[],
            vec![id(1), id(2), id(3), id(4)],
        ),
    ));
    nodes.push((id(1), text(format!("{} {}", card.verb, card.object))));
    nodes.push((
        id(2),
        text(format!(
            "{} — Pourquoi ? (trace : {})",
            card.reason, card.why_trace
        )),
    ));
    nodes.push((
        id(3),
        text(format!(
            "Coût : {} — Données : {} — {}",
            card.cost_label, card.data_label, card.reversibility_label
        )),
    ));
    let validate_label = if card.needs_hold {
        "Valider — appui long 600 millisecondes (action définitive)"
    } else {
        "Valider"
    };
    nodes.push((
        id(4),
        node(
            Role::GenericContainer,
            "Valider, Modifier, Refuser",
            &[],
            vec![id(40), id(41), id(42)],
        ),
    ));
    nodes.push((
        id(40),
        node(
            Role::Button,
            validate_label,
            &[Action::Focus, Action::Click],
            vec![],
        ),
    ));
    nodes.push((
        id(41),
        node(
            Role::Button,
            "Modifier",
            &[Action::Focus, Action::Click],
            vec![],
        ),
    ));
    nodes.push((
        id(42),
        node(
            Role::Button,
            "Refuser",
            &[Action::Focus, Action::Click],
            vec![],
        ),
    ));
    if card.is_external
        && let Some((_, v)) = nodes.iter_mut().find(|(i, _)| *i == id(1))
    {
        *v = text(format!(
            "{} {} [externe — filet 2 px]",
            card.verb, card.object
        ));
    }
    TreeUpdate {
        nodes,
        tree: Some(Tree::new(id(0))),
        tree_id: TreeId::ROOT,
        focus: id(40),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_ui::{CostCounter, EphemeralHud, PresenceIndicator, SilenceBudgetTracker};

    fn sample() -> StableSnapshot {
        let presence = PresenceIndicator::new();
        let mut hud = EphemeralHud::new();
        hud.request_validation("w-1", "Envoi 25.00 EUR", Some(2500), "Financial");
        let cost = CostCounter::new(500, "EUR");
        let silence = SilenceBudgetTracker::new(3);
        StableSnapshot::from_states(&presence, &hud, &cost, &silence, 0, 1)
    }

    #[test]
    fn shell_tree_labels_focus_and_cost() {
        let snap = sample();
        let update = build_shell_tree(&snap, &[], &[]);
        assert!(audit_interactive_labels(&update).is_empty());
        let valid = update.nodes.iter().find(|(i, _)| *i == id(22)).unwrap();
        assert!(valid.1.label().unwrap().contains("25.00"));
        assert_eq!(update.focus, id(1));
    }

    #[test]
    fn card_tree_pastilles_hold_external() {
        let card = ActionCardA11y {
            verb: "Envoyer".into(),
            object: "ce mail à Awa".into(),
            reason: "Tu as demandé de répondre.".into(),
            why_trace: "Mail cran 1".into(),
            cost_label: "0 €".into(),
            data_label: "Local".into(),
            reversibility_label: "annulable".into(),
            needs_hold: false,
            provenance_label: "Hakille".into(),
            is_external: false,
        };
        let u = build_action_card_tree(&card);
        assert!(audit_interactive_labels(&u).is_empty());
        let title = u.nodes.iter().find(|(i, _)| *i == id(1)).unwrap();
        assert!(title.1.value().unwrap().contains("Awa"));
        let mut hard = card.clone();
        hard.needs_hold = true;
        let u2 = build_action_card_tree(&hard);
        let v = u2.nodes.iter().find(|(i, _)| *i == id(40)).unwrap();
        assert!(v.1.label().unwrap().contains("600"));
        let mut ext = card.clone();
        ext.is_external = true;
        let u3 = build_action_card_tree(&ext);
        let t = u3.nodes.iter().find(|(i, _)| *i == id(1)).unwrap();
        assert!(t.1.value().unwrap().contains("[externe"));
    }

    #[test]
    fn locked_power_has_no_click() {
        let snap = sample();
        let powers = vec![PowerRow {
            group: "Agir".into(),
            scope: "Gui".into(),
            enabled: true,
            policy_locked: true,
        }];
        let u = build_shell_tree(&snap, &[], &powers);
        let locked = u.nodes.iter().find(|(i, _)| *i == id(200)).unwrap();
        assert!(!locked.1.supports_action(Action::Click));
        assert!(locked.1.label().unwrap().contains("verrouillé"));
    }
    #[test]
    fn a11y_consult_panel_exposes_complete_content_and_scroll_actions() {
        use crate::renderer::{ConsultLine, ConsultLineKind, ConsultView};
        let view = ConsultView {
            title: "Mémoire".into(),
            subtitle: "50 faits reçus".into(),
            footer: String::new(),
            lines: (0..50)
                .map(|n| ConsultLine::new(format!("fait réel {n}"), ConsultLineKind::Normal))
                .collect(),
        };
        let mut update = build_shell_tree(&sample(), &[], &[]);
        append_consult_panel(&mut update, &view);
        let root = &update
            .nodes
            .iter()
            .find(|(nid, _)| *nid == id(0))
            .unwrap()
            .1;
        assert!(root.children().contains(&CONSULT_PANEL_NODE));
        let panel = &update
            .nodes
            .iter()
            .find(|(nid, _)| *nid == CONSULT_PANEL_NODE)
            .unwrap()
            .1;
        assert_eq!(panel.children().len(), 50);
        assert!(panel.supports_action(Action::ScrollUp));
        assert!(panel.supports_action(Action::ScrollDown));
        assert!(
            update
                .nodes
                .iter()
                .any(|(_, node)| node.value() == Some("fait réel 49"))
        );
        let ids: std::collections::HashSet<_> = update.nodes.iter().map(|(nid, _)| *nid).collect();
        assert_eq!(
            ids.len(),
            update.nodes.len(),
            "identifiants AccessKit uniques"
        );
    }
    #[test]
    fn a11y_voice_button_and_live_status_have_unique_ids_and_real_state() {
        let app = crate::app::HudApp::new(
            false,
            true,
            crate::runtime::FrameBudget::battery_saver(),
            true,
        );
        let mut tree = build_shell_tree(&app.snapshot(), &[], &[]);
        append_voice_controls(&mut tree, &crate::app::VoiceState::Transcribing, true);
        let button = &tree
            .nodes
            .iter()
            .find(|(id, _)| *id == VOICE_TOGGLE_NODE)
            .unwrap()
            .1;
        assert_eq!(button.role(), Role::Button);
        assert!(button.supports_action(Action::Click));
        assert!(button.label().unwrap().contains("Arrêter"));
        let status = &tree
            .nodes
            .iter()
            .find(|(id, _)| *id == VOICE_STATUS_NODE)
            .unwrap()
            .1;
        assert_eq!(status.live(), Some(accesskit::Live::Polite));
        assert!(status.value().unwrap().contains("Transcription locale"));
        let ids: std::collections::HashSet<_> = tree.nodes.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids.len(), tree.nodes.len());
        assert!(tree.nodes[0].1.children().contains(&VOICE_TOGGLE_NODE));
    }
}
