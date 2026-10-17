//! Spike S1 — arbre AccessKit construit depuis les view-models.
//!
//! Règle d'or testée ici : tout nœud interactif porte un label non vide
//! (lecteurs d'écran), la validation porte l'action Default + son coût dans
//! le label, le focus est toujours défini. Même arbre que iced-accesskit
//! exposera : si ce test passe, la donnée est prête pour le labo S1.
//!
//! Critère labo restant (spec §10) : VoiceOver/NVDA/Orca parcourent Carte
//! d'action + Journal sur macOS/Windows/Linux. Plan B : Xilem/Masonry.

use accesskit::{Action, Node, NodeBuilder, NodeClassSet, NodeId, Role, Tree, TreeUpdate};
use annora_design::ActionCardView;
use annora_ui_shell::{JournalRow, PowerRow, ShellSnapshot};

fn id(n: u64) -> NodeId {
    NodeId(n)
}

fn node(
    classes: &mut NodeClassSet,
    role: Role,
    label: &str,
    actions: &[Action],
    children: Vec<NodeId>,
) -> Node {
    let mut b = NodeBuilder::default();
    b.set_role(role);
    if !label.is_empty() {
        b.set_name(label);
        if role == Role::StaticText {
            b.set_value(label);
        }
    }
    for a in actions {
        b.add_action(*a);
    }
    if !children.is_empty() {
        b.set_children(children);
    }
    b.build(classes)
}

/// Construit l'arbre complet depuis un snapshot + lignes journal + pouvoirs.
pub fn build_shell_tree(
    classes: &mut NodeClassSet,
    snap: &ShellSnapshot,
    journal: &[JournalRow],
    powers: &[PowerRow],
) -> TreeUpdate {
    let mut nodes: Vec<(NodeId, Node)> = Vec::new();

    // Racine : fenêtre Akora.
    nodes.push((
        id(0),
        node(
            classes,
            Role::Window,
            "Akora",
            &[],
            vec![id(1), id(2), id(3), id(4)],
        ),
    ));

    // 1. Présence : bouton avec label d'état (jamais couleur seule).
    nodes.push((
        id(1),
        node(
            classes,
            Role::Button,
            &snap.presence_a11y_label,
            &[Action::Focus, Action::Default],
            vec![],
        ),
    ));

    // 2. HUD : groupe prompt + réponse + validation éventuelle.
    let mut hud_kids = vec![id(20)];
    let mut prompt = NodeBuilder::default();
    prompt.set_role(Role::TextInput);
    prompt.set_name("Commande Akora — Entrée pour envoyer, Échap pour fermer");
    prompt.set_value(snap.hud_query.clone());
    prompt.add_action(Action::Focus);
    nodes.push((id(20), prompt.build(classes)));
    if !snap.hud_response.is_empty() {
        let mut resp = NodeBuilder::default();
        resp.set_role(Role::StaticText);
        resp.set_value(snap.hud_response.clone());
        nodes.push((id(21), resp.build(classes)));
        hud_kids.push(id(21));
    }
    if snap.hud_validation {
        nodes.push((
            id(22),
            node(
                classes,
                Role::Button,
                &format!(
                    "Valider : {} — {}",
                    snap.hud_validation_text, snap.hud_validation_cost
                ),
                &[Action::Focus, Action::Default],
                vec![],
            ),
        ));
        hud_kids.push(id(22));
    }
    nodes.push((
        id(2),
        node(
            classes,
            Role::Group,
            "Ligne de commande Akora",
            &[],
            hud_kids,
        ),
    ));

    // 3. Journal : liste, chaque ligne = item avec action + réversibilité.
    let mut kids = Vec::new();
    for (i, row) in journal.iter().take(50).enumerate() {
        let nid = id(100 + i as u64);
        nodes.push((
            nid,
            node(
                classes,
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
            classes,
            Role::List,
            "Journal d'audit — frise, annulation en un geste",
            &[],
            kids,
        ),
    ));

    // 4. Pouvoirs : boutons interrupteurs nommés avec état en toutes lettres.
    let mut pkids = Vec::new();
    for (i, p) in powers.iter().enumerate() {
        let nid = id(200 + i as u64);
        let mut acts = vec![Action::Focus];
        if !p.policy_locked {
            acts.push(Action::Default);
        }
        nodes.push((
            nid,
            node(
                classes,
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
    nodes.push((id(4), node(classes, Role::Group, "Pouvoirs", &[], pkids)));

    TreeUpdate {
        nodes,
        tree: Some(Tree::new(id(0))),
        focus: id(1),
    }
}

/// Tous les nœuds interactifs (Button/TextInput) ont-ils un label non vide ?
pub fn audit_interactive_labels(update: &TreeUpdate) -> Vec<NodeId> {
    update
        .nodes
        .iter()
        .filter(|(_, n)| {
            matches!(n.role(), Role::Button | Role::TextInput)
                && n.name().map(|l| l.is_empty()).unwrap_or(true)
        })
        .map(|(id, _)| *id)
        .collect()
}

/// Arbre d'une carte d'action (spec §8 + §17) : verbe + objet, raison +
/// lien Pourquoi, 3 pastilles (coût, données, réversibilité), bande de
/// provenance non éditable, Valider (braise) / Modifier / Refuser.
/// Irréversible → le label Valider annonce l'appui long 600 ms.
pub fn build_action_card_tree(classes: &mut NodeClassSet, card: &ActionCardView) -> TreeUpdate {
    let mut nodes: Vec<(NodeId, Node)> = Vec::new();
    // Texte statique : contenu dans `value` (convention AccessKit).
    let text = |classes: &mut NodeClassSet, value: String| {
        let mut b = NodeBuilder::default();
        b.set_role(Role::StaticText);
        b.set_value(value);
        b.build(classes)
    };
    nodes.push((
        id(0),
        node(
            classes,
            Role::Group,
            &format!("Carte d'action — {}", card.provenance_label),
            &[],
            vec![id(1), id(2), id(3), id(4)],
        ),
    ));
    nodes.push((
        id(1),
        text(classes, format!("{} {}", card.verb, card.object)),
    ));
    nodes.push((
        id(2),
        text(
            classes,
            format!("{} — Pourquoi ? (trace : {})", card.reason, card.why_trace),
        ),
    ));
    let pastilles = card.pastilles();
    nodes.push((
        id(3),
        text(
            classes,
            format!(
                "Coût : {} — Données : {} — {}",
                pastilles[0], pastilles[1], pastilles[2]
            ),
        ),
    ));
    let validate_label = if card.needs_hold {
        "Valider — appui long 600 millisecondes (action définitive)"
    } else {
        "Valider"
    };
    // Groupe d'actions : enfants Valider/Modifier/Refuser.
    nodes.push((
        id(4),
        node(
            classes,
            Role::Group,
            "Valider, Modifier, Refuser",
            &[],
            vec![id(40), id(41), id(42)],
        ),
    ));
    nodes.push((
        id(40),
        node(
            classes,
            Role::Button,
            validate_label,
            &[Action::Focus, Action::Default],
            vec![],
        ),
    ));
    nodes.push((
        id(41),
        node(
            classes,
            Role::Button,
            "Modifier",
            &[Action::Focus, Action::Default],
            vec![],
        ),
    ));
    nodes.push((
        id(42),
        node(
            classes,
            Role::Button,
            "Refuser",
            &[Action::Focus, Action::Default],
            vec![],
        ),
    ));
    // Marqueur externe : pastille dans le texte si contenu externe.
    if card.is_external {
        if let Some((_, v)) = nodes.iter_mut().find(|(i, _)| *i == id(1)) {
            *v = text(
                classes,
                format!("{} {} [externe — filet 2 px]", card.verb, card.object),
            );
        }
    }
    TreeUpdate {
        nodes,
        tree: Some(Tree::new(id(0))),
        focus: id(40),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use annora_ui::{CostCounter, EphemeralHud, PresenceIndicator, SilenceBudgetTracker};
    use annora_ui_shell::PowersView;

    fn sample_snapshot() -> ShellSnapshot {
        let presence = PresenceIndicator::new();
        let mut hud = EphemeralHud::new();
        hud.request_validation("w-1", "Envoi 25.00 EUR", Some(2500), "Financial");
        let cost = CostCounter::new(500, "EUR");
        let silence = SilenceBudgetTracker::new(3);
        ShellSnapshot::from_states(&presence, &hud, &cost, &silence, 0, 1)
    }

    #[test]
    fn test_s1_tree_complete_labels_and_click() {
        let mut classes = NodeClassSet::new();
        let snap = sample_snapshot();
        let powers =
            PowersView::from_manifest(&["Camera".to_string()], &["FullSystem".to_string()]);
        let update = build_shell_tree(&mut classes, &snap, &[], &powers.rows);
        // Zéro interactif sans label : parcours lecteur d'écran complet.
        assert!(audit_interactive_labels(&update).is_empty());
        // Le bouton de validation porte le coût dans son label.
        let valid = update.nodes.iter().find(|(i, _)| *i == id(22)).unwrap();
        assert!(valid.1.name().unwrap().contains("25.00"));
        // Focus toujours défini (ici la Présence).
        assert_eq!(update.focus, id(1));
        // Pouvoir verrouillé : nommé coupé + verrouillé (pas d'action Default).
        let locked = update
            .nodes
            .iter()
            .find(|(_, n)| n.name().map(|l| l.contains("verrouillé")).unwrap_or(false))
            .unwrap();
        assert!(locked.1.name().unwrap().contains("coupé"));
    }

    #[test]
    fn test_s1_action_card_tree_pastilles_and_hold() {
        use annora_design::{ActionCardView, DataPastille, Reversibility};
        let mut classes = NodeClassSet::new();
        let card = ActionCardView {
            verb: "Envoyer".into(),
            object: "ce mail à Awa".into(),
            reason: "Tu as demandé de répondre.".into(),
            why_trace: "Mail cran 1".into(),
            cost_label: "0 €".into(),
            data: DataPastille::Local,
            reversibility: Reversibility::Undoable { undo_seconds: 30 },
            needs_hold: false,
            provenance_label: "Akora".into(),
            is_external: false,
        };
        let update = build_action_card_tree(&mut classes, &card);
        assert!(audit_interactive_labels(&update).is_empty());
        // Verbe + objet lisibles, pastilles dans l'ordre.
        let title = update.nodes.iter().find(|(i, _)| *i == id(1)).unwrap();
        assert!(title.1.value().unwrap().contains("Awa"));
        let past = update.nodes.iter().find(|(i, _)| *i == id(3)).unwrap();
        let pv = past.1.value().unwrap();
        assert!(pv.find("0 €").unwrap() < pv.find("Local").unwrap());
        // Irréversible → Valider annonce l'appui long 600 ms.
        let mut hard = card.clone();
        hard.needs_hold = true;
        hard.reversibility = Reversibility::Final;
        let update2 = build_action_card_tree(&mut classes, &hard);
        let v = update2.nodes.iter().find(|(i, _)| *i == id(40)).unwrap();
        assert!(v.1.name().unwrap().contains("600"));
        // Externe → filet + pastille dans le texte.
        let mut ext = card.clone();
        ext.is_external = true;
        let update3 = build_action_card_tree(&mut classes, &ext);
        let t = update3.nodes.iter().find(|(i, _)| *i == id(1)).unwrap();
        assert!(t.1.value().unwrap().contains("[externe"));
    }
}
