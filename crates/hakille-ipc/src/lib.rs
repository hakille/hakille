//! Contrat IPC local entre le HUD desktop et le démon Hakille.
//!
//! Trames JSON d'une ligne (plafond 1 Mio) sur named pipe Windows
//! (`\\.\pipe\hakille-daemon-v1`) ou socket Unix. Pas de TCP en v1 : le fil
//! reste strictement local au poste.
//!
//! Conserver le lecteur bufferisé retourné par `transport::connect` / `accept`
//! pendant toute la connexion : les octets lus en avance appartiennent aux
//! trames suivantes, y compris lorsqu'ils suivent le handshake.
//!
//! Sur Unix, un endpoint existant est refusé sans suppression automatique.
//! Après un crash, vérifier que le démon est arrêté avant de nettoyer son
//! socket ; un second lancement ne doit jamais remplacer un socket actif.

pub mod frame;
pub mod transport;

#[cfg(not(any(windows, unix)))]
compile_error!(
    "hakille-ipc ne supporte que Windows (named pipe) et Unix (socket) : pas de TCP en v1"
);

pub use frame::{ErrorCode, Frame, FrameBody, MAX_FRAME_BYTES};
pub use transport::IpcError;
