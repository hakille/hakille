//! Contrat IPC local entre le HUD desktop et le démon Hakille.
//!
//! Trames JSON d'une ligne (plafond 1 Mio) sur named pipe Windows
//! (`\\.\pipe\hakille-daemon-v1`) ou socket Unix. Pas de TCP en v1 : le fil
//! reste strictement local au poste.

pub mod frame;
pub mod transport;

#[cfg(not(any(windows, unix)))]
compile_error!(
    "hakille-ipc ne supporte que Windows (named pipe) et Unix (socket) : pas de TCP en v1"
);

pub use frame::{ErrorCode, Frame, FrameBody, MAX_FRAME_BYTES};
pub use transport::IpcError;
