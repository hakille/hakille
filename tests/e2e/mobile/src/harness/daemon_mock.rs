//! Mock daemon providing a real IPC endpoint (`hakille_ipc`) for mobile gateway tests.
//!
//! Replaces neither cognitive models nor business agents: executes the exact
//! protocol framing with status barrier detection (`BARRIER_ID = u64::MAX`),
//! monotonic request ID validation, token delta streaming, and controlled fault injection.

use hakille_ipc::frame::{ErrorCode, Frame, FrameBody};
use hakille_ipc::transport::{
    IpcError, IpcListener, ServerStream, read_frame, server_handshake, write_frame,
};
use hakille_mobile_gateway::BARRIER_ID;
use hakille_ui_protocol::{
    ActionCard, DataExit, KernelToUi, Money, PresenceState, Provenance, StatusDto, UiToKernel,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::time::timeout;

const FRAME_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("IPC transport error: {0}")]
    Ipc(#[from] IpcError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Request ID mismatch: expected {expected}, got {actual}")]
    IdMismatch { expected: u64, actual: u64 },
    #[error("Request ID not monotonic: previous {previous}, current {current}")]
    NonMonotonicId { previous: u64, current: u64 },
    #[error("Unexpected request body: expected {expected:?}, got {actual:?}")]
    BodyMismatch {
        expected: UiToKernel,
        actual: FrameBody,
    },
    #[error("Barrier error: {0}")]
    Barrier(String),
    #[error("Timeout waiting for frame")]
    Timeout,
}

/// A scriptable step executed by the mock daemon in response to a gateway request.
#[derive(Debug, Clone)]
pub struct DaemonStep {
    pub request: UiToKernel,
    pub reply: KernelToUi,
    pub before: Vec<KernelToUi>,
    pub after: Vec<KernelToUi>,
    pub delay: Option<Duration>,
}

impl DaemonStep {
    pub fn new(request: UiToKernel, reply: KernelToUi) -> Self {
        Self {
            request,
            reply,
            before: Vec::new(),
            after: Vec::new(),
            delay: None,
        }
    }

    pub fn status() -> Self {
        Self::new(UiToKernel::StatusRequest, status_reply(default_status()))
    }

    pub fn chat(query: &str, reply_text: &str, card: Option<ActionCard>) -> Self {
        let mut after = vec![KernelToUi::Presence(PresenceState::DecisionRequired)];
        if let Some(c) = card {
            after.push(KernelToUi::ShowCard(c));
        }

        Self {
            request: UiToKernel::HudSubmit {
                query: query.to_string(),
            },
            reply: KernelToUi::HudReply {
                response: reply_text.to_string(),
                validation: None,
            },
            before: vec![
                KernelToUi::Presence(PresenceState::Acting),
                KernelToUi::TokenDelta {
                    text: format!("{reply_text} (provisional stream)"),
                },
            ],
            after,
            delay: None,
        }
    }

    pub fn before(mut self, frames: Vec<KernelToUi>) -> Self {
        self.before = frames;
        self
    }

    pub fn after(mut self, frames: Vec<KernelToUi>) -> Self {
        self.after = frames;
        self
    }

    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }
}

pub fn default_status() -> StatusDto {
    StatusDto {
        model: "mock-daemon-qwen".into(),
        cost: "0.00 EUR".into(),
        state: "Idle".into(),
        memory_count: 42,
    }
}

pub fn status_reply(status: StatusDto) -> KernelToUi {
    KernelToUi::StatusReply { status }
}

pub fn default_card(id: u64) -> ActionCard {
    ActionCard {
        id,
        verb: "Envoyer".into(),
        object: "rapport de test E2E".into(),
        reason: "Validation opaque-box Phase 1".into(),
        cost: Money::from_cents(0, "EUR"),
        data_exit: DataExit::Local,
        reversible: true,
        provenance: Provenance::Hakille,
    }
}

/// Mock IPC daemon managing connection lifecycle and script execution.
pub struct ControlledDaemon {
    pub endpoint: String,
    pub token: String,
    listener: IpcListener,
    last_id: Arc<AtomicU64>,
}

impl ControlledDaemon {
    pub async fn bind(endpoint: &str, token: &str) -> std::io::Result<Self> {
        let listener = IpcListener::bind(endpoint).await?;
        Ok(Self {
            endpoint: endpoint.to_string(),
            token: token.to_string(),
            listener,
            last_id: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Accepts an IPC connection from the gateway and performs the protocol handshake.
    pub async fn accept(&self) -> Result<ServerStream, DaemonError> {
        let mut stream = timeout(FRAME_TIMEOUT, self.listener.accept())
            .await
            .map_err(|_| DaemonError::Timeout)?
            .map_err(DaemonError::Io)?;

        server_handshake(&mut stream, &self.token)
            .await
            .map_err(DaemonError::Ipc)?;

        Ok(stream)
    }

    /// Accepts a raw connection without running the handshake (for negative handshake tests).
    pub async fn accept_raw(&self) -> std::io::Result<ServerStream> {
        self.listener.accept().await
    }

    /// Reads a frame with timeout.
    pub async fn read_frame(stream: &mut ServerStream) -> Result<Frame, DaemonError> {
        let opt = timeout(FRAME_TIMEOUT, read_frame(stream))
            .await
            .map_err(|_| DaemonError::Timeout)?
            .map_err(DaemonError::Ipc)?;

        opt.ok_or(DaemonError::Ipc(IpcError::Closed))
    }

    /// Reads a single frame using the instance reference (convenience wrapper around `read_frame`).
    pub async fn accept_one_frame(&self, stream: &mut ServerStream) -> Result<Frame, DaemonError> {
        Self::read_frame(stream).await
    }

    /// Writes a frame.
    pub async fn write_frame(stream: &mut ServerStream, frame: &Frame) -> Result<(), DaemonError> {
        write_frame(stream, frame).await.map_err(DaemonError::Ipc)
    }

    /// Reads barrier frame from the stream and sends back a `StatusReply`.
    pub async fn finish_barrier(
        stream: &mut ServerStream,
        status: Option<StatusDto>,
    ) -> Result<(), DaemonError> {
        let barrier_frame = Self::read_frame(stream).await?;
        if barrier_frame.id != BARRIER_ID {
            return Err(DaemonError::Barrier(format!(
                "expected BARRIER_ID {}, got {}",
                BARRIER_ID, barrier_frame.id
            )));
        }
        match barrier_frame.body {
            FrameBody::Request {
                message: UiToKernel::StatusRequest,
            } => {
                let reply_status = status.unwrap_or_else(default_status);
                Self::write_frame(
                    stream,
                    &Frame::reply(BARRIER_ID, status_reply(reply_status)),
                )
                .await?;
                Ok(())
            }
            other => Err(DaemonError::Barrier(format!(
                "expected StatusRequest barrier, got {other:?}"
            ))),
        }
    }

    /// Executes a single scripted step: checks monotonic ID, matches request, sends replies, and handles barrier.
    pub async fn handle_step(
        &self,
        stream: &mut ServerStream,
        expected_id: u64,
        step: DaemonStep,
    ) -> Result<(), DaemonError> {
        let frame = Self::read_frame(stream).await?;

        // Monotonic check
        let prev = self.last_id.load(Ordering::SeqCst);
        if frame.id <= prev && frame.id != BARRIER_ID {
            return Err(DaemonError::NonMonotonicId {
                previous: prev,
                current: frame.id,
            });
        }
        if frame.id != expected_id {
            return Err(DaemonError::IdMismatch {
                expected: expected_id,
                actual: frame.id,
            });
        }
        self.last_id.store(frame.id, Ordering::SeqCst);

        // Body match check
        match &frame.body {
            FrameBody::Request { message } if message == &step.request => {}
            _ => {
                return Err(DaemonError::BodyMismatch {
                    expected: step.request,
                    actual: frame.body,
                });
            }
        }

        // Optional delay simulation
        if let Some(delay) = step.delay {
            tokio::time::sleep(delay).await;
        }

        // Before frames (unsolicited / streaming id 0)
        for msg in step.before {
            Self::write_frame(stream, &Frame::reply(0, msg)).await?;
        }

        // Correlated reply frame
        Self::write_frame(stream, &Frame::reply(expected_id, step.reply)).await?;

        // After frames (trailed events id 0)
        for msg in step.after {
            Self::write_frame(stream, &Frame::reply(0, msg)).await?;
        }

        // Status barrier completion
        Self::finish_barrier(stream, None).await?;

        Ok(())
    }

    /// Executes a sequence of steps starting at `start_id`.
    pub async fn handle_steps(
        &self,
        stream: &mut ServerStream,
        start_id: u64,
        steps: Vec<DaemonStep>,
    ) -> Result<(), DaemonError> {
        for (i, step) in steps.into_iter().enumerate() {
            self.handle_step(stream, start_id + i as u64, step).await?;
        }
        Ok(())
    }

    /// Handles a step by returning an IPC error frame instead of a regular reply.
    pub async fn handle_error_step(
        &self,
        stream: &mut ServerStream,
        expected_id: u64,
        expected_request: UiToKernel,
        code: ErrorCode,
        message: &str,
    ) -> Result<(), DaemonError> {
        let frame = Self::read_frame(stream).await?;
        if frame.id != expected_id {
            return Err(DaemonError::IdMismatch {
                expected: expected_id,
                actual: frame.id,
            });
        }
        match &frame.body {
            FrameBody::Request { message: req } if req == &expected_request => {}
            _ => {
                return Err(DaemonError::BodyMismatch {
                    expected: expected_request,
                    actual: frame.body,
                });
            }
        }

        Self::write_frame(stream, &Frame::error(expected_id, code, message)).await?;
        Self::finish_barrier(stream, None).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_card_attributes() {
        let card = default_card(42);
        assert_eq!(card.id, 42);
        assert_eq!(card.data_exit, DataExit::Local);
        assert!(card.reversible);
        assert_eq!(card.provenance, Provenance::Hakille);
    }

    #[test]
    fn test_default_status_attributes() {
        let status = default_status();
        assert_eq!(status.state, "Idle");
        assert_eq!(status.memory_count, 42);
    }

    #[test]
    fn test_daemon_step_builders() {
        let step = DaemonStep::status();
        assert_eq!(step.request, UiToKernel::StatusRequest);
        assert!(step.before.is_empty());
        assert!(step.after.is_empty());

        let chat = DaemonStep::chat("hello", "world", Some(default_card(1)));
        assert_eq!(chat.before.len(), 2);
        assert_eq!(chat.after.len(), 2);
    }
}
