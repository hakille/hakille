//! AccessKit — même hiérarchie que l'affichage, testée avec lecteurs réels.
//!
//! S3 est bloquant : NVDA / VoiceOver / Orca doivent parcourir Carte +
//! Journal au clavier. Tout contrôle interactif a nom + rôle + action.

use accesskit::{Action, Node, NodeId, Role, Tree, TreeId, TreeUpdate};
use annora_ui_model::{JournalRow, PowerRow, StableSnapshot};

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
    nodes.push((id(0), node(Role::Window, "Akora", &[], vec![id(1), id(2), id(3), id(4)])));
    nodes.push((
        id(1),
        node(Role::Button, &snap.presence_a11y_label, &[Action::Focus, Action::Click], vec![]),
    ));

    let mut hud_kids = vec![id(20)];
    let mut prompt = Node::new(Role::TextInput);
    prompt.set_label("Commande Akora — Entrée pour envoyer, Échap pour fermer");
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
                &format!("Valider : {} — {}", snap.hud_validation_text, snap.hud_validation_cost),
                &[Action::Focus, Action::Click],
                vec![],
            ),
        ));
        hud_kids.push(id(22));
    }
    nodes.push((id(2), node(Role::GenericContainer, "Ligne de commande Akora", &[], hud_kids)));

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
                    if row.verified { "signé, " } else { "signature douteuse, " },
                    if row.can_revert { "annulable" } else { "définitif" }
                ),
                &[],
                vec![],
            ),
        ));
        kids.push(nid);
    }
    nodes.push((id(3), node(Role::List, "Journal d'audit — frise, annulation en un geste", &[], kids)));

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
                    if p.policy_locked { "— verrouillé par politique" } else { "" }
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
        node(Role::GenericContainer, &format!("Carte d'action — {}", card.provenance_label), &[], vec![id(1), id(2), id(3), id(4)]),
    ));
    nodes.push((id(1), text(format!("{} {}", card.verb, card.object))));
    nodes.push((id(2), text(format!("{} — Pourquoi ? (trace : {})", card.reason, card.why_trace))));
    nodes.push((
        id(3),
        text(format!("Coût : {} — Données : {} — {}", card.cost_label, card.data_label, card.reversibility_label)),
    ));
    let validate_label = if card.needs_hold {
        "Valider — appui long 600 millisecondes (action définitive)"
    } else {
        "Valider"
    };
    nodes.push((
        id(4),
        node(Role::GenericContainer, "Valider, Modifier, Refuser", &[], vec![id(40), id(41), id(42)]),
    ));
    nodes.push((id(40), node(Role::Button, validate_label, &[Action::Focus, Action::Click], vec![])));
    nodes.push((id(41), node(Role::Button, "Modifier", &[Action::Focus, Action::Click], vec![])));
    nodes.push((id(42), node(Role::Button, "Refuser", &[Action::Focus, Action::Click], vec![])));
    if card.is_external {
        if let Some((_, v)) = nodes.iter_mut().find(|(i, _)| *i == id(1)) {
            *v = text(format!("{} {} [externe — filet 2 px]", card.verb, card.object));
        }
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
    use annora_ui::{CostCounter, EphemeralHud, PresenceIndicator, SilenceBudgetTracker};

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
            verb: "Envoyer".into(), object: "ce mail à Awa".into(),
            reason: "Tu as demandé de répondre.".into(), why_trace: "Mail cran 1".into(),
            cost_label: "0 €".into(), data_label: "Local".into(),
            reversibility_label: "annulable".into(), needs_hold: false,
            provenance_label: "Akora".into(), is_external: false,
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
        let powers = vec![PowerRow { group: "Agir".into(), scope: "Gui".into(), enabled: true, policy_locked: true }];
        let u = build_shell_tree(&snap, &[], &powers);
        let locked = u.nodes.iter().find(|(i, _)| *i == id(200)).unwrap();
        assert!(!locked.1.supports_action(Action::Click));
        assert!(locked.1.label().unwrap().contains("verrouillé"));
    }
}
