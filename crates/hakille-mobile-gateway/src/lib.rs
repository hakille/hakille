//! Passerelle mobile locale ; une unique session IPC persistante.
//!
//! Les trames de suivi sont délimitées par une requête de statut interne, pas
//! par un délai de silence. Toute interruption après envoi ferme la session et
//! produit une issue inconnue, sans jamais rejouer la requête.

mod http;
pub use http::serve;

use hakille_ipc::{Frame, FrameBody, MAX_FRAME_BYTES, transport};
use hakille_ui_protocol::{KernelToUi, UiToKernel};
use serde::Serialize;
use std::io::Read;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;
use subtle::ConstantTimeEq;
use tokio::sync::Mutex;
use tokio::time::Instant;

pub const MAX_RESPONSE_BYTES: usize = 1 << 20;
pub const MAX_RESPONSE_FRAMES: usize = 256;
pub const BARRIER_ID: u64 = u64::MAX;

#[derive(Clone)]
pub struct GatewayConfig {
    pub listen: SocketAddr,
    pub ipc_endpoint: String,
    pub ipc_token_file: PathBuf,
    pub request_timeout: Duration,
    pub body_timeout: Duration,
    pub session_idle_timeout: Duration,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            listen: (Ipv4Addr::LOCALHOST, 17840).into(),
            ipc_endpoint: transport::default_endpoint(),
            ipc_token_file: transport::default_token_path(),
            request_timeout: Duration::from_secs(680),
            body_timeout: Duration::from_secs(10),
            session_idle_timeout: Duration::from_secs(900),
        }
    }
}

impl GatewayConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.listen.ip() != Ipv4Addr::LOCALHOST {
            return Err("écoute autorisée uniquement sur 127.0.0.1");
        }
        if self.request_timeout.is_zero()
            || self.request_timeout > Duration::from_secs(900)
            || self.body_timeout.is_zero()
            || self.body_timeout > Duration::from_secs(30)
            || self.session_idle_timeout.is_zero()
            || self.session_idle_timeout > Duration::from_secs(3600)
        {
            return Err("délais invalides : requête ≤900s, corps ≤30s, session inactive ≤3600s");
        }
        Ok(())
    }
}

/// Jeton distinct de l'IPC, jamais affiché par Debug.
pub struct BearerToken([u8; 64]);

impl std::fmt::Debug for BearerToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BearerToken([secret])")
    }
}

impl BearerToken {
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("jeton attendu : 64 caractères hexadécimaux");
        }
        Ok(Self(
            value.as_bytes().try_into().expect("longueur vérifiée"),
        ))
    }

    /// Publier après bind HTTP réussi, pour ne pas remplacer le jeton d'une
    /// autre instance. Le parent doit être sous le contrôle de l'utilisateur.
    pub fn create_file(path: &Path) -> std::io::Result<Self> {
        let fresh = transport::fresh_token_hex();
        transport::write_token_file(path, &fresh)?;
        Self::parse(&fresh).map_err(std::io::Error::other)
    }

    pub(crate) fn matches(&self, value: &[u8]) -> bool {
        value.len() == self.0.len() && bool::from(self.0.as_slice().ct_eq(value))
    }
}

/// Lecture bornée du jeton IPC existant ; aucun contenu n'entre dans les erreurs.
pub fn read_private_token(path: &Path) -> std::io::Result<String> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(std::io::Error::other(
            "le jeton doit être un fichier ordinaire",
        ));
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Ne pas suivre un lien ni bloquer sur un FIFO remplacé entre le
        // contrôle du chemin et open. Les permissions sont celles du handle.
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
        // Même protection que O_NOFOLLOW : ouvrir le point de réanalyse sans
        // suivre une cible substituée entre symlink_metadata et open.
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > 256 {
        return Err(std::io::Error::other(
            "fichier de jeton invalide ou trop volumineux",
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(std::io::Error::other(
                "point de réanalyse refusé pour le jeton",
            ));
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(std::io::Error::other("permissions du jeton trop ouvertes"));
        }
    }
    let mut bytes = Vec::new();
    file.take(257).read_to_end(&mut bytes)?;
    if bytes.len() > 256 {
        return Err(std::io::Error::other("fichier de jeton trop volumineux"));
    }
    let value = std::str::from_utf8(&bytes)
        .map_err(|_| std::io::Error::other("encodage du jeton invalide"))?
        .trim();
    BearerToken::parse(value).map_err(std::io::Error::other)?;
    Ok(value.to_owned())
}

#[derive(Debug, Serialize)]
pub struct GatewayError {
    pub code: &'static str,
    pub message: &'static str,
    pub issue_unknown: bool,
}

#[derive(Debug, Serialize)]
pub struct GatewayResponse {
    pub session_id: Option<String>,
    pub frames: Vec<Frame>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<GatewayError>,
}

impl GatewayResponse {
    pub(crate) fn failure(
        code: &'static str,
        message: &'static str,
        issue_unknown: bool,
        session_id: Option<String>,
    ) -> Self {
        Self {
            session_id,
            frames: Vec::new(),
            error: Some(GatewayError {
                code,
                message,
                issue_unknown,
            }),
        }
    }
}

struct Connection {
    id: String,
    stream: transport::ClientStream,
    last_used: Instant,
}

pub struct Gateway {
    pub(crate) config: GatewayConfig,
    pub(crate) bearer: BearerToken,
    connection: std::sync::Arc<Mutex<Option<Connection>>>,
}

impl Gateway {
    pub fn new(config: GatewayConfig, bearer: BearerToken) -> Result<Self, &'static str> {
        config.validate()?;
        Ok(Self {
            config,
            bearer,
            connection: std::sync::Arc::new(Mutex::new(None)),
        })
    }

    pub async fn close_session(&self) {
        *self.connection.lock().await = None;
    }

    pub(crate) fn expire_idle_session(&self) {
        if let Ok(mut state) = self.connection.try_lock()
            && state.as_ref().is_some_and(|session| {
                session.last_used.elapsed() >= self.config.session_idle_timeout
            })
        {
            *state = None;
        }
    }

    fn validate_request(&self, frame: &Frame) -> Result<bool, (u16, GatewayResponse)> {
        if frame.check().is_err() {
            return Err((
                400,
                GatewayResponse::failure("bad_version", "version IPC incompatible", false, None),
            ));
        }
        if frame.id == 0 || frame.id == BARRIER_ID {
            return Err((
                400,
                GatewayResponse::failure("bad_id", "identifiant réservé", false, None),
            ));
        }
        let message = match &frame.body {
            FrameBody::Request { message } if allowed(message) => message,
            _ => {
                return Err((
                    400,
                    GatewayResponse::failure(
                        "unsupported",
                        "requête non exposée par la passerelle",
                        false,
                        None,
                    ),
                ));
            }
        };
        let card_decision = matches!(
            message,
            UiToKernel::Approve { .. } | UiToKernel::Edit { .. } | UiToKernel::Reject { .. }
        );
        // Sérialisation vérifiée avant d'ouvrir la session ou de marquer l'issue
        // inconnue ; aucune erreur locale de taille ne doit envoyer de requête.
        if serde_json::to_vec(frame).map_or(true, |bytes| bytes.len() > MAX_FRAME_BYTES) {
            return Err((
                413,
                GatewayResponse::failure("too_large", "requête trop volumineuse", false, None),
            ));
        }
        Ok(card_decision)
    }

    async fn ensure_connection(
        &self,
        state: &mut Option<Connection>,
        card_decision: bool,
        expected_session: Option<&str>,
    ) -> Result<Connection, (u16, GatewayResponse)> {
        if state
            .as_ref()
            .is_some_and(|session| session.last_used.elapsed() >= self.config.session_idle_timeout)
        {
            *state = None;
        }
        let current_id = state.as_ref().map(|session| session.id.as_str());
        if (card_decision && expected_session.is_none())
            || expected_session.is_some_and(|expected| Some(expected) != current_id)
            || (card_decision && current_id.is_none())
        {
            return Err((
                409,
                GatewayResponse::failure(
                    "stale_session",
                    "session absente ou expirée : les cartes doivent être rechargées",
                    false,
                    current_id.map(str::to_owned),
                ),
            ));
        }
        if state.is_none() {
            let connected = tokio::time::timeout(Duration::from_secs(10), async {
                let token = read_private_token(&self.config.ipc_token_file).map_err(|_| ())?;
                if self.bearer.matches(token.as_bytes()) {
                    return Err(());
                }
                let mut stream = transport::connect(&self.config.ipc_endpoint)
                    .await
                    .map_err(|_| ())?;
                transport::client_handshake(&mut stream, &token)
                    .await
                    .map_err(|_| ())?;
                Ok::<_, ()>(Connection {
                    id: uuid::Uuid::new_v4().to_string(),
                    stream,
                    last_used: Instant::now(),
                })
            })
            .await;
            match connected {
                Ok(Ok(connection)) => *state = Some(connection),
                _ => {
                    return Err((
                        503,
                        GatewayResponse::failure(
                            "ipc_unavailable",
                            "IPC indisponible ou authentification refusée ; aucun envoi métier",
                            false,
                            None,
                        ),
                    ));
                }
            }
        }
        Ok(state.take().expect("connexion établie"))
    }

    pub async fn request(
        &self,
        frame: Frame,
        expected_session: Option<&str>,
    ) -> (u16, GatewayResponse) {
        let card_decision = match self.validate_request(&frame) {
            Ok(c) => c,
            Err(res) => return res,
        };
        let mut state = match self.connection.try_lock() {
            Ok(state) => state,
            Err(_) => {
                return (
                    429,
                    GatewayResponse::failure("busy", "une requête est déjà en cours", false, None),
                );
            }
        };
        let mut connection = match self
            .ensure_connection(&mut state, card_decision, expected_session)
            .await
        {
            Ok(conn) => conn,
            Err(res) => return res,
        };
        // Retirer la connexion de l'état avant tout await d'envoi : si HTTP
        // abandonne la future, le flux local est détruit plutôt que réutilisé
        // avec une requête dont l'issue est inconnue.
        let session_id = connection.id.clone();
        let result = tokio::time::timeout(
            self.config.request_timeout,
            exchange(&mut connection.stream, &frame),
        )
        .await;
        match result {
            Ok(Ok(frames)) => {
                let response = GatewayResponse {
                    session_id: Some(session_id.clone()),
                    frames,
                    error: None,
                };
                if serde_json::to_vec(&response)
                    .map_or(true, |bytes| bytes.len() > MAX_RESPONSE_BYTES)
                {
                    return (
                        502,
                        GatewayResponse::failure(
                            "response_too_large",
                            "réponse trop volumineuse ; session fermée, ne pas rejouer",
                            true,
                            Some(session_id),
                        ),
                    );
                }
                connection.last_used = Instant::now();
                *state = Some(connection);
                (200, response)
            }
            Err(_) => (
                504,
                GatewayResponse::failure(
                    "ipc_timeout",
                    "échéance IPC dépassée ; issue inconnue, ne pas rejouer",
                    true,
                    Some(session_id),
                ),
            ),
            Ok(Err(_)) => (
                502,
                GatewayResponse::failure(
                    "ipc_transport",
                    "échange IPC interrompu ou invalide ; issue inconnue, ne pas rejouer",
                    true,
                    Some(session_id),
                ),
            ),
        }
    }

    pub async fn request_stream(
        self: &std::sync::Arc<Self>,
        frame: Frame,
        expected_session: Option<&str>,
    ) -> Result<(String, tokio::sync::mpsc::Receiver<bytes::Bytes>), (u16, GatewayResponse)> {
        let card_decision = self.validate_request(&frame)?;
        let mut state = match self.connection.clone().try_lock_owned() {
            Ok(state) => state,
            Err(_) => {
                return Err((
                    429,
                    GatewayResponse::failure("busy", "une requête est déjà en cours", false, None),
                ));
            }
        };
        let mut connection = self
            .ensure_connection(&mut state, card_decision, expected_session)
            .await?;
        let session_id = connection.id.clone();
        let (tx, rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(32);
        let gateway = self.clone();
        let request_id = frame.id;

        tokio::spawn(async move {
            let mut state = state;
            let res = tokio::time::timeout(gateway.config.request_timeout, async {
                transport::write_frame(&mut connection.stream, &frame)
                    .await
                    .map_err(|_| ())?;
                transport::write_frame(
                    &mut connection.stream,
                    &Frame::request(BARRIER_ID, UiToKernel::StatusRequest),
                )
                .await
                .map_err(|_| ())?;

                let mut saw_final = false;
                let mut accounted_bytes = 0_usize;
                let mut frame_count = 0_usize;

                loop {
                    let incoming = transport::read_frame(&mut connection.stream)
                        .await
                        .map_err(|_| ())?
                        .ok_or(())?;
                    incoming.check().map_err(|_| ())?;

                    if incoming.id == BARRIER_ID {
                        if !saw_final
                            || !matches!(
                                incoming.body,
                                FrameBody::Reply {
                                    message: KernelToUi::StatusReply { .. }
                                }
                            )
                        {
                            return Err(());
                        }
                        let done_event = b"event: done\ndata: {}\n\n";
                        let _ = tx.send(bytes::Bytes::from_static(done_event)).await;
                        return Ok(());
                    }

                    if !matches!(
                        incoming.body,
                        FrameBody::Reply { .. } | FrameBody::Error { .. }
                    ) || (incoming.id != 0 && incoming.id != request_id)
                    {
                        return Err(());
                    }

                    if incoming.id == request_id {
                        if saw_final {
                            return Err(());
                        }
                        saw_final = true;
                    }

                    let json = serde_json::to_string(&incoming).map_err(|_| ())?;
                    accounted_bytes = accounted_bytes.checked_add(json.len()).ok_or(())?;
                    if accounted_bytes > MAX_RESPONSE_BYTES - 1024 {
                        return Err(());
                    }
                    frame_count += 1;
                    if frame_count > MAX_RESPONSE_FRAMES {
                        return Err(());
                    }

                    let sse = format!("data: {json}\n\n");
                    if tx.send(bytes::Bytes::from(sse)).await.is_err() {
                        return Err(());
                    }
                }
            })
            .await;

            match res {
                Ok(Ok(())) => {
                    connection.last_used = Instant::now();
                    *state = Some(connection);
                }
                _ => {
                    drop(connection);
                    *state = None;
                }
            }
        });

        Ok((session_id, rx))
    }
}

fn allowed(message: &UiToKernel) -> bool {
    match message {
        UiToKernel::HudSubmit { query } => !query.trim().is_empty() && query.len() <= 64 * 1024,
        UiToKernel::MemoryRemember { text } => !text.trim().is_empty() && text.len() <= 64 * 1024,
        UiToKernel::MemoryForget { row_id } => !row_id.is_empty() && row_id.len() <= 256,
        UiToKernel::JournalRequest { limit } => *limit > 0 && *limit <= 200,
        UiToKernel::MemoryRequest { query, limit } => {
            *limit > 0
                && *limit <= 200
                && query.as_ref().is_none_or(|query| query.len() <= 64 * 1024)
        }
        UiToKernel::AgentRunRequest { name, input_json } => {
            !name.is_empty() && name.len() <= 128 && input_json.len() <= 64 * 1024
        }
        UiToKernel::Approve { .. }
        | UiToKernel::Reject { .. }
        | UiToKernel::Edit { .. }
        | UiToKernel::StatusRequest
        | UiToKernel::AgentListRequest
        | UiToKernel::PerceptionControl { .. } => true,
        UiToKernel::NotificationAction { id, action_id } => {
            !id.is_empty() && id.len() <= 256 && !action_id.is_empty() && action_id.len() <= 256
        }
        UiToKernel::NotificationDismiss { id }
        | UiToKernel::GhostActionAccept { id }
        | UiToKernel::GhostActionDismiss { id } => !id.is_empty() && id.len() <= 256,
        UiToKernel::SpatialGesture(_) => true,
        UiToKernel::Undo { journal_id } => *journal_id > 0,
        UiToKernel::RevokeAll | UiToKernel::SettingsToggle { .. } | UiToKernel::OutboxFlush => {
            false
        }
    }
}

async fn exchange(stream: &mut transport::ClientStream, request: &Frame) -> Result<Vec<Frame>, ()> {
    transport::write_frame(stream, request)
        .await
        .map_err(|_| ())?;
    // Le démon traite le flux séquentiellement. La réponse de statut arrive
    // nécessairement après le final ET ses ShowCard/Presence/Toast de suivi.
    transport::write_frame(
        stream,
        &Frame::request(BARRIER_ID, UiToKernel::StatusRequest),
    )
    .await
    .map_err(|_| ())?;
    let mut frames: Vec<Frame> = Vec::new();
    let mut saw_final = false;
    let mut accounted_bytes = 0_usize;
    loop {
        let frame = transport::read_frame(stream)
            .await
            .map_err(|_| ())?
            .ok_or(())?;
        frame.check().map_err(|_| ())?;
        if frame.id == BARRIER_ID {
            if !saw_final
                || !matches!(
                    frame.body,
                    FrameBody::Reply {
                        message: KernelToUi::StatusReply { .. }
                    }
                )
            {
                return Err(());
            }
            return Ok(frames);
        }
        if !matches!(
            frame.body,
            FrameBody::Reply { .. } | FrameBody::Error { .. }
        ) || (frame.id != 0 && frame.id != request.id)
        {
            return Err(());
        }
        if frame.id == request.id {
            if saw_final {
                return Err(());
            }
            saw_final = true;
        }
        accounted_bytes = accounted_bytes
            .checked_add(serde_json::to_vec(&frame).map_err(|_| ())?.len())
            .ok_or(())?;
        if accounted_bytes > MAX_RESPONSE_BYTES - 1024 {
            return Err(());
        }
        if frame.id == 0
            && let FrameBody::Reply {
                message: KernelToUi::TokenDelta { text },
            } = &frame.body
            && let Some(Frame {
                id: 0,
                body:
                    FrameBody::Reply {
                        message: KernelToUi::TokenDelta { text: previous },
                    },
                ..
            }) = frames.last_mut()
        {
            previous.push_str(text);
            continue;
        }
        if frames.len() >= MAX_RESPONSE_FRAMES {
            return Err(());
        }
        frames.push(frame);
    }
}
