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

// ============================================================================
// DTOs Perceptuels & Cognitifs Multimodaux (M1 Additif v1)
// ============================================================================

/// Masques de canaux perceptuels pour `PerceptionControl`.
pub const PERCEPTION_CHANNEL_VISION: u32 = 1 << 0;     // 0x01
pub const PERCEPTION_CHANNEL_AUDIO: u32 = 1 << 1;      // 0x02
pub const PERCEPTION_CHANNEL_EMOTION: u32 = 1 << 2;    // 0x04
pub const PERCEPTION_CHANNEL_POSTURE: u32 = 1 << 3;    // 0x08
pub const PERCEPTION_CHANNEL_FATIGUE: u32 = 1 << 4;    // 0x10
pub const PERCEPTION_CHANNEL_GAZE: u32 = 1 << 5;       // 0x20
pub const PERCEPTION_CHANNEL_ALL: u32 = 0x3F;

/// État qualifié de la présence physique de l'utilisateur face au poste.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserPresenceKind {
    /// Utilisateur présent et attentif en face de l'écran.
    Present,
    /// Utilisateur absent du champ visuel (> 3 secondes d'absence confirmée).
    Absent,
    /// Utilisateur physiquement présent mais distrait (regard fuyant, tête tournée > 75°).
    Distracted,
    /// Alerte de confidentialité : plusieurs personnes détectées (shoulder surfing).
    MultiplePeople,
}

/// DTO rapportant l'état de présence physique de l'utilisateur.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UserPresenceDto {
    /// Catégorie de présence.
    pub kind: UserPresenceKind,
    /// Confiance normalisée de la détection [0.0, 1.0].
    pub confidence: f32,
    /// Distance estimée en mètres (par annulation focale IPD ou sténopé).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance_meters: Option<f32>,
    /// Nombre de visages / silhouettes détectés dans le champ visuel.
    pub people_count: u32,
    /// Horodatage Unix en millisecondes.
    pub timestamp_ms: u64,
}

/// Catégorie qualifiée du niveau d'attention.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttentionKind {
    /// Regard convergent fixé sur l'écran actif (GazeRay intersecting screen).
    Focused,
    /// Attention partagée (regard mobile, clignements réguliers).
    Divided,
    /// Utilisateur distrait (regard hors écran, pitch/yaw dévié).
    Distracted,
    /// Absence d'attention ou yeux fermés.
    Absent,
}

/// DTO rapportant l'attention et le regard de l'utilisateur.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AttentionLevelDto {
    /// Catégorie d'attention.
    pub level: AttentionKind,
    /// Score continu d'attention [0.0, 1.0].
    pub score: f32,
    /// `true` si le regard converge sur la surface de l'écran.
    pub gaze_on_screen: bool,
    /// Durée de fixation continue sur l'écran en millisecondes.
    pub fixation_duration_ms: u64,
    /// Horodatage Unix en millisecondes.
    pub timestamp_ms: u64,
}

/// Classification ergonomique de la posture corporelle de l'utilisateur.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostureKindDto {
    /// Buste droit, colonne alignée.
    Upright,
    /// Buste affalé / dos voûté.
    Slouched,
    /// Buste penché vers l'avant (concentration ou fatigue visuelle).
    LeaningForward,
    /// Buste adossé / incliné vers l'arrière (relaxation).
    Reclined,
    /// Posture debout.
    Standing,
    /// Posture assise standard.
    Seated,
    /// Posture indéterminée (occlusion ou cadrage serré).
    Unknown,
}

/// DTO rapportant la posture physique et les métriques ergonomiques.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PostureStatusDto {
    /// Posture dominante classifiée.
    pub posture: PostureKindDto,
    /// `true` si l'utilisateur est détecté en position assise.
    pub is_sitting: bool,
    /// Angle d'inclinaison de la colonne vertébrale en degrés (0° = vertical droit).
    pub spine_inclination_deg: f32,
    /// Taille / stature physique estimée en centimètres (via annulation focale IPD 63 mm et ratio vitruvien 52%).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_height_cm: Option<f32>,
    /// Indice de confiance de la pose corporelle [0.0, 1.0].
    pub confidence: f32,
    /// Horodatage Unix en millisecondes.
    pub timestamp_ms: u64,
}

/// Sévérité de l'alerte de fatigue.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FatigueSeverity {
    /// Fatigue légère.
    Mild,
    /// Fatigue modérée (PERCLOS 10-15%).
    Moderate,
    /// Somnolence critique (PERCLOS > 15%, micro-sommeils imminents).
    Critical,
}

/// DTO d'alerte de fatigue émis pour action préventive.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FatigueAlertEvent {
    /// Niveau de sévérité.
    pub severity: FatigueSeverity,
    /// PERCLOS P80 calculé sur fenêtre glissante de 60 secondes [0.0, 1.0].
    pub perclos_p80: f32,
    /// Eye Aspect Ratio (EAR) instantané.
    pub ear: f32,
    /// Mouth Aspect Ratio (MAR) instantané pour détection de bâillement.
    pub mar: f32,
    /// Nombre de bâillements récents détectés.
    pub yawn_count: u32,
    /// Recommandation contextuelle (ex. "Pause conseillée").
    pub recommendation: String,
    /// Horodatage Unix en millisecondes.
    pub timestamp_ms: u64,
}

/// DTO rapportant les mises à jour affectives et émotionnelles.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EmotionUpdateEvent {
    /// Émotion dominante (ex. "Neutral", "Happy", "Sad", "Surprised", "Anger", "Disgust", "Fear", "Contempt").
    pub primary_emotion: String,
    /// Indice de confiance normalisé [0.0, 1.0].
    pub confidence: f32,
    /// Valence continue circumplex de Russell [-1.0, 1.0].
    pub valence: f32,
    /// Arousal continu circumplex de Russell [-1.0, 1.0].
    pub arousal: f32,
    /// Source multimodale de l'inférence : "Facial", "Vocal", "MultimodalFusion".
    pub source: String,
    /// Balises acoustiques optionnelles SenseVoice (ex. ["Laughter", "Applause"]).
    pub acoustic_tags: Vec<String>,
    /// Horodatage Unix en millisecondes.
    pub timestamp_ms: u64,
}

/// Déclencheur d'un événement de garde de souveraineté et vie privée.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PrivacyGuardTrigger {
    /// Détection de tiers regardant l'écran (shoulder surfing).
    ShoulderSurfingDetected { people_count: u32 },
    /// Tentative bloquée d'exfiltration de données brutes sans capability.
    RawDataExfiltrationBlocked { blocked_scope: String },
    /// Coupure matérielle (interrupteur physique caméra ou microphone coupé).
    HardwareKillSwitchActivated { device: String },
    /// Purge physique des données biométriques éphémères (TTL RAM expiré).
    EphemeralTtlPurged,
}

/// DTO émis lors de l'activation d'une protection de vie privée.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PrivacyGuardEvent {
    /// Déclencheur de l'alerte.
    pub trigger: PrivacyGuardTrigger,
    /// Description explicite de l'événement.
    pub description: String,
    /// Action préventive appliquée ou recommandée (ex. "Floutage écran activé").
    pub recommended_action: String,
    /// Horodatage Unix en millisecondes.
    pub timestamp_ms: u64,
}

/// Synthèse unifiée et cohérente du contexte utilisateur.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UserContextEvent {
    /// État de présence physique de l'utilisateur.
    pub presence: UserPresenceDto,
    /// Attention et regard.
    pub attention: AttentionLevelDto,
    /// Posture et ergonomie corporelle.
    pub posture: PostureStatusDto,
    /// Âge estimé par inférence faciale continue (SSR-Net).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_age: Option<f32>,
    /// Genre estimé par inférence faciale ("Male", "Female").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_gender: Option<String>,
    /// Émotion dominante actuelle.
    pub primary_emotion: String,
    /// Valence circumplex courante [-1.0, 1.0].
    pub valence: f32,
    /// Arousal circumplex courant [-1.0, 1.0].
    pub arousal: f32,
    /// Indice de fatigue global [0.0, 1.0] (PERCLOS P80).
    pub fatigue_index: f32,
    /// Horodatage Unix en millisecondes.
    pub timestamp_ms: u64,
}

/// Action interactive rattachée à une notification (SMNF).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct NotificationActionDto {
    pub id: String,
    pub label: String,
    pub is_destructive: bool,
    pub is_braise: bool,
}

/// DTO unifié pour les notifications éphémères et alertes système (SMNF).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NotificationDto {
    pub id: String,
    pub title: String,
    pub body: String,
    pub urgency: String,
    pub category: String,
    pub actions: Vec<NotificationActionDto>,
    pub timeout_ms: Option<u32>,
    pub is_braise: bool,
    pub visual_style: String,
    pub created_at_ms: u64,
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
    /// Commande de pilotage de la perception multimodale et souveraineté matérielle (M1 additif v1).
    PerceptionControl {
        hardware_camera_enabled: bool,
        hardware_mic_enabled: bool,
        cognitive_inference_enabled: bool,
        active_channels_mask: u32,
    },
    /// Action interactive cliquée sur une notification (SMNF).
    NotificationAction {
        id: String,
        action_id: String,
    },
    /// Notification fermée ou acquittée par l'utilisateur (SMNF).
    NotificationDismiss {
        id: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
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
    /// Delta de token : la réponse en cours de génération (additif v1).
    ///
    /// `text` est le MEILLEUR EFFORT d'affichage (décodage incrémental) ; le
    /// texte final de `HudReply` reste la référence. Le HUD doit donc
    /// REMPLACER son texte cumulé par la réponse finale quand elle arrive.
    TokenDelta {
        text: String,
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
    /// Contexte utilisateur multimodal complet unifié (M1 additif v1).
    UserContext(UserContextEvent),
    /// Mise à jour affective / émotionnelle (M1 additif v1).
    EmotionUpdate(EmotionUpdateEvent),
    /// Alerte de fatigue et somnolence pour ActionCard ou notification (M1 additif v1).
    FatigueAlert(FatigueAlertEvent),
    /// Alerte de garde de vie privée et souveraineté matérielle (M1 additif v1).
    PrivacyGuardAlert(PrivacyGuardEvent),
    /// Notification unifiée émise vers l'interface ou le toast HUD (SMNF).
    Notification(NotificationDto),
    /// Notification fermée ou révoquée (SMNF).
    NotificationDismissed { id: String },
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
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
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

    #[test]
    fn test_perception_control_roundtrip_v1() {
        let cmd = UiToKernel::PerceptionControl {
            hardware_camera_enabled: true,
            hardware_mic_enabled: false,
            cognitive_inference_enabled: true,
            active_channels_mask: PERCEPTION_CHANNEL_VISION | PERCEPTION_CHANNEL_EMOTION,
        };
        let msg = UiMessage::new(cmd.clone());
        assert!(msg.check().is_ok());
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"PerceptionControl\""));
        assert!(json.contains("\"hardware_camera_enabled\":true"));
        assert!(json.contains("\"hardware_mic_enabled\":false"));
        assert!(json.contains("\"cognitive_inference_enabled\":true"));
        let back: UiMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.inner, cmd);
        assert!(back.check().is_ok());
    }

    #[test]
    fn test_cognitive_events_roundtrip_v1() {
        let presence = UserPresenceDto {
            kind: UserPresenceKind::Present,
            confidence: 0.98,
            distance_meters: Some(0.65),
            people_count: 1,
            timestamp_ms: 1794078900000,
        };
        let attention = AttentionLevelDto {
            level: AttentionKind::Focused,
            score: 0.95,
            gaze_on_screen: true,
            fixation_duration_ms: 4500,
            timestamp_ms: 1794078900000,
        };
        let posture = PostureStatusDto {
            posture: PostureKindDto::Upright,
            is_sitting: true,
            spine_inclination_deg: 4.2,
            estimated_height_cm: Some(178.5),
            confidence: 0.92,
            timestamp_ms: 1794078900000,
        };
        let ctx = UserContextEvent {
            presence,
            attention,
            posture,
            estimated_age: Some(34.5),
            estimated_gender: Some("Male".into()),
            primary_emotion: "Neutral".into(),
            valence: 0.15,
            arousal: -0.05,
            fatigue_index: 0.04,
            timestamp_ms: 1794078900000,
        };
        let emo = EmotionUpdateEvent {
            primary_emotion: "Happy".into(),
            confidence: 0.89,
            valence: 0.75,
            arousal: 0.40,
            source: "MultimodalFusion".into(),
            acoustic_tags: vec!["Laughter".into()],
            timestamp_ms: 1794078900000,
        };
        let fatigue = FatigueAlertEvent {
            severity: FatigueSeverity::Moderate,
            perclos_p80: 0.14,
            ear: 0.18,
            mar: 0.65,
            yawn_count: 3,
            recommendation: "Pause conseillée".into(),
            timestamp_ms: 1794078900000,
        };
        let guard = PrivacyGuardEvent {
            trigger: PrivacyGuardTrigger::ShoulderSurfingDetected { people_count: 2 },
            description: "Deuxième personne détectée dans le champ visuel".into(),
            recommended_action: "Floutage des cartes confidentielles activé".into(),
            timestamp_ms: 1794078900000,
        };

        let cases = [
            KernelToUi::UserContext(ctx),
            KernelToUi::EmotionUpdate(emo),
            KernelToUi::FatigueAlert(fatigue),
            KernelToUi::PrivacyGuardAlert(guard),
        ];

        for r in cases {
            let msg = KernelMessage::new(r.clone());
            assert!(msg.check().is_ok());
            let json = serde_json::to_string(&msg).unwrap();
            let back: KernelMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(back.inner, r);
            assert!(back.check().is_ok());
        }
    }

    #[test]
    fn test_notifications_roundtrip_v1() {
        let action = NotificationActionDto {
            id: "pause_10m".into(),
            label: "Prendre une pause 10m".into(),
            is_destructive: false,
            is_braise: false,
        };
        let notif = NotificationDto {
            id: "notif-fatigue-1".into(),
            title: "Fatigue Détectée".into(),
            body: "PERCLOS P80 > 15% — baisse de vigilance observée".into(),
            urgency: "high".into(),
            category: "cognitive".into(),
            actions: vec![action],
            timeout_ms: Some(6000),
            is_braise: false,
            visual_style: "minimalist_toast".into(),
            created_at_ms: 1794078900000,
        };

        // KernelToUi
        let k_msg = KernelMessage::new(KernelToUi::Notification(notif));
        assert!(k_msg.check().is_ok());
        let k_json = serde_json::to_string(&k_msg).unwrap();
        let k_back: KernelMessage = serde_json::from_str(&k_json).unwrap();
        assert_eq!(k_back, k_msg);

        // UiToKernel Action
        let u_action = UiMessage::new(UiToKernel::NotificationAction {
            id: "notif-fatigue-1".into(),
            action_id: "pause_10m".into(),
        });
        assert!(u_action.check().is_ok());
        let u_json = serde_json::to_string(&u_action).unwrap();
        let u_back: UiMessage = serde_json::from_str(&u_json).unwrap();
        assert_eq!(u_back, u_action);

        // UiToKernel Dismiss
        let u_dismiss = UiMessage::new(UiToKernel::NotificationDismiss {
            id: "notif-fatigue-1".into(),
        });
        assert!(u_dismiss.check().is_ok());
        let d_json = serde_json::to_string(&u_dismiss).unwrap();
        let d_back: UiMessage = serde_json::from_str(&d_json).unwrap();
        assert_eq!(d_back, u_dismiss);
    }
}

