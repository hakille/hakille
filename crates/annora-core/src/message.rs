use crate::capability::CapabilityScope;
use crate::types::{ModelReference, Timestamp, UserId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Trust level for messages. Untrusted content can NEVER become an executable instruction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TrustLevel {
    System,
    User,
    Guest,
    Untrusted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ActorId {
    Kernel,
    Agent(String),
    User(UserId),
    External(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRequest {
    pub action_type: String,
    pub parameters: serde_json::Value,
    pub required_capability: CapabilityScope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResponse {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PerceptionEvent {
    SpeechRecognized {
        text: String,
        language: String,
        emotion: String,
        speaker_id: String,
        confidence: f64,
    },
    WakeWordDetected,
    VoiceActivityStart,
    VoiceActivityEnd,
    ScreenCaptured,
    SoundEvent(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryResult {
    pub results: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemEvent {
    Boot,
    Shutdown,
    UserAuthenticated(UserId),
    UserDeauthenticated,
    CapabilityGranted,
    CapabilityRevoked,
    ModelLoaded(ModelReference),
    ModelUnloaded(ModelReference),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageContent {
    Text(String),
    AudioChunk(bytes::Bytes),
    ScreenCapture(bytes::Bytes),
    Action(ActionRequest),
    ActionResult(ActionResponse),
    PerceptionEvent(PerceptionEvent),
    MemoryQuery(MemoryQuery),
    MemoryResult(MemoryResult),
    SystemEvent(SystemEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub sender: ActorId,
    pub recipient: ActorId,
    pub content: MessageContent,
    pub trust_level: TrustLevel,
    pub timestamp: Timestamp,
    pub correlation_id: Option<Uuid>,
}
