//! Transport IPC local : named pipe Windows, socket Unix.
//!
//! Trames JSON UTF-8 terminées par `\n`, plafonnées à [`MAX_FRAME_BYTES`].
//! La lecture ne fait jamais confiance à une longueur distante : le tampon
//! est plafonné strictement et toute ligne au-delà du plafond est refusée.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rand::RngCore as _;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::frame::{ErrorCode, Frame, FrameBody, MAX_FRAME_BYTES};

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("ipc io: {0}")]
    Io(#[from] std::io::Error),
    #[error("ipc json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ipc: version {0} incompatible")]
    BadVersion(u16),
    #[error("ipc: trame trop grande (> 1 Mio)")]
    TooLarge,
    #[error("ipc: connexion fermée")]
    Closed,
    #[error("ipc: handshake invalide: {0}")]
    Handshake(String),
}

/// Nom du named pipe Windows du démon v1.
pub const PIPE_NAME_WINDOWS: &str = r"\\.\pipe\hakille-daemon-v1";

const ENV_ENDPOINT: &str = "HAKILLE_IPC_ENDPOINT";
const ENV_TOKEN_FILE: &str = "HAKILLE_IPC_TOKEN_FILE";
#[cfg(unix)]
const ENV_XDG_RUNTIME_DIR: &str = "XDG_RUNTIME_DIR";
#[cfg(windows)]
const ENV_LOCALAPPDATA: &str = "LOCALAPPDATA";

const READ_CHUNK_BYTES: usize = 8 * 1024;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const TOKEN_BYTES: usize = 32;

/// Endpoint par défaut : `HAKILLE_IPC_ENDPOINT` sinon pipe Windows / socket Unix.
pub fn default_endpoint() -> String {
    if let Some(value) = env_non_empty(ENV_ENDPOINT) {
        return value.to_string_lossy().into_owned();
    }
    platform_endpoint()
}

#[cfg(windows)]
fn platform_endpoint() -> String {
    PIPE_NAME_WINDOWS.to_string()
}

#[cfg(unix)]
fn platform_endpoint() -> String {
    runtime_dir()
        .join("hakille")
        .join("ipc.sock")
        .to_string_lossy()
        .into_owned()
}

/// Chemin du jeton partagé : `HAKILLE_IPC_TOKEN_FILE` sinon
/// `%LOCALAPPDATA%\Hakille\ipc.token` / `$XDG_RUNTIME_DIR|/tmp/hakille/ipc.token`.
pub fn default_token_path() -> PathBuf {
    if let Some(value) = env_non_empty(ENV_TOKEN_FILE) {
        return PathBuf::from(value);
    }
    platform_token_path()
}

#[cfg(windows)]
fn platform_token_path() -> PathBuf {
    match env_non_empty(ENV_LOCALAPPDATA) {
        Some(base) => PathBuf::from(base).join("Hakille").join("ipc.token"),
        None => std::env::temp_dir().join("Hakille").join("ipc.token"),
    }
}

#[cfg(unix)]
fn platform_token_path() -> PathBuf {
    runtime_dir().join("hakille").join("ipc.token")
}

#[cfg(unix)]
fn runtime_dir() -> PathBuf {
    env_non_empty(ENV_XDG_RUNTIME_DIR)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn env_non_empty(name: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(name).filter(|value| !value.is_empty())
}

/// Jeton neuf : 32 octets tirés du CSPRNG de l'OS, encodés en 64 hex minuscules.
pub fn fresh_token_hex() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    encode_hex(&bytes)
}

/// Écrit le jeton (crée les parents ; sur Unix, permissions 0600).
pub fn write_token_file(path: &Path, token_hex: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    {
        use std::io::Write as _;
        use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(token_hex.as_bytes())?;
        file.flush()?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, token_hex)?;
    }
    Ok(())
}

/// Lit le jeton et retire les blancs périphériques.
pub fn read_token_file(path: &Path) -> std::io::Result<String> {
    Ok(std::fs::read_to_string(path)?.trim().to_string())
}

#[cfg(windows)]
pub type ServerStream = tokio::net::windows::named_pipe::NamedPipeServer;
#[cfg(windows)]
pub type ClientStream = tokio::net::windows::named_pipe::NamedPipeClient;
#[cfg(unix)]
pub type ServerStream = tokio::net::UnixStream;
#[cfg(unix)]
pub type ClientStream = tokio::net::UnixStream;

#[cfg(windows)]
use tokio::net::windows::named_pipe::{ClientOptions, ServerOptions};

#[cfg(windows)]
struct ListenerState {
    pending: Option<ServerStream>,
    failure: Option<std::io::Error>,
}

/// Écoute locale. Windows : instances de named pipe recréées à chaque accept.
#[cfg(windows)]
pub struct IpcListener {
    endpoint: String,
    state: tokio::sync::Mutex<ListenerState>,
}

#[cfg(windows)]
impl IpcListener {
    /// Crée la première instance (revendique le nom, refuse les clients distants).
    pub async fn bind(endpoint: &str) -> std::io::Result<Self> {
        let first = pipe_instance(endpoint, true)?;
        Ok(Self {
            endpoint: endpoint.to_string(),
            state: tokio::sync::Mutex::new(ListenerState {
                pending: Some(first),
                failure: None,
            }),
        })
    }

    /// Attend un client puis prépare l'instance suivante.
    pub async fn accept(&self) -> std::io::Result<ServerStream> {
        let mut state = self.state.lock().await;
        if let Some(failure) = state.failure.take() {
            return Err(failure);
        }
        let Some(instance) = state.pending.take() else {
            return Err(std::io::Error::other(
                "hakille-ipc: aucune instance d'écoute en attente",
            ));
        };
        if let Err(err) = instance.connect().await {
            match pipe_instance(&self.endpoint, false) {
                Ok(next) => state.pending = Some(next),
                Err(rebind) => state.failure = Some(rebind),
            }
            return Err(err);
        }
        match pipe_instance(&self.endpoint, false) {
            Ok(next) => state.pending = Some(next),
            Err(err) => state.failure = Some(err),
        }
        Ok(instance)
    }
}

#[cfg(windows)]
fn pipe_instance(endpoint: &str, first: bool) -> std::io::Result<ServerStream> {
    ServerOptions::new()
        .first_pipe_instance(first)
        .reject_remote_clients(true)
        .create(endpoint)
}

/// Écoute locale. Unix : `UnixListener` (socket périmé supprimé avant bind).
#[cfg(unix)]
pub struct IpcListener {
    listener: tokio::net::UnixListener,
}

#[cfg(unix)]
impl IpcListener {
    pub async fn bind(endpoint: &str) -> std::io::Result<Self> {
        match tokio::net::UnixListener::bind(endpoint) {
            Ok(listener) => Ok(Self { listener }),
            Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
                match std::fs::remove_file(endpoint) {
                    Ok(()) => {}
                    Err(unlink) if unlink.kind() == std::io::ErrorKind::NotFound => {}
                    Err(unlink) => return Err(unlink),
                }
                Ok(Self {
                    listener: tokio::net::UnixListener::bind(endpoint)?,
                })
            }
            Err(err) => Err(err),
        }
    }

    pub async fn accept(&self) -> std::io::Result<ServerStream> {
        self.listener.accept().await.map(|(stream, _addr)| stream)
    }
}

/// Se connecte à l'endpoint (une seule tentative ; les retries sont à l'appelant).
#[cfg(windows)]
pub async fn connect(endpoint: &str) -> std::io::Result<ClientStream> {
    ClientOptions::new().open(endpoint)
}

/// Se connecte à l'endpoint (une seule tentative ; les retries sont à l'appelant).
#[cfg(unix)]
pub async fn connect(endpoint: &str) -> std::io::Result<ClientStream> {
    tokio::net::UnixStream::connect(endpoint).await
}

/// Lit une trame JSON terminée par `\n`.
///
/// `Ok(None)` = EOF propre (aucun octet en tampon). Plafond strict de
/// [`MAX_FRAME_BYTES`] : au-delà → [`IpcError::TooLarge`], sans jamais
/// allouer au-delà du plafond plus un morceau de lecture. La version est
/// vérifiée par [`Frame::check`] ; EOF sur ligne partielle → erreur de parsing.
pub async fn read_frame<R: AsyncRead + Unpin>(r: &mut R) -> Result<Option<Frame>, IpcError> {
    let mut line: Vec<u8> = Vec::with_capacity(READ_CHUNK_BYTES);
    let mut searched = 0usize;
    let mut chunk = [0u8; READ_CHUNK_BYTES];
    loop {
        if let Some(offset) = line[searched..].iter().position(|&byte| byte == b'\n') {
            let end = searched + offset;
            if end > MAX_FRAME_BYTES {
                return Err(IpcError::TooLarge);
            }
            return parse_line(&line[..end]).map(Some);
        }
        searched = line.len();
        if line.len() > MAX_FRAME_BYTES {
            return Err(IpcError::TooLarge);
        }
        let read = r.read(&mut chunk).await?;
        if read == 0 {
            if line.is_empty() {
                return Ok(None);
            }
            return parse_line(&line).map(Some);
        }
        line.extend_from_slice(&chunk[..read]);
    }
}

/// Sérialise puis écrit la trame suivie de `\n` (flush inclus).
pub async fn write_frame<W: AsyncWrite + Unpin>(w: &mut W, frame: &Frame) -> Result<(), IpcError> {
    frame.check()?;
    let mut json = serde_json::to_string(frame)?;
    if json.len() > MAX_FRAME_BYTES {
        return Err(IpcError::TooLarge);
    }
    json.push('\n');
    w.write_all(json.as_bytes()).await?;
    w.flush().await?;
    Ok(())
}

/// Handshake client : Hello{token}, puis Welcome exigé sous 10 s.
pub async fn client_handshake<S: AsyncRead + AsyncWrite + Unpin>(
    s: &mut S,
    token_hex: &str,
) -> Result<(), IpcError> {
    write_frame(s, &Frame::hello(token_hex)).await?;
    let frame = match tokio::time::timeout(HANDSHAKE_TIMEOUT, read_frame(s)).await {
        Err(_elapsed) => return Err(IpcError::Handshake("timeout".to_string())),
        Ok(received) => received?,
    };
    match frame {
        None => Err(IpcError::Closed),
        Some(frame) => match frame.body {
            FrameBody::Welcome { .. } => Ok(()),
            FrameBody::Error { message, .. } => Err(IpcError::Handshake(message)),
            other => Err(IpcError::Handshake(format!(
                "réponse inattendue au Hello: {other:?}"
            ))),
        },
    }
}

/// Handshake serveur : Hello exigé sous 10 s, jeton comparé en temps
/// ~constant sur les octets décodés, puis Welcome (ou Error explicite).
pub async fn server_handshake<S: AsyncRead + AsyncWrite + Unpin>(
    s: &mut S,
    expected_token_hex: &str,
) -> Result<(), IpcError> {
    let frame = match tokio::time::timeout(HANDSHAKE_TIMEOUT, read_frame(s)).await {
        Err(_elapsed) => return Err(IpcError::Handshake("timeout".to_string())),
        Ok(Ok(Some(frame))) => frame,
        Ok(Ok(None)) => return Err(IpcError::Closed),
        Ok(Err(IpcError::BadVersion(version))) => {
            let message = format!("version {version} incompatible");
            let _ = write_frame(s, &Frame::error(0, ErrorCode::BadVersion, message.clone())).await;
            return Err(IpcError::Handshake(message));
        }
        Ok(Err(err)) => return Err(err),
    };
    let Frame { id, body, .. } = frame;
    match body {
        FrameBody::Hello { token_hex } => {
            if tokens_equal(expected_token_hex, &token_hex) {
                let daemon_version = format!("hakille-daemon v{}", env!("CARGO_PKG_VERSION"));
                write_frame(s, &Frame::welcome(daemon_version)).await?;
                Ok(())
            } else {
                let _ =
                    write_frame(s, &Frame::error(id, ErrorCode::BadToken, "jeton invalide")).await;
                Err(IpcError::Handshake("jeton invalide".to_string()))
            }
        }
        other => {
            let message = "handshake: Hello attendu".to_string();
            let _ = write_frame(s, &Frame::error(id, ErrorCode::BadFrame, message.clone())).await;
            Err(IpcError::Handshake(format!("{message} (reçu {other:?})")))
        }
    }
}

fn parse_line(line: &[u8]) -> Result<Frame, IpcError> {
    let frame: Frame = serde_json::from_slice(line)?;
    frame.check()?;
    Ok(frame)
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    let bytes = hex.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        out.push((hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?);
    }
    Some(out)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn tokens_equal(expected_hex: &str, received_hex: &str) -> bool {
    let (Some(expected), Some(received)) = (decode_hex(expected_hex), decode_hex(received_hex))
    else {
        return false;
    };
    if expected.is_empty() || expected.len() != received.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in expected.iter().zip(received.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}
