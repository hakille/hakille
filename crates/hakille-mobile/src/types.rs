//! DTO du cœur mobile. Aucun jeton de transport ou clé n'est conservé ici.
use hakille_ipc::Frame;
use hakille_ui_protocol::{
    ActionCard, AgentDto, JournalEntryDto, KernelToUi, MemoryRowDto, StatusDto,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MobileAction {
    SetHub {
        hub_id: String,
    },
    SetDraft {
        text: String,
    },
    Submit {},
    BeginVoice {},
    CancelVoice {},
    SubmitVoice {
        context_id: u64,
        text: String,
    },
    RefreshStatus {},
    LoadMemory {
        query: Option<String>,
        limit: usize,
    },
    Remember {
        text: String,
    },
    LoadJournal {
        limit: usize,
    },
    LoadAgents {},
    RunAgent {
        name: String,
        input_json: String,
    },
    BeginApproval {
        card_id: u64,
    },
    Approve {
        card_id: u64,
    },
    CancelApproval {},
    Reject {
        card_id: u64,
    },
    SetForeground {
        active: bool,
    },
    SetMicrophone {
        enabled: bool,
    },
    /// Attestation de l’hôte : aucun envoi réseau n’a commencé.
    FailBeforeSend {
        request_id: u64,
        message: String,
    },
    /// Prise de connaissance locale : aucune annulation/résolution côté hub.
    AcknowledgeUnknown {
        request_id: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationKind {
    Submit,
    Remember,
    RunAgent,
    Approve,
    Reject,
}

impl MutationKind {
    pub(crate) fn from_kind(kind: &str) -> Option<Self> {
        match kind {
            "submit" => Some(Self::Submit),
            "remember" => Some(Self::Remember),
            "run_agent" => Some(Self::RunAgent),
            "approve" => Some(Self::Approve),
            "reject" => Some(Self::Reject),
            _ => None,
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Submit => "submit",
            Self::Remember => "remember",
            Self::RunAgent => "run_agent",
            Self::Approve => "approve",
            Self::Reject => "reject",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationPhase {
    InFlight,
    Unknown,
}

/// Métadonnées minimales : aucun texte de commande, hub ou credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UncertainOperation {
    pub request_id: u64,
    pub kind: MutationKind,
    pub phase: OperationPhase,
}

/// L'utilisateur a lu l'avertissement ; l'effet de l'opération reste inconnu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcknowledgedOperation {
    pub request_id: u64,
    pub kind: MutationKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatEntry {
    pub role: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingRequest {
    pub request_id: u64,
    pub kind: String,
    pub mutating: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApprovalState {
    pub card_id: u64,
    pub required_hold_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestOutcome {
    pub request_id: u64,
    pub kind: String,
    /// confirmed, failed ou unknown ; jamais « exécuté » par supposition.
    pub state: String,
    pub message: String,
    pub result: Option<KernelToUi>,
    pub acknowledged: bool,
}

/// Réponse finale du seul tour vocal explicitement armé, jamais un extrait du chat.
#[derive(Debug, Clone, Serialize)]
pub struct VoiceReply {
    pub context_id: u64,
    pub request_id: u64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MobileSnapshot {
    pub schema_version: u16,
    pub hub_id: Option<String>,
    pub gateway_session: Option<String>,
    pub connection: ConnectionState,
    pub foreground: bool,
    /// Le cœur ne pilote pas le micro ; les permissions et l'état audio sont natifs.
    pub microphone_enabled: bool,
    pub voice_context_id: Option<u64>,
    pub voice_request_id: Option<u64>,
    pub voice_reply: Option<VoiceReply>,
    pub draft: String,
    pub chat: Vec<ChatEntry>,
    pub streaming: bool,
    /// Les entrées les plus anciennes ont été retirées pour borner l'état.
    pub history_trimmed: bool,
    pub presence: Option<hakille_ui_protocol::PresenceState>,
    pub notifications: Vec<String>,
    pub last_journal_entry_id: Option<u64>,
    pub status: Option<StatusDto>,
    pub memory: Vec<MemoryRowDto>,
    pub journal: Vec<JournalEntryDto>,
    pub agents: Vec<AgentDto>,
    pub cards: Vec<ActionCard>,
    pub outcomes: Vec<RequestOutcome>,
    pub pending: Option<PendingRequest>,
    pub approval: Option<ApprovalState>,
    /// Une incertitude doit être lue explicitement avant une nouvelle mutation.
    pub recovery_required: bool,
    pub uncertain_operations: Vec<UncertainOperation>,
    pub acknowledged_operations: Vec<AcknowledgedOperation>,
}

impl Default for MobileSnapshot {
    fn default() -> Self {
        Self {
            schema_version: 1,
            hub_id: None,
            gateway_session: None,
            connection: ConnectionState::Disconnected,
            foreground: true,
            microphone_enabled: false,
            voice_context_id: None,
            voice_request_id: None,
            voice_reply: None,
            draft: String::new(),
            chat: vec![],
            streaming: false,
            history_trimmed: false,
            presence: None,
            notifications: vec![],
            last_journal_entry_id: None,
            status: None,
            memory: vec![],
            journal: vec![],
            agents: vec![],
            cards: vec![],
            outcomes: vec![],
            pending: None,
            approval: None,
            recovery_required: false,
            uncertain_operations: vec![],
            acknowledged_operations: vec![],
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BridgeResult {
    pub ok: bool,
    pub error: Option<String>,
    pub request: Option<Frame>,
    pub snapshot: MobileSnapshot,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GatewayResponse {
    pub session_id: Option<String>,
    pub frames: Vec<Frame>,
    pub error: Option<GatewayError>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GatewayError {
    pub code: String,
    pub message: String,
    pub issue_unknown: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedDraftV1 {
    pub schema_version: u16,
    pub draft: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedDraftV2 {
    pub schema_version: u16,
    pub draft: String,
    pub next_request_id: u64,
    pub uncertain_operations: Vec<UncertainOperation>,
    pub acknowledged_operations: Vec<AcknowledgedOperation>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum SavedDraft {
    V2(SavedDraftV2),
    V1(SavedDraftV1),
}
