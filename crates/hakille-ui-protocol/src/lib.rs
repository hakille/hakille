//! Protocole UI ↔ noyau, versionné (spec §22).
//!
//! Même schéma pour natif, UniFFI (SwiftUI/Compose) et WASM.
//! L'interface ne transmet JAMAIS la durée d'appui long : le noyau la mesure
//! par le chemin d'entrée de confiance. `Approve` n'est qu'une demande.

use serde::{Deserialize, Serialize};

/// Version du protocole. Toute rupture incrémente et garde un test de compat.
pub const PROTOCOL_VERSION: u16 = 1;

/// Négociation de version : `true` ssi la version distante est supportée.
/// Aujourd'hui seule v1 est acceptée ; v0 (et futures v>1) sont rejetées
/// proprement via `UiMessage::check` / `KernelMessage::check` (Err, pas panic).
pub fn compatible(v: u32) -> bool {
    v == PROTOCOL_VERSION as u32
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PresenceState {
    Idle,
    Listening,
    Acting,
    Speaking,
    DecisionRequired,
    Urgent,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum DataExit {
    Local,
    Cloud { provider: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Provenance {
    Hakille,
    VerifiedSkill { publisher: String, name: String },
    External,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Money {
    /// Micro-unités (1 EUR = 1_000_000). Chiffres tabulaires côté rendu.
    pub micros: i64,
    pub currency: String,
}

impl Money {
    pub fn from_cents(cents: u64, currency: &str) -> Self {
        Self {
            micros: cents as i64 * 10_000,
            currency: currency.to_string(),
        }
    }

    pub fn display(&self) -> String {
        format!("{:.2} {}", self.micros as f64 / 1_000_000.0, self.currency)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ActionCard {
    pub id: u64,
    pub verb: String,
    pub object: String,
    pub reason: String,
    pub cost: Money,
    pub data_exit: DataExit,
    pub reversible: bool,
    pub provenance: Provenance,
}

/// Entrée de journal d'audit exposée au HUD (résumé lisible ; jamais la
/// trace complète ni de secret).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct JournalEntryDto {
    /// Identifiant de l'entrée (UUID) — pourra servir à un futur `Undo`.
    pub id: String,
    /// Horodatage RFC 3339 (UTC).
    pub timestamp: String,
    /// Type d'action journalisée (ex. `CognitiveTurn`).
    pub action: String,
    /// Résumé humain de l'entrée (description signée).
    pub summary: String,
}

/// Ligne de mémoire souveraine exposée au HUD (vue `N dernières` ou recall).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MemoryRowDto {
    /// Identifiant du triplet (UUID) — vide si la ligne n'est pas adressable.
    pub id: String,
    /// Représentation texte réelle du fait.
    pub text: String,
    /// Étiquettes catégorielles (partition, niveau de confiance).
    pub tags: Vec<String>,
    /// Source du fait (`User`, `System`, `Inferred`, `External`).
    pub source: String,
}

/// Statut minimal du démon — uniquement des champs réels, jamais de succès
/// simulé (pas de booléen « modèle chargé » inventé faute d'API).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StatusDto {
    /// Modèle configuré (étiquette honnête, timeout réel).
    pub model: String,
    /// Coût cumulé formaté.
    pub cost: String,
    /// État de présence courant.
    pub state: String,
    /// Nombre de triplets en mémoire souveraine.
    pub memory_count: u64,
}

/// Description d'un agent exposable au HUD.
///
/// `artifact_present` = `true` ssi un binaire WASM exécutable est présent et
/// vérifié (SHA-256). Un agent déclaratif est listé avec
/// `artifact_present = false` : jamais de faux « opérationnel ».
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentDto {
    pub id: String,
    pub name: String,
    /// Scopes déclarés par l'agent (jamais présentés comme appliqués).
    pub required_caps: Vec<String>,
    /// `true` ssi l'agent est RÉELLEMENT exécutable (binaire vérifié).
    pub artifact_present: bool,
    /// SHA-256 du binaire (vide si l'agent n'a pas de binaire).
    pub sha256: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum UiToKernel {
    Approve {
        card_id: u64,
    },
    Edit {
        card_id: u64,
    },
    Reject {
        card_id: u64,
    },
    Undo {
        journal_id: u64,
    },
    RevokeAll,
    /// Requête texte libre du HUD → noyau (routage System1/System2).
    HudSubmit {
        query: String,
    },
    /// Bascule d'un réglage persisté (le noyau valide et journalise).
    SettingsToggle {
        index: usize,
    },
    /// Oubli mémoire : reçu cryptographique exigé côté noyau.
    MemoryForget {
        row_id: String,
    },
    /// Vidage groupé de l'outbox (le noyau applique les quotas).
    OutboxFlush,
    /// Consultation additive v1 : `limit` dernières entrées d'audit.
    JournalRequest {
        limit: usize,
    },
    /// Consultation additive v1 : `query: None` = `limit` dernières lignes
    /// mémoire (dernière activation décroissante) ; `Some(q)` = recall.
    MemoryRequest {
        query: Option<String>,
        limit: usize,
    },
    /// Mémorisation additive v1 d'un fait en texte libre (accusé = id réel).
    MemoryRemember {
        text: String,
    },
    /// Statut minimal du démon (modèle, coût, état).
    StatusRequest,
    /// Liste des agents exposés par le démon (additif v1).
    AgentListRequest,
    /// Exécution d'un agent WASM réel (additif v1) : entrée JSON → sortie
    /// JSON. Le démon refuse tout agent sans binaire exécutable.
    AgentRunRequest {
        name: String,
        input_json: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum KernelToUi {
    Presence(PresenceState),
    ShowCard(ActionCard),
    JournalAppended {
        id: u64,
    },
    /// Réponse HUD ; `validation` = (description, coût en centimes).
    HudReply {
        response: String,
        validation: Option<(String, u64)>,
    },
    /// Message transitoire honnête (erreur, refus, information).
    Toast {
        text: String,
    },
    /// Réponse `JournalRequest` : entrées réelles, plus récentes d'abord.
    JournalReply {
        entries: Vec<JournalEntryDto>,
    },
    /// Réponse `MemoryRequest` : lignes réelles du graphe souverain.
    MemoryReply {
        rows: Vec<MemoryRowDto>,
    },
    /// Accusé `MemoryRemember` : identifiant du triplet inséré (ou remplaçant).
    MemoryRememberAck {
        id: String,
    },
    /// Réponse `StatusRequest` : statut réel minimal.
    StatusReply {
        status: StatusDto,
    },
    /// Réponse `AgentListRequest` : agents exposés par le démon.
    ///
    /// HONNÊTETÉ : `artifact_present` distingue un agent RÉELLEMENT exécutable
    /// (binaire WASM vérifié) d'un manifeste déclaratif. Aucun statut
    /// « opérationnel » n'est annoncé pour un agent sans binaire.
    AgentListReply {
        agents: Vec<AgentDto>,
    },
    /// Réponse `AgentRunRequest` : exécution réelle dans le bac à sable.
    ///
    /// Les empreintes SHA-256 portent sur l'entrée et la sortie : le journal
    /// peut prouver CE QUI a été exécuté sans conserver la charge utile.
    AgentRunReply {
        name: String,
        output_json: String,
        duration_ms: u64,
        input_sha256: String,
        output_sha256: String,
    },
    /// Accusé d'une décision de carte : la décision a été auditée avant
    /// l'accusé. `audit_id` est l'identifiant réel de l'entrée de journal.
    /// `effect` décrit l'effet RÉEL produit par la décision (ex. mise en file
    /// durable) ; `None` = aucun effet. Aucun effet métier n'est impliqué
    /// par ce message : le champ ne fait que rapporter ce que le noyau a
    /// réellement fait, jamais une promesse.
    CardDecisionAck {
        card_id: u64,
        approved: bool,
        audit_id: String,
        #[serde(default)]
        effect: Option<String>,
    },
}

/// Message UI → noyau versionné : champ `version` transporté sur le fil
/// (natif, UniFFI, WASM). Construire via `::new` (tamponne v1 courante) ;
/// valider via `check` (rejet propre des versions incompatibles).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UiMessage {
    pub version: u16,
    pub inner: UiToKernel,
}

/// Message noyau → UI versionné (même contrat que `UiMessage`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct KernelMessage {
    pub version: u16,
    pub inner: KernelToUi,
}

impl UiToKernel {
    /// Version du protocole portée par ce message.
    pub fn version(&self) -> u16 {
        PROTOCOL_VERSION
    }

    /// Enveloppe versionnée prête à sérialiser.
    pub fn into_message(self) -> UiMessage {
        UiMessage::new(self)
    }
}

impl KernelToUi {
    /// Version du protocole portée par ce message.
    pub fn version(&self) -> u16 {
        PROTOCOL_VERSION
    }

    /// Enveloppe versionnée prête à sérialiser.
    pub fn into_message(self) -> KernelMessage {
        KernelMessage::new(self)
    }
}

impl UiMessage {
    pub fn new(inner: UiToKernel) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            inner,
        }
    }

    /// Rejet propre : `Err` si `version` incompatible (ex. v0 → v1).
    pub fn check(&self) -> Result<(), String> {
        if compatible(self.version as u32) {
            Ok(())
        } else {
            Err(format!(
                "version protocole incompatible : got v{}, supportée v{PROTOCOL_VERSION}",
                self.version
            ))
        }
    }
}

impl KernelMessage {
    pub fn new(inner: KernelToUi) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            inner,
        }
    }

    /// Rejet propre : `Err` si `version` incompatible (ex. v0 → v1).
    pub fn check(&self) -> Result<(), String> {
        if compatible(self.version as u32) {
            Ok(())
        } else {
            Err(format!(
                "version protocole incompatible : got v{}, supportée v{PROTOCOL_VERSION}",
                self.version
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version_pinned() {
        assert_eq!(PROTOCOL_VERSION, 1);
    }

    #[test]
    fn test_extended_variants_roundtrip_v1() {
        // Extension additive v1 : HUD, réglages, mémoire, outbox, toast.
        let cases = [
            UiToKernel::HudSubmit {
                query: "résume ma journée".into(),
            },
            UiToKernel::SettingsToggle { index: 2 },
            UiToKernel::MemoryForget {
                row_id: "m-42".into(),
            },
            UiToKernel::OutboxFlush,
        ];
        for m in cases {
            let msg = UiMessage::new(m.clone());
            assert!(msg.check().is_ok());
            let json = serde_json::to_string(&msg).unwrap();
            let back: UiMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(back.inner, m);
            assert!(back.check().is_ok());
        }
        let replies = [
            KernelToUi::HudReply {
                response: "Journée calme.".into(),
                validation: None,
            },
            KernelToUi::HudReply {
                response: String::new(),
                validation: Some(("Envoi 25.00 EUR".into(), 2500)),
            },
            KernelToUi::Toast {
                text: "nécessite daemon".into(),
            },
            KernelToUi::Presence(PresenceState::DecisionRequired),
        ];
        for r in replies {
            let msg = KernelMessage::new(r.clone());
            assert!(msg.check().is_ok());
            let json = serde_json::to_string(&msg).unwrap();
            let back: KernelMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(back.inner, r);
        }
    }

    #[test]
    fn test_agents_extension_roundtrip_v1() {
        // Extension additive v1 (agents) : liste + exécution réelle.
        // La version reste 1, le schéma golden existant ne change pas.
        let requests = [
            UiToKernel::AgentListRequest,
            UiToKernel::AgentRunRequest {
                name: "echo-agent".into(),
                input_json: "{\"msg\":\"bonjour\"}".into(),
            },
        ];
        for m in requests {
            let msg = UiMessage::new(m.clone());
            assert!(msg.check().is_ok());
            let json = serde_json::to_string(&msg).unwrap();
            let back: UiMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(back.inner, m);
            assert!(back.check().is_ok());
        }
        let replies = [
            KernelToUi::AgentListReply {
                agents: vec![AgentDto {
                    id: "echo-agent".into(),
                    name: "echo-agent".into(),
                    required_caps: vec![],
                    artifact_present: true,
                    sha256: "a".repeat(64),
                }],
            },
            KernelToUi::AgentRunReply {
                name: "echo-agent".into(),
                output_json: "{\"echo\":\"bonjour\"}".into(),
                duration_ms: 7,
                input_sha256: "b".repeat(64),
                output_sha256: "c".repeat(64),
            },
        ];
        for r in replies {
            let msg = KernelMessage::new(r.clone());
            assert!(msg.check().is_ok());
            let json = serde_json::to_string(&msg).unwrap();
            let back: KernelMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(back.inner, r);
        }
    }

    #[test]
    fn test_consultation_extension_roundtrip_v1() {
        // Extension additive v1 (consultation) : journal, mémoire, remember,
        // statut — la version reste 1, le schéma existant ne change pas.
        let requests = [
            UiToKernel::JournalRequest { limit: 20 },
            UiToKernel::MemoryRequest {
                query: None,
                limit: 10,
            },
            UiToKernel::MemoryRequest {
                query: Some("jazz".into()),
                limit: 5,
            },
            UiToKernel::MemoryRemember {
                text: "Marie aime le jazz".into(),
            },
            UiToKernel::StatusRequest,
        ];
        for m in requests {
            let msg = UiMessage::new(m.clone());
            assert!(msg.check().is_ok());
            let json = serde_json::to_string(&msg).unwrap();
            let back: UiMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(back.inner, m);
            assert!(back.check().is_ok());
        }
        let replies = [
            KernelToUi::JournalReply {
                entries: vec![JournalEntryDto {
                    id: "018f0000-0000-7000-8000-000000000001".into(),
                    timestamp: "2026-10-19T09:00:00+00:00".into(),
                    action: "CognitiveTurn".into(),
                    summary: "tour servi".into(),
                }],
            },
            KernelToUi::MemoryReply {
                rows: vec![MemoryRowDto {
                    id: "018f0000-0000-7000-8000-000000000002".into(),
                    text: "Marie aime le jazz".into(),
                    tags: vec!["Personal".into(), "User".into()],
                    source: "User".into(),
                }],
            },
            KernelToUi::MemoryRememberAck {
                id: "018f0000-0000-7000-8000-000000000003".into(),
            },
            KernelToUi::StatusReply {
                status: StatusDto {
                    model: "Qwen 0.5B Q4_K_M (timeout 30000 ms)".into(),
                    cost: "0.00 EUR".into(),
                    state: "Idle".into(),
                    memory_count: 3,
                },
            },
            KernelToUi::CardDecisionAck {
                card_id: 7,
                approved: true,
                audit_id: "018f0000-0000-7000-8000-000000000004".into(),
                effect: Some("action mise en file".into()),
            },
        ];
        for r in replies {
            let msg = KernelMessage::new(r.clone());
            assert!(msg.check().is_ok());
            let json = serde_json::to_string(&msg).unwrap();
            let back: KernelMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(back.inner, r);
        }
    }

    #[test]
    fn test_extended_messages_still_reject_v0() {
        let stale = UiMessage {
            version: 0,
            inner: UiToKernel::HudSubmit { query: "x".into() },
        };
        assert!(stale.check().is_err());
    }

    #[test]
    fn test_card_roundtrip_json() {
        let card = ActionCard {
            id: 7,
            verb: "Envoyer".into(),
            object: "ce mail à Awa".into(),
            reason: "Tu as demandé de répondre.".into(),
            cost: Money::from_cents(0, "EUR"),
            data_exit: DataExit::Local,
            reversible: true,
            provenance: Provenance::VerifiedSkill {
                publisher: "Hakille".into(),
                name: "mail".into(),
            },
        };
        let json = serde_json::to_string(&card).unwrap();
        let back: ActionCard = serde_json::from_str(&json).unwrap();
        assert_eq!(card, back);
        assert_eq!(card.cost.display(), "0.00 EUR");
    }

    #[test]
    fn test_messages_roundtrip() {
        let m = UiToKernel::Approve { card_id: 3 };
        let json = serde_json::to_string(&m).unwrap();
        // Pas de durée d'appui long dans le message : le noyau mesure.
        assert!(!json.contains("hold") && !json.contains("duration"));
        let back: UiToKernel = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn test_schema_snapshot_stable() {
        // Verrouille la compat UniFFI/WASM future : tout changement de schéma
        // doit mettre à jour le golden file + PROTOCOL_VERSION.
        let card = ActionCard {
            id: 7,
            verb: "Envoyer".into(),
            object: "ce mail à Awa".into(),
            reason: "Tu as demandé de répondre.".into(),
            cost: Money::from_cents(0, "EUR"),
            data_exit: DataExit::Local,
            reversible: true,
            provenance: Provenance::VerifiedSkill {
                publisher: "Hakille".into(),
                name: "mail".into(),
            },
        };
        let msg = UiToKernel::Approve { card_id: 3 };
        let actual = serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "action_card": serde_json::to_value(&card).unwrap(),
            "ui_to_kernel": serde_json::to_value(&msg).unwrap(),
        });
        let golden_str = include_str!("schema_snapshot.json");
        let golden: serde_json::Value = serde_json::from_str(golden_str).unwrap();
        assert_eq!(
            actual, golden,
            "schéma protocole dérivé — mettre à jour schema_snapshot.json + PROTOCOL_VERSION"
        );
    }

    #[test]
    fn test_compatible_v1_only() {
        assert!(compatible(1));
        assert!(!compatible(0));
        assert!(!compatible(2));
        assert!(!compatible(999));
    }

    #[test]
    fn test_versioned_messages_v0_rejected_proprement() {
        // v1 courante acceptée.
        let ok = UiToKernel::Approve { card_id: 3 }.into_message();
        assert_eq!(ok.version, PROTOCOL_VERSION);
        assert_eq!(ok.inner.version(), PROTOCOL_VERSION);
        assert!(ok.check().is_ok());
        // v0 → v1 rejetée proprement (Err explicite, pas panic).
        let stale = UiMessage {
            version: 0,
            inner: UiToKernel::Approve { card_id: 3 },
        };
        assert!(!compatible(stale.version as u32));
        let err = stale.check().expect_err("v0 doit être rejetée");
        assert!(err.contains("incompatible") && err.contains('0'), "{err}");
        // Roundtrip JSON conserve le champ version.
        let json = serde_json::to_string(&stale).unwrap();
        assert!(json.contains("\"version\":0"), "{json}");
        let back: UiMessage = serde_json::from_str(&json).unwrap();
        assert!(back.check().is_err());

        let kok = KernelToUi::JournalAppended { id: 9 }.into_message();
        assert!(kok.check().is_ok());
        let kstale = KernelMessage {
            version: 0,
            inner: KernelToUi::Presence(PresenceState::Idle),
        };
        assert!(kstale.check().is_err());
    }
}
