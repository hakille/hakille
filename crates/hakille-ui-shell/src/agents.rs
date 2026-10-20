//! Cartes agents pour le shell : 5 agents de référence, honnêtes.
//!
//! Chaque carte expose : nom, capabilities requises, ce qui est réel
//! (parsers/index locaux) vs démo (SMTP/CalDAV/MQTT/banque), et si une
//! validation Coral est exigée avant effet.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub id: String,
    pub name: String,
    pub required_caps: Vec<String>,
    pub real: String,
    pub demo: String,
    pub needs_validation: bool,
    pub badge: String,
}

pub fn reference_agent_cards() -> Vec<AgentCard> {
    vec![
        AgentCard {
            id: "agent-mail".into(),
            name: "Mail Agent".into(),
            required_caps: vec!["Network(Read)".into(), "Filesystem(Read)".into()],
            real: "Dossiers/recherche/brouillons locaux + parse RFC5322".into(),
            demo: "Boîte seed 2 messages — SMTP simulé (dossier Sent local)".into(),
            needs_validation: true,
            badge: "DÉMO LOCALE".into(),
        },
        AgentCard {
            id: "agent-calendar".into(),
            name: "Calendar Agent".into(),
            required_caps: vec!["Filesystem(Read+Write)".into()],
            real: "Ajout + conflits overlap + ICS parse/export + sync graphe".into(),
            demo: "2 events seed — pas de CalDAV/Google".into(),
            needs_validation: false,
            badge: "AGENDA LOCAL + ICS".into(),
        },
        AgentCard {
            id: "agent-code".into(),
            name: "Code Agent".into(),
            required_caps: vec!["Filesystem(Read+Write)".into()],
            real: "Index symboles Rust/Python/TS + patch unifié (refuse si divergé)".into(),
            demo: "Pas d'exécution/build/lint".into(),
            needs_validation: false,
            badge: "INDEX + PATCH LOCAL".into(),
        },
        AgentCard {
            id: "agent-home".into(),
            name: "Smart Home Agent".into(),
            required_caps: vec!["Network(Read+Write)".into(), "Sensor(Read)".into()],
            real: "Discovery + on/off + brightness clamp + température + lock".into(),
            demo: "4 devices mock — pas de MQTT/Zigbee ; unlock exige validation".into(),
            needs_validation: true,
            badge: "DOMOTIQUE SIMULÉE".into(),
        },
        AgentCard {
            id: "agent-wallet-readonly".into(),
            name: "Wallet ReadOnly".into(),
            required_caps: vec!["Wallet(Read)".into()],
            real: "Comptes/relevés/budget/recherche sur 2 comptes fictifs".into(),
            demo: "Virements toujours Err (invariant dur) — lecture seule".into(),
            needs_validation: false,
            badge: "LECTURE SEULE".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_five_cards_all_badged_honest() {
        let cards = reference_agent_cards();
        assert_eq!(cards.len(), 5);
        for c in &cards {
            assert!(!c.badge.is_empty());
            assert!(!c.required_caps.is_empty());
        }
        let wallet = cards
            .iter()
            .find(|c| c.id == "agent-wallet-readonly")
            .unwrap();
        assert_eq!(wallet.badge, "LECTURE SEULE");
        let mail = cards.iter().find(|c| c.id == "agent-mail").unwrap();
        assert!(mail.needs_validation);
    }
}
