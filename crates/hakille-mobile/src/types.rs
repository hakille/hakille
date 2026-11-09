//! DTO du cœur mobile. Aucun jeton de transport ou clé n'est conservé ici.
use hakille_ipc::Frame;
use hakille_ui_protocol::{
    ActionCard, AgentDto, GhostActionDto, JournalEntryDto, KernelToUi, MemoryRowDto,
    PrivacyGuardEvent, StatusDto,
};
use serde::{Deserialize, Serialize};

pub const GUEST_RING_PX: u32 = 1;
pub const MAX_CAMERA_PHOTO_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_CAMERA_LONG_EDGE_PX: u32 = 1536;
pub const MAX_CAMERA_TOTAL_PIXELS: u64 = 4_000_000;
pub const MAX_DOCUMENT_PICKER_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_DOCUMENT_PREVIEW_CHARS: usize = 4000;

pub const ALLOWED_EXTERNAL_DESTINATIONS: &[&str] = &[
    "maps://",
    "calshow:",
    "app-settings:",
    "https://github.com/hakille/hakille#readme",
    "clock://",
];

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
    /// Détachement explicite d’une opération en vol (cycle de vie arrière-plan).
    DetachInFlight {
        request_id: u64,
    },
    GhostActionAccept {
        id: String,
    },
    GhostActionDismiss {
        id: String,
    },
    Undo {
        journal_id: u64,
    },
    ForgetMemory {
        row_id: String,
    },
    SetAutonomyCrank {
        domain: String,
        crank: u8,
        #[serde(default)]
        max_cost_cents: Option<u64>,
        #[serde(default)]
        max_actions: Option<u32>,
        #[serde(default)]
        days_valid: Option<u32>,
    },
    RequestWhy {
        target_id: String,
        target_type: String,
    },
    CloseWhy {},
    DismissPrivacyGuard {},
    StartGuestPass {
        name: String,
        power: String,
        #[serde(default)]
        duration_secs: Option<u32>,
    },
    TickGuestPass {
        elapsed_secs: u32,
    },
    RevokeGuestPass {},
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationKind {
    Submit,
    Remember,
    RunAgent,
    Approve,
    Reject,
    Undo,
    ForgetMemory,
}

impl MutationKind {
    pub(crate) fn from_kind(kind: &str) -> Option<Self> {
        match kind {
            "submit" => Some(Self::Submit),
            "remember" => Some(Self::Remember),
            "run_agent" => Some(Self::RunAgent),
            "approve" => Some(Self::Approve),
            "reject" => Some(Self::Reject),
            "undo" => Some(Self::Undo),
            "forget_memory" => Some(Self::ForgetMemory),
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
            Self::Undo => "undo",
            Self::ForgetMemory => "forget_memory",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationPhase {
    InFlight,
    InFlightDetached,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FactRefDto {
    pub label: String,
    pub kind: String,
    pub date: String,
    pub source: String,
    pub freshness: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WhySheetDto {
    pub target_id: String,
    pub sentence: String,
    pub facts: Vec<FactRefDto>,
    pub plan: String,
    pub powers: Vec<String>,
    pub confidence_word: String,
    pub confidence_value: f64,
    pub alternatives: Vec<String>,
    pub model_line: String,
    pub cost_label: String,
    pub data_line: String,
    pub would_change: String,
}

impl WhySheetDto {
    pub fn missing(target_id: &str, reason: &str) -> Self {
        Self {
            target_id: target_id.to_string(),
            sentence: "Je n'ai pas gardé la raison de ce choix.".to_string(),
            facts: vec![],
            plan: reason.to_string(),
            powers: vec![],
            confidence_word: "basse".to_string(),
            confidence_value: 0.0,
            alternatives: vec![],
            model_line: "inconnu".to_string(),
            cost_label: "inconnu".to_string(),
            data_line: "inconnu".to_string(),
            would_change: "inconnu".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnvelopeDto {
    pub max_cost_cents: u64,
    pub max_actions: u32,
    pub days_valid: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DomainAutonomyDto {
    pub domain: String,
    pub crank: u8,
    pub crank_label: String,
    pub envelope: Option<EnvelopeDto>,
    pub validated_count: u32,
    pub promotion_offerable: bool,
}

pub fn crank_label(crank: u8) -> &'static str {
    match crank {
        0 => "Observer",
        1 => "Préparer",
        2 => "Agir sur le réversible",
        3 => "Agir dans une enveloppe",
        4 => "Déléguer",
        _ => "Inconnu",
    }
}

impl DomainAutonomyDto {
    pub fn label_for_crank(crank: u8) -> &'static str {
        crank_label(crank)
    }

    pub fn new(domain: &str) -> Self {
        Self {
            domain: domain.to_string(),
            crank: 1,
            crank_label: crank_label(1).to_string(),
            envelope: None,
            validated_count: 0,
            promotion_offerable: false,
        }
    }

    pub fn record_validated(&mut self) {
        self.validated_count += 1;
        self.promotion_offerable =
            (self.crank == 1 || self.crank == 2) && self.validated_count >= 20;
    }

    pub fn record_reverted(&mut self) {
        self.crank = match self.crank {
            0 => 0,
            1 => 0,
            2 => 1,
            3 => 2,
            4 => 3,
            _ => 0,
        };
        self.crank_label = crank_label(self.crank).to_string();
        self.validated_count = 0;
        self.promotion_offerable = false;
        if self.crank < 3 {
            self.envelope = None;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GuestPassDto {
    pub name: String,
    pub power: String,
    pub duration_secs: u32,
    pub elapsed_secs: u32,
    pub ring_fraction: f32,
    pub ring_px: u32,
    pub expired: bool,
}

impl GuestPassDto {
    pub fn new(name: &str, power: &str, duration_secs: u32) -> Self {
        let ring_fraction = if duration_secs == 0 { 0.0 } else { 1.0 };
        Self {
            name: name.to_string(),
            power: power.to_string(),
            duration_secs,
            elapsed_secs: 0,
            ring_fraction,
            ring_px: GUEST_RING_PX,
            expired: duration_secs == 0,
        }
    }

    pub fn tick(&mut self, elapsed_secs: u32) {
        self.elapsed_secs = elapsed_secs;
        if self.duration_secs == 0 {
            self.ring_fraction = 0.0;
            self.expired = true;
        } else {
            self.ring_fraction =
                (1.0 - self.elapsed_secs as f32 / self.duration_secs as f32).clamp(0.0, 1.0);
            self.expired = self.elapsed_secs >= self.duration_secs;
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairingQrPayload {
    pub endpoint: String,
    pub token: String,
}

impl std::fmt::Debug for PairingQrPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairingQrPayload")
            .field("endpoint", &self.endpoint)
            .field("token", &"[redacted]")
            .finish()
    }
}

pub fn parse_pairing_qr(raw_json: &str) -> Result<PairingQrPayload, &'static str> {
    if raw_json.is_empty() || raw_json.len() > 8192 {
        return Err("Payload QR d'appairage vide ou trop volumineux");
    }
    let parsed: PairingQrPayload = serde_json::from_str(raw_json)
        .map_err(|_| "Format JSON d'appairage invalide ou champs inconnus")?;
    let endpoint = parsed.endpoint.trim().trim_end_matches('/').to_string();
    if endpoint.is_empty()
        || endpoint.len() > 2048
        || endpoint.contains(['@', '?', '#', '\0'])
        || endpoint.chars().any(char::is_whitespace)
    {
        return Err("URL de passerelle invalide (identifiants, paramètres ou espaces interdits)");
    }
    let (scheme, rest) = endpoint
        .split_once("://")
        .ok_or("Schéma d'URL de passerelle manquant")?;
    if rest.is_empty() || rest.contains('/') {
        return Err("URL de base sans chemin requise");
    }
    let host = rest.split(':').next().unwrap_or("");
    if host.is_empty() {
        return Err("Hôte de passerelle manquant");
    }
    if let Some((_, port_str)) = rest.split_once(':') {
        let port: u16 = port_str
            .parse()
            .map_err(|_| "Port de passerelle invalide")?;
        if port == 0 {
            return Err("Port 0 interdit");
        }
    }
    let is_valid_scheme = scheme == "https" || (scheme == "http" && host == "127.0.0.1");
    if !is_valid_scheme {
        return Err("HTTPS requis (HTTP autorisé uniquement sur 127.0.0.1)");
    }
    let token = parsed.token.trim().to_string();
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Jeton d'appairage invalide : 64 caractères hexadécimaux attendus");
    }
    let canonical_endpoint = format!("{}://{}", scheme, rest.to_ascii_lowercase());
    Ok(PairingQrPayload {
        endpoint: canonical_endpoint,
        token,
    })
}

pub fn compute_camera_scale(width: u32, height: u32) -> f32 {
    let long_edge = width.max(height);
    if long_edge == 0 {
        return 0.0;
    }
    (MAX_CAMERA_LONG_EDGE_PX as f32 / long_edge as f32).min(1.0)
}

pub fn validate_camera_frame(
    width: u32,
    height: u32,
    byte_len: usize,
) -> Result<(u32, u32), &'static str> {
    if width == 0 || height == 0 {
        return Err("Dimensions de caméra invalides (0 px)");
    }
    if (width as u64) * (height as u64) > MAX_CAMERA_TOTAL_PIXELS {
        return Err("Résolution de photo excessive (> 4 mégapixels)");
    }
    if byte_len == 0 {
        return Err("Image vide refusée (0 octet)");
    }
    if byte_len > MAX_CAMERA_PHOTO_BYTES {
        return Err("Taille de photo supérieure à 8 Mio");
    }
    let scale = compute_camera_scale(width, height);
    let scaled_w = ((width as f32) * scale).round() as u32;
    let scaled_h = ((height as f32) * scale).round() as u32;
    Ok((scaled_w, scaled_h))
}

pub fn validate_gps_fix(
    latitude: f64,
    longitude: f64,
    accuracy_m: f64,
    age_secs: f64,
    is_mock: bool,
) -> Result<f64, &'static str> {
    if !latitude.is_finite()
        || !longitude.is_finite()
        || !accuracy_m.is_finite()
        || !age_secs.is_finite()
    {
        return Err("Coordonnées GPS non finies (NaN ou Inf)");
    }
    if !(-90.0..=90.0).contains(&latitude) {
        return Err("Latitude hors limites [-90, +90]");
    }
    if !(-180.0..=180.0).contains(&longitude) {
        return Err("Longitude hors limites [-180, +180]");
    }
    if !(0.0..=40_100_000.0).contains(&accuracy_m) {
        return Err("Rayon de précision invalide [0, 40100000 m]");
    }
    if !(-2.0..=15.0).contains(&age_secs) {
        return Err("Position GPS périmée (> 15s) ou horodatage futur invalide");
    }
    if is_mock {
        return Err("Position simulée (mock provider) refusée");
    }
    Ok(age_secs)
}

pub fn is_allowed_external_destination(url: &str) -> bool {
    ALLOWED_EXTERNAL_DESTINATIONS.contains(&url)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErasureReceiptDto {
    pub receipt_id: String,
    pub target_id: String,
    pub timestamp_str: String,
    pub reason: String,
    pub verification_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRunOutcomeDto {
    pub name: String,
    pub output_json: String,
    pub duration_ms: u64,
    pub input_sha256: String,
    pub output_sha256: String,
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
    pub ghost_action: Option<GhostActionDto>,
    /// Une incertitude doit être lue explicitement avant une nouvelle mutation.
    pub recovery_required: bool,
    pub uncertain_operations: Vec<UncertainOperation>,
    pub acknowledged_operations: Vec<AcknowledgedOperation>,
    pub why_sheet: Option<WhySheetDto>,
    pub domain_autonomies: Vec<DomainAutonomyDto>,
    pub tombstones: Vec<String>,
    pub erasure_receipts: Vec<ErasureReceiptDto>,
    pub last_agent_run: Option<AgentRunOutcomeDto>,
    pub privacy_guard: Option<PrivacyGuardEvent>,
    pub privacy_veil_active: bool,
    pub guest_pass: Option<GuestPassDto>,
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
            ghost_action: None,
            recovery_required: false,
            uncertain_operations: vec![],
            acknowledged_operations: vec![],
            why_sheet: None,
            domain_autonomies: vec!["Mail", "Agenda", "Maison", "Fichiers", "Paiement", "Code"]
                .into_iter()
                .map(DomainAutonomyDto::new)
                .collect(),
            tombstones: vec![],
            erasure_receipts: vec![],
            last_agent_run: None,
            privacy_guard: None,
            privacy_veil_active: false,
            guest_pass: None,
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
