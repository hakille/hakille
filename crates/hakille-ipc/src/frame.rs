//! Trames du fil IPC : enveloppe JSON d'une ligne, versionnée.

use hakille_ui_protocol::{KernelToUi, PROTOCOL_VERSION, UiToKernel, compatible};
use serde::{Deserialize, Serialize};

use crate::transport::IpcError;

/// Plafond d'une trame sérialisée (1 Mio), hors terminateur `\n`.
pub const MAX_FRAME_BYTES: usize = 1 << 20;

/// Enveloppe commune à toutes les trames du fil HUD ↔ démon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub version: u16,
    pub id: u64,
    pub body: FrameBody,
}

/// Corps typé d'une trame. Le handshake précède tout `Request`/`Reply`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FrameBody {
    Hello { token_hex: String },
    Welcome { daemon_version: String },
    Request { message: UiToKernel },
    Reply { message: KernelToUi },
    Error { code: ErrorCode, message: String },
}

/// Erreurs structurées du fil, pour refus explicites (jamais de succès simulé).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    BadVersion,
    BadToken,
    BadFrame,
    TooLarge,
    Busy,
    Unsupported,
    Internal,
}

impl Frame {
    /// Hello client : version courante, `id` 0 (hors flux de requêtes).
    pub fn hello(token_hex: impl Into<String>) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            id: 0,
            body: FrameBody::Hello {
                token_hex: token_hex.into(),
            },
        }
    }

    /// Welcome serveur : version courante, `id` 0.
    pub fn welcome(daemon_version: impl Into<String>) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            id: 0,
            body: FrameBody::Welcome {
                daemon_version: daemon_version.into(),
            },
        }
    }

    pub fn request(id: u64, message: UiToKernel) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            id,
            body: FrameBody::Request { message },
        }
    }

    pub fn reply(id: u64, message: KernelToUi) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            id,
            body: FrameBody::Reply { message },
        }
    }

    pub fn error(id: u64, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            id,
            body: FrameBody::Error {
                code,
                message: message.into(),
            },
        }
    }

    /// Rejette les versions incompatibles sans panic ni repli silencieux.
    pub fn check(&self) -> Result<(), IpcError> {
        if compatible(self.version as u32) {
            Ok(())
        } else {
            Err(IpcError::BadVersion(self.version))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_carry_protocol_version() {
        assert_eq!(Frame::hello("aa").version, PROTOCOL_VERSION);
        assert_eq!(
            Frame::welcome("hakille-daemon v0.1.0").version,
            PROTOCOL_VERSION
        );
        assert_eq!(
            Frame::request(1, UiToKernel::OutboxFlush).version,
            PROTOCOL_VERSION
        );
        assert_eq!(
            Frame::reply(1, KernelToUi::Toast { text: "x".into() }).version,
            PROTOCOL_VERSION
        );
        assert_eq!(
            Frame::error(1, ErrorCode::Internal, "x").version,
            PROTOCOL_VERSION
        );
    }
}
